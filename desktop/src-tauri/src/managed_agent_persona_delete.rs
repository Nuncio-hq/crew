//! Durable persona deletion coordinator and exact linked-record cascade.
//!
//! This module is deliberately separate from direct managed-agent deletion so
//! the upstream-sized command module remains within Crew's file-size gate.

use super::*;
use crate::{
    app_state::{
        owner_scope::{assert_current, assert_current_blocking, capture, OwnerScopeToken},
        AppState,
    },
    managed_agents::{
        load_personas, load_teams, save_personas, try_regenerate_nest, validate_persona_deletion,
        AgentDefinition, BackendKind, ManagedAgentRecord,
    },
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const MAX_CASCADE_TARGETS: usize = 15;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersonaFence {
    id: String,
    d_tag: String,
    created_at: String,
    updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CascadeTarget {
    operation_id: String,
    persona_id: String,
    fence: RecordFence,
    channels: Vec<ChannelCleanup>,
    local_removed: bool,
    key_removed: bool,
    tombstone_enqueued: bool,
    failures: u8,
    last_error: Option<String>,
    #[serde(default)]
    settled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct CascadePayload {
    persona: PersonaFence,
    targets: Vec<CascadeTarget>,
    /// A zero-target coordinator still owns the persona deletion and its
    /// tombstone retry. It uses a synthetic, deterministic resource fence.
    #[serde(default)]
    coordinator_only: bool,
    #[serde(default)]
    persona_removed: bool,
}

impl CascadeTarget {
    fn from_payload(operation_id: String, persona_id: String, payload: &Payload) -> Self {
        Self {
            operation_id,
            persona_id,
            fence: payload.fence.clone(),
            channels: payload.channels.clone(),
            local_removed: payload.local_removed,
            key_removed: payload.key_removed,
            tombstone_enqueued: payload.tombstone_enqueued,
            failures: payload.failures,
            last_error: payload.last_error.clone(),
            settled: false,
        }
    }

    fn to_payload(&self, parent_id: Option<String>) -> Payload {
        Payload {
            version: VERSION,
            fence: self.fence.clone(),
            channels: self.channels.clone(),
            local_removed: self.local_removed,
            key_removed: self.key_removed,
            tombstone_enqueued: self.tombstone_enqueued,
            failures: self.failures,
            last_error: self.last_error.clone(),
            cascade_parent: parent_id,
            cascade_persona_id: Some(self.persona_id.clone()),
            cascade: None,
        }
    }

    fn apply_operation(&mut self, operation: &Operation, parent_id: &str) -> Result<(), String> {
        let payload: Payload = serde_json::from_value(operation.payload.clone())
            .map_err(|_| "invalid managed-agent deletion record".to_string())?;
        if payload.cascade.is_some()
            || payload.cascade_parent.as_deref() != Some(parent_id)
            || payload.cascade_persona_id.as_deref() != Some(self.persona_id.as_str())
            || payload.fence != self.fence
        {
            return Err("managed-agent cascade child fence changed; review required".into());
        }
        self.channels = payload.channels;
        self.local_removed = payload.local_removed;
        self.key_removed = payload.key_removed;
        self.tombstone_enqueued = payload.tombstone_enqueued;
        self.failures = payload.failures;
        self.last_error = payload.last_error;
        self.settled = operation.reconciled;
        Ok(())
    }
}

impl PersonaFence {
    fn capture(persona: &crate::managed_agents::AgentDefinition) -> Self {
        Self {
            id: persona.id.clone(),
            d_tag: crate::managed_agents::persona_events::persona_d_tag(persona),
            created_at: persona.created_at.clone(),
            updated_at: persona.updated_at.clone(),
        }
    }

    fn matches(&self, persona: &crate::managed_agents::AgentDefinition) -> bool {
        self == &Self::capture(persona)
    }
}

impl Payload {
    fn sync_from_target(&mut self, target: &CascadeTarget) {
        self.fence = target.fence.clone();
        self.channels = target.channels.clone();
        self.local_removed = target.local_removed;
        self.key_removed = target.key_removed;
        self.tombstone_enqueued = target.tombstone_enqueued;
        self.failures = target.failures;
        self.last_error = target.last_error.clone();
    }
}

fn persona_cascade_operation_id(persona_id: &str) -> String {
    uuid::Uuid::new_v5(
        &crate::owner_operations::PERSONA_CASCADE_NAMESPACE,
        format!("persona:{persona_id}").as_bytes(),
    )
    .to_string()
}

fn persona_cascade_child_id(parent_id: &str, pubkey: &str) -> String {
    uuid::Uuid::new_v5(
        &crate::owner_operations::PERSONA_CASCADE_NAMESPACE,
        format!("child:{parent_id}:{pubkey}").as_bytes(),
    )
    .to_string()
}

fn native_now() -> Result<i64, String> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system clock unavailable".to_string())?
        .as_secs();
    i64::try_from(seconds).map_err(|_| "system clock unavailable".to_string())
}

fn persona_snapshot<R: tauri::Runtime>(
    app: &AppHandle<R>,
    persona_id: &str,
) -> Result<(AgentDefinition, Vec<ManagedAgentRecord>), String> {
    let state = app.state::<AppState>();
    let _transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|error| error.to_string())?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let personas = load_personas(app)?;
    let persona = personas
        .iter()
        .find(|record| record.id == persona_id)
        .cloned()
        .ok_or_else(|| format!("persona {persona_id} not found"))?;
    let referenced_by_team = load_teams(app)?.iter().any(|team| {
        team.persona_ids
            .iter()
            .any(|candidate| candidate == persona_id)
    });
    validate_persona_deletion(&persona, referenced_by_team)?;
    let records = load_managed_agents(app)?;
    let targets = records
        .into_iter()
        .filter(|record| record.persona_id.as_deref() == Some(persona_id))
        .collect();
    Ok((persona, targets))
}

fn revalidate_persona_targets_locked<R: tauri::Runtime>(
    app: &AppHandle<R>,
    persona_fence: &PersonaFence,
    expected_targets: &[ManagedAgentRecord],
) -> Result<(AgentDefinition, Vec<ManagedAgentRecord>), String> {
    let personas = load_personas(app)?;
    let persona = personas
        .iter()
        .find(|record| record.id == persona_fence.id)
        .cloned()
        .ok_or_else(|| format!("persona {} not found", persona_fence.id))?;
    if !persona_fence.matches(&persona) {
        return Err("persona changed while deletion was being prepared; retry".into());
    }
    let referenced_by_team = load_teams(app)?.iter().any(|team| {
        team.persona_ids
            .iter()
            .any(|candidate| candidate == &persona_fence.id)
    });
    validate_persona_deletion(&persona, referenced_by_team)?;
    let records = load_managed_agents(app)?;
    if records
        .iter()
        .filter(|record| record.persona_id.as_deref() == Some(persona_fence.id.as_str()))
        .count()
        != expected_targets.len()
    {
        return Err(
            "new managed-agent link appeared while deletion was being prepared; retry".into(),
        );
    }
    for expected in expected_targets {
        let Some(current) = records
            .iter()
            .find(|record| record.pubkey == expected.pubkey)
        else {
            return Err(
                "managed agent disappeared while deletion was being prepared; retry".into(),
            );
        };
        if RecordFence::capture(current) != RecordFence::capture(expected)
            || current.persona_id.as_deref() != Some(persona_fence.id.as_str())
        {
            return Err("managed agent changed while deletion was being prepared; retry".into());
        }
        validate_delete_target(current, false)?;
    }
    Ok((persona, records))
}

fn revalidate_persona_targets<R: tauri::Runtime>(
    app: &AppHandle<R>,
    persona_fence: &PersonaFence,
    expected_targets: &[ManagedAgentRecord],
) -> Result<(AgentDefinition, Vec<ManagedAgentRecord>), String> {
    let state = app.state::<AppState>();
    let _transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|error| error.to_string())?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    revalidate_persona_targets_locked(app, persona_fence, expected_targets)
}

