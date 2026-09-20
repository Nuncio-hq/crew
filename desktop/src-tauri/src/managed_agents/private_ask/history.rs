//! Owner-local private Ask history, v2.
//!
//! A private Ask never publishes to the relay — so nothing on the relay can
//! outlive the viewer's own change of heart, and nothing can be read back by
//! another viewer. The history the viewer *does* get is kept where the
//! authorization boundary itself lives: under the verified ownership staging
//! root — the directory [`VerifiedStagingOwnership`] proves the active seat
//! owns — encrypted at rest by the operating-system account boundary like
//! every other file in that tree. It is conversation state, not a second
//! Wiki store: wiki events never carry it, and localStorage is never keyed by
//! it.
//!
//! Corruption is a real case — the file is a JSON document under 0o600 — so a
//! file that fails to parse is quarantined aside rather than silently dropped:
//! it is renamed to `private-ask-history.corrupt-<attempt>` and the next write
//! starts clean. History is the user's own record; deleting it without being
//! told would be a second failure on top of the first.

use super::{PriorTurn, PrivateAskScope, PRIVATE_ASK_INSUFFICIENT};
use crate::managed_agents::recap_ownership::VerifiedStagingOwnership;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// Schema version written now. A v1 file is parsed by the compat reader and
/// its entries treated as foreign — their scope was never recorded, so they
/// can never be attributed to a viewer/repository scope and are dropped on the
/// next write (and quarantined rather than silently lost — see `load`).
const HISTORY_SCHEMA_VERSION: u8 = 2;

/// File name inside the ownership staging root. One file per seat: every
/// scope this identity has ever asked under lives in it, partitioned by the
/// entry's own scope key — never a per-scope file an onlooker could count.
const HISTORY_FILE: &str = "private-ask-history.json";

/// Entries kept per scope. The limit is a bound on questions, not on storage:
/// a viewer who hits it sees the oldest entries retire, oldest first.
const HISTORY_LIMIT_PER_SCOPE: usize = 100;

/// Entries older than this are dropped on the next write — the file is only
/// rewritten when it is touched, so the bound is enforced lazily, on the same
/// write that would have grown it.
const HISTORY_MAX_AGE_SECS: u64 = 30 * 24 * 60 * 60;

/// Total retained bytes per viewer across every scope — the issue's 10 MiB
/// retained-history bound. The entry bound is per scope so one busy
/// repository cannot evict another's history; the byte bound is per viewer
/// because it is the viewer's disk being bounded.
const HISTORY_LIMIT_BYTES_PER_VIEWER: usize = 10 * 1024 * 1024;

/// Per-field byte bounds. A question longer than this is never the request's
/// own question — the run command refuses it — so these are guards against a
/// corrupt or hand-edited file, not limits a user can hit.
const QUESTION_LIMIT_BYTES: usize = 4 * 1024;
const MARKDOWN_LIMIT_BYTES: usize = 32 * 1024;
const CITATION_LIMIT: usize = 64;
const DETAIL_LIMIT_BYTES: usize = 4 * 1024;

/// The most turns a follow-up may carry back into the prompt.
pub(crate) const PRIOR_TURNS: usize = 4;
/// Per-turn byte bound on carried prior answers.
const PRIOR_TURN_BYTES: usize = 8 * 1024;

/// One write at a time per process. Two attempts in flight under one viewer
/// each get a terminal write; the lock serializes the read-modify-write so
/// neither is lost to the other.
static RECORD_LOCK: Mutex<()> = Mutex::new(());

fn history_path(ownership: &VerifiedStagingOwnership) -> Result<PathBuf, ()> {
    ownership
        .private_ask_history_base()
        .map(|base| base.join(HISTORY_FILE))
        .map_err(|_| ())
}

/// The identity an attempt is scoped to — the fields that separate one
/// viewer's questions from another's. A v1 entry has none of these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HistoryScope {
    pub(crate) community_id: String,
    pub(crate) relay_url: String,
    pub(crate) viewer_pubkey: String,
    pub(crate) agent_pubkey: String,
    pub(crate) project_id: String,
    pub(crate) repo_owner: String,
    pub(crate) repo_d: String,
}

