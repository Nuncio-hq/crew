use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::app_state::AppState;
use crate::managed_agents::runtime_commands::{emit_status, status_for_with, StatusInputs};
use crate::managed_agents::{load_global_agent_config, load_managed_agents, load_personas};
use tauri::{AppHandle, Manager};

/// Metadata-only explicit leave/rejoin operation. Ownership is native; this
/// never changes membership, credentials, process state, or another relay.
#[tauri::command]
pub fn set_managed_transport_eligibility(
    relay_url: String,
    enabled: bool,
    app: AppHandle,
) -> Result<(), String> {
    let relay_url = buzz_core_pkg::relay::normalize_relay_url(&relay_url)
        .map_err(|_| "invalid relay identity for local transport diagnostics")?;
    let state = app.state::<AppState>();
    let personas = load_personas(&app).unwrap_or_default();
    let global = load_global_agent_config(&app).unwrap_or_default();
    let _transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|_| "runtime transition unavailable")?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|_| "managed records unavailable")?;
    let records = load_managed_agents(&app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|_| "managed process state unavailable")?;
    if state.shutdown_started.load(Ordering::Acquire) {
        return Err("desktop shutdown has started".into());
    }
    let owner = state
        .keys
        .lock()
        .map_err(|_| "native owner unavailable")?
        .public_key()
        .to_hex();
    let mut changed = HashSet::new();
    for (key, runtime) in runtimes.iter_mut() {
        let Some(monitor) = runtime.transport.as_mut() else {
            continue;
        };
        if !monitor.belongs_to(&owner, &relay_url) {
            continue;
        }
        let did_change = if enabled {
            match runtime.process.child.try_wait() {
                Ok(None) => monitor.enable(&owner)?,
                Ok(Some(_)) => continue,
                Err(_) => {
                    monitor.inspection_failed();
                    runtime.error = Some(super::PROCESS_INSPECTION_ERROR.into());
                    return Err(super::PROCESS_INSPECTION_ERROR.into());
                }
            }
        } else {
            monitor.disable()
        };
        if did_change {
            changed.insert(key.clone());
        }
    }
    if !enabled {
        let mut diagnostics = state
            .managed_transport_diagnostics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        changed.extend(
            diagnostics
                .keys_for_owner(&owner, Instant::now())
                .into_iter()
                .filter(|key| key.relay_url == relay_url),
        );
        diagnostics.clear_owner_relay(&owner, &relay_url);
    }
    for key in changed {
        if let Some(record) = records
            .iter()
            .find(|record| record.pubkey.eq_ignore_ascii_case(&key.pubkey))
        {
            let status = status_for_with(
                &app,
                record,
                &key,
                runtimes.get(&key),
                None,
                StatusInputs {
                    personas: &personas,
                    global: &global,
                },
            );
            emit_status(&app, &status);
        }
    }
    Ok(())
}
