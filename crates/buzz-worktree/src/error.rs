//! Typed errors for lease and lifecycle-record operations.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Errors from worktree lifecycle helpers.
#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error(transparent)]
    Lease(#[from] LeaseError),
    #[error(transparent)]
    Record(#[from] RecordError),
}

/// Cross-process lease failures.
#[derive(Debug, Error)]
pub enum LeaseError {
    /// Another holder already owns a conflicting lease.
    #[error("worktree lease is busy")]
    Busy,
    /// Root event id failed validation.
    #[error("invalid worktree root identity: {0}")]
    InvalidIdentity(String),
    /// Lease metadata version is unsupported.
    #[error("unsupported worktree lease version {found} (supported {supported})")]
    UnsupportedVersion { found: u32, supported: u32 },
    /// Filesystem I/O failure while creating or locking.
    #[error("worktree lease I/O error at {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Lifecycle record failures.
#[derive(Debug, Error)]
pub enum RecordError {
    /// Root event id failed validation.
    #[error("invalid worktree root identity: {0}")]
    InvalidIdentity(String),
    /// Record schema version is unsupported.
    #[error("unsupported worktree lifecycle record version {found} (supported {supported})")]
    UnsupportedVersion { found: u32, supported: u32 },
    /// Existing record conflicts with the trusted adoption request.
    #[error("worktree lifecycle record conflict: {0}")]
    Conflict(String),
    /// Record bytes could not be parsed.
    #[error("malformed worktree lifecycle record at {}: {source}", path.display())]
    Malformed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    /// Filesystem I/O failure.
    #[error("worktree lifecycle record I/O error at {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}