async fn finalize_persona_delete<R: tauri::Runtime>(
    app: &AppHandle<R>,
    token: &OwnerScopeToken,
    persona: &PersonaFence,
    targets: &[CascadeTarget],
) -> Result<(), String> {
    // Capture the signing owner and relay before the store mutation.  The
    // captured scope is used for the tombstone after the locks are released;
    // no owner capture or async work occurs while either native mutex is held.
    let captured = capture(app.clone()).await?;
    if captured.token != *token {
        return Err(crate::app_state::owner_scope::OWNER_SCOPE_STALE.into());
    }
    {
        let state = app.state::<AppState>();
        let _transition = state
            .managed_agent_runtime_transition
            .lock()
            .map_err(|error| error.to_string())?;
        let _store = state
            .managed_agents_store_lock
            .lock()
            .map_err(|error| error.to_string())?;
        assert_current_blocking(app.clone(), token)?;
        let personas = load_personas(app)?;
        let records = load_managed_agents(app)?;
        // Check these fences even when the persona row is already absent. A
        // crash after our local persona commit, followed by a concurrent
        // import or link, must not let the retry silently leave a newly
        // linked/replaced managed record behind.
        if records
            .iter()
            .any(|record| record.persona_id.as_deref() == Some(persona.id.as_str()))
        {
            return Err(
                "a linked managed agent still exists or changed during persona deletion; retry"
                    .into(),
            );
        }
        if targets.iter().any(|target| {
            records
                .iter()
                .any(|record| record.pubkey == target.fence.pubkey)
        }) {
            return Err(
                "a prepared managed agent was replaced before persona deletion; retry".into(),
            );
        }
        let referenced_by_team = load_teams(app)?.iter().any(|team| {
            team.persona_ids
                .iter()
                .any(|candidate| candidate == &persona.id)
        });
        if let Some(current) = personas.iter().find(|record| record.id == persona.id) {
            if !persona.matches(current) {
                return Err("persona changed before final deletion; retry".into());
            }
            validate_persona_deletion(current, referenced_by_team)?;
            assert_current_blocking(app.clone(), token)?;
            let mut remaining = personas;
            remaining.retain(|record| record.id != persona.id);
            save_personas(app, &remaining)?;
        } else if referenced_by_team {
            // A retry after the persona file commit can still observe a
            // dangling team reference, for example from an inbound team
            // update. Keep the durable coordinator unresolved until that
            // reference is repaired instead of silently tombstoning a
            // coordinate that the team still names.
            return Err("persona is still referenced by a team; retry".into());
        } else {
            // A crash after the persona file commit but before the coordinator
            // CAS is safe once the global linked/replacement checks above have
            // passed: the parent operation remains the retry witness.
        }
    }

    let base_dir = managed_agents_base_dir(app)?;
    // Recovery may precede retention hydration on a fresh installation.
    std::fs::create_dir_all(base_dir.join("retention"))
        .map_err(|error| format!("failed to create retention scope directory: {error}"))?;
    let db_path = crate::managed_agents::retention::scoped_retention_db_path(
        &base_dir,
        &captured.relay_url,
        &captured.keys.public_key().to_hex(),
    );
    crate::commands::tombstone_persona_at(&db_path, &captured.keys, &persona.d_tag)?;
    assert_current(app.clone(), token).await
}