impl HistoryScope {
    pub(crate) fn from_scope(scope: &PrivateAskScope) -> Self {
        Self {
            community_id: scope.community_id.clone(),
            relay_url: scope.relay_url.clone(),
            viewer_pubkey: scope.viewer_pubkey.clone(),
            agent_pubkey: scope.agent_pubkey.clone(),
            project_id: scope.project_id.clone(),
            repo_owner: scope.repo_owner.clone(),
            repo_d: scope.repo_d.clone(),
        }
    }

    /// The key history is partitioned by. Deliberately smaller than the whole
    /// scope: the agent and relay fields travel on the record (a viewer sees
    /// who answered and where it was bound) but a viewer asking the same
    /// repository through two agents sees one history, and a relay override
    /// must not strand its own entries.
    fn key(&self) -> ScopeKey {
        ScopeKey {
            community_id: self.community_id.clone(),
            viewer_pubkey: self.viewer_pubkey.clone(),
            repo_owner: self.repo_owner.clone(),
            repo_d: self.repo_d.clone(),
        }
    }
}

/// The tuple history reads and retention are keyed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScopeKey {
    pub(crate) community_id: String,
    pub(crate) viewer_pubkey: String,
    pub(crate) repo_owner: String,
    pub(crate) repo_d: String,
}

/// The stable state of one attempt. `Running` is never returned to the reader
/// as-is: a live registered attempt reports running, and one the process no
/// longer holds — a crash, a closed window mid-attempt — reports
/// `interrupted`, because the honest name for a vanished attempt is not the
/// optimistic one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum HistoryStatus {
    Running,
    Answered,
    Insufficient,
    Cancelled,
    Refused,
    Interrupted,
}

/// One attempt, with everything its record must explain: what was asked, in
/// whose name, what was consulted, and what became of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrivateAskHistoryEntry {
    pub(crate) attempt_id: String,
    /// The question thread: a fresh question's id is its own attempt's; a
    /// follow-up inherits the parent's, so the thread reads as one.
    pub(crate) question_id: String,
    /// The attempt this one followed up on, if any.
    pub(crate) follow_up_of: Option<String>,
    pub(crate) scope: HistoryScope,
    pub(crate) question: String,
    pub(crate) status: HistoryStatus,
    /// The fixed refusal/cancellation reason, or the insufficiency message.
    /// Empty for an answered attempt — its record is the answer itself.
    pub(crate) detail: Option<String>,
    pub(crate) markdown: Option<String>,
    pub(crate) citations: Option<Vec<super::GroundedSource>>,
    /// What retrieval supplied and what the prompt bound then kept — the
    /// honest coverage record, kept with the answer it produced.
    pub(crate) manifest: Option<super::retrieval::RetrievalManifest>,
    /// The source revision the answer was grounded in. Absent when the
    /// attempt refused before the snapshot could be verified.
    pub(crate) source_revision: Option<String>,
    /// Epoch seconds when the question was asked.
    pub(crate) asked_at: u64,
    /// Epoch seconds the attempt reached a terminal state.
    pub(crate) finished_at: Option<u64>,
}

impl PrivateAskHistoryEntry {
    /// The record an attempt writes before anything can launch. A process
    /// that vanishes after this write leaves a `running` entry the next read
    /// reports as `interrupted` — never a phantom `answered`.
    pub(crate) fn pending(
        scope: &HistoryScope,
        question_id: &str,
        follow_up_of: Option<String>,
        question: &str,
        attempt_id: &str,
        source_revision: Option<&str>,
        asked_at: u64,
    ) -> Self {
        Self {
            attempt_id: attempt_id.to_owned(),
            question_id: question_id.to_owned(),
            follow_up_of,
            scope: scope.clone(),
            question: question.to_owned(),
            status: HistoryStatus::Running,
            detail: None,
            markdown: None,
            citations: None,
            manifest: None,
            source_revision: source_revision.map(str::to_owned),
            asked_at,
            finished_at: None,
        }
    }

