//! Exact selected-invocation steering for the Crew Activity control.
//!
//! The ordinary goose steering queue is intentionally left separate.  This
//! module owns the stronger contract: a request is admitted only while the
//! invocation identified by `expectedTurnId` is running, and it can be
//! appended only by that invocation's `RunCtx` at a round boundary.

use std::collections::{HashMap, VecDeque};
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::types::ContentBlock;

pub const MAX_QUEUE: usize = 8;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_DEDUP_IDS: usize = 128;
pub const REQUEST_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10);

const CLAIM_PENDING: u8 = 0;
const CLAIM_APPENDED: u8 = 1;
const CLAIM_STALE: u8 = 2;
const CLAIM_REJECTED: u8 = 3;
const CLAIM_BUSY: u8 = 4;
const CLAIM_EXPIRED: u8 = 5;

/// Terminal result of a strict selected-run steer request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Appended,
    StaleTarget,
    Rejected,
    Busy,
    Expired,
}

impl Outcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Appended => "appended",
            Self::StaleTarget => "stale_target",
            Self::Rejected => "rejected",
            Self::Busy => "busy",
            Self::Expired => "expired",
        }
    }
}

/// A request accepted into the invocation's bounded round-boundary queue.
pub(crate) struct Request {
    pub(crate) request_id: String,
    pub(crate) turn_id: String,
    pub(crate) text: String,
    pub(crate) deadline: Instant,
    pub(crate) claim: Claim,
    pub(crate) completion: oneshot::Sender<Outcome>,
}

/// Shared terminal claim for one admitted request.
///
/// The ACP waiter and the invocation round-boundary drain race with one
/// another: the waiter may reach its deadline while the provider is still in
/// a round. A single atomic claim makes that race deterministic. Once the
/// waiter wins with `Expired`, the invocation cannot append the request later;
/// once the round wins with `Appended`, the waiter reports that outcome even if
/// its timeout branch is also ready.
#[derive(Clone, Debug)]
pub(crate) struct Claim(Arc<AtomicU8>);

impl Claim {
    fn new() -> Self {
        Self(Arc::new(AtomicU8::new(CLAIM_PENDING)))
    }

    pub(crate) fn settle(&self, requested: Outcome) -> Outcome {
        let desired = encode_outcome(requested);
        match self
            .0
            .compare_exchange(CLAIM_PENDING, desired, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => requested,
            Err(current) => decode_outcome(current).unwrap_or(requested),
        }
    }
}

fn encode_outcome(outcome: Outcome) -> u8 {
    match outcome {
        Outcome::Appended => CLAIM_APPENDED,
        Outcome::StaleTarget => CLAIM_STALE,
        Outcome::Rejected => CLAIM_REJECTED,
        Outcome::Busy => CLAIM_BUSY,
        Outcome::Expired => CLAIM_EXPIRED,
    }
}

fn decode_outcome(value: u8) -> Option<Outcome> {
    match value {
        CLAIM_APPENDED => Some(Outcome::Appended),
        CLAIM_STALE => Some(Outcome::StaleTarget),
        CLAIM_REJECTED => Some(Outcome::Rejected),
        CLAIM_BUSY => Some(Outcome::Busy),
        CLAIM_EXPIRED => Some(Outcome::Expired),
        _ => None,
    }
}

#[derive(Debug)]
struct Entry {
    turn_id: String,
    text: String,
    claim: Claim,
    outcome: Option<Outcome>,
}

/// Result of checking and reserving one strict request ID.
pub(crate) enum Admission {
    Accepted {
        sender: mpsc::Sender<Request>,
        claim: Claim,
    },
    PendingDuplicate,
    TerminalDuplicate(Outcome),
    ConflictingRequest,
    StaleTarget,
    CapacityFull,
}

/// Per-session strict invocation state.
///
/// This state is always accessed while the owning `App::sessions` mutex is
/// held.  The channel is bounded and `try_send` is used by the caller, so no
/// asynchronous operation is performed while that mutex is held.
pub(crate) struct State {
    invocation_id: Option<String>,
    accepting: bool,
    sender: Option<mpsc::Sender<Request>>,
    entries: HashMap<String, Entry>,
    terminal_order: VecDeque<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            invocation_id: None,
            accepting: false,
            sender: None,
            entries: HashMap::new(),
            terminal_order: VecDeque::new(),
        }
    }
}

impl State {
    /// Begin a fresh invocation and return its bounded queue receiver.
    pub(crate) fn start(&mut self, invocation_id: Option<String>) -> mpsc::Receiver<Request> {
        let (sender, receiver) = mpsc::channel(MAX_QUEUE);
        self.invocation_id = invocation_id.clone();
        self.accepting = invocation_id.is_some();
        self.sender = invocation_id.map(|_| sender);
        self.entries.clear();
        self.terminal_order.clear();
        receiver
    }

    /// Stop admission for the invocation while retaining terminal dedup
    /// entries until the next invocation starts.
    pub(crate) fn close(&mut self) {
        self.accepting = false;
        self.sender = None;
    }

