use super::*;

/// Kill stale agent processes from a previous session whose PID is still alive
/// but not tracked in the current `runtimes` map. Updates the record fields and
/// returns `true` if any records were modified.
pub fn kill_stale_tracked_processes(
    records: &mut [ManagedAgentRecord],
    runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
    instance_id: &str,
) -> bool {
    kill_stale_tracked_processes_with(
        records,
        runtimes,
        |pid| process_has_buzz_marker(pid, instance_id),
        terminate_process,
    )
}

/// Injectable version of `kill_stale_tracked_processes` for testing.
/// `has_marker(pid)` returns true when the process carries this instance's
/// `BUZZ_MANAGED_AGENT` marker; `kill(pid)` performs the termination.
pub(crate) fn kill_stale_tracked_processes_with(
    records: &mut [ManagedAgentRecord],
    runtimes: &HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
    has_marker: impl Fn(u32) -> bool,
    mut kill: impl FnMut(u32) -> Result<(), String>,
) -> bool {
    use crate::managed_agents::BackendKind;

    let mut changed = false;
    for record in records.iter_mut() {
        if record.backend != BackendKind::Local {
            continue;
        }
        let Some(pid) = record.runtime_pid else {
            continue;
        };
        if !runtimes.keys().any(|key| key.pubkey == record.pubkey) {
            // Name-gate is omitted intentionally: custom harnesses use arbitrary
            // binary names not in KNOWN_AGENT_BINARIES. BUZZ_MANAGED_AGENT is the
            // authoritative ownership proof; terminate only if it matches.
            if has_marker(pid) {
                let _ = kill(pid);
            }
            record.runtime_pid = None;
            record.last_stopped_at = Some(crate::util::now_iso());
            record.updated_at = crate::util::now_iso();
            changed = true;
        }
    }
    changed
}

pub fn sync_managed_agent_processes(
    records: &mut [ManagedAgentRecord],
    runtimes: &mut HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
    _instance_id: &str,
) -> (bool, Vec<String>) {
    let mut changed = false;
    let mut exited = Vec::new();

    for (key, runtime) in runtimes.iter_mut() {
        let status = match runtime.child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                if let Some(record) = records
                    .iter_mut()
                    .find(|record| record.pubkey == key.pubkey)
                {
                    record.updated_at = now_iso();
                    record.last_error = Some(format!("failed to inspect process state: {error}"));
                    record.last_error_code = None;
                }
                runtime.error =
                    Some(super::super::transport_status::PROCESS_INSPECTION_ERROR.into());
                if let Some(monitor) = runtime.transport.as_mut() {
                    monitor.inspection_failed();
                }
                changed = true;
                continue;
            }
        };

        let Some(status) = status else {
            continue;
        };

        if let Some(record) = records
            .iter_mut()
            .find(|record| record.pubkey == key.pubkey)
        {
            record.updated_at = now_iso();
            record.last_stopped_at = Some(now_iso());
            record.last_exit_code = status.code();
            let log_err = if status.success() {
                None
            } else {
                Some(
                    super::super::meaningful_agent_error_from_log(&runtime.log_path)
                        .unwrap_or_else(|| super::super::storage::AgentLogError {
                            message: format!("harness exited with status {status}"),
                            code: None,
                        }),
                )
            };
            record.last_error = log_err.as_ref().map(|e| e.message.clone());
            record.last_error_code = log_err.as_ref().and_then(|e| e.code);
        }

        if let Some(monitor) = runtime.transport.as_mut() {
            monitor.retire(!status.success(), std::time::Instant::now());
        }
        changed = true;
        exited.push(key.clone());
    }

    let exited_pubkeys: Vec<String> = exited.iter().map(|key| key.pubkey.clone()).collect();
    for key in exited {
        runtimes.remove(&key);
    }

    // `runtime_pid` is legacy bookkeeping. Pair runtimes and receipts are the
    // authoritative lifecycle source; migration cleanup is handled separately.
    for record in records.iter_mut() {
        if record.runtime_pid.take().is_some() {
            record.updated_at = now_iso();
            changed = true;
        }
    }

    (changed, exited_pubkeys)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::managed_agents::spawn_snapshot::SpawnConfigSnapshot;
    use crate::managed_agents::transport_status::{Diagnostics, Monitor, ReadTicket};
    use crate::managed_agents::ManagedAgentProcess;
    use buzz_core_pkg::transport_status::TransportState;
    use std::collections::{BTreeMap, HashMap};
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    fn test_spawn_snapshot() -> SpawnConfigSnapshot {
        SpawnConfigSnapshot {
            acp_command: "buzz-acp".into(),
            command: "/usr/bin/false".into(),
            args: Vec::new(),
            mcp_command: String::new(),
            env: BTreeMap::new(),
            relay_url: "ws://fixture".into(),
            team_instructions: None,
            system_prompt: None,
            model: None,
            provider: None,
            session_title: None,
            auth_tag: None,
            respond_to: "owner-only".into(),
            respond_to_allowlist: None,
            idle_timeout_seconds: None,
            max_turn_duration_seconds: None,
            parallelism: 1,
            effort_level: None,
            session_policy: "channel".into(),
        }
    }

    #[test]
    fn sync_retires_an_exited_generation_through_the_lifecycle_seam() {
        let key = ManagedAgentRuntimeKey::new("a".repeat(64), "ws://fixture").unwrap();
        let owner = "b".repeat(64);
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let child = Command::new("/usr/bin/false")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the short-lived lifecycle fixture");
        let process = ManagedAgentProcess {
            child,
            log_path: PathBuf::new(),
            spawn_config: test_spawn_snapshot(),
            setup_mode: false,
            adapter_availability: None,
            start_nonce: nonce.clone(),
        };
        let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
        let ticket = ReadTicket {
            key: key.clone(),
            nonce,
            path: PathBuf::from("/fixture/status.json"),
            owner: owner.clone(),
            epoch: 0,
        };
        let mut runtime = ManagedAgentPairRuntime::starting(process);
        runtime.transport = Some(Monitor::new(ticket, &diagnostics));
        let mut runtimes = HashMap::from([(key.clone(), runtime)]);

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut exited = Vec::new();
        while Instant::now() < deadline {
            let (_, keys) = sync_managed_agent_processes(&mut [], &mut runtimes, "test");
            if !keys.is_empty() {
                exited = keys;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert_eq!(exited, vec![key.pubkey.clone()]);
        assert!(runtimes.is_empty());
        let (status, failed) = diagnostics
            .lock()
            .unwrap()
            .projection(&key, &owner, Instant::now())
            .expect("the production lifecycle seam must retire the monitor");
        assert!(failed);
        assert_eq!(status.state, TransportState::Unknown);
    }
}
