//! #367 — durable private-Wiki task dispatch.
//!
//! One explicit Start in the private-Ask dispatch panel is one intended
//! channel kickoff. The kickoff is a kind:9 root message produced by the
//! existing `build_message_with_client_tags` seam, so channel (`h`), mention
//! (`p`), and client marker tags are all constructed by the production
//! builder — this module only supplies content and the two
//! `["client", "crew-wiki-task*", …]` markers that make the kickoff a
//! queryable, idempotent unit.
//!
//! Durability uses the accepted owner-local operation journal
//! (`OperationKind::ThreadHandoff`):
//!
//! 1. `wiki_task_dispatch_prepare` reserves the operation with an immutable
//!    intent digest (draft key + reviewed title/prompt + chosen channel,
//!    agent, references, and origin coordinate). The same user action — a
//!    retry, a second window holding the same draft, a restart — resumes the
//!    row via `CreateResult::Existing`; a changed intent on an unresolved
//!    claim surfaces as `Conflict`, never a second hidden kickoff. Once the
//!    row exists the event is signed with `created_at = op.created_at` and
//!    the signed bytes plus the event id are CAS'ed into the payload, so the
//!    kickoff's identity is fixed before any publication attempt.
//! 2. `wiki_task_dispatch_submit` re-verifies live channel membership and the
//!    chosen agent's membership immediately before handoff, then republishes
//!    the *persisted* signed event verbatim. A retry cannot mint a second
//!    event: identical bytes dedupe at the relay to the same id.
//! 3. `wiki_task_dispatch_reconcile` resolves an ambiguous outcome by
//!    reading the relay for the persisted event id — a lost acknowledgement
//!    settles to the one kickoff that actually landed, not a new one.
//! 4. `wiki_task_dispatch_abandon` records user cancellation. Before any
//!    publish attempt it is a clean cancel; after an attempt it is refused
//!    unless `force`, and the forced record keeps an honest
//!    `abandoned_with_unresolved_publish` note — committed work is never
//!    pretended away.
//!
//! The membership authority read reuses the production kind:39002 snapshot
//! path (`channel_membership_snapshot` + `channel_members_from_event` plus
//! the kind:0 owner-attestation probe for agents) rather than trusting the
//! picker's cached roster.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use crate::app_state::owner_scope::{assert_current, capture, OwnerScopeToken, OWNER_SCOPE_STALE};
use crate::app_state::AppState;
use crate::commands::owner_operations::ScopedOperationResult;
use crate::owner_operations::{
    CreateResult, Limits, NewOperation, Operation, OperationKind, OperationScope, OperationStatus,
    OperationStore, OperationUpdate,
};
use crate::relay::{query_relay_at_with_keys, relay_http_base_url};

/// Idempotency marker on the kickoff event (`["client", MARKER, op_id]`).
pub(crate) const WIKI_TASK_CLIENT_MARKER: &str = "crew-wiki-task";
/// Origin pointer on the kickoff event
/// (`["client", MARKER, coordinate]`). Only the repository coordinate is
/// published — the private question/attempt ids stay in the local journal
/// payload where the author can recover them.
pub(crate) const WIKI_TASK_ORIGIN_MARKER: &str = "crew-wiki-task-origin";

const RESOURCE_PREFIX: &str = "crew-wiki-task:";
const MAX_TITLE_CHARS: usize = 200;
const MAX_REFERENCES: usize = 64;
const MAX_PATH_CHARS: usize = 1024;
/// One bounded retry when a concurrent writer moved the row revision.
const CAS_RETRIES: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WikiTaskReferenceRecord {
    pub path: String,
    pub start_line: u64,
    pub end_line: u64,
}

/// The reviewed dispatch intent, exactly as the renderer collected it from
/// the editable draft. Everything here participates in the operation's
/// creation digest — changing any field on an unresolved claim conflicts
/// instead of silently diverging.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WikiTaskDispatchInput {
    /// Caller-chosen uuid-v4; persisted in the draft so every retry of the
    /// same Start resolves to the same operation (and event id).
    pub dispatch_id: String,
    /// The `wiki:` draft key — the unit of "one draft, one pending kickoff".
    pub draft_key: String,
    pub question_id: String,
    pub attempt_id: String,
    /// Bare `<owner-hex>:<repo-d>` coordinate of the answered question.
    pub origin_coordinate: String,
    #[serde(default)]
    pub source_revision: Option<String>,
    pub title: String,
    pub prompt: String,
    pub channel_id: String,
    pub agent_pubkey: String,
    /// Only the references the user left checked.
    #[serde(default)]
    pub references: Vec<WikiTaskReferenceRecord>,
}