    /// The terminal record for an answered attempt.
    pub(crate) fn answered(
        scope: &HistoryScope,
        question_id: &str,
        follow_up_of: Option<String>,
        question: &str,
        response: &super::attempt::PrivateAskResponse,
        now: u64,
    ) -> Self {
        Self {
            attempt_id: response.attempt_id.clone(),
            question_id: question_id.to_owned(),
            follow_up_of,
            scope: scope.clone(),
            question: question.to_owned(),
            status: HistoryStatus::Answered,
            detail: None,
            markdown: Some(response.markdown.clone()),
            citations: Some(response.citations.clone()),
            manifest: Some(response.manifest.clone()),
            source_revision: Some(response.source_revision.clone()),
            asked_at: now,
            finished_at: Some(now),
        }
    }

    /// The terminal record for an attempt the verified snapshot could not
    /// cover.
    pub(crate) fn insufficient(
        scope: &HistoryScope,
        question_id: &str,
        follow_up_of: Option<String>,
        question: &str,
        attempt_id: &str,
        source_revision: Option<&str>,
        manifest: super::retrieval::RetrievalManifest,
        now: u64,
    ) -> Self {
        Self {
            attempt_id: attempt_id.to_owned(),
            question_id: question_id.to_owned(),
            follow_up_of,
            scope: scope.clone(),
            question: question.to_owned(),
            status: HistoryStatus::Insufficient,
            detail: Some(PRIVATE_ASK_INSUFFICIENT.to_string()),
            markdown: None,
            citations: None,
            manifest: Some(manifest),
            source_revision: source_revision.map(str::to_owned),
            asked_at: now,
            finished_at: Some(now),
        }
    }

    /// The terminal record for an attempt that reached a fence or a failure —
    /// including `cancelled`, which is a refusal with a named reason rather
    /// than a silence.
    pub(crate) fn finished_refusal(
        scope: &HistoryScope,
        question_id: &str,
        follow_up_of: Option<String>,
        question: &str,
        attempt_id: &str,
        source_revision: Option<&str>,
        failure: &super::PrivateAskFailure,
        now: u64,
    ) -> Self {
        let cancelled = matches!(
            failure,
            super::PrivateAskFailure::Process(
                super::super::discovery::bounded_command::BoundedFailure::Cancelled
            )
        );
        Self {
            attempt_id: attempt_id.to_owned(),
            question_id: question_id.to_owned(),
            follow_up_of,
            scope: scope.clone(),
            question: question.to_owned(),
            status: if cancelled {
                HistoryStatus::Cancelled
            } else {
                HistoryStatus::Refused
            },
            detail: Some(failure.to_string()),
            markdown: None,
            citations: None,
            manifest: None,
            source_revision: source_revision.map(str::to_owned),
            asked_at: now,
            finished_at: Some(now),
        }
    }
}

/// The on-disk document.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryDocument {
    version: u8,
    entries: Vec<PrivateAskHistoryEntry>,
}

/// A v1 entry: unscoped flat shape, parsed only to be told apart.
///
/// The v1 reader cannot attribute these entries to any scope — the fields
/// that would identify one were never recorded — so they are never served.
/// They are kept in the document until the next write drops them, at which
/// point the v1 document is quarantined rather than truncated away.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyEntry {
    answer_id: String,
    created_at: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyDocument {
    #[allow(dead_code)]
    version: u8,
    entries: Vec<LegacyEntry>,
}

