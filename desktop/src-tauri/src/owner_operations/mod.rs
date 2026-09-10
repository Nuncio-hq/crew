//! Owner-local recovery metadata. Signed relay events remain domain authority.
//!
//! The native command adapter supplies a captured, validated owner/community
//! scope. This core never derives authority from a renderer-provided owner.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Scope captured by the native identity/workspace adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationScope {
    /// Native owner public key as lowercase hex.
    pub owner: String,
    /// Canonical HTTP community origin.
    pub community: String,
}

/// Recovery consumers; this is not a registry of domain entities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationKind {
    /// Repository Wiki publication and reconciliation.
    WikiPublication,
    /// Project or repository change and reconciliation.
    ProjectChange,
    /// Thread handoff recovery.
    ThreadHandoff,
    /// Channel Crew role configuration with fixed native admission policy.
    ChannelCrewConfig,
    /// App-global keyed instance offboarding with durable relay cleanup.
    ManagedAgentDelete,
}

/// Observable operation phase. Reconciliation is independent of phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationStatus {
    /// The consumer is preparing its operation.
    Preparing,
    /// The consumer has pending work.
    Pending,
    /// The consumer is checking external side effects.
    Reconciling,
    /// The last attempt failed; unresolved effects may remain.
    Failed,
    /// The consumer recorded completion.
    Complete,
    /// The consumer recorded cancellation; effects may remain unresolved.
    Canceled,
    /// A newer operation or head superseded this attempt.
    Superseded,
}

/// One complete atomic recovery snapshot, including exact event envelopes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    /// Recovery envelope schema version.
    pub version: u32,
    /// Native owner and community boundary.
    pub scope: OperationScope,
    /// Canonical operation UUID.
    pub id: String,
    /// Consumer responsible for reconciliation.
    pub kind: OperationKind,
    /// Domain coordinate protected by the unresolved claim.
    pub resource_key: String,
    /// Local compare-and-swap revision.
    pub revision: u64,
    /// Native creation time in Unix seconds.
    pub created_at: i64,
    /// Native last-update time in Unix seconds.
    pub updated_at: i64,
    /// Recorded phase; independent of reconciliation.
    pub status: OperationStatus,
    /// True only after domain side effects have been reconciled.
    pub reconciled: bool,
    /// Whole domain-owned recovery snapshot, never secret keys.
    pub payload: Value,
}

/// Bounded recovery-list entry. Load the exact ID separately for its payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationSummary {
    /// Canonical operation UUID, also the stable pagination cursor.
    pub id: String,
    /// Consumer that owns reconciliation.
    pub kind: OperationKind,
    /// Coordinate whose unresolved claim is retained.
    pub resource_key: String,
    /// Expected revision for the next local CAS.
    pub revision: u64,
    /// Last recorded phase; does not imply side-effect reconciliation.
    pub status: OperationStatus,
    /// Whether the consumer has reconciled all side effects.
    pub reconciled: bool,
    /// Native update time in Unix seconds.
    pub updated_at: i64,
}

/// Initial unresolved operation; identity and timestamps are supplied natively.
#[derive(Deserialize)]
pub struct NewOperation {
    /// Canonical operation UUID.
    pub id: String,
    /// Consumer responsible for reconciliation.
    pub kind: OperationKind,
    /// Domain coordinate protected by the unresolved claim.
    pub resource_key: String,
    /// Whole domain-owned recovery snapshot, never secret keys.
    pub payload: Value,
}

/// Domain-validated replacement of a recovery snapshot.
#[derive(Deserialize)]
pub struct OperationUpdate {
    /// Recorded phase; independent of reconciliation.
    pub status: OperationStatus,
    /// True only after domain side effects have been reconciled.
    pub reconciled: bool,
    /// Whole domain-owned recovery snapshot, never secret keys.
    pub payload: Value,
}

/// Result of an idempotent create or an existing resource claim.
#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "result", content = "operation", rename_all = "kebab-case")]
pub enum CreateResult {
    /// A new operation was reserved.
    Created(Operation),
    /// The current durable operation for an existing intent or resource claim.
    Existing(Operation),
}

/// Admission limits. Unresolved operations are never evicted.
#[derive(Clone, Copy)]
pub struct Limits {
    /// Maximum unresolved records across an owner's communities.
    pub pending_per_owner: usize,
    /// Maximum serialized record bytes per owner.
    pub bytes_per_owner: usize,
    /// Maximum serialized bytes in one recovery record.
    pub bytes_per_operation: usize,
    /// Maximum retained reconciled records per owner.
    pub terminal_per_owner: usize,
    /// Maximum age of reconciled records before transactional trimming.
    pub terminal_age_secs: i64,
    /// Bounded SQLite lock wait, at most 250 milliseconds.
    pub busy_timeout_ms: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            pending_per_owner: 16,
            bytes_per_owner: 256 * 1024 * 1024,
            bytes_per_operation: 64 * 1024 * 1024,
            terminal_per_owner: 100,
            terminal_age_secs: 30 * 24 * 60 * 60,
            busy_timeout_ms: 250,
        }
    }
}

/// Redacted failures: payloads, keys, paths and raw SQLite errors never escape.
#[derive(Debug, PartialEq)]
pub enum StoreError {
    Unavailable,
    Busy,
    Invalid,
    Conflict,
    Missing,
    Unreconciled,
    Quota,
    Corrupt,
    Version,
    UnsafePath,
}

/// Transactional operation store; no transaction spans network or process IO.
pub struct OperationStore {
    connection: Connection,
    limits: Limits,
}

mod mutations;
mod storage;

#[cfg(all(test, unix))]
mod tests;

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "recovery storage is not available",
            Self::Busy => "recovery storage is busy; retry",
            Self::Invalid => "invalid recovery operation",
            Self::Conflict => "recovery operation changed; reload",
            Self::Missing => "recovery operation is missing",
            Self::Unreconciled => "recovery operation has unresolved side effects",
            Self::Quota => "recovery storage quota reached",
            Self::Corrupt => "recovery storage is corrupt",
            Self::Version => "recovery storage version is unsupported",
            Self::UnsafePath => "recovery storage path is unsafe",
        })
    }
}

impl std::error::Error for StoreError {}

#[cfg(all(test, unix))]
mod kind_policy_tests;
mod managed_delete_claim;
#[cfg(all(test, unix))]
mod managed_delete_claim_tests;

// Fixed native policy; never caller/renderer-configurable.
fn record_byte_limit(limits: Limits, kind: OperationKind) -> usize {
    match kind {
        OperationKind::ChannelCrewConfig => limits.bytes_per_operation.min(1024 * 1024),
        OperationKind::ManagedAgentDelete => limits.bytes_per_operation.min(64 * 1024),
        _ => limits.bytes_per_operation,
    }
}

#[cfg(all(test, not(unix)))]
#[test]
fn recovery_path_fails_closed_without_private_acl_support() {
    assert!(matches!(
        OperationStore::open(std::path::Path::new("recovery.db"), Limits::default()),
        Err(StoreError::UnsafePath)
    ));
}
