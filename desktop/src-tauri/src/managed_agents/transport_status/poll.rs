use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::app_state::AppState;
use crate::managed_agents::runtime_commands::{emit_status, status_for_with, StatusInputs};
use crate::managed_agents::{
    current_instance_id, load_global_agent_config, load_managed_agents, load_personas,
    remove_agent_runtime_receipt, save_managed_agents, sync_managed_agent_processes,
};
use tauri::{AppHandle, Manager};

use super::export::{self, ExportCandidate};

const MAX_READS_PER_TICK: usize = 256;

#[derive(Debug, Clone, Copy)]
enum ExportTarget {
    Live,
    Retired,
}

#[derive(Debug)]
struct ExportWork {
    target: ExportTarget,
    candidate: ExportCandidate,
}

/// Exactly one task is created during native app setup. Removing a generation
/// removes its read capability; already-running reads still require exact apply.
pub(crate) fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticks = tokio::time::interval(Duration::from_secs(1));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut cursor = 0;
        let mut last_error: Option<String> = None;
        loop {
            ticks.tick().await;
            if app
                .state::<AppState>()
                .shutdown_started
                .load(Ordering::Acquire)
            {
                break;
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

fn poll_once<R: tauri::Runtime>(app: &AppHandle<R>, cursor: usize) -> Result<usize, String> {
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
    // Retired final reads are removed from `tickets` once they set
    // `final_applied`, but their registration and AUTH markers are still
    // drained below. Keep the export-enabled worker alive for that
    // retired-only phase; otherwise the first marker can be emitted on the
    // final-read tick and the following AUTH marker is stranded forever.
    if tickets.is_empty() && !export::enabled() {
        return Ok(cursor);
    }
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
            let result = super::reader::read_owned_envelope(&ticket.path);
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
    let mut records = load_managed_agents(app)?;
    let wall_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "local status clock unavailable")?
        .as_millis();
    let wall_ms = u64::try_from(wall_ms).unwrap_or(u64::MAX);
    let mut records_changed = false;
    let exports = apply_with_current_owner(&state, &owner, |runtimes| {
        let now = Instant::now();
        let mut changed = HashSet::new();
        let mut exports = Vec::new();
        let previous_errors: std::collections::HashMap<_, _> = runtimes
            .iter()
            .map(|(key, runtime)| (key.clone(), runtime.error.clone()))
            .collect();
        let previous_keys: Vec<_> = runtimes.keys().cloned().collect();
        let (lifecycle_changed, exited_pubkeys) =
            sync_managed_agent_processes(&mut records, runtimes, &current_instance_id(app));
        records_changed |= lifecycle_changed;
        for key in previous_keys {
            let exited = exited_pubkeys
                .iter()
                .any(|pubkey| pubkey.eq_ignore_ascii_case(&key.pubkey))
                && !runtimes.contains_key(&key);
            let inspection_changed = runtimes
                .get(&key)
                .map(|runtime| runtime.error != previous_errors.get(&key).cloned().flatten())
                .unwrap_or(false);
            if exited {
                remove_agent_runtime_receipt(app, &key);
                state.clear_agent_session_cache(&key);
                changed.insert(key);
            } else if inspection_changed {
                changed.insert(key);
            }
        }
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
        if export::enabled() {
            for runtime in runtimes.values_mut() {
                if let Some(monitor) = runtime.transport.as_mut() {
                    if let Some(candidate) = monitor.take_export_candidate(now) {
                        exports.push(ExportWork {
                            target: ExportTarget::Live,
                            candidate,
                        });
                    }
                }
            }
            let mut diagnostics = state
                .managed_transport_diagnostics
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            for ticket in diagnostics.export_tickets(&owner, now) {
                if let Some(candidate) = diagnostics.take_export_candidate(&ticket, now) {
                    exports.push(ExportWork {
                        target: ExportTarget::Retired,
                        candidate,
                    });
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
        exports
    })?
    .unwrap_or_default();
    if records_changed {
        save_managed_agents(app, &records)?;
    }
    // Emission is deliberately outside the process, transition, store, and
    // diagnostics locks. A pipe or stderr sink must never stall native state.
    drop(_store);
    drop(_transition);
    finish_exports(&state, &owner, exports)?;
    Ok(if total == 0 {
        0
    } else {
        (cursor + MAX_READS_PER_TICK.min(total)) % total
    })
}

fn finish_exports(state: &AppState, owner: &str, exports: Vec<ExportWork>) -> Result<(), String> {
    let mut failed = false;
    for work in exports {
        let now = Instant::now();
        let current = apply_with_current_owner(state, owner, |runtimes| match work.target {
            ExportTarget::Live => {
                let ticket = match &work.candidate {
                    ExportCandidate::Registration(candidate) => &candidate.ticket,
                    ExportCandidate::Auth(candidate) => &candidate.ticket,
                };
                runtimes.get_mut(&ticket.key).is_some_and(|runtime| {
                    let registered_child = runtime.start_nonce == ticket.nonce
                        && runtime.child.id() == ticket.process_id
                        && matches!(runtime.child.try_wait(), Ok(None));
                    runtime.transport.as_mut().is_some_and(|monitor| {
                        monitor.export_is_current(&work.candidate, now, registered_child)
                    })
                })
            }
            ExportTarget::Retired => state
                .managed_transport_diagnostics
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .export_is_current(&work.candidate, now),
        })?
        .unwrap_or(false);
        if !current {
            continue;
        }
        let success = export::emit(&work.candidate).is_ok();
        failed |= !success;
        apply_with_current_owner(state, owner, |runtimes| match work.target {
            ExportTarget::Live => {
                let ticket = match &work.candidate {
                    ExportCandidate::Registration(candidate) => &candidate.ticket,
                    ExportCandidate::Auth(candidate) => &candidate.ticket,
                };
                if let Some(runtime) = runtimes.get_mut(&ticket.key) {
                    if let Some(monitor) = runtime.transport.as_mut() {
                        monitor.finish_export(&work.candidate, success, now);
                    }
                }
            }
            ExportTarget::Retired => {
                state
                    .managed_transport_diagnostics
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .finish_export(&work.candidate, success, now);
            }
        })?;
    }
    if failed {
        Err("native transport evidence export failed".into())
    } else {
        Ok(())
    }
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
    use crate::managed_agents::transport_status::{Monitor, ReadTicket};
    use buzz_core_pkg::transport_status::{
        TransportAuthClassification, TransportCode, TransportConnectionAttempt,
        TransportReceivedAuth, TransportRecordEnvelope, TransportRecordV2, TransportState,
        TransportStatus,
    };
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Instant;

    struct PollTestApp {
        app: tauri::App<tauri::test::MockRuntime>,
        app_data_dir: PathBuf,
    }

    impl PollTestApp {
        fn new() -> Self {
            let identifier = format!(
                "xyz.nuncio.crew.test.transport-status-poll-{}",
                uuid::Uuid::new_v4().simple()
            );
            let mut context = tauri::test::mock_context(tauri::test::noop_assets());
            // Storage-backed poll_once tests must never resolve the production
            // application-data directory from the mock context's empty default
            // identifier. A per-test identifier keeps this path isolated from
            // another test run and lets the guard below prove ownership before
            // removing it.
            context.config_mut().identifier = identifier.into();
            let app = tauri::test::mock_builder()
                .manage(crate::app_state::build_app_state())
                .build(context)
                .expect("mock app");
            let app_data_dir = app.path().app_data_dir().expect("mock app data directory");
            assert!(
                !app_data_dir.exists(),
                "unique poll test app data path already exists: {}",
                app_data_dir.display()
            );
            Self { app, app_data_dir }
        }
    }

    impl Drop for PollTestApp {
        fn drop(&mut self) {
            if self.app_data_dir.exists() {
                std::fs::remove_dir_all(&self.app_data_dir)
                    .expect("remove owned poll test app data");
            }
        }
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    fn retired_auth_ticket(owner: String) -> ReadTicket {
        ReadTicket {
            key: crate::managed_agents::ManagedAgentRuntimeKey::new("a".repeat(64), "ws://fixture")
                .unwrap(),
            nonce: uuid::Uuid::from_u128(7).to_string(),
            path: PathBuf::from("/fixture/status.json"),
            owner,
            epoch: 0,
            process_id: 7,
            spawn_started_at_ms: 90_000,
            wire_version: 2,
        }
    }

    fn retired_auth_record(ticket: &ReadTicket) -> TransportRecordV2 {
        let attempt_id = uuid::Uuid::from_u128(8).to_string();
        TransportRecordV2 {
            version: 2,
            runtime_id: ticket.key.runtime_id(),
            start_nonce: ticket.nonce.clone(),
            sequence: 1,
            timestamp_ms: 100_000,
            terminal: true,
            transport: TransportStatus {
                state: TransportState::AuthRejected,
                code: TransportCode::AuthDenied,
                attempts: 1,
                elapsed_ms: 1,
                next_retry_at_ms: None,
                last_error: TransportCode::AuthDenied.message().map(str::to_owned),
            },
            process_id: ticket.process_id,
            spawn_started_at_ms: ticket.spawn_started_at_ms,
            connection_attempt: Some(TransportConnectionAttempt {
                sequence: 1,
                id: attempt_id.clone(),
                started_at_ms: 91_000,
            }),
            received_auth: Some(TransportReceivedAuth {
                auth_event_id: "c".repeat(64),
                attempt_id,
                attempt_sequence: 1,
                accepted: false,
                classification: TransportAuthClassification::CommunityBanned,
                received_at_ms: 92_000,
            }),
        }
    }

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

    #[test]
    fn retired_only_ticks_drain_registration_then_auth_without_read_tickets() {
        let _env = crate::managed_agents::lock_env_mutex();
        let _export = EnvVarGuard::set(export::NATIVE_AUTH_EVIDENCE_EXPORT_ENV, "1");

        let test_app = PollTestApp::new();
        let state = test_app.app.state::<AppState>();
        let owner = state.keys.lock().unwrap().public_key().to_hex();
        let ticket = retired_auth_ticket(owner.clone());
        let record = retired_auth_record(&ticket);
        let now = Instant::now();
        let diagnostics = Arc::clone(&state.managed_transport_diagnostics);
        let mut monitor = Monitor::new(ticket.clone(), &diagnostics);
        assert!(monitor.apply(
            &ticket,
            Ok(TransportRecordEnvelope::V2(record.clone())),
            true,
            now,
            100_000,
        ));
        monitor.retire(true, now);
        diagnostics.lock().unwrap().apply(
            &ticket,
            TransportRecordEnvelope::V2(record),
            false,
            now,
            100_000,
        );
        assert_eq!(
            diagnostics
                .lock()
                .unwrap()
                .export_tickets(&owner, Instant::now())
                .len(),
            1,
            "the final retired read must be applied before export polling"
        );

        // The final retired read is already applied, so this worker tick has
        // no file-read tickets. It must still emit registration and leave AUTH
        // queued for the next tick.
        assert_eq!(poll_once(test_app.app.handle(), 0).unwrap(), 0);
        let auth = diagnostics
            .lock()
            .unwrap()
            .take_export_candidate(&ticket, Instant::now())
            .expect("registration must have emitted before AUTH becomes due");
        assert!(matches!(auth, ExportCandidate::Auth(_)));
        diagnostics.lock().unwrap().finish_export(
            &auth,
            false,
            Instant::now() - std::time::Duration::from_secs(5),
        );

        // A second retired-only tick must reach the export phase and retry the
        // failed AUTH marker. With the old early return, this take would find
        // the still-due candidate and fail the assertion.
        assert_eq!(poll_once(test_app.app.handle(), 0).unwrap(), 0);
        assert!(diagnostics
            .lock()
            .unwrap()
            .take_export_candidate(&ticket, Instant::now())
            .is_none());
    }
}
