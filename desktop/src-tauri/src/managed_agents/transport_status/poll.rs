use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::app_state::AppState;
use crate::managed_agents::runtime_commands::{emit_status, status_for_with, StatusInputs};
use crate::managed_agents::{load_global_agent_config, load_managed_agents, load_personas};
use tauri::{AppHandle, Manager};

const MAX_READS_PER_TICK: usize = 256;

/// Exactly one task is created during native app setup. Removing a generation
/// removes its read capability; already-running reads still require exact apply.
pub(crate) fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticks = tokio::time::interval(Duration::from_secs(1));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut cursor = 0;
        let mut last_error: Option<String> = None;
        let mut last_process_error: Option<String> = None;
        loop {
            ticks.tick().await;
            if app
                .state::<AppState>()
                .shutdown_started
                .load(Ordering::Acquire)
            {
                break;
            }
            // Reuse the process manager's existing exit/bookkeeping operation.
            // It captures the real exit status before minting a retirement token.
            let result =
                crate::managed_agents::runtime_commands::list_managed_agent_runtimes(app.clone())
                    .await;
            if let Err(error) = result {
                if last_process_error.as_ref() != Some(&error) {
                    eprintln!("local transport process poll: {error}");
                }
                last_process_error = Some(error);
            } else {
                last_process_error = None;
            }
            let reader_app = app.clone();
            match tokio::task::spawn_blocking(move || poll_once(&reader_app, cursor)).await {
                Ok(Ok(next)) => {
                    cursor = next;
                    last_error = None;
                }
                Ok(Err(error)) => {
                    if last_error.as_ref() != Some(&error) {
                        eprintln!("local transport status poll: {error}");
                    }
                    last_error = Some(error);
                }
                Err(_) => {
                    // A worker panic cannot mint a generation or relax a fence.
                    let error = "local transport reader task failed".to_string();
                    if last_error.as_ref() != Some(&error) {
                        eprintln!("{error}");
                    }
                    last_error = Some(error);
                }
            }
        }
    });
}