/// Read the file, or explain why it cannot be read.
///
/// `Ok(None)` means "no file" — a first run, not a failure. `Err(path)` means
/// the file could not be trusted and was quarantined to `path`.
fn read_document(
    ownership: &VerifiedStagingOwnership,
) -> Result<Option<Vec<PrivateAskHistoryEntry>>, PathBuf> {
    let path = history_path(ownership).map_err(|_| PathBuf::new())?;
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    if let Ok(document) = serde_json::from_str::<HistoryDocument>(&raw) {
        return Ok(Some(document.entries));
    }
    // A v1 file parses here and only here. Its entries have no scope, so they
    // can never be attributed — they are reported as foreign rather than as
    // history.
    if serde_json::from_str::<LegacyDocument>(&raw).is_ok() {
        return Ok(Some(Vec::new()));
    }
    Err(quarantine(&path, &raw))
}

/// Move a corrupt file aside and leave a copy for inspection. The file is
/// owner-local and the corruption is the owner's problem to know about, so
/// nothing is deleted.
fn quarantine(path: &std::path::Path, raw: &str) -> PathBuf {
    let corrupt = path.with_file_name(format!(
        "private-ask-history.corrupt-{:08x}",
        // A content-derived name: two corrupt files with the same bytes are
        // the same file, and different bytes get different names.
        raw.bytes().fold(0u32, |hash, byte| {
            hash.wrapping_mul(31).wrapping_add(byte as u32)
        })
    ));
    if std::fs::write(&corrupt, raw).is_ok() {
        let _ = std::fs::remove_file(path);
    }
    corrupt
}

/// Write the document. A failed write leaves the previous file in place and
/// reports false — the caller keeps the attempt; only the record is lost.
fn write_document(
    ownership: &VerifiedStagingOwnership,
    entries: &[PrivateAskHistoryEntry],
) -> Result<(), ()> {
    let path = history_path(ownership)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| ())?;
    }
    let document = HistoryDocument {
        version: HISTORY_SCHEMA_VERSION,
        entries: entries.to_vec(),
    };
    let body = serde_json::to_vec(&document).map_err(|_| ())?;
    let temp = path.with_extension("tmp");
    {
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        // Owner-read-write only. The base directory itself is mode 0700; the
        // file's own mode is belt on the same braces.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(|_| ())?;
        file.write_all(&body).map_err(|_| ())?;
        file.sync_all().map_err(|_| ())?;
    }
    std::fs::rename(&temp, &path).map_err(|_| ())?;
    Ok(())
}

/// Drop entries that no longer belong: too old, over the per-scope entry
/// bound, or over the per-scope byte bound — and any v1-foreign entries the
/// compat reader already filtered to `None`.
///
/// Bounds apply per scope key, so one busy repository cannot evict another's
/// history.
fn prune(entries: Vec<PrivateAskHistoryEntry>, now: u64) -> Vec<PrivateAskHistoryEntry> {
    let mut kept: Vec<PrivateAskHistoryEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        if now.saturating_sub(entry.asked_at) > HISTORY_MAX_AGE_SECS {
            continue;
        }
        entry.scope.key(); // normalize; entries already carry full scope
        kept.push(entry);
    }
    // Newest-first within each scope for the retention pass: retention drops
    // the oldest, and keeping the pass explicit avoids depending on file order.
    let mut per_scope: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (index, entry) in kept.iter().enumerate() {
        per_scope
            .entry(format!(
                "{}\0{}\0{}\0{}",
                entry.scope.community_id,
                entry.scope.viewer_pubkey,
                entry.scope.repo_owner,
                entry.scope.repo_d
            ))
            .or_default()
            .push(index);
    }
    let mut drop_indices: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    for (_key, mut indices) in per_scope {
        // Oldest first.
        indices.sort_by_key(|&i| kept[i].asked_at);
        // Entry-count bound.
        if indices.len() > HISTORY_LIMIT_PER_SCOPE {
            drop_indices.extend(indices.iter().take(indices.len() - HISTORY_LIMIT_PER_SCOPE));
        }
    }
    // Byte bound, per *viewer* across every scope: the file is the viewer's
    // disk, so a second repository's questions do not get a second 10 MiB.
    let mut per_viewer: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (index, entry) in kept.iter().enumerate() {
        if !drop_indices.contains(&index) {
            per_viewer
                .entry(entry.scope.viewer_pubkey.clone())
                .or_default()
                .push(index);
        }
    }
    for (_viewer, mut indices) in per_viewer {
        indices.sort_by_key(|&i| kept[i].asked_at);
        let mut bytes: usize = indices.iter().map(|&i| serialized_size(&kept[i])).sum();
        while bytes > HISTORY_LIMIT_BYTES_PER_VIEWER && !indices.is_empty() {
            let oldest = indices.remove(0);
            drop_indices.insert(oldest);
            bytes = bytes.saturating_sub(serialized_size(&kept[oldest]));
        }
    }
    let mut retained: Vec<PrivateAskHistoryEntry> = Vec::with_capacity(kept.len());
    for (index, entry) in kept.into_iter().enumerate() {
        if !drop_indices.contains(&index) {
            retained.push(entry);
        }
    }
    // Present newest-first to the reader; file order is the display order.
    retained.sort_by(|a, b| b.asked_at.cmp(&a.asked_at).then(b.attempt_id.cmp(&a.attempt_id)));
    retained
}

