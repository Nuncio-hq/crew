//! Developer-only command surface for the private Wiki Ask.
//!
//! Registration is unconditional, because a registered-or-not decision made at
//! build time is invisible to the renderer and produces a confusing "command
//! not found" instead of a reason. The *gate* is in each command body: the
//! first thing every command does is ask
//! [`dev_gate::private_ask_dev_enabled`], and a closed gate returns
//! [`PRIVATE_ASK_UNAVAILABLE`] before any argument is read.
//!
//! The surface is deliberately thin. Resolution owns every safety decision,
//! and these commands carry its answer — an answer, an insufficiency, or a
//! typed refusal — to a developer's screen without softening it. History
//! reads are scoped by the same captured identity the run resolves under: a
//! renderer supplies the repository coordinate, never the viewer or community
//! it is reading as.

use super::private_ask::dev_gate::private_ask_dev_enabled;
use super::private_ask::history::{self, PrivateAskHistoryEntry};
use super::private_ask::{
    AskEvent, AskReporter, AttemptIdentity, DevAskMeta, DevOutcome, PrivateAskAttempts,
    PrivateAskFailure, RegisterFailure, PRIVATE_ASK_INSUFFICIENT,
};
use super::recap_ownership::VerifiedStagingOwnership;
use serde::Serialize;
use tauri::Emitter;

/// Accept an id only in the shape this surface mints.
///
/// Attempt ids key the cancel registry and the owner-local record, and they
/// arrive from a webview, so they are checked where they enter rather than
/// deep inside. Question ids are minted the same way.
fn valid_uuid_v4(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok_and(|parsed| parsed.get_version_num() == 4)
}

/// A managed-agent pubkey, in the shape the store keys records on.
fn valid_agent_id(agent_id: &str) -> bool {
    agent_id.len() == 64 && agent_id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A `<owner-hex>:<repo-d>` repository coordinate. The native resolver checks
/// it against the signed snapshot manifest; this only refuses a shape that
/// could never be one.
fn valid_coordinate(coordinate: &str) -> bool {
    coordinate.split_once(':').is_some_and(|(owner, repo_d)| {
        owner.len() == 64
            && owner.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !repo_d.is_empty()
            && repo_d.len() <= 256
            && repo_d == repo_d.trim()
            && !repo_d.chars().any(char::is_control)
    })
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

/// What a renderer is told when the surface is closed. It is deliberately the
/// same shape as any other refusal so the UI has one path.
pub(crate) const PRIVATE_ASK_UNAVAILABLE: &str = "private Ask is not enabled in this build";

/// The event an attempt's reporter emits for a phase change.
const ASK_PROGRESS_EVENT: &str = "private-ask:progress";
/// The event carrying kept answer bytes as they arrive.
const ASK_CHUNK_EVENT: &str = "private-ask:chunk";

/// The (event name, payload) an attempt's own event becomes, tagged with the
/// attempt it belongs to — the fence a renderer matches stream events against.
fn progress_event(attempt_id: &str, event: AskEvent) -> (&'static str, serde_json::Value) {
    match event {
        AskEvent::Retrieving => (
            ASK_PROGRESS_EVENT,
            serde_json::json!({"attemptId": attempt_id, "phase": "retrieving"}),
        ),
        AskEvent::Running => (
            ASK_PROGRESS_EVENT,
            serde_json::json!({"attemptId": attempt_id, "phase": "running"}),
        ),
        AskEvent::Chunk(text) => (
            ASK_CHUNK_EVENT,
            serde_json::json!({"attemptId": attempt_id, "text": text}),
        ),
    }
}

/// The status string a renderer switches on — deliberately a closed
/// vocabulary so a new outcome cannot silently render as an old one.
const STATUS_ANSWERED: &str = "answered";
const STATUS_INSUFFICIENT: &str = "insufficient";
const STATUS_CANCELLED: &str = "cancelled";
const STATUS_REFUSED: &str = "refused";

/// What the renderer needs to decide which composer to draw.
///
/// `enabled` is the backend's answer, never a renderer-side flag: the webview
/// asks, it does not decide.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateAskDevStatus {
    pub enabled: bool,
    /// Present when the surface is open but a request would still be refused,
    /// so a developer sees the real reason rather than an empty answer box.
    pub blocked_reason: Option<String>,
}

/// One finished attempt as the renderer receives it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateAskRunResult {
    /// The attempt the result belongs to — the fence a late stream event is
    /// matched against.
    pub attempt_id: String,
    /// The question thread this attempt landed on.
    pub question_id: String,
    /// `answered` | `insufficient` | `refused` | `cancelled`.
    pub status: &'static str,
    /// Empty unless `status` is `answered`.
    pub markdown: String,
    /// The typed refusal, in the viewer-safe vocabulary — including the fixed
    /// insufficiency reason when `status` is `insufficient`.
    pub refusal: Option<String>,
    pub citations: Vec<PrivateAskCitation>,
    /// The source revision the answer was grounded in, when resolution got
    /// far enough to verify one.
    pub source_revision: Option<String>,
    /// The coverage record: what the prompt consulted and what it omitted.
    pub manifest: Option<super::private_ask::retrieval::RetrievalManifest>,
    /// False when the attempt finished but could not be written to the
    /// owner-local history. It is reported rather than swallowed: the outcome
    /// still happened, and a history quietly missing it would be a worse lie
    /// than a visible note.
    pub history_recorded: bool,
}

