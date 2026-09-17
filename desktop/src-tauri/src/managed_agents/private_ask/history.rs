//! The viewer's own record of what they asked and what came back.
//!
//! A private Ask publishes nothing, so there is no remote copy of it anywhere:
//! if this file does not hold the attempt, nobody does. That is the whole
//! reason it exists — and also why it is bounded. An owner-local log nobody
//! prunes grows until the disk notices, and it holds the questions a viewer
//! asked in private, so "keep everything forever" is the wrong default in both
//! directions.
//!
//! The retention shape is the owned-run one: a bounded newest-first window,
//! capped by **count and by age**, in a 0o700 directory this uid owns, written
//! 0o600 through a temporary file and a rename so a torn write is never
//! readable as a history.
//!
//! Refusals are recorded alongside answers. A history that kept only the
//! answers would silently omit exactly the attempts a developer needs to see.

use super::{GroundedSource, PrivateAskFailure, PrivateAskResponse};
use crate::managed_agents::recap_ownership::VerifiedStagingOwnership;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Newest attempts retained. Beyond this the oldest are dropped.
pub(super) const HISTORY_LIMIT: usize = 50;

/// How long one attempt is retained, in seconds. Thirty days: long enough to
/// look back over a working period, short enough that a machine does not
/// accumulate a viewer's private questions indefinitely.
pub(super) const HISTORY_MAX_AGE: u64 = 30 * 24 * 60 * 60;

/// Bytes accepted from the history file. A file larger than this is not one
/// this writer produced, and is treated as absent rather than parsed.
const HISTORY_LIMIT_BYTES: u64 = 4 * 1024 * 1024;

const HISTORY_SCHEMA: &str = "crew-private-ask-history";
const HISTORY_VERSION: u8 = 1;
const HISTORY_FILE: &str = "attempts.json";

/// One finished attempt, answered or refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct PrivateAskHistoryEntry {
    pub(crate) attempt_id: String,
    pub(crate) question: String,
    /// Present when the attempt produced an answer.
    pub(crate) markdown: Option<String>,
    /// Present when the attempt was refused. Mutually exclusive with `markdown`
    /// by construction — the two constructors below are the only way in.
    pub(crate) refusal: Option<String>,
    pub(crate) citations: Vec<HistoryCitation>,
    pub(crate) asked_at: u64,
}

/// A citation as the history keeps it: where it pointed, not what it said.
///
/// The source text is deliberately not retained. It is already on disk in the
/// repository, and copying it here would turn a short log into a second copy of
/// the codebase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct HistoryCitation {
    pub(crate) path: String,
    pub(crate) start_line: u64,
    pub(crate) end_line: u64,
}

impl From<&GroundedSource> for HistoryCitation {
    fn from(source: &GroundedSource) -> Self {
        Self {
            path: source.path.clone(),
            start_line: source.start_line,
            end_line: source.end_line,
        }
    }
}

impl PrivateAskHistoryEntry {
    pub(crate) fn answered(question: &str, response: &PrivateAskResponse, asked_at: u64) -> Self {
        Self {
            attempt_id: response.attempt_id.clone(),
            question: question.to_owned(),
            markdown: Some(response.markdown.clone()),
            refusal: None,
            citations: response
                .citations
                .iter()
                .map(HistoryCitation::from)
                .collect(),
            asked_at,
        }
    }

