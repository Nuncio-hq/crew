//! Which private Ask attempts are in flight, and how to stop one.
//!
//! A private Ask runs to completion inside one command invocation, so the
//! renderer cannot learn the attempt id from the answer and then cancel it —
//! by then there is nothing left to cancel. The id is therefore minted by the
//! caller, validated at the command boundary, and registered here *before* the
//! run starts. `private_ask_cancel` looks the id up and raises the same flag
//! the attempt already polls, so cancellation travels through the bounded
//! runner's existing seam rather than through a second teardown path.
//!
//! Three properties this type owns, each because getting it wrong is silent:
//!
//! * a registration is removed on every exit from the run, by `Drop`, so a
//!   finished attempt can never be cancelled by a later caller reusing its id;
//! * an id that is already in flight is refused rather than aliased — two runs
//!   sharing one flag would let either cancel the other;
//! * the map is bounded, so a caller cannot grow it by starting runs.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// How many private Asks may be in flight at once on this machine.
///
/// Each one owns a contained child process and a model call, so this is a
/// resource bound rather than a policy: a developer surface that can be driven
/// from a webview must not be able to start an unbounded number of them.
pub(crate) const MAX_IN_FLIGHT_ASKS: usize = 2;

/// Why an attempt could not be registered. Each maps to a distinct, honest
/// message: "this id is already running" is not "too many are running".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegisterFailure {
    /// The same attempt id is already in flight.
    AlreadyRunning,
    /// The in-flight bound is reached.
    TooManyRunning,
}

/// The process-wide registry, held in Tauri managed state.
#[derive(Clone, Default)]
pub struct PrivateAskAttempts {
    inner: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

/// Take the map even if a previous holder panicked.
///
/// A poisoned lock must not make cancellation impossible: the map is a plain
/// `HashMap` of flags with no invariant a panic could half-break, and refusing
/// to unlock it would strand every later attempt in the "already running"
/// state forever.
fn entries(
    lock: &Mutex<HashMap<String, Arc<AtomicBool>>>,
) -> MutexGuard<'_, HashMap<String, Arc<AtomicBool>>> {
    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl PrivateAskAttempts {
    /// Claim one attempt id for the run that is about to start.
    ///
    /// The returned guard owns the registration: dropping it removes the entry,
    /// which is what makes every return path out of the run — answer, refusal
    /// or early fence — leave the registry clean.
    pub(crate) fn register(
        &self,
        attempt_id: &str,
    ) -> Result<AttemptRegistration, RegisterFailure> {
        let mut entries = entries(&self.inner);
        if entries.contains_key(attempt_id) {
            return Err(RegisterFailure::AlreadyRunning);
        }
        if entries.len() >= MAX_IN_FLIGHT_ASKS {
            return Err(RegisterFailure::TooManyRunning);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        entries.insert(attempt_id.to_owned(), Arc::clone(&cancel));
        drop(entries);
        Ok(AttemptRegistration {
            registry: self.clone(),
            attempt_id: attempt_id.to_owned(),
            cancel,
        })
    }

    /// Withdraw one attempt.
    ///
    /// Cancelling an unknown or already-finished id is a no-op: a renderer that
    /// cancels twice, or cancels after the answer arrived, has not done
    /// anything wrong. The return value says whether a live attempt was
    /// signalled, so a test can tell the two apart.
    pub(crate) fn cancel(&self, attempt_id: &str) -> bool {
        match entries(&self.inner).get(attempt_id) {
            Some(cancel) => {
                cancel.store(true, Ordering::Release);
                true
            }
            None => false,
        }
    }

    /// Whether this attempt id still owns a live registration.
    ///
    /// This is the question the history's `running`-vs-`interrupted` reading
    /// asks: a recorded attempt that is still registered is genuinely running;
    /// one the process no longer holds vanished with it.
    pub(crate) fn is_registered(&self, attempt_id: &str) -> bool {
        entries(&self.inner).contains_key(attempt_id)
    }

    /// How many attempts are in flight. Used by the bound's own test.
    #[cfg(test)]
    pub(crate) fn in_flight(&self) -> usize {
        entries(&self.inner).len()
    }
}

/// One live registration. Dropping it releases the id.
pub(crate) struct AttemptRegistration {
    registry: PrivateAskAttempts,
    attempt_id: String,
    cancel: Arc<AtomicBool>,
}

impl AttemptRegistration {
    /// The flag the attempt and its probe both poll.
    pub(crate) fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }

    /// The id this registration holds.
    pub(crate) fn attempt_id(&self) -> &str {
        &self.attempt_id
    }

    /// Whether this attempt has been withdrawn.
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Acquire)
    }
}

impl Drop for AttemptRegistration {
    fn drop(&mut self) {
        entries(&self.registry.inner).remove(&self.attempt_id);
    }
}

#[cfg(test)]
#[path = "cancel_registry_tests.rs"]
mod tests;