fn poll_once(app: &AppHandle, cursor: usize) -> Result<usize, String> {
    let state = app.state::<AppState>();
    let owner = state
        .keys
        .lock()
        .map_err(|_| "native owner unavailable")?
        .public_key()
        .to_hex();
    let mut tickets = {
        let runtimes = state
            .managed_agent_processes
            .lock()
            .map_err(|_| "managed process state unavailable")?;
        if state.shutdown_started.load(Ordering::Acquire) {
            return Ok(cursor);
        }
        let mut tickets: Vec<_> = runtimes
            .values()
            .filter_map(|runtime| runtime.transport.as_ref()?.snapshot())
            .filter(|ticket| ticket.owner == owner)
            .collect();
        let mut diagnostics = state
            .managed_transport_diagnostics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        diagnostics.prune(Instant::now());
        tickets.extend(
            diagnostics
                .pending(Instant::now())
                .into_iter()
                .filter(|ticket| ticket.owner == owner),
        );
        tickets
    };
    tickets.sort_by(|left, right| {
        left.key
            .runtime_id()
            .cmp(&right.key.runtime_id())
            .then(left.nonce.cmp(&right.nonce))
    });
    let total = tickets.len();
    let selected: Vec<_> = (0..total.min(MAX_READS_PER_TICK))
        .map(|offset| tickets[(cursor + offset) % total].clone())
        .collect();
    // No process, transition, store, or cache guard is held during file I/O.
    let results: Vec<_> = selected
        .into_iter()
        .map(|ticket| {
            let result = super::reader::read_owned_record(&ticket.path);
            (ticket, result)
        })
        .collect();
    let personas = load_personas(app).unwrap_or_default();
    let global = load_global_agent_config(app).unwrap_or_default();
    let _transition = state
        .managed_agent_runtime_transition
        .lock()
        .map_err(|_| "runtime transition unavailable")?;
    let _store = state
        .managed_agents_store_lock
        .lock()
        .map_err(|_| "managed records unavailable")?;
    let records = load_managed_agents(app)?;
    let wall_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "local status clock unavailable")?
        .as_millis();
    let wall_ms = u64::try_from(wall_ms).unwrap_or(u64::MAX);
    apply_with_current_owner(&state, &owner, |runtimes| {
        let now = Instant::now();
        let mut changed = HashSet::new();
        for (key, runtime) in runtimes.iter_mut() {
            if let Some(monitor) = runtime.transport.as_mut() {
                if monitor.expire(now) {
                    changed.insert(key.clone());
                }
            }
        }
        for (ticket, result) in results {
            if ticket.owner != owner
                || !records
                    .iter()
                    .any(|record| record.pubkey.eq_ignore_ascii_case(&ticket.key.pubkey))
            {
                continue;
            }
            if let Some(runtime) = runtimes.get_mut(&ticket.key) {
                let alive = matches!(runtime.child.try_wait(), Ok(None));
                if let Some(monitor) = runtime.transport.as_mut() {
                    if monitor.apply(&ticket, result, alive, now, wall_ms) {
                        changed.insert(ticket.key);
                    }
                }
            } else if let Ok(record) = result {
                // The processes guard remains held for the absence check AND cache
                // apply. Sync removal paths need not take the transition mutex.
                if state
                    .managed_transport_diagnostics
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .apply(&ticket, record, false, now, wall_ms)
                {
                    changed.insert(ticket.key);
                }
            }
        }
        for key in changed {
            if let Some(record) = records
                .iter()
                .find(|record| record.pubkey.eq_ignore_ascii_case(&key.pubkey))
            {
                let status = status_for_with(
                    app,
                    record,
                    &key,
                    runtimes.get(&key),
                    None,
                    StatusInputs {
                        personas: &personas,
                        global: &global,
                    },
                );
                emit_status(app, &status);
            }
        }
    })?;
    Ok(if total == 0 {
        0
    } else {
        (cursor + MAX_READS_PER_TICK.min(total)) % total
    })
}

/// Guard the complete check-and-apply operation with the actual process map.
fn apply_with_current_owner<T>(
    state: &AppState,
    owner: &str,
    apply: impl FnOnce(
        &mut std::collections::HashMap<
            crate::managed_agents::ManagedAgentRuntimeKey,
            crate::managed_agents::ManagedAgentPairRuntime,
        >,
    ) -> T,
) -> Result<Option<T>, String> {
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|_| "managed process state unavailable")?;
    if state.shutdown_started.load(Ordering::Acquire) {
        return Ok(None);
    }
    let current_owner = state
        .keys
        .lock()
        .map_err(|_| "native owner unavailable")?
        .public_key()
        .to_hex();
    if current_owner != owner {
        return Ok(None);
    }
    Ok(Some(apply(&mut runtimes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_switch_between_read_and_apply_drops_the_whole_batch() {
        let state = crate::app_state::build_app_state();
        let captured_owner = state.keys.lock().unwrap().public_key().to_hex();
        // A read completed for the snapshot owner while identity import won.
        *state.keys.lock().unwrap() = nostr::Keys::generate();
        let mut applied = false;
        let result = apply_with_current_owner(&state, &captured_owner, |_| {
            applied = true;
        })
        .unwrap();
        assert!(result.is_none());
        assert!(
            !applied,
            "old-owner file results must not reach projection/cache writes"
        );
    }

    #[test]
    fn shutdown_between_read_and_apply_drops_the_whole_batch() {
        let state = crate::app_state::build_app_state();
        let owner = state.keys.lock().unwrap().public_key().to_hex();
        state.shutdown_started.store(true, Ordering::Release);
        let mut applied = false;
        assert!(apply_with_current_owner(&state, &owner, |_| {
            applied = true;
        })
        .unwrap()
        .is_none());
        assert!(!applied);
    }
}