/// One agent as the picker sees it — named and honest about availability.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateAskAgent {
    pub pubkey: String,
    pub name: String,
    /// The relay its harness is bound to; the runtime-start command needs it
    /// to bring an offline agent back.
    pub relay_url: String,
    /// `ready` | `busy` | `offline` — what the resolver would conclude now.
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateAskCitation {
    pub path: String,
    pub start_line: u64,
    pub end_line: u64,
}

impl From<&super::private_ask::GroundedSource> for PrivateAskCitation {
    fn from(citation: &super::private_ask::GroundedSource) -> Self {
        Self {
            path: citation.path().to_owned(),
            start_line: citation.start_line(),
            end_line: citation.end_line(),
        }
    }
}

/// The read-only draft input #367 consumes: the exact question/attempt, the
/// answer material as validated, the source manifest, and provenance. Nothing
/// here creates a channel task — producing this shape never sends anything.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateAskDraftInput {
    pub question_id: String,
    pub attempt_id: String,
    pub question: String,
    pub markdown: String,
    pub citations: Vec<PrivateAskCitation>,
    pub manifest: Option<super::private_ask::retrieval::RetrievalManifest>,
    pub source_revision: Option<String>,
    /// Where the answer came from, for a future dispatch record: the scope's
    /// repository coordinate and the agent that answered.
    pub origin_coordinate: String,
    pub origin_agent: String,
}

/// One history entry as the renderer needs it — the record's own fields minus
/// the citation bodies, which would make every list read carry the source
/// bytes it cites.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateAskHistoryItem {
    pub attempt_id: String,
    pub question_id: String,
    pub follow_up_of: Option<String>,
    /// `running` | `interrupted` | `answered` | `insufficient` | `cancelled` |
    /// `refused` — the stored status resolved against the live registry.
    pub status: &'static str,
    pub detail: Option<String>,
    pub question: String,
    pub markdown: Option<String>,
    pub citations: Vec<PrivateAskCitation>,
    pub manifest: Option<super::private_ask::retrieval::RetrievalManifest>,
    pub source_revision: Option<String>,
    /// Who answered — for the "asked <agent>" label. Stable pubkey, not a
    /// display name that could have changed since.
    pub agent_pubkey: String,
    pub asked_at: u64,
    pub finished_at: Option<u64>,
}

impl PrivateAskHistoryItem {
    fn from_entry(entry: PrivateAskHistoryEntry) -> Self {
        Self {
            attempt_id: entry.attempt_id,
            question_id: entry.question_id,
            follow_up_of: entry.follow_up_of,
            status: match entry.status {
                history::HistoryStatus::Running => "running",
                history::HistoryStatus::Answered => "answered",
                history::HistoryStatus::Insufficient => "insufficient",
                history::HistoryStatus::Cancelled => "cancelled",
                history::HistoryStatus::Refused => "refused",
                history::HistoryStatus::Interrupted => "interrupted",
            },
            detail: entry.detail,
            question: entry.question,
            markdown: entry.markdown,
            citations: entry
                .citations
                .unwrap_or_default()
                .iter()
                .map(PrivateAskCitation::from)
                .collect(),
            manifest: entry.manifest,
            source_revision: entry.source_revision,
            agent_pubkey: entry.scope.agent_pubkey,
            asked_at: entry.asked_at,
            finished_at: entry.finished_at,
        }
    }
}