/// The persisted operation payload. Intent fields are fixed at creation
/// (they are what the creation digest covers); the `event_*` and outcome
/// fields are filled in by later CAS steps.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WikiTaskDispatchRecord {
    version: u32,
    draft_key: String,
    question_id: String,
    attempt_id: String,
    origin_coordinate: String,
    source_revision: Option<String>,
    title: String,
    prompt: String,
    channel_id: String,
    agent_pubkey: String,
    references: Vec<WikiTaskReferenceRecord>,
    #[serde(default)]
    event_id: Option<String>,
    /// The exact signed kickoff bytes, persisted before any publication.
    #[serde(default)]
    signed_event: Option<serde_json::Value>,
    #[serde(default)]
    submit_attempts: u32,
    #[serde(default)]
    accepted: Option<DispatchAcceptedRecord>,
    #[serde(default)]
    last_error: Option<String>,
    #[serde(default)]
    abandoned_with_unresolved_publish: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DispatchAcceptedRecord {
    event_id: String,
    submitted_at: i64,
    relay_message: String,
}

/// The bounded projection the renderer renders.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WikiTaskDispatchJob {
    pub(crate) dispatch_id: String,
    pub(crate) event_id: Option<String>,
    pub(crate) channel_id: String,
    pub(crate) root_event_id: Option<String>,
    pub(crate) status: &'static str,
    pub(crate) accepted: bool,
    pub(crate) submit_attempts: u32,
    pub(crate) error: Option<String>,
}

fn now_secs() -> Result<i64, String> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system clock unavailable".to_string())?
        .as_secs();
    i64::try_from(seconds).map_err(|_| "system clock unavailable".into())
}

fn valid_uuid_v4(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok_and(|parsed| parsed.get_version_num() == 4)
}

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

fn is_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_input(input: &WikiTaskDispatchInput) -> Result<(), String> {
    if !valid_uuid_v4(&input.dispatch_id) {
        return Err("dispatch id must be a uuid-v4".into());
    }
    if input.draft_key.trim().is_empty()
        || !input.draft_key.starts_with("wiki:")
        || input.draft_key.len() > 400
    {
        return Err("dispatch requires its wiki draft key".into());
    }
    if !valid_uuid_v4(&input.question_id) || !valid_uuid_v4(&input.attempt_id) {
        return Err("dispatch requires its question and attempt ids".into());
    }
    if !valid_coordinate(&input.origin_coordinate) {
        return Err("dispatch requires a valid origin coordinate".into());
    }
    if input.title.chars().count() > MAX_TITLE_CHARS {
        return Err("title is too long".into());
    }
    if input.title.trim().is_empty() && input.prompt.trim().is_empty() {
        return Err("an empty task cannot be dispatched".into());
    }
    if uuid::Uuid::parse_str(&input.channel_id).is_err() {
        return Err("invalid channel id".into());
    }
    if !is_hex64(&input.agent_pubkey) {
        return Err("invalid agent pubkey".into());
    }
    if input.references.len() > MAX_REFERENCES {
        return Err("too many references".into());
    }
    if input.references.iter().any(|reference| {
        reference.path.is_empty()
            || reference.path.chars().count() > MAX_PATH_CHARS
            || reference.path.chars().any(char::is_control)
            || reference.start_line > reference.end_line
    }) {
        return Err("invalid reference".into());
    }
    Ok(())
}

fn record_from_input(input: &WikiTaskDispatchInput) -> WikiTaskDispatchRecord {
    WikiTaskDispatchRecord {
        version: 1,
        draft_key: input.draft_key.clone(),
        question_id: input.question_id.clone(),
        attempt_id: input.attempt_id.clone(),
        origin_coordinate: input.origin_coordinate.clone(),
        source_revision: input.source_revision.clone(),
        title: input.title.clone(),
        prompt: input.prompt.clone(),
        channel_id: input.channel_id.clone(),
        agent_pubkey: input.agent_pubkey.clone(),
        references: input.references.clone(),
        event_id: None,
        signed_event: None,
        submit_attempts: 0,
        accepted: None,
        last_error: None,
        abandoned_with_unresolved_publish: false,
    }
}

