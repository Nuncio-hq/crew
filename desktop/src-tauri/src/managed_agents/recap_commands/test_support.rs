use std::sync::{Arc, Mutex, OnceLock};

use super::ThreadSource;

#[cfg(test)]
struct TestSourceBarrier {
    source: ThreadSource,
    after_assert: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

#[cfg(test)]
struct TestCommitBarrier {
    after_provider: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

#[cfg(test)]
struct TestSettingsLoadBarrier {
    after_register: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

#[cfg(test)]
struct TestSettingsPersistBarrier {
    after_capture: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

#[cfg(test)]
fn test_source_barrier() -> &'static Mutex<Option<TestSourceBarrier>> {
    static BARRIER: OnceLock<Mutex<Option<TestSourceBarrier>>> = OnceLock::new();
    BARRIER.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
fn test_commit_barrier() -> &'static Mutex<Option<TestCommitBarrier>> {
    static BARRIER: OnceLock<Mutex<Option<TestCommitBarrier>>> = OnceLock::new();
    BARRIER.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
fn test_settings_load_barrier() -> &'static Mutex<Option<TestSettingsLoadBarrier>> {
    static BARRIER: OnceLock<Mutex<Option<TestSettingsLoadBarrier>>> = OnceLock::new();
    BARRIER.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
fn test_settings_persist_barrier() -> &'static Mutex<Option<TestSettingsPersistBarrier>> {
    static BARRIER: OnceLock<Mutex<Option<TestSettingsPersistBarrier>>> = OnceLock::new();
    BARRIER.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
pub(super) fn install_test_source_barrier(
    source: ThreadSource,
) -> (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>) {
    let after_assert = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    *test_source_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(TestSourceBarrier {
        source,
        after_assert: after_assert.clone(),
        release: release.clone(),
    });
    (after_assert, release)
}

#[cfg(test)]
pub(super) fn clear_test_source_barrier() {
    *test_source_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

pub(super) async fn await_test_signal(signal: Arc<tokio::sync::Notify>, label: &'static str) {
    tokio::time::timeout(std::time::Duration::from_secs(10), signal.notified())
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {label}"));
}

#[cfg(test)]
pub(super) fn install_test_commit_barrier() -> (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)
{
    let after_provider = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    *test_commit_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(TestCommitBarrier {
        after_provider: after_provider.clone(),
        release: release.clone(),
    });
    (after_provider, release)
}

#[cfg(test)]
pub(super) fn clear_test_commit_barrier() {
    *test_commit_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

#[cfg(test)]
pub(super) fn install_test_settings_load_barrier(
) -> (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>) {
    let after_register = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    *test_settings_load_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(TestSettingsLoadBarrier {
        after_register: after_register.clone(),
        release: release.clone(),
    });
    (after_register, release)
}

#[cfg(test)]
pub(super) fn clear_test_settings_load_barrier() {
    *test_settings_load_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

#[cfg(test)]
pub(super) fn install_test_settings_persist_barrier(
) -> (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>) {
    let after_capture = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    *test_settings_persist_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(TestSettingsPersistBarrier {
        after_capture: after_capture.clone(),
        release: release.clone(),
    });
    (after_capture, release)
}

#[cfg(test)]
pub(super) fn clear_test_settings_persist_barrier() {
    *test_settings_persist_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

#[cfg(test)]
pub(super) fn test_source_override() -> Option<ThreadSource> {
    test_source_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|barrier| barrier.source.clone())
}

#[cfg(test)]
pub(super) async fn wait_for_test_source_after_assert() {
    let signals = test_source_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|barrier| (barrier.after_assert.clone(), barrier.release.clone()));
    if let Some((after_assert, release)) = signals {
        after_assert.notify_one();
        await_test_signal(release, "source barrier release").await;
    }
}

#[cfg(test)]
pub(super) async fn wait_for_test_commit_before_persist() {
    let signals = test_commit_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|barrier| (barrier.after_provider.clone(), barrier.release.clone()));
    if let Some((after_provider, release)) = signals {
        after_provider.notify_one();
        await_test_signal(release, "commit barrier release").await;
    }
}

#[cfg(test)]
pub(super) async fn wait_for_test_settings_load() {
    let signals = test_settings_load_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|barrier| (barrier.after_register.clone(), barrier.release.clone()));
    if let Some((after_register, release)) = signals {
        after_register.notify_one();
        await_test_signal(release, "settings-load barrier release").await;
    }
}

#[cfg(test)]
pub(super) async fn wait_for_test_settings_persist_before_lock() {
    let signals = test_settings_persist_barrier()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()
        .map(|barrier| (barrier.after_capture.clone(), barrier.release.clone()));
    if let Some((after_capture, release)) = signals {
        after_capture.notify_one();
        await_test_signal(release, "settings-persist barrier release").await;
    }
}