/// Report whether the private Ask surface exists in this build.
///
/// This must never launch anything. It is polled whenever the composer mounts,
/// and the run path now reaches a contained process, so a status that asked the
/// production entry point for a reason would start a capability probe on every
/// poll. It stops at the fences that cost nothing: the dev gate, and whether
/// this install has an owned staging tree to run inside at all.
#[tauri::command]
pub async fn private_ask_dev_status<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<PrivateAskDevStatus, String> {
    let enabled = private_ask_dev_enabled();
    let blocked_reason = if !enabled {
        None
    } else {
        VerifiedStagingOwnership::load(&app)
            .err()
            .map(|_| "this install has no owned staging tree for a private Ask".to_string())
    };
    Ok(PrivateAskDevStatus {
        enabled,
        blocked_reason,
    })
}

/// The viewer's own record of what they asked about this repository.
///
/// It never left this machine: a private Ask publishes nothing, so there is no
/// relay copy to reconcile with and no other reader. The scope is captured
/// here — a renderer that supplies a different viewer's pubkey gets its own
/// key ignored, not another viewer's history.
#[tauri::command]
pub async fn private_ask_history<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    attempts: tauri::State<'_, PrivateAskAttempts>,
    coordinate: String,
) -> Result<Vec<PrivateAskHistoryItem>, String> {
    if !private_ask_dev_enabled() {
        return Err(PRIVATE_ASK_UNAVAILABLE.to_string());
    }
    if !valid_coordinate(&coordinate) {
        return Err("private Ask history needs the repository it is about".to_string());
    }
    let ownership = VerifiedStagingOwnership::load(&app)
        .map_err(|_| "private Ask history is unavailable".to_string())?;
    let key = super::private_ask::ask_scope_key(&app, &coordinate)
        .await
        .map_err(|failure| failure.to_string())?;
    let registry = attempts.inner().clone();
    Ok(history::load_scoped(&ownership, &key, &move |id| {
        registry.is_registered(id)
    })
    .into_iter()
    .map(PrivateAskHistoryItem::from_entry)
    .collect())
}

/// One attempt's record — what a viewer reopening a question reads back.
#[tauri::command]
pub async fn private_ask_attempt<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    attempts: tauri::State<'_, PrivateAskAttempts>,
    coordinate: String,
    attempt_id: String,
) -> Result<Option<PrivateAskHistoryItem>, String> {
    if !private_ask_dev_enabled() {
        return Err(PRIVATE_ASK_UNAVAILABLE.to_string());
    }
    if !valid_coordinate(&coordinate) || !valid_uuid_v4(&attempt_id) {
        return Err("a private Ask attempt needs its repository and its id".to_string());
    }
    let ownership = VerifiedStagingOwnership::load(&app)
        .map_err(|_| "private Ask history is unavailable".to_string())?;
    let key = super::private_ask::ask_scope_key(&app, &coordinate)
        .await
        .map_err(|failure| failure.to_string())?;
    let registry = attempts.inner().clone();
    Ok(history::load_attempt(
        &ownership,
        &key,
        &attempt_id,
        &move |id| registry.is_registered(id),
        now_seconds(),
    )
    .map(PrivateAskHistoryItem::from_entry))
}

/// Remove one attempt's record — the only write that takes history away.
#[tauri::command]
pub async fn private_ask_forget<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    coordinate: String,
    attempt_id: String,
) -> Result<bool, String> {
    if !private_ask_dev_enabled() {
        return Err(PRIVATE_ASK_UNAVAILABLE.to_string());
    }
    if !valid_coordinate(&coordinate) || !valid_uuid_v4(&attempt_id) {
        return Err("forgetting a private Ask needs its repository and its id".to_string());
    }
    let ownership = VerifiedStagingOwnership::load(&app)
        .map_err(|_| "private Ask history is unavailable".to_string())?;
    let key = super::private_ask::ask_scope_key(&app, &coordinate)
        .await
        .map_err(|failure| failure.to_string())?;
    history::forget(&ownership, &key, &attempt_id, now_seconds())
        .map_err(|_| "the private Ask record could not be written".to_string())
}

