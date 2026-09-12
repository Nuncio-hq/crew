use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use tauri::AppHandle;

use super::RECAP_SCOPE_POLL_MS;

pub(super) struct ScopeWatchGuard(tokio::task::JoinHandle<()>);

impl ScopeWatchGuard {
    pub(super) fn new<R: tauri::Runtime>(
        app: AppHandle<R>,
        token: crate::app_state::owner_scope::OwnerScopeToken,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self(tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(RECAP_SCOPE_POLL_MS)).await;
                if cancelled.load(Ordering::Acquire) {
                    break;
                }
                if crate::app_state::owner_scope::assert_current(app.clone(), &token)
                    .await
                    .is_err()
                {
                    cancelled.store(true, Ordering::Release);
                    break;
                }
            }
        }))
    }

    pub(super) fn abort(&mut self) {
        self.0.abort();
    }
}

impl Drop for ScopeWatchGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}