fn read_record(operation: &Operation) -> Result<WikiTaskDispatchRecord, String> {
    serde_json::from_value(operation.payload.clone())
        .map_err(|_| "wiki task dispatch record is unreadable".to_string())
}

/// The channel message body: reviewed title, reviewed prompt, then only the
/// references the user left checked. Question/attempt ids and the rest of
/// the private history never enter the payload.
fn dispatch_content(record: &WikiTaskDispatchRecord) -> String {
    let mut content = String::new();
    if !record.title.trim().is_empty() {
        content.push_str(record.title.trim());
        content.push_str("\n\n");
    }
    content.push_str(record.prompt.trim());
    if !record.references.is_empty() {
        content.push_str("\n\nSources:\n");
        for (index, reference) in record.references.iter().enumerate() {
            if index > 0 {
                content.push('\n');
            }
            content.push_str("- ");
            content.push_str(&reference.path);
            content.push(':');
            content.push_str(&reference.start_line.to_string());
            content.push('-');
            content.push_str(&reference.end_line.to_string());
        }
    }
    content
}

/// Build the signed kickoff event. `created_at` is the operation's own
/// creation second — the durable binding that keeps the event id identical
/// across every retry of the same Start.
fn build_signed_dispatch_event(
    record: &WikiTaskDispatchRecord,
    created_at: i64,
    operation_id: &str,
    keys: &nostr::Keys,
    relay_base: &str,
) -> Result<nostr::Event, String> {
    let channel_uuid = uuid::Uuid::parse_str(&record.channel_id)
        .map_err(|_| "dispatch channel id is corrupt".to_string())?;
    let agent = record.agent_pubkey.as_str();
    let client_tags: Vec<Vec<String>> = vec![
        vec![
            "client".to_string(),
            WIKI_TASK_CLIENT_MARKER.to_string(),
            operation_id.to_string(),
        ],
        vec![
            "client".to_string(),
            WIKI_TASK_ORIGIN_MARKER.to_string(),
            record.origin_coordinate.clone(),
        ],
    ];
    let builder = crate::events::build_message_with_client_tags(
        channel_uuid,
        &dispatch_content(record),
        None,
        &[agent],
        &[],
        &[],
        &[],
        &[],
        None,
        relay_base,
        &client_tags,
    )?
    .custom_created_at(nostr::Timestamp::from(
        u64::try_from(created_at).map_err(|_| "dispatch created_at is corrupt")?,
    ));
    builder
        .sign_with_keys(keys)
        .map_err(|error| format!("failed to sign dispatch event: {error}"))
}

/// Live authorization read at handoff: the relay-signed kind:39002 roster is
/// the membership authority, and the chosen member must still be an agent
/// (roster `bot` role or a kind:0 profile carrying a valid OA owner marker).
/// A stale picker selection can never authorize the send.
async fn assert_dispatch_authority(
    state: &AppState,
    relay_url: &str,
    keys: &nostr::Keys,
    channel_id: &str,
    agent_pubkey: &str,
) -> Result<(), String> {
    let relay_pubkey = crate::commands::fetch_relay_self_at(state, relay_url)
        .await?
        .ok_or_else(|| "channel membership authority is unavailable".to_string())?;
    let api_base = relay_http_base_url(relay_url);
    let events = query_relay_at_with_keys(
        state,
        &api_base,
        &[serde_json::json!({
            "kinds": [39002],
            "authors": [relay_pubkey],
            "#d": [channel_id],
            "limit": 1
        })],
        keys,
        None,
    )
    .await?;
    let snapshot =
        crate::commands::channels::channel_membership_snapshot(&events, &relay_pubkey, channel_id)?;
    let members = crate::nostr_convert::channel_members_from_event(snapshot)?.members;
    let signer = keys.public_key().to_hex();
    if !members
        .iter()
        .any(|member| member.pubkey.eq_ignore_ascii_case(&signer))
    {
        return Err("you are not a member of the selected channel".into());
    }
    let member = members
        .iter()
        .find(|member| member.pubkey.eq_ignore_ascii_case(agent_pubkey))
        .ok_or_else(|| "the selected agent is not a channel member".to_string())?;
    if member.is_agent {
        return Ok(());
    }
    let profiles = query_relay_at_with_keys(
        state,
        &api_base,
        &[serde_json::json!({
            "kinds": [0],
            "authors": [agent_pubkey],
            "limit": 1
        })],
        keys,
        None,
    )
    .await
    .unwrap_or_default();
    if profiles
        .iter()
        .any(crate::nostr_convert::profile_has_valid_oa_owner)
    {
        return Ok(());
    }
    Err("the selected member is not an agent".into())
}

