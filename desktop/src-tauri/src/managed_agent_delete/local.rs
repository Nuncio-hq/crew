//! Existing runtime stop and Bestie rollback remain the local commit boundary.
use super::{admission::now, journal, native::Native, record::Phase};
use crate::{
    app_state::AppState,
    managed_agents,
    owner_operations::{Limits, OperationStore},
};
use std::sync::Arc;
use tauri::Manager;

#[derive(Clone, Copy)]
pub(super) enum LocalStep {
    Quiesce,
    Remove,
    DeleteKey,
}

impl Native {
    pub async fn local_step(&self, step: LocalStep) -> Result<(), String> {
        self.guard().await?;
        let app = self.app.clone();
        let token = self.token.clone();
        let id = self.id.clone();
        let worker = self.worker.clone();
        tokio::task::spawn_blocking(move || {
            let state = app.state::<AppState>();
            let _workspace = state
                .workspace_apply_lock
                .clone()
                .try_lock_owned()
                .map_err(|_| "Workspace is changing; retry removal")?;
            let identity = state
                .identity_mutation
                .try_lock()
                .map_err(|_| "Identity is changing; retry removal")?;
            if state.capture_owner_scope(&identity)?.token != token {
                return Err("Removal scope changed".into());
            }
            let path = crate::commands::journal_path(&app)?;
            let operation = OperationStore::open(&path, Limits::default())
                .map_err(|e| e.to_string())?
                .load(&token.scope, &id)
                .map_err(|e| e.to_string())?;
            let payload = journal::decode(&operation)?;
            let lock = {
                let mut locks = state
                    .provider_deploy_locks
                    .lock()
                    .map_err(|_| "Instance mutation lock unavailable")?;
                Arc::clone(
                    locks
                        .entry(payload.instance.pubkey.clone())
                        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
                )
            };
            let _provider = lock
                .try_lock_owned()
                .map_err(|_| "Instance mutation is busy")?;
            let _transition = state
                .managed_agent_runtime_transition
                .try_lock()
                .map_err(|_| "Runtime transition is busy")?;
            let _managed = state
                .managed_agents_store_lock
                .lock()
                .map_err(|_| "Instance store unavailable")?;
            // Re-read after admission locks: a cancel or successor may have won CAS.
            let current = OperationStore::open(&path, Limits::default())
                .map_err(|e| e.to_string())?
                .load(&token.scope, &id)
                .map_err(|e| e.to_string())?;
            if current.revision != operation.revision {
                return Err("Removal changed before local step".into());
            }
            journal::assert_worker(&payload, &worker, now()?)?;
            let mut records = managed_agents::load_managed_agents(&app)?;
            let position = records
                .iter()
                .position(|r| r.pubkey == payload.instance.pubkey);
            if let Some(index) = position {
                if !payload.instance.matches(&records[index]) {
                    return Err("Managed instance changed; review required".into());
                }
            }
            match step {
                LocalStep::DeleteKey => {
                    if payload.phase != Phase::Offboarding
                        || !payload.local_removed
                        || position.is_some()
                    {
                        return Err("Instance removal must commit before key cleanup".into());
                    }
                    managed_agents::try_delete_agent_key(&payload.instance.pubkey)
                }
                LocalStep::Quiesce | LocalStep::Remove => {
                    let removing = matches!(step, LocalStep::Remove);
                    if (removing
                        && (payload.phase != Phase::LocalRemoval || payload.cancel_requested))
                        || (!removing && payload.phase != Phase::Quiesce)
                    {
                        return Err("Removal phase changed before local step".into());
                    }
                    let Some(index) = position else {
                        // The durable LocalRemoval phase precedes the existing atomic
                        // record save; its global claim excludes replacement on replay.
                        if removing {
                            return Ok(());
                        }
                        return Err("Managed instance disappeared before quiesce".into());
                    };
                    let mut runtimes = state
                        .managed_agent_processes
                        .lock()
                        .map_err(|_| "Runtime inventory unavailable")?;
                    if removing {
                        let base = managed_agents::managed_agents_base_dir(&app)?;
                        crate::commands::run_managed_agent_deletion(
                            &base,
                            &payload.instance.pubkey,
                            &mut records,
                            |records| {
                                let record = records
                                    .get_mut(index)
                                    .ok_or("Managed instance unavailable")?;
                                managed_agents::stop_managed_agent_process(
                                    &app,
                                    record,
                                    &mut runtimes,
                                )?;
                                state.clear_agent_session_caches(&payload.instance.pubkey);
                                records.retain(|r| r.pubkey != payload.instance.pubkey);
                                managed_agents::save_managed_agents(&app, records)
                            },
                        )
                    } else {
                        managed_agents::stop_managed_agent_process(
                            &app,
                            &mut records[index],
                            &mut runtimes,
                        )?;
                        state.clear_agent_session_caches(&payload.instance.pubkey);
                        managed_agents::save_managed_agents(&app, &records)
                    }
                }
            }
        })
        .await
        .map_err(|_| "Local removal worker failed")?
    }
}
