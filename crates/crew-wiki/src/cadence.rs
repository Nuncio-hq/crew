//! Cadence + on-push debounce. Pure so tests do not need a clock server.

use buzz_core::wiki_page::WikiCadence;
use std::time::Duration;

/// Default debounce after a kind 30618 default-branch tip change.
pub const ON_PUSH_DEBOUNCE: Duration = Duration::from_secs(30);

/// Clock for cadence due checks (injectable).
pub trait CadenceClock {
    /// Unix seconds.
    fn now_unix(&self) -> i64;
}

/// System clock.
pub struct SystemClock;

impl CadenceClock for SystemClock {
    fn now_unix(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

/// Whether an on-push refresh should fire given last-fire and last-30618 times.
pub fn debounce_due(last_fired_unix: i64, last_push_unix: i64, now_unix: i64) -> bool {
    if last_push_unix <= 0 {
        return false;
    }
    if now_unix < last_push_unix + ON_PUSH_DEBOUNCE.as_secs() as i64 {
        return false;
    }
    last_fired_unix < last_push_unix
}

/// Next cadence fire. `last_generated_unix` is the TOC `generated_at`.
pub fn next_cadence_due(cadence: WikiCadence, last_generated_unix: i64, now_unix: i64) -> bool {
    match cadence {
        WikiCadence::Manual | WikiCadence::OnPush => false,
        WikiCadence::Daily => now_unix - last_generated_unix >= 86_400,
        WikiCadence::Weekly => now_unix - last_generated_unix >= 86_400 * 7,
    }
}

/// In-process generate lock (one repo at a time).
#[derive(Debug, Default)]
pub struct GenerateLock {
    active: std::sync::Mutex<std::collections::BTreeSet<String>>,
}

impl GenerateLock {
    /// Try to begin a generate. Err if that repo is already running.
    pub fn acquire(&self, repo_key: &str) -> Result<GenerateGuard<'_>, crate::WikiError> {
        let mut set = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if !set.insert(repo_key.to_string()) {
            return Err(crate::WikiError::GenerateInProgress);
        }
        Ok(GenerateGuard {
            lock: self,
            repo_key: repo_key.to_string(),
        })
    }
}

/// Drops the in-progress marker.
pub struct GenerateGuard<'a> {
    lock: &'a GenerateLock,
    repo_key: String,
}

impl Drop for GenerateGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut set) = self.lock.active.lock() {
            set.remove(&self.repo_key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_waits_thirty_seconds() {
        assert!(!debounce_due(0, 100, 120));
        assert!(debounce_due(0, 100, 131));
        assert!(!debounce_due(150, 100, 200));
    }

    #[test]
    fn lock_rejects_parallel() {
        let lock = GenerateLock::default();
        let a = lock.acquire("crew").expect("first");
        assert!(lock.acquire("crew").is_err());
        drop(a);
        lock.acquire("crew").expect("after drop");
    }
}
