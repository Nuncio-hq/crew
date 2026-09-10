//! Admission takes existing lifecycle guards before claiming the same journal.
use super::record::{self, Instance, Payload, Phase};
use crate::{
    app_state::{
        owner_scope::{assert_current, OwnerScopeToken},
        AppState,
    },
    managed_agents::{self, instance_identity, BackendKind},
    owner_operations::{
        CreateResult, Limits, NewOperation, Operation, OperationKind, OperationStore, StoreError,
    },
};
use nostr::JsonUtil;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

pub(super) fn now() -> Result<i64, String> {
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "Removal clock unavailable")?
        .as_secs();
    i64::try_from(time).map_err(|_| "Removal clock unavailable".into())
}

pub(super) async fn prepare(
    app: AppHandle,
    expected: OwnerScopeToken,
    id: String,
    pubkey: String,
    force: bool,
) -> Result<Operation, String> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|_| "Invalid removal operation")?;
    if parsed.is_nil() || parsed.to_string() != id {
        return Err("Invalid removal operation".into());
    }
    let public = nostr::PublicKey::from_hex(&pubkey).map_err(|_| "Invalid managed instance")?;
    if public.to_hex() != pubkey {
        return Err("Invalid managed instance".into());
    }
    assert_current(app.clone(), &expected).await?;
    let worker_app = app.clone();
    let token = expected.clone();
    let operation = tokio::task::spawn_blocking(move || {
        let state = worker_app.state::<AppState>();
        // Try-only admission never waits while holding another lifecycle lock.
        let _workspace = state
            .workspace_apply_lock
            .clone()
            .try_lock_owned()
            .map_err(|_| "Workspace is changing; retry removal")?;
        let identity = state
            .identity_mutation
            .try_lock()
            .map_err(|_| "Identity is changing; retry removal")?;
        let captured = state.capture_owner_scope(&identity)?;
        if captured.token != token {
            return Err("Removal scope changed; return to the originating workspace".into());
        }
        let lock = {
            let mut locks = state
                .provider_deploy_locks
                .lock()
                .map_err(|_| "Instance mutation lock unavailable")?;
            Arc::clone(
                locks
                    .entry(pubkey.clone())
                    .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
            )
        };
        let _provider = lock
            .try_lock_owned()
            .map_err(|_| "Instance mutation is busy; retry removal")?;
        let _transition = state
            .managed_agent_runtime_transition
            .try_lock()
            .map_err(|_| "Runtime transition is busy; retry removal")?;
        let _managed = state
            .managed_agents_store_lock
            .lock()
            .map_err(|_| "Instance store unavailable")?;
        let path = crate::commands::journal_path(&worker_app)?;
        let mut journal =
            OperationStore::open(&path, Limits::default()).map_err(|e| e.to_string())?;
        match journal.load(&token.scope, &id) {
            Ok(existing) => {
                let payload: Payload = serde_json::from_value(existing.payload.clone())
                    .map_err(|_| "Invalid removal record")?;
                record::validate(&existing, &payload)?;
                if payload.instance.pubkey != pubkey || payload.force_remote_delete != force {
                    return Err("Removal operation belongs to another intent".into());
                }
                return Ok(existing);
            }
            Err(StoreError::Missing) => {}
            Err(error) => return Err(error.to_string()),
        }
        instance_identity::assert_instance_available(&worker_app, &pubkey)?;
        let mut records = managed_agents::load_managed_agents(&worker_app)?;
        instance_identity::ensure_instance_generation(&mut records, &pubkey, |records| {
            managed_agents::save_managed_agents(&worker_app, records)
        })?;
        let current = records
            .iter()
            .find(|r| r.pubkey == pubkey)
            .ok_or("Managed instance is no longer available")?;
        let instance = Instance::capture(current)?;
        drop(record::owned_keys(current, &token.scope.owner)?);
        if !instance.relay_url.is_empty() && instance.relay_url != token.scope.community {
            return Err(
                "This instance is configured for another community; review removal there".into(),
            );
        }
        if current.backend != BackendKind::Local && current.backend_agent_id.is_some() && !force {
            return Err("A deployed remote instance requires explicit orphan consent".into());
        }
        {
            let runtimes = state
                .managed_agent_processes
                .lock()
                .map_err(|_| "Runtime inventory unavailable")?;
            for key in managed_agents::managed_agent_runtime_keys(&runtimes, &pubkey) {
                if crate::relay::relay_http_base_url(&key.relay_url) != token.scope.community {
                    return Err(
                        "This instance has runtime state in another community; review required"
                            .into(),
                    );
                }
            }
        }
        let base = managed_agents::managed_agents_base_dir(&worker_app)?;
        let retention_path = managed_agents::retention::scoped_retention_db_path(
            &base,
            &captured.relay_url,
            &token.scope.owner,
        );
        let retention_scope_id = retention_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid retention scope")?
            .to_owned();
        let conn = managed_agents::retention::open_retention_db(&retention_path)?;
        let head = managed_agents::retention::get_retained_event(
            &conn,
            buzz_core_pkg::kind::KIND_MANAGED_AGENT,
            &token.scope.owner,
            &pubkey,
        )?;
        let expected_agent_head = head
            .map(|row| {
                let event = nostr::Event::from_json(&row.raw_event)
                    .map_err(|_| "Invalid retained managed agent")?;
                event
                    .verify()
                    .map_err(|_| "Invalid retained managed agent")?;
                if event.pubkey.to_hex() != token.scope.owner
                    || event.kind.as_u16() as u32 != buzz_core_pkg::kind::KIND_MANAGED_AGENT
                    || !event
                        .tags
                        .iter()
                        .any(|tag| tag.as_slice() == ["d", pubkey.as_str()])
                {
                    return Err("Retained managed agent belongs to another instance");
                }
                Ok(event.id.to_hex())
            })
            .transpose()
            .map_err(str::to_owned)?;
        drop(conn);
        let payload = Payload {
            version: 1,
            instance,
            force_remote_delete: force,
            retention_scope_id,
            expected_agent_head,
            phase: Phase::Inventory,
            channels: None,
            retained_pair: None,
            local_removed: false,
            key_removed: false,
            lease: None,
            failures: 0,
            next_retry_at: None,
            last_error: None,
            effects_started: false,
            cancel_requested: false,
        };
        let created = journal
            .create(
                &token.scope,
                NewOperation {
                    id,
                    kind: OperationKind::ManagedAgentDelete,
                    resource_key: pubkey,
                    payload: serde_json::to_value(payload)
                        .map_err(|_| "Could not encode removal intent")?,
                },
                now()?,
            )
            .map_err(|e| e.to_string())?;
        Ok(match created {
            CreateResult::Created(operation) | CreateResult::Existing(operation) => operation,
        })
    })
    .await
    .map_err(|_| "Removal admission failed")??;
    assert_current(app, &expected).await?;
    Ok(operation)
}