/// Read one dispatch row on a blocking worker; the store is sync-only.
async fn load_dispatch_at_path(
    path: &std::path::Path,
    scope: &OperationScope,
    dispatch_id: &str,
) -> Result<Operation, String> {
    let path = path.to_owned();
    let scope = scope.clone();
    let dispatch_id = dispatch_id.to_owned();
    tokio::task::spawn_blocking(move || {
        let store =
            OperationStore::open(&path, Limits::default()).map_err(|error| error.to_string())?;
        store
            .load(&scope, &dispatch_id)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "recovery operation failed".to_string())?
}

/// CAS one field-level mutation into the dispatch record, bounded retries.
async fn update_dispatch_record<R: Runtime>(
    app: &AppHandle<R>,
    path: &std::path::Path,
    expected: &OwnerScopeToken,
    dispatch_id: &str,
    mutate: impl Fn(&mut WikiTaskDispatchRecord) -> (OperationStatus, bool) + Send,
) -> Result<Operation, String> {
    for _ in 0..CAS_RETRIES {
        let operation = load_dispatch_at_path(path, &expected.scope, dispatch_id).await?;
        let mut record = read_record(&operation)?;
        let (status, reconciled) = mutate(&mut record);
        let payload = serde_json::to_value(&record)
            .map_err(|_| "dispatch record serialization failed".to_string())?;
        let result = super::owner_operations::owner_operation_update_at_path(
            app.clone(),
            path.to_owned(),
            expected.clone(),
            dispatch_id.to_owned(),
            operation.revision,
            OperationUpdate {
                status,
                reconciled,
                payload,
            },
            false,
        )
        .await;
        match result {
            Ok(updated) => return Ok(updated.value),
            // Revision moved under us — reload and retry the same mutation.
            Err(error) if error.to_lowercase().contains("conflict") => continue,
            Err(error) => return Err(error),
        }
    }
    Err("dispatch record kept changing; reload".into())
}

fn job_from(operation: &Operation, record: &WikiTaskDispatchRecord) -> WikiTaskDispatchJob {
    WikiTaskDispatchJob {
        dispatch_id: operation.id.clone(),
        event_id: record.event_id.clone(),
        channel_id: record.channel_id.clone(),
        root_event_id: record
            .accepted
            .as_ref()
            .and_then(|_| record.event_id.clone()),
        status: status_name(operation.status),
        accepted: record.accepted.is_some(),
        submit_attempts: record.submit_attempts,
        error: record.last_error.clone(),
    }
}

fn status_name(status: OperationStatus) -> &'static str {
    match status {
        OperationStatus::Preparing => "preparing",
        OperationStatus::Pending => "pending",
        OperationStatus::Reconciling => "reconciling",
        OperationStatus::Failed => "failed",
        OperationStatus::Complete => "complete",
        OperationStatus::Canceled => "canceled",
        OperationStatus::Superseded => "superseded",
    }
}

/// Reserve-or-resume the durable kickoff and persist its exact signed event.
///
/// A retry of the same Start (same draft, same dispatch id, same intent)
/// resolves to the same operation and the same event id. No relay write
/// happens here.
#[tauri::command]
pub(crate) async fn wiki_task_dispatch_prepare(
    app: AppHandle,
    expected: OwnerScopeToken,
    input: WikiTaskDispatchInput,
) -> Result<ScopedOperationResult<WikiTaskDispatchJob>, String> {
    let path = super::owner_operations::journal_path(&app)?;
    wiki_task_dispatch_prepare_at_path(app, path, expected, input).await
}

