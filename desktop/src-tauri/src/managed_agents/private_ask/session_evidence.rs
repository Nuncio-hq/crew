//! The only way to obtain session-isolation evidence: observe a real session.
//!
//! [`SessionIsolationEvidence`]'s fields are private to this module, so no
//! caller — production or test — can assemble a favourable digest, sequence or
//! PID by hand and hand it to [`super::capability::PrivateAskProbe`]. The
//! production path is [`SessionSnapshot::capture`], which reads the employee
//! session's own ledger bytes and observer sequence off disk and takes the PID
//! from the handle that owns the process, followed by
//! [`SessionIsolationEvidence::observe`].
//!
//! The negative tests still need to spoil exactly one field, so a
//! `#[cfg(test)]` constructor exists for them; it is compiled out of the
//! shipped binary, which is the point of the split.

use super::PrivateAskFailure;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Maximum bytes read from an observed session artefact. A ledger larger than
/// this is not summarised — it is refused, because a truncated digest would
/// silently stop covering the bytes a run could have changed.
const SESSION_ARTEFACT_LIMIT: u64 = 8 * 1024 * 1024;

/// One observation of a running employee session, taken from its own bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionSnapshot {
    ledger_digest: String,
    observer_sequence: u64,
    /// `None` when the owning handle reports no live process. It is kept
    /// distinct from `Some(0)`: an absent session is not a session at PID zero.
    acp_pid: Option<u32>,
}

impl SessionSnapshot {
    /// Read one session's observable state.
    ///
    /// `acp_pid` comes from the handle that owns the child, never from a PID
    /// probe: a recycled PID can answer a liveness syscall for a process that
    /// already exited, which would turn a restarted session into a pass.
    ///
    /// An unreadable or oversized artefact is an error, not an empty digest —
    /// a session whose state cannot be observed has not been shown to be
    /// unchanged.
    pub(crate) fn capture(
        ledger_path: &Path,
        observer_sequence_path: &Path,
        acp_pid: Option<u32>,
    ) -> Result<Self, PrivateAskFailure> {
        let ledger = read_bounded(ledger_path)?;
        let sequence = read_bounded(observer_sequence_path)?;
        let observer_sequence = std::str::from_utf8(&sequence)
            .ok()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or(PrivateAskFailure::SessionObservationUnavailable)?;
        Ok(Self {
            ledger_digest: hex::encode(Sha256::digest(&ledger)),
            observer_sequence,
            acp_pid: acp_pid.filter(|pid| *pid != 0),
        })
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, PrivateAskFailure> {
    let metadata =
        std::fs::metadata(path).map_err(|_| PrivateAskFailure::SessionObservationUnavailable)?;
    if !metadata.is_file() || metadata.len() > SESSION_ARTEFACT_LIMIT {
        return Err(PrivateAskFailure::SessionObservationUnavailable);
    }
    std::fs::read(path).map_err(|_| PrivateAskFailure::SessionObservationUnavailable)
}

/// Before/after observation of the employee's own running session, captured
/// around a private Ask that ran while that session was busy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionIsolationEvidence {
    before: SessionSnapshot,
    after: SessionSnapshot,
    /// Parent of the private Ask child. It must be the desktop process: a child
    /// reparented onto the employee's harness would not be an independent
    /// invocation.
    child_parent_pid: u32,
    desktop_pid: u32,
}

impl SessionIsolationEvidence {
    /// Pair two real observations with the observed process lineage.
    ///
    /// This does not decide whether the session was isolated — that is
    /// [`Self::is_verified`]. It only refuses input that cannot describe a
    /// lineage at all, so a missing PID becomes a refusal rather than a
    /// silently unverified dimension.
    pub(crate) fn observe(
        before: SessionSnapshot,
        after: SessionSnapshot,
        child_parent_pid: u32,
        desktop_pid: u32,
    ) -> Result<Self, PrivateAskFailure> {
        if child_parent_pid == 0 || desktop_pid == 0 {
            return Err(PrivateAskFailure::SessionObservationUnavailable);
        }
        Ok(Self {
            before,
            after,
            child_parent_pid,
            desktop_pid,
        })
    }

    /// The session is isolated only when every observed dimension is unchanged
    /// and the child belongs to the desktop.
    pub(crate) fn is_verified(&self) -> bool {
        self.before.ledger_digest == self.after.ledger_digest
            && self.before.observer_sequence == self.after.observer_sequence
            && self.before.acp_pid.is_some()
            && self.before.acp_pid == self.after.acp_pid
            && self.child_parent_pid == self.desktop_pid
    }

    /// Assemble evidence field-by-field for the negative tests, which exist to
    /// spoil exactly one dimension. Compiled out of the shipped binary.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        ledger_digest_before: String,
        ledger_digest_after: String,
        observer_sequence_before: u64,
        observer_sequence_after: u64,
        acp_pid_before: Option<u32>,
        acp_pid_after: Option<u32>,
        child_parent_pid: u32,
        desktop_pid: u32,
    ) -> Self {
        Self {
            before: SessionSnapshot {
                ledger_digest: ledger_digest_before,
                observer_sequence: observer_sequence_before,
                acp_pid: acp_pid_before,
            },
            after: SessionSnapshot {
                ledger_digest: ledger_digest_after,
                observer_sequence: observer_sequence_after,
                acp_pid: acp_pid_after,
            },
            child_parent_pid,
            desktop_pid,
        }
    }
}

#[cfg(test)]
#[path = "session_evidence_tests.rs"]
mod tests;