/// The read-only draft input for one answered attempt (#367's seam).
///
/// It exists only for an answered record in this viewer's own scoped history —
/// a refusal or an insufficiency has no validated material to hand off, and
/// another scope's answer is simply absent.
#[tauri::command]
pub async fn private_ask_draft<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    attempts: tauri::State<'_, PrivateAskAttempts>,
    coordinate: String,
    attempt_id: String,
) -> Result<Option<PrivateAskDraftInput>, String> {
    if !private_ask_dev_enabled() {
        return Err(PRIVATE_ASK_UNAVAILABLE.to_string());
    }
    if !valid_coordinate(&coordinate) || !valid_uuid_v4(&attempt_id) {
        return Err("a private Ask draft needs its repository and its attempt".to_string());
    }
    let ownership = VerifiedStagingOwnership::load(&app)
        .map_err(|_| "private Ask history is unavailable".to_string())?;
    let key = super::private_ask::ask_scope_key(&app, &coordinate)
        .await
        .map_err(|failure| failure.to_string())?;
    let registry = attempts.inner().clone();
    let entry = history::load_attempt(
        &ownership,
        &key,
        &attempt_id,
        &move |id| registry.is_registered(id),
        now_seconds(),
    );
    Ok(entry.and_then(|entry| {
        if entry.status != history::HistoryStatus::Answered {
            return None;
        }
        Some(PrivateAskDraftInput {
            question_id: entry.question_id.clone(),
            attempt_id: entry.attempt_id.clone(),
            question: entry.question.clone(),
            markdown: entry.markdown.clone()?,
            citations: entry
                .citations
                .as_ref()
                .map(|citations| citations.iter().map(PrivateAskCitation::from).collect())
                .unwrap_or_default(),
            manifest: entry.manifest.clone(),
            source_revision: entry.source_revision.clone(),
            origin_coordinate: coordinate,
            origin_agent: entry.scope.agent_pubkey.clone(),
        })
    }))
}

/// The agents on this machine and their askability.
///
/// Every stored record is listed, with the status the resolver itself would
/// conclude — a picker that hid busy or offline agents could not offer the
/// recovery (start the agent, pick another) the contract requires.
#[tauri::command]
pub async fn private_ask_agents<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<PrivateAskAgent>, String> {
    if !private_ask_dev_enabled() {
        return Err(PRIVATE_ASK_UNAVAILABLE.to_string());
    }
    Ok(super::private_ask::observed_agents(&app)
        .into_iter()
        .map(|agent| PrivateAskAgent {
            pubkey: agent.pubkey,
            name: agent.name,
            relay_url: agent.relay_url,
            status: agent.status,
        })
        .collect())
}

