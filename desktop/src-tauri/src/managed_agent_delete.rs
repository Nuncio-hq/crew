//! Durable managed-agent deletion and channel-role cleanup.
//!
//! The existing agent record is the local identity authority and the existing
//! channel Crew operation store is the relay-write authority.  This module is
//! the small coordinator between them: it records the cleanup operation IDs
//! before removing the local record, releases the local locks before any relay
//! work, and keeps the outer record until every channel cleanup is settled.

use crate::{
    app_state::{
        owner_scope::{assert_current, capture, OwnerScopeToken},
        AppState,
    },
    channel_crew_config::{self, CrewSaveResult},
    commands::{
        owner_operation_list, owner_operation_load, owner_operation_update_native,
        run_managed_agent_deletion,
    },
    managed_agents::{
        load_managed_agents, managed_agents_base_dir, save_managed_agents,
        stop_managed_agent_process, sync_managed_agent_processes, try_delete_agent_key,
        BackendKind, ManagedAgentRecord,
    },
    owner_operations::{
        CreateResult, Limits, ManagedAgentDeletionSummary, NewOperation, Operation, OperationKind,
        OperationStatus, OperationStore, OperationSummary, OperationUpdate,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;
use tauri::{AppHandle, Manager};

const VERSION: u32 = 1;
const MAX_CHANNELS: usize = 64;
const CANVAS_PAGE_SIZE: usize = 64;
const MAX_CANVAS_PAGES: usize = 16;
const MAX_ERROR_BYTES: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct RecordFence {
    pubkey: String,
    name: String,
    created_at: String,
    relay_url: String,
    backend_agent_id: Option<String>,
}

impl RecordFence {
    fn capture(record: &ManagedAgentRecord) -> Self {
        Self {
            pubkey: record.pubkey.clone(),
            name: record.name.clone(),
            created_at: record.created_at.clone(),
            relay_url: record.relay_url.clone(),
            backend_agent_id: record.backend_agent_id.clone(),
        }
    }

    fn matches(&self, record: &ManagedAgentRecord) -> bool {
        self == &Self::capture(record)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ChannelCleanup {
    operation_id: String,
    channel_id: String,
    members: Vec<String>,
    settled: bool,
    review_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Payload {
    version: u32,
    fence: RecordFence,
    channels: Vec<ChannelCleanup>,
    local_removed: bool,
    key_removed: bool,
    #[serde(default)]
    tombstone_enqueued: bool,
    failures: u8,
    last_error: Option<String>,
}

impl Payload {
    fn new(fence: RecordFence, channel_ids: Vec<String>, pubkey: &str) -> Result<Self, String> {
        let mut seen = BTreeSet::new();
        let mut channels = Vec::with_capacity(channel_ids.len());
        for channel_id in channel_ids {
            validate_channel_id(&channel_id)?;
            if !seen.insert(channel_id.clone()) {
                return Err("managed-agent cleanup contains a duplicate channel".into());
            }
            channels.push(ChannelCleanup {
                operation_id: uuid::Uuid::new_v4().to_string(),
                channel_id,
                members: vec![pubkey.to_string()],
                settled: false,
                review_required: false,
            });
        }
        if channels.len() > MAX_CHANNELS {
            return Err("managed-agent cleanup exceeds the channel limit".into());
        }
        Ok(Self {
            version: VERSION,
            fence,
            channels,
            local_removed: false,
            key_removed: false,
            tombstone_enqueued: false,
            failures: 0,
            last_error: None,
        })
    }

    fn all_settled(&self) -> bool {
        self.channels
            .iter()
            .all(|channel| channel.settled && !channel.review_required)
    }
}

fn validate_channel_id(value: &str) -> Result<(), String> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| "invalid cleanup channel")?;
    if id.to_string() != value {
        return Err("cleanup channel must be canonical".into());
    }
    Ok(())
}

fn validate_operation(operation: &Operation, _payload: &Payload) -> Result<(), String> {
    crate::owner_operations::validate_managed_agent_delete_record(operation)
        .map_err(|error| error.to_string())
}

fn bounded_error(error: impl AsRef<str>) -> String {
    let mut bounded = String::new();
    for character in error.as_ref().chars() {
        if bounded.len() + character.len_utf8() > MAX_ERROR_BYTES {
            break;
        }
        bounded.push(character);
    }
    bounded
}

fn current_record<R: tauri::Runtime>(
    app: &AppHandle<R>,
    pubkey: &str,
) -> Result<Option<ManagedAgentRecord>, String> {
    let state = app.state::<AppState>();
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(load_managed_agents(app)?
        .into_iter()
        .find(|record| record.pubkey == pubkey))
}

fn validate_delete_target(
    record: &ManagedAgentRecord,
    force_remote_delete: bool,
) -> Result<(), String> {
    if record.backend != BackendKind::Local
        && record.backend_agent_id.is_some()
        && !force_remote_delete
    {
        return Err(
            "cannot delete a deployed remote agent without force_remote_delete: true".into(),
        );
    }
    Ok(())
}

async fn discover_channels(
    app: &AppHandle,
    token: &OwnerScopeToken,
    pubkey: &str,
) -> Result<Vec<String>, String> {
    let agents = crate::commands::revalidate_relay_agents(
        vec![pubkey.to_string()],
        None,
        app.state::<AppState>(),
    )
    .await?;
    assert_current(app.clone(), token).await?;
    let mut ids = BTreeSet::new();
    for agent in agents {
        if agent.pubkey != pubkey {
            continue;
        }
        for channel in agent.channel_ids {
            validate_channel_id(&channel)?;
            ids.insert(channel);
        }
    }

    // Membership is the authorization view, but it is not a complete
    // historical cleanup view: a relay can retain a channel canvas after the
    // agent's membership has been revoked. Walk a bounded composite cursor and
    // add every channel whose signed canvas still names this exact agent. A
    // short page proves exhaustion; exhausting the page budget is an error so
    // an older channel cannot be silently omitted. The cleanup worker then
    // re-checks owner authority and the current head before writing.
    let captured = capture(app.clone()).await?;
    if captured.token != *token {
        return Err(crate::app_state::owner_scope::OWNER_SCOPE_STALE.into());
    }
    let state = app.state::<AppState>();
    let mut filter = json!({"kinds": [40100], "limit": CANVAS_PAGE_SIZE});
    let mut exhausted = false;
    for _ in 0..MAX_CANVAS_PAGES {
        let guard_app = app.clone();
        let guard_token = token.clone();
        let page = crate::commands::channel_crew_transport_query(
            &state,
            token.scope.community.clone(),
            captured.keys.clone(),
            filter.clone(),
            async move { assert_current(guard_app, &guard_token).await },
        )
        .await?;
        if page.len() > CANVAS_PAGE_SIZE {
            return Err("canvas coverage returned an unbounded page".into());
        }
        let page_len = page.len();
        for canvas in &page {
            canvas
                .verify()
                .map_err(|_| "canvas coverage returned an invalid signed event")?;
            if canvas.kind.as_u16() != 40100 {
                return Err("canvas coverage returned an unexpected event kind".into());
            }
            let channel_ids: Vec<String> = canvas
                .tags
                .iter()
                .filter_map(|tag| {
                    let fields = tag.as_slice();
                    (fields.first().map(String::as_str) == Some("h"))
                        .then(|| fields.get(1).cloned())
                        .flatten()
                })
                .collect();
            let [channel_id] = channel_ids.as_slice() else {
                return Err("canvas coverage has an ambiguous channel".into());
            };
            validate_channel_id(channel_id)?;
            let metadata = buzz_core_pkg::crew_role::read_canvas_crew_metadata(
                Some(&canvas.content),
                Some(&canvas.pubkey.to_hex()),
                &token.scope.owner,
            );
            let references_agent = metadata.contact_pubkey.as_deref() == Some(pubkey)
                || metadata.stored_assignments.keys().any(|raw| {
                    nostr::PublicKey::parse(raw.trim()).is_ok_and(|key| key.to_hex() == pubkey)
                })
                // An invalid Crew fence must not turn a retained assignment into
                // an invisible deletion. Keep the channel for explicit review if
                // the exact key is present in the bounded signed content.
                || (metadata.crew_parse_state == "invalid" && canvas.content.contains(pubkey));
            if references_agent {
                ids.insert(channel_id.clone());
            }
            if ids.len() > MAX_CHANNELS {
                return Err("managed-agent channel coverage exceeds the removal limit".into());
            }
        }
        if page_len < CANVAS_PAGE_SIZE {
            exhausted = true;
            break;
        }
        let last = page
            .last()
            .ok_or_else(|| "canvas coverage returned an empty full page".to_string())?;
        filter["until"] = json!(last.created_at.as_secs());
        filter["before_id"] = json!(last.id.to_hex());
    }
    if !exhausted {
        return Err(format!(
            "canvas coverage exceeds the {MAX_CANVAS_PAGES}-page removal scan bound"
        ));
    }
    Ok(ids.into_iter().collect())
}

/// Return whether any unresolved managed-agent deletion exists in the native
/// journal, regardless of the currently selected owner/community scope.
/// Startup and manual starts use this global fence so a workspace switch
/// cannot resurrect an instance whose local deletion is still in flight.
pub(crate) fn has_pending_any_scope(app: &AppHandle, pubkey: &str) -> Result<bool, String> {
    let store = open_journal_store(app)?;
    pending_in_store(&store, pubkey)
}

/// Open the native journal before taking managed-agent runtime locks. Callers
/// that need an atomic spawn/delete fence keep this connection and perform the
/// read or create after acquiring the transition/store locks.
pub(crate) fn open_journal_store(app: &AppHandle) -> Result<OperationStore, String> {
    let path = crate::commands::journal_path(app)?;
    OperationStore::open(&path, Limits::default()).map_err(|error| error.to_string())
}

pub(crate) fn pending_in_store(store: &OperationStore, pubkey: &str) -> Result<bool, String> {
    store
        .managed_agent_delete_is_pending(pubkey)
        .map_err(|error| error.to_string())
}

async fn existing_operation(
    app: &AppHandle,
    token: &OwnerScopeToken,
    pubkey: &str,
) -> Result<Option<Operation>, String> {
    let mut after: Option<String> = None;
    for _ in 0..10 {
        let page = owner_operation_list(app.clone(), token.clone(), after, 100)
            .await?
            .value;
        let done = page.len() < 100;
        after = page.last().map(|summary| summary.id.clone());
        for summary in page {
            if summary.kind == OperationKind::ManagedAgentDelete
                && summary.resource_key == pubkey
                && !summary.reconciled
            {
                return Ok(Some(
                    owner_operation_load(app.clone(), token.clone(), summary.id, None)
                        .await?
                        .value,
                ));
            }
        }
        if done {
            return Ok(None);
        }
    }
    Err("managed-agent deletion recovery scan exceeded its bound".into())
}

async fn persist(
    app: &AppHandle,
    token: &OwnerScopeToken,
    operation: &Operation,
    payload: &Payload,
    status: OperationStatus,
    reconciled: bool,
) -> Result<Operation, String> {
    validate_operation(operation, payload)?;
    let result = owner_operation_update_native(
        app.clone(),
        token.clone(),
        operation.id.clone(),
        operation.revision,
        OperationUpdate {
            status,
            reconciled,
            payload: serde_json::to_value(payload)
                .map_err(|_| "could not encode managed-agent deletion progress")?,
        },
    )
    .await?;
    Ok(result.value)
}

async fn fail(
    app: &AppHandle,
    token: &OwnerScopeToken,
    operation: &Operation,
    payload: &mut Payload,
    error: impl AsRef<str>,
) -> Result<(), String> {
    payload.failures = payload.failures.saturating_add(1).min(5);
    payload.last_error = Some(bounded_error(error));
    let _ = persist(
        app,
        token,
        operation,
        payload,
        OperationStatus::Failed,
        false,
    )
    .await?;
    Ok(())
}

fn local_commit<R: tauri::Runtime>(
    app: &AppHandle<R>,
    payload: &Payload,
) -> Result<(bool, bool), String> {
    let state = app.state::<AppState>();
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let mut records = load_managed_agents(app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|error| error.to_string())?;
    let (changed, exited) = sync_managed_agent_processes(
        &mut records,
        &mut runtimes,
        &crate::managed_agents::current_instance_id(app),
    );
    if changed {
        save_managed_agents(app, &records)?;
    }
    for pubkey in exited {
        state.clear_agent_session_caches(&pubkey);
    }
    let position = records
        .iter()
        .position(|record| record.pubkey == payload.fence.pubkey);
    if let Some(index) = position {
        if !payload.fence.matches(&records[index]) {
            return Err("managed agent changed before deletion; review required".into());
        }
        let base_dir = managed_agents_base_dir(app)?;
        run_managed_agent_deletion(&base_dir, &payload.fence.pubkey, &mut records, |records| {
            let record = records
                .iter_mut()
                .find(|record| record.pubkey == payload.fence.pubkey)
                .ok_or_else(|| "managed agent disappeared before deletion".to_string())?;
            stop_managed_agent_process(app, record, &mut runtimes)?;
            state.clear_agent_session_caches(&payload.fence.pubkey);
            records.retain(|record| record.pubkey != payload.fence.pubkey);
            save_managed_agents(app, records)
        })?;
    }
    // Key and retention cleanup happen after this local commit. A crash here
    // leaves the outer operation pending and the exact steps are retried.
    Ok((true, false))
}

async fn resume(
    app: AppHandle,
    token: OwnerScopeToken,
    mut operation: Operation,
    manual: bool,
) -> Result<(), String> {
    let mut payload: Payload = serde_json::from_value(operation.payload.clone())
        .map_err(|_| "invalid managed-agent deletion record")?;
    validate_operation(&operation, &payload)?;
    if operation.reconciled {
        return Ok(());
    }
    if payload.failures >= 5 && !manual {
        return Err("managed-agent deletion requires manual retry".into());
    }
    if manual {
        payload.failures = 0;
        payload.last_error = None;
    }
    operation = persist(
        &app,
        &token,
        &operation,
        &payload,
        OperationStatus::Reconciling,
        false,
    )
    .await?;

    if !payload.local_removed {
        match current_record(&app, &payload.fence.pubkey)? {
            Some(record) if !payload.fence.matches(&record) => {
                fail(
                    &app,
                    &token,
                    &operation,
                    &mut payload,
                    "managed agent was replaced before its deletion committed",
                )
                .await?;
                return Err("managed agent was replaced before deletion; review required".into());
            }
            Some(_) => match local_commit(&app, &payload) {
                Ok((removed, key_removed)) => {
                    payload.local_removed = removed;
                    payload.key_removed = key_removed;
                }
                Err(error) => {
                    fail(&app, &token, &operation, &mut payload, &error).await?;
                    return Err(error);
                }
            },
            None => {
                // The local write may have committed immediately before a
                // process crash. Treat the absent exact record as committed;
                // a re-created record is fenced by the branch above.
                payload.local_removed = true;
            }
        }
        operation = persist(
            &app,
            &token,
            &operation,
            &payload,
            OperationStatus::Reconciling,
            false,
        )
        .await?;
    }

    if !payload.key_removed {
        match try_delete_agent_key(&payload.fence.pubkey) {
            Ok(()) => {
                payload.key_removed = true;
            }
            Err(error) => {
                fail(&app, &token, &operation, &mut payload, &error).await?;
                return Err(format!(
                    "agent removed; key cleanup pending: {}",
                    bounded_error(error)
                ));
            }
        }
        operation = persist(
            &app,
            &token,
            &operation,
            &payload,
            OperationStatus::Reconciling,
            false,
        )
        .await?;
    }

    if !payload.tombstone_enqueued {
        match enqueue_tombstone_for_scope(&app, &token, &operation, &payload.fence.pubkey).await {
            Ok(()) => payload.tombstone_enqueued = true,
            Err(error) => {
                fail(&app, &token, &operation, &mut payload, &error).await?;
                return Err(format!(
                    "agent removed; tombstone enqueue pending: {}",
                    bounded_error(error)
                ));
            }
        }
        operation = persist(
            &app,
            &token,
            &operation,
            &payload,
            OperationStatus::Reconciling,
            false,
        )
        .await?;
    }

    for index in 0..payload.channels.len() {
        if payload.channels[index].settled || (payload.channels[index].review_required && !manual) {
            continue;
        }
        // A manual retry is an explicit request to re-read the current canvas
        // head and attempt the same member removal against that head. Automatic
        // recovery never clears review state on its own.
        if manual {
            payload.channels[index].review_required = false;
        }
        let cleanup = payload.channels[index].clone();
        let result = channel_crew_config::save_channel_crew_member_cleanup(
            app.clone(),
            token.clone(),
            cleanup.operation_id,
            cleanup.channel_id,
            None,
            cleanup.members,
            manual,
        )
        .await;
        match result {
            Ok(result) => match result.value {
                CrewSaveResult::Unchanged { .. } => payload.channels[index].settled = true,
                CrewSaveResult::Saved { progress }
                    if progress.reconciled
                        && progress.outcome
                            == crate::channel_crew_config::CrewSaveOutcome::Applied =>
                {
                    payload.channels[index].settled = true;
                }
                CrewSaveResult::Saved { progress }
                    if progress.reconciled
                        && progress.outcome
                            == crate::channel_crew_config::CrewSaveOutcome::Superseded =>
                {
                    payload.channels[index].review_required = true;
                    payload.last_error = Some(
                        "channel changed while agent cleanup was in flight; review required".into(),
                    );
                }
                CrewSaveResult::Conflict { .. } | CrewSaveResult::ReviewRequired { .. } => {
                    payload.channels[index].review_required = true;
                    payload.last_error = Some("channel cleanup needs review".into());
                }
                CrewSaveResult::RecoveryPending { .. } | CrewSaveResult::Saved { .. } => {
                    payload.last_error = Some("channel cleanup is pending retry".into());
                }
            },
            Err(error) => {
                payload.last_error = Some(bounded_error(&error));
            }
        }
        operation = persist(
            &app,
            &token,
            &operation,
            &payload,
            OperationStatus::Reconciling,
            false,
        )
        .await?;
        if !payload.channels[index].settled {
            let error = payload
                .last_error
                .clone()
                .unwrap_or_else(|| "channel cleanup pending".into());
            fail(&app, &token, &operation, &mut payload, &error).await?;
            return Err(format!("agent removed; channel cleanup pending: {error}"));
        }
    }

    if payload.all_settled() {
        payload.last_error = None;
        let _ = persist(
            &app,
            &token,
            &operation,
            &payload,
            OperationStatus::Complete,
            true,
        )
        .await?;
        Ok(())
    } else {
        let error = payload
            .last_error
            .clone()
            .unwrap_or_else(|| "channel cleanup requires review".into());
        fail(&app, &token, &operation, &mut payload, &error).await?;
        Err(format!("agent removed; channel cleanup pending: {error}"))
    }
}

fn create_native_claim(
    store: &mut OperationStore,
    scope: &crate::owner_operations::OperationScope,
    operation: NewOperation,
) -> Result<CreateResult, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system clock unavailable".to_string())?
        .as_secs();
    let now = i64::try_from(now).map_err(|_| "system clock unavailable".to_string())?;
    store
        .create(scope, operation, now)
        .map_err(|error| error.to_string())
}

async fn begin(
    app: &AppHandle,
    token: OwnerScopeToken,
    record: ManagedAgentRecord,
    force_remote_delete: bool,
) -> Result<Operation, String> {
    validate_delete_target(&record, force_remote_delete)?;
    // Provider deployment and deletion share one per-agent async lock. If a
    // provider call is already in flight, wait for it to persist its receipt
    // before capturing the deletion fence. Once this function claims the
    // journal row, later provider starts observe the pending-delete fence.
    let provider_guard = if record.backend != BackendKind::Local {
        Some(
            crate::commands::acquire_provider_deploy_lock(&app.state::<AppState>(), &record.pubkey)
                .await?,
        )
    } else {
        None
    };
    let record = if provider_guard.is_some() {
        current_record(app, &record.pubkey)?
            .ok_or_else(|| format!("agent {} not found", record.pubkey))?
    } else {
        record
    };
    validate_delete_target(&record, force_remote_delete)?;
    if let Some(operation) = existing_operation(app, &token, &record.pubkey).await? {
        let payload: Payload = serde_json::from_value(operation.payload.clone())
            .map_err(|_| "invalid managed-agent deletion record")?;
        validate_operation(&operation, &payload)?;
        if !payload.fence.matches(&record) {
            return Err("a deletion for an earlier managed-agent instance needs review".into());
        }
        return Ok(operation);
    }
    let channels = discover_channels(app, &token, &record.pubkey).await?;
    let payload = Payload::new(RecordFence::capture(&record), channels, &record.pubkey)?;
    // A spawn path and this final delete claim use the same transition →
    // managed-store lock order. The journal connection is opened before those
    // locks so SQLite setup cannot invert the lock order.
    assert_current(app.clone(), &token).await?;
    let mut journal = open_journal_store(app)?;
    let state = app.state::<AppState>();
    let _transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|error| error.to_string())?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let current = load_managed_agents(app)?
        .into_iter()
        .find(|candidate| candidate.pubkey == record.pubkey)
        .ok_or_else(|| format!("agent {} not found", record.pubkey))?;
    if !RecordFence::capture(&current).matches(&record) {
        return Err("managed agent changed before deletion; retry the deletion".into());
    }
    validate_delete_target(&current, force_remote_delete)?;
    let result = create_native_claim(
        &mut journal,
        &token.scope,
        NewOperation {
            id: uuid::Uuid::new_v4().to_string(),
            kind: OperationKind::ManagedAgentDelete,
            resource_key: record.pubkey,
            payload: serde_json::to_value(payload)
                .map_err(|_| "could not encode managed-agent deletion record")?,
        },
    )?;
    drop(_store);
    drop(_transition);
    assert_current(app.clone(), &token).await?;
    match result {
        CreateResult::Created(operation) | CreateResult::Existing(operation) => Ok(operation),
    }
}

/// Delete one exact managed instance through the durable removal coordinator.
pub(crate) async fn delete(
    app: AppHandle,
    pubkey: String,
    force_remote_delete: bool,
) -> Result<(), String> {
    let token = capture(app.clone()).await?.token;
    let record =
        current_record(&app, &pubkey)?.ok_or_else(|| format!("agent {pubkey} not found"))?;
    let operation = begin(&app, token.clone(), record, force_remote_delete).await?;
    match resume(app.clone(), token, operation, true).await {
        Ok(()) => {
            crate::managed_agents::try_regenerate_nest(&app);
            Ok(())
        }
        Err(error) => Err(error),
    }
}

async fn enqueue_tombstone_for_scope(
    app: &AppHandle,
    token: &OwnerScopeToken,
    operation: &Operation,
    pubkey: &str,
) -> Result<(), String> {
    assert_current(app.clone(), token).await?;
    if operation.scope != token.scope {
        return Err("managed-agent deletion scope changed; review required".into());
    }
    let captured = capture(app.clone()).await?;
    if captured.token != *token || captured.token.scope.owner != operation.scope.owner {
        return Err(crate::app_state::owner_scope::OWNER_SCOPE_STALE.into());
    }
    let base_dir = managed_agents_base_dir(app)?;
    let db_path = crate::managed_agents::retention::scoped_retention_db_path(
        &base_dir,
        &operation.scope.community,
        &operation.scope.owner,
    );
    crate::commands::tombstone_managed_agent_at(&db_path, &captured.keys, pubkey)
}

/// Replay one unresolved deletion by its durable operation ID. This is the
/// explicit manual retry affordance after startup recovery reports a failure.
pub(crate) async fn retry(app: AppHandle, operation_id: String) -> Result<(), String> {
    let token = capture(app.clone()).await?.token;
    let operation = load_any_scope_operation(&app, &operation_id).await?;
    assert_current(app.clone(), &token).await?;
    if operation.kind != OperationKind::ManagedAgentDelete {
        return Err("operation is not a managed-agent deletion".into());
    }
    ensure_active_scope(&token, &operation)?;
    resume(app, token, operation, true).await
}

/// List unresolved managed-agent deletions across every local scope. The
/// journal returns redacted summaries; loading a payload still requires the
/// active scope to match the operation's captured owner/community.
pub(crate) async fn list(app: AppHandle) -> Result<Vec<ManagedAgentDeletionSummary>, String> {
    let path = crate::commands::journal_path(&app)?;
    tokio::task::spawn_blocking(move || {
        let store =
            OperationStore::open(&path, Limits::default()).map_err(|error| error.to_string())?;
        store
            .list_managed_agent_deletions()
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "managed-agent deletion list failed".to_string())?
}

/// Load one deletion record for a native status/review surface.
pub(crate) async fn status(app: AppHandle, operation_id: String) -> Result<Operation, String> {
    let token = capture(app.clone()).await?.token;
    let operation = load_any_scope_operation(&app, &operation_id).await?;
    assert_current(app.clone(), &token).await?;
    if operation.kind != OperationKind::ManagedAgentDelete {
        return Err("operation is not a managed-agent deletion".into());
    }
    ensure_active_scope(&token, &operation)?;
    let payload: Payload = serde_json::from_value(operation.payload.clone())
        .map_err(|_| "invalid managed-agent deletion record")?;
    validate_operation(&operation, &payload)?;
    Ok(operation)
}

fn ensure_active_scope(token: &OwnerScopeToken, operation: &Operation) -> Result<(), String> {
    if operation.scope != token.scope {
        return Err(format!(
            "managed-agent deletion belongs to community {}; switch to that workspace before retrying",
            operation.scope.community
        ));
    }
    Ok(())
}

async fn load_any_scope_operation(
    app: &AppHandle,
    operation_id: &str,
) -> Result<Operation, String> {
    let path = crate::commands::journal_path(app)?;
    let operation_id = operation_id.to_string();
    tokio::task::spawn_blocking(move || {
        let store =
            OperationStore::open(&path, Limits::default()).map_err(|error| error.to_string())?;
        store
            .load_managed_agent_delete_any_scope(&operation_id)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "managed-agent deletion status failed".to_string())?
}

/// Replay unresolved deletion records after the active owner/workspace is
/// restored. One bounded pass is intentional; the channel worker owns its own
/// backoff, and a later workspace apply retries this outer record.
pub(crate) async fn recover(app: &AppHandle) -> Result<(), String> {
    let token = capture(app.clone()).await?.token;
    let mut after: Option<String> = None;
    let mut pending = Vec::new();
    let mut complete = false;
    for _ in 0..10 {
        let page = owner_operation_list(app.clone(), token.clone(), after, 100)
            .await?
            .value;
        let done = page.len() < 100;
        after = page.last().map(|summary| summary.id.clone());
        pending.extend(page.into_iter().filter(|summary: &OperationSummary| {
            summary.kind == OperationKind::ManagedAgentDelete && !summary.reconciled
        }));
        if done {
            complete = true;
            break;
        }
    }
    if !complete {
        return Err("managed-agent deletion recovery exceeded its bound".into());
    }
    let mut first_error = None;
    for summary in pending {
        let operation = owner_operation_load(app.clone(), token.clone(), summary.id, None)
            .await?
            .value;
        if let Err(error) = resume(app.clone(), token.clone(), operation, false).await {
            eprintln!("buzz-desktop: managed-agent deletion recovery: {error}");
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::owner_operations::{OperationScope, OperationStatus};

    fn operation(payload: &Payload) -> Operation {
        Operation {
            version: 1,
            scope: OperationScope {
                owner: "a".repeat(64),
                community: "https://example.com".into(),
            },
            id: "00000000-0000-0000-0000-000000000001".into(),
            kind: OperationKind::ManagedAgentDelete,
            resource_key: payload.fence.pubkey.clone(),
            revision: 0,
            created_at: 1,
            updated_at: 1,
            status: OperationStatus::Preparing,
            reconciled: false,
            payload: serde_json::to_value(payload).unwrap(),
        }
    }

    fn payload() -> Payload {
        Payload::new(
            RecordFence {
                pubkey: "b".repeat(64),
                name: "agent".into(),
                created_at: "created".into(),
                relay_url: "wss://relay.example".into(),
                backend_agent_id: None,
            },
            vec!["00000000-0000-0000-0000-000000000002".into()],
            &"b".repeat(64),
        )
        .unwrap()
    }

    #[test]
    fn journal_records_channel_operation_before_local_removal() {
        let payload = payload();
        assert!(!payload.local_removed);
        assert_eq!(payload.channels.len(), 1);
        assert!(!payload.channels[0].operation_id.is_empty());
        validate_operation(&operation(&payload), &payload).unwrap();
    }

    #[test]
    fn journal_rejects_duplicate_channel_and_key_before_local_removal() {
        let mut duplicate_payload = payload();
        duplicate_payload
            .channels
            .push(duplicate_payload.channels[0].clone());
        assert!(validate_operation(&operation(&duplicate_payload), &duplicate_payload).is_err());
        let mut key_payload = payload();
        key_payload.key_removed = true;
        assert!(validate_operation(&operation(&key_payload), &key_payload).is_err());
    }

    #[test]
    fn review_state_never_counts_as_settled() {
        let mut payload = payload();
        payload.channels[0].review_required = true;
        assert!(!payload.all_settled());
    }

    #[test]
    fn tombstone_progress_is_fenced_after_key_cleanup() {
        let mut payload = payload();
        payload.tombstone_enqueued = true;
        assert!(validate_operation(&operation(&payload), &payload).is_err());

        payload.key_removed = true;
        assert!(validate_operation(&operation(&payload), &payload).is_err());

        payload.local_removed = true;
        assert!(validate_operation(&operation(&payload), &payload).is_ok());
    }

    #[test]
    fn older_records_default_tombstone_progress_to_pending() {
        let payload = payload();
        let mut encoded = serde_json::to_value(&payload).unwrap();
        encoded
            .as_object_mut()
            .unwrap()
            .remove("tombstone_enqueued");
        let decoded: Payload = serde_json::from_value(encoded).unwrap();
        assert!(!decoded.tombstone_enqueued);
    }

    #[test]
    fn error_bound_is_utf8_byte_safe() {
        let bounded = bounded_error("é".repeat(MAX_ERROR_BYTES));
        assert!(bounded.len() <= MAX_ERROR_BYTES);
        assert!(std::str::from_utf8(bounded.as_bytes()).is_ok());
    }
}