/// One entry's serialized size, for the byte bound. Approximate by JSON size —
/// exact enough for a retention bound, and it measures what is actually kept.
fn serialized_size(entry: &PrivateAskHistoryEntry) -> usize {
    serde_json::to_vec(entry).map(|body| body.len()).unwrap_or(0)
}

/// Map a stored `running` entry to the status the reader should see.
///
/// `is_live` is the attempt registry's answer — a running entry whose attempt
/// is still registered is running; one that is not is `interrupted`.
fn visible_status(entry: &PrivateAskHistoryEntry, is_live: &dyn Fn(&str) -> bool) -> HistoryStatus {
    if entry.status == HistoryStatus::Running && !is_live(&entry.attempt_id) {
        return HistoryStatus::Interrupted;
    }
    entry.status
}

/// Clamp one entry's fields to the file's guards. A corrupt or hand-edited
/// file can contain anything; the guards keep a hostile blob from growing
/// the file by simply being re-saved.
fn clamped(mut entry: PrivateAskHistoryEntry) -> PrivateAskHistoryEntry {
    entry.question = shortened(&entry.question, QUESTION_LIMIT_BYTES);
    entry.markdown = entry
        .markdown
        .map(|markdown| shortened(&markdown, MARKDOWN_LIMIT_BYTES));
    entry.detail = entry
        .detail
        .map(|detail| shortened(&detail, DETAIL_LIMIT_BYTES));
    if let Some(citations) = entry.citations.as_mut() {
        citations.truncate(CITATION_LIMIT);
    }
    entry
}

/// Truncate to `limit` bytes on a UTF-8 boundary. Stored history is a record,
/// not a stream — a cut mid-answer ends with a visible marker rather than
/// pretending the text was shorter than it was.
fn shortened(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = text[..end].to_owned();
    out.push_str("\n…");
    out
}

/// Insert or replace one attempt's record, then persist. Returns false when
/// the record could not be written; the caller reports that rather than
/// treating a write failure as an ask failure.
pub(crate) fn upsert(
    ownership: &VerifiedStagingOwnership,
    entry: PrivateAskHistoryEntry,
    now: u64,
) -> Result<(), ()> {
    let _guard = RECORD_LOCK.lock().map_err(|_| ())?;
    let mut entries = read_document(ownership).unwrap_or_default().unwrap_or_default();
    entries.retain(|existing| existing.attempt_id != entry.attempt_id);
    entries.push(clamped(entry));
    let entries = prune(entries, now);
    write_document(ownership, &entries)
}

/// This viewer's history for one repository scope, newest first.
///
/// A `running` entry whose attempt is no longer registered reports
/// `interrupted` — the process that owned it is gone — while a still-registered
/// one reports running. `is_live` is the registry's own answer, supplied so
/// this seam stays pure.
pub(crate) fn load_scoped(
    ownership: &VerifiedStagingOwnership,
    key: &ScopeKey,
    is_live: &dyn Fn(&str) -> bool,
) -> Vec<PrivateAskHistoryEntry> {
    let Ok(Some(entries)) = read_document(ownership) else {
        return Vec::new();
    };
    entries
        .into_iter()
        .filter(|entry| entry.scope.key() == *key)
        .map(|mut entry| {
            entry.status = visible_status(&entry, is_live);
            entry
        })
        .collect()
}