pub(super) async fn wiki_task_dispatch_prepare_at_path<R: Runtime>(
    app: AppHandle<R>,
    path: std::path::PathBuf,
    expected: OwnerScopeToken,
    input: WikiTaskDispatchInput,
) -> Result<ScopedOperationResult<WikiTaskDispatchJob>, String> {
    validate_input(&input)?;
    let captured = capture(app.clone()).await?;
    if captured.token != expected {
        return Err(OWNER_SCOPE_STALE.into());
    }
    {
        let state = app.state::<AppState>();
        assert_dispatch_authority(
            state.inner(),
            &captured.relay_url,
            &captured.keys,
            &input.channel_id,
            &input.agent_pubkey,
        )
        .await?;
    }

    let scope: OperationScope = captured.token.scope.clone();
    let resource_key = format!("{}{}", RESOURCE_PREFIX, input.draft_key);
    let intent = record_from_input(&input);
    let intent_value = serde_json::to_value(&intent)
        .map_err(|_| "dispatch record serialization failed".to_string())?;
    let new_operation = NewOperation {
        id: input.dispatch_id.clone(),
        kind: OperationKind::ThreadHandoff,
        resource_key,
        payload: intent_value,
    };
    let created = {
        let path = path.clone();
        let scope = scope.clone();
        let now = now_secs()?;
        tokio::task::spawn_blocking(move || {
            let mut store = OperationStore::open(&path, Limits::default())
                .map_err(|error| error.to_string())?;
            store
                .create(&scope, new_operation, now)
                .map_err(|error| error.to_string())
        })
        .await
        .map_err(|_| "recovery operation failed".to_string())??
    };
    let operation = match created {
        CreateResult::Created(operation) | CreateResult::Existing(operation) => operation,
    };

    let mut record = read_record(&operation)?;
    // The kickoff's identity is pinned to the operation's creation second:
    // rebuild deterministically, then persist the exact signed bytes once.
    let relay_base = relay_http_base_url(&captured.relay_url);
    let event = build_signed_dispatch_event(
        &record,
        operation.created_at,
        &operation.id,
        &captured.keys,
        &relay_base,
    )?;
    let event_id = event.id.to_hex();
    let job = match record.event_id.as_deref() {
        Some(existing) if existing == event_id && record.signed_event.is_some() => {
            // Already prepared — the persisted kickoff stays canonical.
            job_from(&operation, &record)
        }
        Some(_) => return Err("dispatch record is corrupt: persisted event id mismatch".into()),
        None => {
            let event_id_for_update = event_id.clone();
            let event_json = serde_json::to_value(&event)
                .map_err(|_| "dispatch record serialization failed".to_string())?;
            let updated =
                update_dispatch_record(&app, &path, &expected, &operation.id, move |record| {
                    record.event_id = Some(event_id_for_update.clone());
                    record.signed_event = Some(event_json.clone());
                    (OperationStatus::Pending, false)
                })
                .await?;
            record = read_record(&updated)?;
            job_from(&updated, &record)
        }
    };
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value: job,
    })
}

/// Publish the persisted signed kickoff. Idempotent: a retry republishes the
/// exact same bytes, which the relay dedupes to the same event id — the one
/// kickoff the user intended. The relay's answer lands in the journal before
/// the UI learns it.
#[tauri::command]
pub(crate) async fn wiki_task_dispatch_submit(
    app: AppHandle,
    expected: OwnerScopeToken,
    dispatch_id: String,
) -> Result<ScopedOperationResult<WikiTaskDispatchJob>, String> {
    let path = super::owner_operations::journal_path(&app)?;
    wiki_task_dispatch_submit_at_path(app, path, expected, dispatch_id).await
}