    pub(crate) fn refused(
        question: &str,
        attempt_id: &str,
        failure: &PrivateAskFailure,
        asked_at: u64,
    ) -> Self {
        Self {
            attempt_id: attempt_id.to_owned(),
            question: question.to_owned(),
            markdown: None,
            // The viewer-safe vocabulary, the same string the screen shows.
            refusal: Some(failure.to_string()),
            citations: Vec::new(),
            asked_at,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryDocument {
    schema: String,
    version: u8,
    /// Newest first.
    entries: Vec<PrivateAskHistoryEntry>,
}

/// Serializes the read-modify-write in [`record`].
///
/// The command is `async` and a renderer can invoke it twice, so two attempts
/// finishing together would otherwise both read the same list and the second
/// write would drop the first's entry. The critical section is one small file
/// read plus one rename, so a plain mutex is the right size; a poisoned lock is
/// recovered rather than propagated, because a panicking writer left the file
/// itself untouched — the write is temp-file-plus-rename.
static RECORD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn history_path(ownership: &VerifiedStagingOwnership) -> Result<PathBuf, PrivateAskFailure> {
    Ok(ownership
        .private_ask_history_base()
        .map_err(PrivateAskFailure::State)?
        .join(HISTORY_FILE))
}

/// Read the retained history, newest first.
///
/// Every "there is nothing usable here" case returns an empty history rather
/// than an error: no file, an unreadable or oversized file, a symlink in place
/// of the file, unparseable bytes, or a document from another schema. A private
/// Ask must not be blocked by a log, and a partial parse must never be
/// presented as the complete record.
///
/// Entries past [`HISTORY_MAX_AGE`] are dropped on read as well as on write, so
/// a machine that has not asked anything in months does not surface a stale
/// window the next time it is opened.
pub(crate) fn load(ownership: &VerifiedStagingOwnership, now: u64) -> Vec<PrivateAskHistoryEntry> {
    let Ok(path) = history_path(ownership) else {
        return Vec::new();
    };
    let Ok(metadata) = std::fs::symlink_metadata(&path) else {
        return Vec::new();
    };
    if !metadata.is_file() || metadata.len() > HISTORY_LIMIT_BYTES {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    let Ok(document) = serde_json::from_slice::<HistoryDocument>(&bytes) else {
        return Vec::new();
    };
    if document.schema != HISTORY_SCHEMA || document.version != HISTORY_VERSION {
        return Vec::new();
    }
    retained(document.entries, now)
}

/// Record one finished attempt and return the pruned history.
///
/// The caller is told whether this succeeded rather than having a failure
/// swallowed: an answer that was produced but not recorded is still an answer,
/// and the screen says the attempt was not kept instead of quietly showing a
/// history that is missing it.
pub(crate) fn record(
    ownership: &VerifiedStagingOwnership,
    entry: PrivateAskHistoryEntry,
    now: u64,
) -> Result<Vec<PrivateAskHistoryEntry>, PrivateAskFailure> {
    let path = history_path(ownership)?;
    let _guard = RECORD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut entries = load(ownership, now);
    // An attempt id is unique per run, but a retry that reused one must replace
    // rather than duplicate: two rows with one id is a history that cannot be
    // read back.
    entries.retain(|existing| existing.attempt_id != entry.attempt_id);
    entries.insert(0, entry);
    let entries = retained(entries, now);
    let document = HistoryDocument {
        schema: HISTORY_SCHEMA.to_owned(),
        version: HISTORY_VERSION,
        entries: entries.clone(),
    };
    let bytes = serde_json::to_vec(&document).map_err(|_| PrivateAskFailure::InvalidState)?;
    write_private(&path, &bytes)?;
    Ok(entries)
}

/// Apply both bounds: newest-first order, count, then age.
fn retained(mut entries: Vec<PrivateAskHistoryEntry>, now: u64) -> Vec<PrivateAskHistoryEntry> {
    // Sorted rather than trusted: a hand-edited or partially written file could
    // present any order, and the count bound must drop the OLDEST.
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.asked_at));
    // A stamp from the future is not a reading of this machine's clock; it is
    // dropped rather than kept forever at the head of the list.
    entries.retain(|entry| {
        entry.asked_at <= now && now.saturating_sub(entry.asked_at) <= HISTORY_MAX_AGE
    });
    entries.truncate(HISTORY_LIMIT);
    entries
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), PrivateAskFailure> {
    use std::os::unix::fs::OpenOptionsExt;
    let parent = path.parent().ok_or(PrivateAskFailure::InvalidState)?;
    let temporary = parent.join(format!(".{}-{}.tmp", HISTORY_FILE, uuid::Uuid::new_v4()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|_| PrivateAskFailure::InvalidState)?;
    let written = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| PrivateAskFailure::InvalidState);
    drop(file);
    if let Err(failure) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(failure);
    }
    // Rename, so a reader never sees a half-written history and a crash leaves
    // the previous one intact rather than a truncated file.
    if std::fs::rename(&temporary, path).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err(PrivateAskFailure::InvalidState);
    }
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), PrivateAskFailure> {
    let _ = (path, bytes);
    Err(PrivateAskFailure::InvalidState)
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