/// Run one private Ask and return its answer.
///
/// The outcome and every refusal come from `private_ask::dev_run`, which
/// reaches `resolve_observed_selection`, `admit_private_ask` and
/// `PrivateAskAttempt::run`. This command adds only the dev gate, the argument
/// shapes, the attempt's registration — so a cancel during resolution still
/// lands — and the reporter that turns the attempt's own events into progress
/// the UI may render. It never invents a result and never decides a reason.
///
/// `question_id` is minted by the caller for a fresh question; a follow-up
/// passes `follow_up_of` and inherits the parent's thread. Both must be ids
/// this surface mints — anything else is refused before it reaches a store.
#[tauri::command]
pub async fn private_ask_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    attempts: tauri::State<'_, PrivateAskAttempts>,
    attempt_id: String,
    question_id: String,
    agent_id: String,
    coordinate: String,
    question: String,
    follow_up_of: Option<String>,
) -> Result<PrivateAskRunResult, String> {
    if !private_ask_dev_enabled() {
        return Err(PRIVATE_ASK_UNAVAILABLE.to_string());
    }
    if !valid_uuid_v4(&attempt_id) || !valid_uuid_v4(&question_id) {
        return Err("a private Ask needs its own attempt and question ids".to_string());
    }
    if let Some(parent) = follow_up_of.as_deref() {
        if !valid_uuid_v4(parent) {
            return Err("a follow-up needs the attempt it builds on".to_string());
        }
    }
    if question.trim().is_empty() {
        return Err("a private Ask needs a question".to_string());
    }
    // The agent id and the repository coordinate are the only two things a
    // caller may name, so both are checked in the shape this surface mints
    // before either reaches a store read.
    if !valid_agent_id(&agent_id) {
        return Err("a private Ask needs the agent it is addressed to".to_string());
    }
    if !valid_coordinate(&coordinate) {
        return Err("a private Ask needs the repository it is about".to_string());
    }
    // Registered before the run starts, so a cancel that arrives while the
    // attempt is still choosing whether it may launch reaches it. The guard
    // releases the id on every return path below, including an early one.
    let registration = attempts.register(&attempt_id).map_err(|failure| {
        match failure {
            RegisterFailure::AlreadyRunning => "that private Ask is already running",
            RegisterFailure::TooManyRunning => "too many private Asks are already running",
        }
        .to_string()
    })?;

    // Progress is the attempt's own events, relayed under its own id: a UI
    // shows the phase the attempt reports and the chunks the drain kept —
    // nothing the renderer infers.
    let emit_attempt = attempt_id.clone();
    let emit_app = app.clone();
    let reporter: AskReporter = std::sync::Arc::new(move |event| {
        let (name, payload) = progress_event(&emit_attempt, event);
        let _ = emit_app.emit(name, payload);
    });
    let identity = AttemptIdentity::new(&attempt_id, registration.cancel_flag())
        .with_reporter(reporter);
    let asked_at = now_seconds();
    // Resolution reads this machine's own state and one scoped Wiki snapshot;
    // the answer itself is moved onto a blocking worker inside `dev_run`, on
    // one thread, which is what the thread-scoped no-publish attribution
    // relies on.
    let registry = attempts.inner().clone();
    let result = super::private_ask::dev_run(
        &app,
        &agent_id,
        &coordinate,
        &question,
        DevAskMeta {
            question_id,
            follow_up_of,
        },
        std::sync::Arc::new(move |id: &str| registry.is_registered(id)),
        &identity,
        asked_at,
    )
    .await;

    let question_id = result.question_id;
    Ok(match result.outcome {
        DevOutcome::Answered(response) => PrivateAskRunResult {
            attempt_id,
            question_id,
            status: STATUS_ANSWERED,
            markdown: response.markdown,
            refusal: None,
            citations: response.citations.iter().map(PrivateAskCitation::from).collect(),
            source_revision: Some(response.source_revision),
            manifest: Some(response.manifest),
            history_recorded: result.history_recorded,
        },
        DevOutcome::Insufficient(manifest) => PrivateAskRunResult {
            attempt_id,
            question_id,
            status: STATUS_INSUFFICIENT,
            markdown: String::new(),
            refusal: Some(PRIVATE_ASK_INSUFFICIENT.to_string()),
            citations: Vec::new(),
            source_revision: result.source_revision,
            manifest: Some(manifest),
            history_recorded: result.history_recorded,
        },
        DevOutcome::Refused(failure) => PrivateAskRunResult {
            attempt_id,
            question_id,
            status: if failure
                == PrivateAskFailure::Process(
                    crate::managed_agents::discovery::bounded_command::BoundedFailure::Cancelled,
                )
            {
                STATUS_CANCELLED
            } else {
                STATUS_REFUSED
            },
            markdown: String::new(),
            refusal: Some(failure.to_string()),
            citations: Vec::new(),
            source_revision: result.source_revision,
            manifest: result.manifest,
            history_recorded: result.history_recorded,
        },
    })
}

/// Cancel an in-flight private Ask.
///
/// Cancelling an attempt that is unknown or already finished is a no-op, not an
/// error: a renderer that cancels twice, or cancels after the answer arrived,
/// has not done anything wrong. A live attempt is signalled through the flag
/// its own run and probe already poll, so teardown stays with the bounded
/// runner that owns the child.
#[tauri::command]
pub async fn private_ask_cancel(
    attempts: tauri::State<'_, PrivateAskAttempts>,
    attempt_id: Option<String>,
) -> Result<(), String> {
    if !private_ask_dev_enabled() {
        return Err(PRIVATE_ASK_UNAVAILABLE.to_string());
    }
    if let Some(attempt_id) = attempt_id.as_deref() {
        if valid_uuid_v4(attempt_id) {
            attempts.cancel(attempt_id);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "private_ask_commands_tests.rs"]
mod tests;