pub(super) async fn wiki_task_dispatch_submit_at_path<R: Runtime>(
    app: AppHandle<R>,
    path: std::path::PathBuf,
    expected: OwnerScopeToken,
    dispatch_id: String,
) -> Result<ScopedOperationResult<WikiTaskDispatchJob>, String> {
    let captured = capture(app.clone()).await?;
    if captured.token != expected {
        return Err(OWNER_SCOPE_STALE.into());
    }
    let operation = load_dispatch_at_path(&path, &captured.token.scope, &dispatch_id).await?;
    if operation.kind != OperationKind::ThreadHandoff {
        return Err("operation is not a wiki task dispatch".into());
    }
    let record = read_record(&operation)?;
    if record.accepted.is_some() {
        // Already acknowledged — same outcome, no second publish request.
        assert_current(app, &expected).await?;
        return Ok(ScopedOperationResult {
            token: expected,
            value: job_from(&operation, &record),
        });
    }
    // Live recheck: the picker state that chose this agent may be stale.
    {
        let state = app.state::<AppState>();
        if let Err(error) = assert_dispatch_authority(
            state.inner(),
            &captured.relay_url,
            &captured.keys,
            &record.channel_id,
            &record.agent_pubkey,
        )
        .await
        {
            let message = error.clone();
            let updated =
                update_dispatch_record(&app, &path, &expected, &operation.id, move |record| {
                    record.last_error = Some(message.clone());
                    (OperationStatus::Pending, false)
                })
                .await?;
            let fresh = read_record(&updated)?;
            let mut job = job_from(&updated, &fresh);
            job.error = Some(error);
            return Ok(ScopedOperationResult {
                token: expected,
                value: job,
            });
        }
    }

    // The persisted signed bytes are the kickoff; when a crash landed between
    // create and the prepare CAS, rebuild deterministically — the op's
    // creation second binds the same event id either way.
    let signed_event = match record.signed_event.clone() {
        Some(value) => serde_json::from_value::<nostr::Event>(value)
            .map_err(|_| "dispatch record is corrupt: unreadable signed event".to_string())?,
        None => build_signed_dispatch_event(
            &record,
            operation.created_at,
            &operation.id,
            &captured.keys,
            &relay_http_base_url(&captured.relay_url),
        )?,
    };
    if signed_event.pubkey != captured.keys.public_key() {
        return Err("dispatch record is corrupt: signer mismatch".into());
    }
    if record
        .event_id
        .as_deref()
        .is_some_and(|id| id != signed_event.id.to_hex())
    {
        return Err("dispatch record is corrupt: persisted event id mismatch".into());
    }
    if record.signed_event.is_none() {
        let event_id_for_update = signed_event.id.to_hex();
        let event_json = serde_json::to_value(&signed_event)
            .map_err(|_| "dispatch record serialization failed".to_string())?;
        update_dispatch_record(&app, &path, &expected, &operation.id, move |record| {
            record.event_id = Some(event_id_for_update.clone());
            record.signed_event = Some(event_json.clone());
            (OperationStatus::Pending, false)
        })
        .await?;
    }
    let state = app.state::<AppState>();
    let api_base = relay_http_base_url(&captured.relay_url);
    let event_id = signed_event.id.to_hex();
    let outcome = crate::relay::submit_signed_event_at_with_keys(
        &signed_event,
        state.inner(),
        &api_base,
        &captured.keys,
    )
    .await;
    let job = match outcome {
        Ok(result) => {
            let submitted_at = now_secs()?;
            let accepted_event_id = event_id.clone();
            let relay_message = result.message.clone();
            let updated =
                update_dispatch_record(&app, &path, &expected, &operation.id, move |record| {
                    record.submit_attempts = record.submit_attempts.saturating_add(1);
                    record.accepted = Some(DispatchAcceptedRecord {
                        event_id: accepted_event_id.clone(),
                        submitted_at,
                        relay_message: relay_message.clone(),
                    });
                    record.last_error = None;
                    (OperationStatus::Complete, true)
                })
                .await?;
            job_from(&updated, &read_record(&updated)?)
        }
        Err(error) => {
            let message = error.clone();
            let updated =
                update_dispatch_record(&app, &path, &expected, &operation.id, move |record| {
                    record.submit_attempts = record.submit_attempts.saturating_add(1);
                    record.last_error = Some(message.clone());
                    (OperationStatus::Failed, false)
                })
                .await?;
            let fresh = read_record(&updated)?;
            let mut job = job_from(&updated, &fresh);
            job.error = Some(error);
            job
        }
    };
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value: job,
    })
}

/// Resolve an ambiguous outcome by reading the relay for the persisted event
/// id — the crash/lost-ack path that must never mint a second kickoff.
#[tauri::command]
pub(crate) async fn wiki_task_dispatch_reconcile(
    app: AppHandle,
    expected: OwnerScopeToken,
    dispatch_id: String,
) -> Result<ScopedOperationResult<WikiTaskDispatchJob>, String> {
    let path = super::owner_operations::journal_path(&app)?;
    wiki_task_dispatch_reconcile_at_path(app, path, expected, dispatch_id).await
}

