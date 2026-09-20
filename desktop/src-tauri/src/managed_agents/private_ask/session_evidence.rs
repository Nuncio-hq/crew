//! The only way to obtain session-isolation evidence: observe a real session.
//!
//! [`SessionIsolationEvidence`]'s fields are private to this module, so no
//! caller — production or test — can assemble a favourable digest, entry count
//! or PID by hand and hand it to [`super::capability::PrivateAskProbe`]. The
//! production path is [`SessionSnapshot::capture`], which reads the employee
//! session's own ACP session-ledger directory off disk and takes the PID from
//! the handle that owns the process, followed by
//! [`SessionIsolationEvidence::observe`].
//!
//! What is observed is the ledger DIRECTORY the harness writes for this exact
//! (relay, agent) pair, not a single file and not an observer sequence. The
//! harness owns those bytes end to end; the desktop only reads them. An
//! observer sequence would have had to be written by the desktop and read back
//! by the desktop, which is not an observation of anything.
//!
//! The negative tests still need to spoil exactly one field, so a
//! `#[cfg(test)]` constructor exists for them; it is compiled out of the
//! shipped binary, which is the point of the split.

use super::PrivateAskFailure;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Maximum bytes read from one ledger entry. A larger entry is not summarised
/// — it is refused, because a truncated digest would silently stop covering
/// the bytes a run could have changed.
const SESSION_ARTEFACT_LIMIT: u64 = 8 * 1024 * 1024;

/// Maximum bytes digested across the whole ledger directory.
const SESSION_LEDGER_TOTAL_LIMIT: u64 = 64 * 1024 * 1024;

/// Maximum ledger entries digested for one agent.
const SESSION_LEDGER_ENTRY_LIMIT: usize = 4096;

/// The file extension `buzz-acp` writes one ledger entry under.
const SESSION_LEDGER_EXTENSION: &str = "json";

/// Where one live employee session's own state can be read.
///
/// The managed-agent layer owns this knowledge — only it knows which session
/// belongs to the selected agent — so it travels as data rather than being
/// rediscovered by whoever needs an observation.
#[derive(Debug, Clone)]
pub(crate) struct SessionObservation {
    /// The harness's own session-ledger directory for this exact (relay,
    /// agent) pair. The desktop never writes inside it.
    pub(crate) ledger_dir: std::path::PathBuf,
    /// Taken from the handle that owns the child, never from a PID probe.
    pub(crate) acp_pid: Option<u32>,
}

impl SessionObservation {
    /// Read this session's observable state right now.
    pub(crate) fn capture(&self) -> Result<SessionSnapshot, PrivateAskFailure> {
        SessionSnapshot::capture(&self.ledger_dir, self.acp_pid)
    }
}

/// The base directory `buzz-acp` spools its session ledger under.
///
/// `None` when this machine has neither the override nor a home directory, in
/// which case there is nothing to observe and the caller refuses.
pub(crate) fn session_ledger_base() -> Option<std::path::PathBuf> {
    match std::env::var_os("BUZZ_ACP_SESSION_LEDGER_DIR") {
        Some(base) if !base.is_empty() => Some(std::path::PathBuf::from(base)),
        _ => {
            let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
            Some(
                std::path::PathBuf::from(home)
                    .join(".local/share/nunciocrew/buzz-acp/session-ledger"),
            )
        }
    }
}

/// Where `buzz-acp` keeps its session ledger for one (relay, agent) pair.
///
/// This mirrors the harness's own derivation. The desktop cannot depend on the
/// harness crate, so the mirror is pinned by a test: if either side moves, the
/// test fails rather than making every private Ask quietly look like an agent
/// that has never run.
///
/// `relay_url` must already be the normalized url the harness was launched
/// with; an unnormalized one hashes to a different directory.
pub(crate) fn session_ledger_dir_under(
    base: &Path,
    relay_url: &str,
    agent_pubkey: &str,
) -> std::path::PathBuf {
    let relay_hash = hex::encode(Sha256::digest(relay_url.as_bytes()));
    base.join(&relay_hash[..16])
        .join(agent_pubkey.to_ascii_lowercase())
}

/// The ledger directory for one (relay, agent) pair on this machine.
pub(crate) fn session_ledger_dir(
    relay_url: &str,
    agent_pubkey: &str,
) -> Option<std::path::PathBuf> {
    Some(session_ledger_dir_under(
        &session_ledger_base()?,
        relay_url,
        agent_pubkey,
    ))
}