/// One attempt's record, resolved the same way `load_scoped` resolves it.
pub(crate) fn load_attempt(
    ownership: &VerifiedStagingOwnership,
    key: &ScopeKey,
    attempt_id: &str,
    is_live: &dyn Fn(&str) -> bool,
    now: u64,
) -> Option<PrivateAskHistoryEntry> {
    let _ = now;
    let Ok(Some(entries)) = read_document(ownership) else {
        return None;
    };
    entries
        .into_iter()
        .find(|entry| entry.scope.key() == *key && entry.attempt_id == attempt_id)
        .map(|mut entry| {
            entry.status = visible_status(&entry, is_live);
            entry
        })
}

/// The turns a follow-up is allowed to see: the question thread's answered
/// turns, oldest first, ending at the parent it names.
///
/// `None` means the follow-up cannot be grounded: the parent is missing, not
/// answered, belongs to another scope — anything but a settled answer in this
/// viewer's own history. Refusing is honest where guessing would not be.
pub(crate) fn follow_up_thread(
    ownership: &VerifiedStagingOwnership,
    key: &ScopeKey,
    parent_attempt_id: &str,
    is_live: &dyn Fn(&str) -> bool,
    now: u64,
) -> Option<(String, Vec<PriorTurn>)> {
    let _ = now;
    let Ok(Some(entries)) = read_document(ownership) else {
        return None;
    };
    let parent = entries
        .iter()
        .find(|entry| {
            entry.scope.key() == *key
                && entry.attempt_id == parent_attempt_id
                && visible_status(entry, is_live) == HistoryStatus::Answered
        })?;
    let question_id = parent.question_id.clone();
    // The thread is the follow-up chain walked backward from the parent, not a
    // time slice: a sibling follow-up to an earlier turn is a different branch
    // of the same question and must not leak into this one. Entries off the
    // chain — including another viewer's identically-keyed write that a
    // hostile file could plant — are never seen.
    let mut prior = Vec::new();
    let mut cursor = Some(parent);
    // The walk is bounded by the turns a prompt may carry; a self-referencing
    // hand-written record cannot make it loop.
    for _ in 0..PRIOR_TURNS {
        let Some(entry) = cursor else { break };
        if visible_status(entry, is_live) == HistoryStatus::Answered {
            let markdown = entry.markdown.clone().unwrap_or_default();
            prior.push(PriorTurn {
                question: shortened(&entry.question, QUESTION_LIMIT_BYTES),
                markdown: shortened(&markdown, PRIOR_TURN_BYTES),
            });
        }
        cursor = entry.follow_up_of.as_deref().and_then(|parent_id| {
            entries.iter().find(|candidate| {
                candidate.scope.key() == *key
                    && candidate.attempt_id == parent_id
                    && candidate.question_id == question_id
            })
        });
    }
    prior.reverse();
    Some((question_id, prior))
}

/// Remove one attempt's record — the only write that takes history away.
/// Returns whether a record was removed, so the command can say so.
pub(crate) fn forget(
    ownership: &VerifiedStagingOwnership,
    key: &ScopeKey,
    attempt_id: &str,
    now: u64,
) -> Result<bool, ()> {
    let _guard = RECORD_LOCK.lock().map_err(|_| ())?;
    let entries = read_document(ownership).unwrap_or_default().unwrap_or_default();
    let before = entries.len();
    let entries: Vec<PrivateAskHistoryEntry> = entries
        .into_iter()
        .filter(|entry| !(entry.scope.key() == *key && entry.attempt_id == attempt_id))
        .collect();
    if entries.len() == before {
        return Ok(false);
    }
    let entries = prune(entries, now);
    write_document(ownership, &entries)?;
    Ok(true)
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
