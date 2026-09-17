//! Relay egress guard for NIP-49 key-backup material.
//!
//! The local `ncryptsec` backup (see [`crate::key_backup`]) must NEVER be
//! transmitted to a relay. This module enforces that contract at runtime,
//! fail-closed, at every relay-bound egress boundary:
//!
//! | # | Boundary | Site |
//! |---|----------|------|
//! | 1 | `submit_signed_event_at_with_keys` (funnel for `submit_event*`) | `relay/submit.rs` |
//! | 2 | `sync_managed_agent_profile` | `relay.rs` |
//! | 3 | pre-signed path into the boundary-1 funnel | `relay/submit.rs` |
//! | 4 | `submit_signed_event_with_keys` | `relay.rs` |
//! | 5 | huddle STT publisher | `huddle/pipeline.rs` |
//! | 6 | `submit_engram_event` (team snapshot) | `commands/team_snapshot.rs` |
//! | 7 | `submit_engram_event` (persona import) | `commands/personas/snapshot/import.rs` |
//! | 8 | native websocket send loop (all webview relay WS) | `native_websocket.rs` |
//! | 9 | captured owner-operation publish/query | `commands/owner_operation_transport.rs` |
//!
//! The inventory-completeness test in `egress_guard_tests.rs` asserts that
//! every `/events` URL-construction site in the tree calls this guard, so a
//! new submission path fails the build until it is wired.
//!
//! Scope: `ncryptsec1` only. The raw `nsec` intentionally transits the
//! NIP-44-encrypted pairing session (NIP-AB payload_type "nsec"); guarding it
//! here would break pairing. Raw-key DLP is separate policy work.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

/// Count of relay-bound egress attempts that have reached a guarded boundary.
///
/// Every relay-bound egress site in the tree calls one of the guard functions
/// below — the `EVENTS_INVENTORY` scan in `egress_guard_tests.rs` fails the
/// build if a new one does not. That makes this counter a complete, always-on
/// census of relay traffic originating from the desktop's own identity, which
/// is what lets a feature assert it performed *no* relay egress at all.
///
/// It counts attempts, not accepted publications, and it counts every frame
/// kind a boundary carries (EVENT, REQ, CLOSE), so a zero reading is the
/// strong claim and a non-zero reading is not by itself a fault.
static RELAY_EGRESS_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// The same census, attributed to the thread that performed the egress.
    ///
    /// The process-global counter cannot support a "this operation published
    /// nothing" assertion, because any concurrent test performing a real
    /// egress inflates it. A synchronous operation's own egress — including a
    /// future polled with `block_on` — lands on its calling thread, so a
    /// thread-scoped reading attributes precisely.
    static EGRESS_ATTEMPTS_ON_THREAD: Cell<u64> = const { Cell::new(0) };
}

/// Relay-bound egress attempts observed since process start.
///
/// Callers compare two readings around an operation; the absolute value is
/// meaningless because the counter is process-global.
///
/// The counting side is ordinary production code on every relay boundary; only
/// this reader is test-scoped, because the sole consumer today is the private
/// Ask zero-publish proof. Widening it to a diagnostic is a deliberate act.
#[cfg(test)]
pub fn relay_egress_attempts() -> u64 {
    RELAY_EGRESS_ATTEMPTS.load(Ordering::SeqCst)
}

/// Relay-bound egress attempts performed by the calling thread.
///
/// Limit: a detached `spawn` escapes this attribution. No `spawn` exists under
/// `managed_agents/private_ask/`; introducing one there is a privacy-boundary
/// change and must come with its own proof.
#[cfg(test)]
pub fn relay_egress_attempts_on_this_thread() -> u64 {
    EGRESS_ATTEMPTS_ON_THREAD.with(Cell::get)
}

/// Bech32 HRP of NIP-49 encrypted secret keys.
const NCRYPTSEC_PREFIX: &str = "ncryptsec1";
/// Bech32 also permits an ALL-UPPERCASE encoding of the same payload
/// (BIP-173); an uppercased valid backup decodes identically, so the guard
/// must reject it too. Mixed case is invalid bech32 and cannot decode — a
/// substring matching either all-lower or all-upper prefix covers every
/// decodable form.
const NCRYPTSEC_PREFIX_UPPER: &str = "NCRYPTSEC1";

/// Reject `text` if it contains NIP-49 key-backup material.
///
/// Returns `Err` when an `ncryptsec1…` (or uppercase `NCRYPTSEC1…`)
/// substring is present. Callers MUST abort the network operation on `Err` —
/// this is a fail-closed guard, not a warning.
pub fn assert_no_key_backup(text: &str, context: &'static str) -> Result<(), String> {
    // Counted before the check so a rejected payload still records that this
    // identity tried to reach the relay.
    RELAY_EGRESS_ATTEMPTS.fetch_add(1, Ordering::SeqCst);
    EGRESS_ATTEMPTS_ON_THREAD.with(|count| count.set(count.get().saturating_add(1)));
    if text.contains(NCRYPTSEC_PREFIX) || text.contains(NCRYPTSEC_PREFIX_UPPER) {
        return Err(format!(
            "blocked {context}: payload contains NIP-49 key-backup material \
             (ncryptsec); the local key backup must never be transmitted to a relay"
        ));
    }
    Ok(())
}

/// Byte-slice variant for callers that hold serialized bodies.
pub fn assert_no_key_backup_bytes(body: &[u8], context: &'static str) -> Result<(), String> {
    // ncryptsec is ASCII bech32; a UTF-8-lossy view preserves any occurrence.
    assert_no_key_backup(&String::from_utf8_lossy(body), context)
}

#[cfg(test)]
#[path = "egress_guard_tests.rs"]
mod tests;