/// One observation of a running employee session, taken from its own bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionSnapshot {
    ledger_digest: String,
    /// How many ledger entries the digest covers. Kept beside the digest so a
    /// directory that lost an entry and gained another cannot compare equal
    /// through the digest alone.
    entry_count: usize,
    /// `None` when the owning handle reports no live process. It is kept
    /// distinct from `Some(0)`: an absent session is not a session at PID zero.
    acp_pid: Option<u32>,
}

impl SessionSnapshot {
    /// Read one session's observable state from its ledger directory.
    ///
    /// `acp_pid` comes from the handle that owns the child, never from a PID
    /// probe: a recycled PID can answer a liveness syscall for a process that
    /// already exited, which would turn a restarted session into a pass.
    ///
    /// A missing, empty, unreadable or oversized directory is an error, not an
    /// empty digest — a session whose state cannot be observed has not been
    /// shown to be unchanged, and a directory with no entries at all is the
    /// absence of an observation rather than an observation of an idle
    /// session.
    pub(crate) fn capture(
        ledger_dir: &Path,
        acp_pid: Option<u32>,
    ) -> Result<Self, PrivateAskFailure> {
        let entries = ledger_entries(ledger_dir)?;
        if entries.is_empty() {
            return Err(PrivateAskFailure::SessionObservationUnavailable);
        }
        let mut hasher = Sha256::new();
        let mut total = 0_u64;
        for name in &entries {
            let path = ledger_dir.join(name);
            let bytes = read_bounded(&path)?;
            total = total
                .checked_add(bytes.len() as u64)
                .ok_or(PrivateAskFailure::SessionObservationUnavailable)?;
            if total > SESSION_LEDGER_TOTAL_LIMIT {
                return Err(PrivateAskFailure::SessionObservationUnavailable);
            }
            // Name, length and bytes all enter the digest, so two entries whose
            // contents were swapped do not collide.
            hasher.update(name.as_encoded_bytes());
            hasher.update([0]);
            hasher.update(bytes.len().to_le_bytes());
            hasher.update([0]);
            hasher.update(&bytes);
        }
        Ok(Self {
            ledger_digest: hex::encode(hasher.finalize()),
            entry_count: entries.len(),
            acp_pid: acp_pid.filter(|pid| *pid != 0),
        })
    }
}

/// The ledger entries this directory holds, sorted by name.
///
/// Regular `*.json` files only: `buzz-acp` writes an entry as a temporary file
/// and renames it, so a `*.tmp` is a write in progress and never an entry, and
/// a subdirectory or symlink is not one either. Sorting makes two readings of
/// the same bytes digest identically regardless of readdir order.
fn ledger_entries(ledger_dir: &Path) -> Result<Vec<std::ffi::OsString>, PrivateAskFailure> {
    let metadata = std::fs::metadata(ledger_dir)
        .map_err(|_| PrivateAskFailure::SessionObservationUnavailable)?;
    if !metadata.is_dir() {
        return Err(PrivateAskFailure::SessionObservationUnavailable);
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(ledger_dir)
        .map_err(|_| PrivateAskFailure::SessionObservationUnavailable)?
    {
        let entry = entry.map_err(|_| PrivateAskFailure::SessionObservationUnavailable)?;
        let file_type = entry
            .file_type()
            .map_err(|_| PrivateAskFailure::SessionObservationUnavailable)?;
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .extension()
            .is_none_or(|extension| extension != SESSION_LEDGER_EXTENSION)
        {
            continue;
        }
        if names.len() >= SESSION_LEDGER_ENTRY_LIMIT {
            return Err(PrivateAskFailure::SessionObservationUnavailable);
        }
        names.push(entry.file_name());
    }
    names.sort();
    Ok(names)
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
            && self.before.entry_count == self.after.entry_count
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
        entry_count_before: usize,
        entry_count_after: usize,
        acp_pid_before: Option<u32>,
        acp_pid_after: Option<u32>,
        child_parent_pid: u32,
        desktop_pid: u32,
    ) -> Self {
        Self {
            before: SessionSnapshot {
                ledger_digest: ledger_digest_before,
                entry_count: entry_count_before,
                acp_pid: acp_pid_before,
            },
            after: SessionSnapshot {
                ledger_digest: ledger_digest_after,
                entry_count: entry_count_after,
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