pub(super) async fn resume_persona_cascade<R: tauri::Runtime>(
    app: AppHandle<R>,
    token: OwnerScopeToken,
    mut operation: Operation,
    manual: bool,
) -> Result<(), String> {
    let mut payload: Payload = serde_json::from_value(operation.payload.clone())
        .map_err(|_| "invalid persona deletion coordinator record")?;
    validate_operation(&operation, &payload)?;
    if operation.reconciled {
        return payload
            .cascade
            .as_ref()
            .filter(|cascade| cascade.persona_removed)
            .map(|_| ())
            .ok_or_else(|| "completed persona deletion has no removal witness".into());
    }
    if payload.failures >= 5 && !manual {
        return Err("persona deletion requires manual retry".into());
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

    let target_count = payload
        .cascade
        .as_ref()
        .map(|cascade| cascade.targets.len())
        .ok_or_else(|| "persona deletion coordinator has no target set".to_string())?;
    for index in 0..target_count {
        let target = payload
            .cascade
            .as_ref()
            .and_then(|cascade| cascade.targets.get(index))
            .cloned()
            .ok_or_else(|| "persona deletion target disappeared from its journal".to_string())?;
        if target.settled {
            continue;
        }

        let mut child = match load_scope_operation(&app, &token, &target.operation_id)? {
            Some(operation) => operation,
            None => create_cascade_child(&app, &token, &operation.id, &target)?,
        };
        assert_current(app.clone(), &token).await?;
        let mut updated_target = target;
        let child_result = if child.reconciled {
            updated_target.apply_operation(&child, &operation.id)
        } else {
            match Box::pin(resume(app.clone(), token.clone(), child.clone(), manual)).await {
                Ok(()) => {
                    child = load_scope_operation(&app, &token, &updated_target.operation_id)?
                        .unwrap_or(child);
                    if child.reconciled {
                        updated_target.apply_operation(&child, &operation.id)
                    } else {
                        Err("managed-agent child returned without durable completion".into())
                    }
                }
                Err(error) => {
                    if let Some(latest) =
                        load_scope_operation(&app, &token, &updated_target.operation_id)?
                    {
                        updated_target.apply_operation(&latest, &operation.id)?;
                    }
                    updated_target.last_error = Some(bounded_error(&error));
                    Err(error)
                }
            }
        };
        if let Err(error) = child_result {
            if error == crate::app_state::owner_scope::OWNER_SCOPE_STALE {
                return Err(error);
            }
            if let Some(cascade) = payload.cascade.as_mut() {
                cascade.targets[index] = updated_target;
            }
            let first_target = payload
                .cascade
                .as_ref()
                .and_then(|cascade| cascade.targets.first())
                .cloned();
            if let Some(target) = first_target.as_ref() {
                // The coordinator's top-level managed-delete fields are an
                // anchored compatibility projection of its first target.
                // Copying a later target here would make the parent fence
                // disagree with `validate_cascade` and strand every
                // multi-target persona deletion at its second child.
                payload.sync_from_target(target);
            }
            fail(&app, &token, &operation, &mut payload, &error).await?;
            return Err(error);
        }

        if let Some(cascade) = payload.cascade.as_mut() {
            cascade.targets[index] = updated_target;
        }
        let first_target = payload
            .cascade
            .as_ref()
            .and_then(|cascade| cascade.targets.first())
            .cloned();
        if let Some(target) = first_target.as_ref() {
            // Keep the coordinator fence anchored to its first prepared
            // target; each child's full progress lives in `cascade.targets`.
            payload.sync_from_target(target);
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

    let (persona, targets) = {
        let cascade = payload
            .cascade
            .as_ref()
            .ok_or_else(|| "persona deletion coordinator has no cascade".to_string())?;
        if !cascade.targets.iter().all(|target| target.settled) {
            return Err("persona deletion has unresolved managed-agent targets".into());
        }
        (cascade.persona.clone(), cascade.targets.clone())
    };
    if !payload
        .cascade
        .as_ref()
        .is_some_and(|cascade| cascade.persona_removed)
    {
        if let Err(error) = finalize_persona_delete(&app, &token, &persona, &targets).await {
            if error == crate::app_state::owner_scope::OWNER_SCOPE_STALE {
                return Err(error);
            }
            fail(&app, &token, &operation, &mut payload, &error).await?;
            return Err(error);
        }
        if let Some(cascade) = payload.cascade.as_mut() {
            cascade.persona_removed = true;
        }
        if target_count == 0 {
            // A coordinator with no managed children still has three durable
            // completion steps: persona removal, its tombstone enqueue, and
            // the terminal journal CAS. Mark the synthetic projection only
            // after the first two have succeeded so a crash remains retryable.
            payload.local_removed = true;
            payload.key_removed = true;
            payload.tombstone_enqueued = true;
        } else if let Some(target) = payload
            .cascade
            .as_ref()
            .and_then(|cascade| cascade.targets.first())
            .cloned()
        {
            payload.sync_from_target(&target);
        }
    }
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
}

fn load_scope_operation<R: tauri::Runtime>(
    app: &AppHandle<R>,
    token: &OwnerScopeToken,
    operation_id: &str,
) -> Result<Option<Operation>, String> {
    let journal = open_journal_store(app)?;
    match journal.load(&token.scope, operation_id) {
        Ok(operation) => Ok(Some(operation)),
        Err(crate::owner_operations::StoreError::Missing) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn child_operation(parent_id: &str, target: &CascadeTarget) -> Result<NewOperation, String> {
    let mut payload = target.to_payload(Some(parent_id.to_string()));
    // A child row is an unresolved work item even when the parent is being
    // reconstructed from a progress snapshot after the terminal child was
    // trimmed.  Its flags still describe the already-observed side effects.
    payload.cascade_parent = Some(parent_id.to_string());
    Ok(NewOperation {
        id: target.operation_id.clone(),
        kind: OperationKind::ManagedAgentDelete,
        resource_key: target.fence.pubkey.clone(),
        payload: serde_json::to_value(payload)
            .map_err(|_| "could not encode managed-agent cascade child".to_string())?,
    })
}

fn create_cascade_child<R: tauri::Runtime>(
    app: &AppHandle<R>,
    token: &OwnerScopeToken,
    parent_id: &str,
    target: &CascadeTarget,
) -> Result<Operation, String> {
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
    assert_current_blocking(app.clone(), token)?;
    let mut operations = journal
        .create_managed_agent_delete_batch(
            &token.scope,
            vec![child_operation(parent_id, target)?],
            native_now()?,
        )
        .map_err(|error| error.to_string())?;
    operations
        .pop()
        .ok_or_else(|| "managed-agent cascade child reservation was empty".into())
}

fn build_persona_cascade_payload(
    persona: &AgentDefinition,
    parent_id: &str,
    target_payloads: &[Payload],
) -> Result<(Payload, Vec<NewOperation>), String> {
    if target_payloads.len() > MAX_CASCADE_TARGETS {
        return Err("persona cascade target count is outside its bound".into());
    }
    let mut targets = Vec::with_capacity(target_payloads.len());
    let mut children = Vec::with_capacity(target_payloads.len());
    for payload in target_payloads {
        let child_id = persona_cascade_child_id(parent_id, &payload.fence.pubkey);
        let target = CascadeTarget::from_payload(child_id.clone(), persona.id.clone(), payload);
        children.push(child_operation(parent_id, &target)?);
        targets.push(target);
    }
    let (fence, channels, local_removed, key_removed, tombstone_enqueued, failures, last_error) =
        match targets.first() {
            Some(first) => (
                first.fence.clone(),
                first.channels.clone(),
                first.local_removed,
                first.key_removed,
                first.tombstone_enqueued,
                first.failures,
                first.last_error.clone(),
            ),
            None => (
                RecordFence {
                    pubkey: crate::owner_operations::persona_cascade_coordinator_resource_key(
                        &persona.id,
                    ),
                    name: "persona-cascade".into(),
                    created_at: persona.created_at.clone(),
                    relay_url: "wss://persona-coordinator.invalid".into(),
                    backend_agent_id: None,
                },
                Vec::new(),
                false,
                false,
                false,
                0,
                None,
            ),
        };
    let parent = Payload {
        version: VERSION,
        fence,
        channels,
        local_removed,
        key_removed,
        tombstone_enqueued,
        failures,
        last_error,
        cascade_parent: None,
        cascade_persona_id: None,
        cascade: Some(CascadePayload {
            persona: PersonaFence::capture(persona),
            targets,
            coordinator_only: target_payloads.is_empty(),
            persona_removed: false,
        }),
    };
    Ok((parent, children))
}

async fn begin_persona_cascade<R: tauri::Runtime>(
    app: &AppHandle<R>,
    token: OwnerScopeToken,
    persona_id: &str,
) -> Result<Operation, String> {
    assert_current(app.clone(), &token).await?;
    let parent_id = persona_cascade_operation_id(persona_id);
    if let Some(existing) = load_scope_operation(app, &token, &parent_id)? {
        if existing.kind != OperationKind::ManagedAgentDelete {
            return Err("persona deletion coordinator has an incompatible journal record".into());
        }
        let payload: Payload = serde_json::from_value(existing.payload.clone())
            .map_err(|_| "invalid persona deletion coordinator record")?;
        validate_operation(&existing, &payload)?;
        let Some(cascade) = payload.cascade.as_ref() else {
            return Err("persona deletion coordinator record has no cascade".into());
        };
        if cascade.persona.id != persona_id {
            return Err("persona deletion coordinator belongs to another persona".into());
        }
        return Ok(existing);
    }

    let (persona, records) = persona_snapshot(app, persona_id)?;
    for record in &records {
        validate_delete_target(record, false)?;
    }
    if records.len() > MAX_CASCADE_TARGETS {
        return Err(format!(
            "persona cascade exceeds the {MAX_CASCADE_TARGETS}-agent journal bound"
        ));
    }

    // Provider deployment holds this same per-agent lock while its external
    // call is in flight and through its final pending-delete check. Acquire
    // every provider lock before the cascade's transition/store claim so a
    // never-deployed provider target cannot be claimed midway through a
    // deployment and leave a remote resource without a local receipt. Sort
    // keys before taking more than one lock to preserve a global lock order
    // across concurrent persona cascades.
    let mut provider_pubkeys: Vec<String> = records
        .iter()
        .filter(|record| record.backend != BackendKind::Local)
        .map(|record| record.pubkey.clone())
        .collect();
    provider_pubkeys.sort();
    provider_pubkeys.dedup();
    let mut _provider_guards = Vec::with_capacity(provider_pubkeys.len());
    for pubkey in provider_pubkeys {
        _provider_guards.push(
            crate::commands::acquire_provider_deploy_lock(&app.state::<AppState>(), &pubkey)
                .await?,
        );
    }
    // A deployment that was already in flight may have changed the captured
    // backend receipt while we waited for its lock. Revalidate before any
    // channel discovery or journal admission, using the same record fence as
    // the final locked check below.
    revalidate_persona_targets(app, &PersonaFence::capture(&persona), &records)?;

    let mut target_payloads = Vec::with_capacity(records.len());
    for record in &records {
        let channels = discover_channels(app, &token, &record.pubkey).await?;
        target_payloads.push(Payload::new(
            RecordFence::capture(record),
            channels,
            &record.pubkey,
        )?);
    }
    assert_current(app.clone(), &token).await?;

    // Revalidate the captured persona and every linked record under the same
    // transition → managed-store order used by spawn and direct deletion.
    // A new link or replacement record aborts before any claim is inserted.
    revalidate_persona_targets(app, &PersonaFence::capture(&persona), &records)?;
    let (parent_payload, mut children) =
        build_persona_cascade_payload(&persona, &parent_id, &target_payloads)?;
    let parent = NewOperation {
        id: parent_id.clone(),
        kind: OperationKind::ManagedAgentDelete,
        resource_key: parent_payload.fence.pubkey.clone(),
        payload: serde_json::to_value(parent_payload)
            .map_err(|_| "could not encode persona deletion coordinator")?,
    };
    let mut operations = Vec::with_capacity(children.len() + 1);
    operations.push(parent);
    operations.append(&mut children);

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
    assert_current_blocking(app.clone(), &token)?;
    // The files are re-read after taking the claim locks so an edit or link
    // created during channel discovery cannot be omitted from the prepared
    // target set.
    let (current_persona, _) =
        revalidate_persona_targets_locked(app, &PersonaFence::capture(&persona), &records)?;
    if !PersonaFence::capture(&current_persona).matches(&persona) {
        return Err("persona changed before deletion claim; retry".into());
    }
    let committed = journal
        .create_managed_agent_delete_batch(&token.scope, operations, native_now()?)
        .map_err(|error| error.to_string())?;
    drop(_store);
    drop(_transition);
    assert_current(app.clone(), &token).await?;
    committed
        .into_iter()
        .find(|operation| operation.id == parent_id)
        .ok_or_else(|| "persona deletion coordinator was not committed".into())
}

/// Delete one persona and its exact linked managed-agent records through a
/// durable coordinator.  The one-shot persona command keeps its existing
/// visible behavior, while every linked instance now gets the same journaled
/// stop, key, tombstone, and canvas cleanup as direct deletion.
pub(crate) async fn delete_persona<R: tauri::Runtime>(
    app: AppHandle<R>,
    persona_id: String,
) -> Result<(), String> {
    let token = capture(app.clone()).await?.token;
    let operation = begin_persona_cascade(&app, token.clone(), &persona_id).await?;
    resume(app.clone(), token, operation, true).await?;
    try_regenerate_nest(&app);
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "managed_agent_persona_delete_tests.rs"]
mod tests;