    /// Reserve a request ID after checking the exact invocation and dedup
    /// ledger.  The caller must enqueue the returned sender immediately with
    /// `try_send`, or call [`Self::rollback`] if that send reports `Full` or
    /// `Closed`.
    pub(crate) fn reserve(
        &mut self,
        expected_turn_id: &str,
        request_id: &str,
        text: &str,
    ) -> Admission {
        if self.invocation_id.as_deref() != Some(expected_turn_id) || !self.accepting {
            return Admission::StaleTarget;
        }
        if let Some(entry) = self.entries.get(request_id) {
            if entry.turn_id != expected_turn_id || entry.text != text {
                return Admission::ConflictingRequest;
            }
            return match entry.outcome {
                Some(outcome) => Admission::TerminalDuplicate(outcome),
                None => Admission::PendingDuplicate,
            };
        }
        self.evict_terminal_entries();
        if self.entries.len() >= MAX_DEDUP_IDS {
            return Admission::CapacityFull;
        }
        let Some(sender) = self.sender.as_ref().cloned() else {
            return Admission::StaleTarget;
        };
        let claim = Claim::new();
        self.entries.insert(
            request_id.to_owned(),
            Entry {
                turn_id: expected_turn_id.to_owned(),
                text: text.to_owned(),
                claim: claim.clone(),
                outcome: None,
            },
        );
        Admission::Accepted { sender, claim }
    }

    /// Remove an entry whose request could not be put on the bounded queue.
    pub(crate) fn rollback(&mut self, request_id: &str, expected_turn_id: &str, text: &str) {
        let remove = self.entries.get(request_id).is_some_and(|entry| {
            entry.outcome.is_none() && entry.turn_id == expected_turn_id && entry.text == text
        });
        if remove {
            self.entries.remove(request_id);
        }
    }

    /// Record the first terminal result for a request.  Late completion from a
    /// timed-out or already-settled request is deliberately ignored.
    pub(crate) fn finish(&mut self, request_id: &str, outcome: Outcome) {
        let Some(entry) = self.entries.get_mut(request_id) else {
            return;
        };
        if entry.outcome.is_none() {
            entry.outcome = Some(entry.claim.settle(outcome));
            self.terminal_order.push_back(request_id.to_owned());
        }
    }

    /// Finish only while the same invocation is still current. A late waiter
    /// from an older turn must never mark a request in a replacement ledger.
    pub(crate) fn finish_for(
        &mut self,
        expected_turn_id: &str,
        request_id: &str,
        outcome: Outcome,
    ) -> Outcome {
        if self.invocation_id.as_deref() == Some(expected_turn_id) {
            self.finish(request_id, outcome);
            self.entries
                .get(request_id)
                .and_then(|entry| entry.outcome)
                .unwrap_or(outcome)
        } else {
            outcome
        }
    }

    fn evict_terminal_entries(&mut self) {
        while self.entries.len() >= MAX_DEDUP_IDS {
            let Some(request_id) = self.terminal_order.pop_front() else {
                break;
            };
            if self
                .entries
                .get(&request_id)
                .is_some_and(|entry| entry.outcome.is_some())
            {
                self.entries.remove(&request_id);
            }
        }
    }
}

/// Validate and canonicalize a strict text-only prompt.
pub(crate) fn prompt_text(prompt: &[ContentBlock]) -> Result<String, &'static str> {
    if prompt.is_empty() {
        return Err("steer: prompt must not be empty");
    }
    let mut parts = Vec::with_capacity(prompt.len());
    for block in prompt {
        match block {
            ContentBlock::Text { text } => parts.push(text.as_str()),
            ContentBlock::ResourceLink { .. } | ContentBlock::Unsupported => {
                return Err("steer: strict prompt must contain text blocks only");
            }
        }
    }
    let text = parts.join("\n");
    if text.trim().is_empty() {
        return Err("steer: prompt must not be empty");
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err("steer: prompt exceeds 16 KiB");
    }
    Ok(text)
}

/// Validate the UUID-shaped identity fields used by the strict contract.
pub(crate) fn is_uuid(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    value.bytes().enumerate().all(|(index, byte)| {
        matches!(index, 8 | 13 | 18 | 23) && byte == b'-'
            || !matches!(index, 8 | 13 | 18 | 23) && byte.is_ascii_hexdigit()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TURN: &str = "11111111-1111-4111-8111-111111111111";
    const REQUEST: &str = "22222222-2222-4222-8222-222222222222";

    #[test]
    fn uuid_and_prompt_validation_are_strict() {
        assert!(is_uuid(TURN));
        assert!(!is_uuid("turn"));
        assert_eq!(
            prompt_text(&[ContentBlock::Text { text: "ok".into() }]).unwrap(),
            "ok"
        );
        assert!(prompt_text(&[ContentBlock::ResourceLink { uri: "file".into() }]).is_err());
    }

    #[test]
    fn pending_and_terminal_duplicates_are_distinguished() {
        let mut state = State::default();
        let _receiver = state.start(Some(TURN.into()));
        assert!(matches!(
            state.reserve(TURN, REQUEST, "same"),
            Admission::Accepted { .. }
        ));
        assert!(matches!(
            state.reserve(TURN, REQUEST, "same"),
            Admission::PendingDuplicate
        ));
        state.finish(REQUEST, Outcome::Appended);
        assert!(matches!(
            state.reserve(TURN, REQUEST, "same"),
            Admission::TerminalDuplicate(Outcome::Appended)
        ));
        assert!(matches!(
            state.reserve(TURN, REQUEST, "different"),
            Admission::ConflictingRequest
        ));
    }

    #[test]
    fn terminal_claim_is_first_wins() {
        let claim = Claim::new();
        assert_eq!(claim.settle(Outcome::Expired), Outcome::Expired);
        assert_eq!(claim.settle(Outcome::Appended), Outcome::Expired);
    }
}
