//! Bounded local diagnostics for one Desktop-owned harness generation.
//!
//! This is a process sidechannel, not a relay event or process authority.
mod lease;
pub use lease::TransportLease;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Maximum flat-directory entries, excluding the shared empty lock file.
pub const MAX_DIRECTORY_ENTRIES: usize = 192;
/// Latest record plus two atomic-write staging names reserved per generation.
pub const GENERATION_ENTRY_RESERVATION: usize = 3;

/// Actionable refusal shared by native preflight and the harness writer.
pub const STORAGE_REVIEW_ERROR: &str = "Local transport history is full or unsafe. Stop the agent, review the .transport folder beside its log, remove only diagnostics for confirmed stopped processes, then retry.";

/// Opaque form of the existing canonical `(pubkey, relay)` runtime key.
/// The relay digest keeps URL query secrets out of local diagnostics.
pub fn runtime_id(pubkey: &str, relay_url: &str) -> Result<String, String> {
    if pubkey.len() != 64 || !pubkey.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("managed transport pubkey must be 64 hexadecimal characters".into());
    }
    let relay = crate::relay::normalize_relay_url(relay_url)
        .map_err(|_| "managed transport requires a valid relay identity")?;
    Ok(format!(
        "{}__{}",
        pubkey.to_ascii_lowercase(),
        hex::encode(Sha256::digest(relay.as_bytes()))
    ))
}

/// Maximum serialized size of one latest status record.
pub const MAX_RECORD_BYTES: u64 = 8192;
/// Writer liveness renewal cadence, independent of meaningful transitions.
pub const RENEWAL_SECONDS: u64 = 5;
/// Native readers expire a non-advancing live record after this interval.
pub const LEASE_SECONDS: u64 = 15;

/// Transport health is independent of the managed process lifecycle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportState {
    /// No trustworthy local diagnostic is available.
    Unknown,
    /// Initial connection is in progress.
    Connecting,
    /// Authentication and connection recovery completed.
    Connected,
    /// An established connection is recovering within its burst budget.
    Degraded,
    /// The burst ended; background probes continue while the process lives.
    Exhausted,
    /// The relay explicitly denied the AUTH event for this attempt.
    AuthRejected,
}

/// Fixed diagnostic codes prevent relay text or credentials reaching disk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransportCode {
    /// No current transport error.
    None,
    /// A transport or handshake failed without credential evidence.
    ConnectionFailed,
    /// The bounded attempt or episode elapsed.
    Timeout,
    /// A correlated explicit AUTH denial requires configuration review.
    AuthDenied,
    /// The local writer or reader could not establish trustworthy status.
    StatusUnavailable,
}

impl TransportCode {
    /// Safe, fixed user-facing text; never derived from a server payload.
    pub fn message(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::ConnectionFailed => {
                Some("Relay connection unavailable. Check the relay and network.")
            }
            Self::Timeout => Some("Relay connection timed out. Check the relay and network."),
            Self::AuthDenied => {
                Some("Relay denied authentication. Review credentials and relay configuration.")
            }
            Self::StatusUnavailable => {
                Some("Local transport status unavailable. Retry by restarting the agent.")
            }
        }
    }
}

/// Latest transport substate projected into the existing runtime status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransportStatus {
    /// Typed connection state, separate from process and task state.
    pub state: TransportState,
    /// Fixed diagnostic code.
    pub code: TransportCode,
    /// Attempts consumed in the current health episode.
    pub attempts: u32,
    /// Monotonic elapsed health episode time, in milliseconds.
    pub elapsed_ms: u64,
    /// Wall-clock estimate of the next retry for display only.
    pub next_retry_at_ms: Option<u64>,
    /// Fixed safe error text, at most 1 KiB.
    pub last_error: Option<String>,
}

impl TransportStatus {
    /// Actionable fallback when local transport diagnostics cannot be trusted.
    pub fn unknown() -> Self {
        Self {
            state: TransportState::Unknown,
            code: TransportCode::StatusUnavailable,
            attempts: 0,
            elapsed_ms: 0,
            next_retry_at_ms: None,
            last_error: TransportCode::StatusUnavailable
                .message()
                .map(str::to_owned),
        }
    }

    /// Reject arbitrary server/error strings rather than truncating secrets.
    pub fn has_safe_error(&self) -> bool {
        self.last_error.as_deref() == self.code.message()
    }
}

/// Atomic latest record, accepted only for an already registered runtime key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransportRecord {
    /// Local protocol version; currently one.
    pub version: u32,
    /// Existing opaque runtime ID derived from canonical public key and relay.
    pub runtime_id: String,
    /// Unpredictable nonce assigned at managed process start.
    pub start_nonce: String,
    /// Strictly increasing sequence, including liveness renewals.
    pub sequence: u64,
    /// Wall-clock write timestamp for initial sanity checking only.
    pub timestamp_ms: u64,
    /// Startup finished unsuccessfully; this record may be retained on exit.
    pub terminal: bool,
    /// Transport diagnosis for this generation.
    pub transport: TransportStatus,
}
