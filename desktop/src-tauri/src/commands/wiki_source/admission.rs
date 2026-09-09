//! Admission belongs to actual callbacks/workers, not just their IPC waiters.
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{oneshot, OwnedSemaphorePermit, Semaphore};

pub(super) type Permit = Arc<OwnedSemaphorePermit>;
pub(super) type Selection = Result<Option<PathBuf>, String>;

pub(super) struct Admission {
    chooser: Arc<Semaphore>,
    reads: Arc<Semaphore>,
}
impl Default for Admission {
    fn default() -> Self {
        Self {
            chooser: Arc::new(Semaphore::new(1)),
            reads: Arc::new(Semaphore::new(2)),
        }
    }
}
impl Admission {
    pub(super) fn choose(&self) -> Result<Permit, String> {
        self.chooser
            .clone()
            .try_acquire_owned()
            .map(Arc::new)
            .map_err(|_| "a folder chooser is already open".into())
    }
    pub(super) fn read(&self) -> Result<Permit, String> {
        self.reads
            .clone()
            .try_acquire_owned()
            .map(Arc::new)
            .map_err(|_| "Source is busy; try again".into())
    }
}

pub(super) struct PendingPicker {
    permit: Permit,
    result: oneshot::Receiver<Selection>,
}
impl PendingPicker {
    pub(super) async fn wait(self) -> Result<(Option<PathBuf>, Permit), String> {
        let selected = self
            .result
            .await
            .map_err(|_| "folder chooser unavailable")??;
        Ok((selected, self.permit))
    }
}

pub(super) fn picker_bridge(
    permit: Permit,
) -> (PendingPicker, impl FnOnce(Selection) + Send + 'static) {
    let (tx, result) = oneshot::channel();
    let callback_permit = Arc::clone(&permit);
    (PendingPicker { permit, result }, move |selection| {
        let _ = tx.send(selection);
        drop(callback_permit);
    })
}

pub(super) fn spawn_source_read<T, F>(permit: Permit, read: F) -> tokio::task::JoinHandle<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        read()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn picker_positive_cancel_and_saturation_bind_the_bridge() {
        let admission = Admission::default();
        let (pending, callback) = picker_bridge(admission.choose().unwrap());
        assert!(admission.choose().is_err());
        callback(Ok(None));
        let (selected, permit) = pending.wait().await.unwrap();
        assert!(selected.is_none());
        assert!(admission.choose().is_err());
        drop(permit);
        assert!(admission.choose().is_ok());
    }

    #[tokio::test]
    async fn canceled_picker_waiter_retains_slot_until_actual_callback_ends() {
        let admission = Admission::default();
        let (pending, callback) = picker_bridge(admission.choose().unwrap());
        drop(pending);
        let blocked = admission.choose().is_err();
        callback(Ok(Some(PathBuf::from("/unused-native-picker-result"))));
        assert!(
            blocked,
            "canceling IPC must not admit another still-open picker"
        );
        assert!(admission.choose().is_ok());
    }

    #[tokio::test]
    async fn canceled_source_waiters_cannot_admit_more_running_workers() {
        let admission = Admission::default();
        let mut workers = Vec::new();
        let mut releases = Vec::new();
        for _ in 0..2 {
            let permit = admission.read().unwrap();
            let (release, wait) = std::sync::mpsc::channel();
            let (started, receive_started) = std::sync::mpsc::channel();
            let worker = spawn_source_read(permit.clone(), move || {
                started.send(()).unwrap();
                wait.recv_timeout(Duration::from_secs(5)).unwrap();
            });
            receive_started
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
            drop(permit); // canceled IPC waiter; actual blocking job remains alive.
            workers.push(worker);
            releases.push(release);
        }
        let blocked = admission.read().is_err();
        for release in releases {
            release.send(()).unwrap();
        }
        for worker in workers {
            worker.await.unwrap();
        }
        assert!(blocked, "running blocking workers retain their admission");
        assert!(admission.read().is_ok());
    }
}