pub(super) async fn wiki_task_dispatch_reconcile_at_path<R: Runtime>(
    app: AppHandle<R>,
    path: std::path::PathBuf,
    expected: OwnerScopeToken,
    dispatch_id: String,
) -> Result<ScopedOperationResult<WikiTaskDispatchJob>, String> {
    let captured = capture(app.clone()).await?;
    if captured.token != expected {
        return Err(OWNER_SCOPE_STALE.into());
    }
    let operation = load_dispatch_at_path(&path, &captured.token.scope, &dispatch_id).await?;
    if operation.kind != OperationKind::ThreadHandoff {
        return Err("operation is not a wiki task dispatch".into());
    }
    let record = read_record(&operation)?;
    if record.accepted.is_some() {
        assert_current(app, &expected).await?;
        return Ok(ScopedOperationResult {
            token: expected,
            value: job_from(&operation, &record),
        });
    }
    let Some(event_id) = record.event_id.clone() else {
        return Err("dispatch has no persisted kickoff to reconcile".into());
    };
    let state = app.state::<AppState>();
    let api_base = relay_http_base_url(&captured.relay_url);
    let found = query_relay_at_with_keys(
        state.inner(),
        &api_base,
        &[serde_json::json!({
            "ids": [event_id],
            "kinds": [9],
            "limit": 1
        })],
        &captured.keys,
        None,
    )
    .await?;
    let job = if found.iter().any(|event| event.id.to_hex() == event_id) {
        let submitted_at = now_secs()?;
        let event_id_for_update = event_id.clone();
        let updated =
            update_dispatch_record(&app, &path, &expected, &operation.id, move |record| {
                record.accepted = Some(DispatchAcceptedRecord {
                    event_id: event_id_for_update.clone(),
                    submitted_at,
                    relay_message: "reconciled".into(),
                });
                record.last_error = None;
                (OperationStatus::Complete, true)
            })
            .await?;
        job_from(&updated, &read_record(&updated)?)
    } else {
        job_from(&operation, &record)
    };
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value: job,
    })
}

/// Record user cancellation. Refused once the kickoff was acknowledged —
/// committed work is not retracted. After a publish attempt without an
/// acknowledgement, `force` records the honest tombstone instead of
/// pretending the event cannot exist.
#[tauri::command]
pub(crate) async fn wiki_task_dispatch_abandon(
    app: AppHandle,
    expected: OwnerScopeToken,
    dispatch_id: String,
    force: bool,
) -> Result<ScopedOperationResult<WikiTaskDispatchJob>, String> {
    let path = super::owner_operations::journal_path(&app)?;
    wiki_task_dispatch_abandon_at_path(app, path, expected, dispatch_id, force).await
}

pub(super) async fn wiki_task_dispatch_abandon_at_path<R: Runtime>(
    app: AppHandle<R>,
    path: std::path::PathBuf,
    expected: OwnerScopeToken,
    dispatch_id: String,
    force: bool,
) -> Result<ScopedOperationResult<WikiTaskDispatchJob>, String> {
    let captured = capture(app.clone()).await?;
    if captured.token != expected {
        return Err(OWNER_SCOPE_STALE.into());
    }
    let operation = load_dispatch_at_path(&path, &captured.token.scope, &dispatch_id).await?;
    if operation.kind != OperationKind::ThreadHandoff {
        return Err("operation is not a wiki task dispatch".into());
    }
    let record = read_record(&operation)?;
    if record.accepted.is_some() {
        return Err(
            "the dispatch was already accepted by the relay; it cannot be retracted".into(),
        );
    }
    if record.submit_attempts > 0 && !force {
        return Err(
            "a publish attempt was already made; reconcile the dispatch before abandoning".into(),
        );
    }
    let had_attempts = record.submit_attempts > 0;
    let updated = update_dispatch_record(&app, &path, &expected, &operation.id, move |record| {
        if had_attempts {
            record.abandoned_with_unresolved_publish = true;
        }
        (OperationStatus::Canceled, true)
    })
    .await?;
    assert_current(app, &expected).await?;
    Ok(ScopedOperationResult {
        token: expected,
        value: job_from(&updated, &read_record(&updated)?),
    })
}
