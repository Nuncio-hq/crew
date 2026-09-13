use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

use crate::managed_agents::ManagedAgentRuntimeReceipt;
use tauri::AppHandle;

const MAX_RUNTIME_RECEIPT_BYTES: u64 = 16 * 1024;

use super::{
    append_log_marker, current_instance_id, now_iso, process_belongs_to_us,
    process_has_buzz_marker, process_is_running, terminate_process, terminate_runtime_receipt_with,
    valid_agent_runtime_receipt, ManagedAgentPairRuntime, ManagedAgentRecord,
    ManagedAgentRuntimeKey,
};

pub(crate) fn managed_agent_runtime_keys<T>(
    runtimes: &HashMap<ManagedAgentRuntimeKey, T>,
    pubkey: &str,
) -> Vec<ManagedAgentRuntimeKey> {
    runtimes
        .keys()
        .filter(|key| key.pubkey.eq_ignore_ascii_case(pubkey))
        .cloned()
        .collect()
}

#[cfg(test)]
pub(crate) fn managed_agent_runtime_relay_urls<T>(
    runtimes: &HashMap<ManagedAgentRuntimeKey, T>,
    pubkey: &str,
) -> Vec<String> {
    managed_agent_runtime_keys(runtimes, pubkey)
        .into_iter()
        .map(|key| key.relay_url)
        .collect()
}

/// Stop the single tracked runtime pair at `key`, if present.
///
/// Terminates the child, records the exit code, removes the pair receipt,
/// and appends a stop marker to the pair log. On teardown failure the
/// runtime is reinserted so the pair stays visible and stoppable instead of
/// becoming an invisible orphan. Touches no other pair for the agent and
/// does no record-level stop bookkeeping — callers own that.
fn stop_managed_agent_pair<R: tauri::Runtime, T: FnMut(u32) -> Result<(), String>>(
    app: &AppHandle<R>,
    record: &mut ManagedAgentRecord,
    runtimes: &mut HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
    key: &ManagedAgentRuntimeKey,
    terminate: &mut T,
) -> Result<(), String> {
    let Some(mut runtime) = runtimes.remove(key) else {
        super::super::transport_status::clear_key(app, key);
        return Ok(());
    };
    let result = (|| -> Result<(), String> {
        #[cfg(unix)]
        terminate(runtime.child.id())?;
        #[cfg(windows)]
        match runtime.job.take() {
            Some(job) => drop(job),
            None => runtime
                .child
                .kill()
                .map_err(|error| format!("failed to kill agent process: {error}"))?,
        }
        #[cfg(not(any(unix, windows)))]
        runtime
            .child
            .kill()
            .map_err(|error| format!("failed to kill agent process: {error}"))?;
        #[cfg(windows)]
        let _ = terminate;
        let status = runtime
            .child
            .wait()
            .map_err(|error| format!("failed to wait for agent shutdown: {error}"))?;
        record.last_exit_code = status.code();
        super::super::remove_agent_runtime_receipt(app, key);
        if let Err(error) = append_log_marker(
            &runtime.log_path,
            &format!(
                "=== stopped {} ({}) at {} ===",
                record.name,
                record.pubkey,
                now_iso()
            ),
        ) {
            eprintln!(
                "buzz-desktop: failed to append stop marker for {} on {}: {error}",
                record.pubkey, key.relay_url
            );
        }
        Ok(())
    })();
    if let Err(error) = result {
        // Keep failed teardown visible/manageable instead of orphaning it.
        runtimes.insert(key.clone(), runtime);
        return Err(error);
    }
    super::super::transport_status::clear_key(app, key);
    Ok(())
}

/// Terminate a legacy scalar-PID child (pre-pair records) and remove the
/// agent-scoped pid file. Pair receipts are restored separately.
fn stop_legacy_scalar_pid<R: tauri::Runtime>(
    app: &AppHandle<R>,
    record: &mut ManagedAgentRecord,
) -> Result<(), String> {
    if let Some(pid) = record.runtime_pid {
        if process_is_running(pid)
            && process_belongs_to_us(pid)
            && process_has_buzz_marker(pid, &current_instance_id(app))
        {
            terminate_process(pid)?;
        }
        record.runtime_pid = None;
        record.updated_at = now_iso();
    }
    super::super::remove_agent_pid_file(app, &record.pubkey);
    Ok(())
}

/// Stop the runtime pair this record resolves to for the active workspace
/// (explicit relay pin, else the active workspace relay) — the pair-scoped
/// counterpart of [`stop_managed_agent_process`], which drains every pair.
///
/// Community-scoped surfaces (profile panel, Agents tab, auto-restart) stop
/// through here so stopping an agent in one community never tears down its
/// pairs in other communities. Clears the matching agent session cache
/// (pair-scoped when a pair key resolves). When no pair is tracked for this
/// workspace, legacy scalar-PID cleanup is all that remains; agent-wide
/// deletion uses [`stop_managed_agent_process`] to drain durable receipts.
pub fn stop_managed_agent_workspace_pair(
    app: &AppHandle,
    record: &mut ManagedAgentRecord,
    runtimes: &mut HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
) -> Result<(), String> {
    use tauri::Manager;
    let state = app.state::<crate::app_state::AppState>();
    match super::workspace_pair_key(app, record) {
        Some(pair_key) if runtimes.contains_key(&pair_key) => {
            stop_managed_agent_pair(app, record, runtimes, &pair_key, &mut terminate_process)?;
            state.clear_agent_session_cache(&pair_key);
            super::super::remove_agent_pid_file(app, &record.pubkey);
            let now = now_iso();
            record.runtime_pid = None;
            record.updated_at = now.clone();
            record.last_stopped_at = Some(now);
            record.last_error = None;
            record.last_error_code = None;
        }
        Some(pair_key) => {
            // No tracked pair here — a pubkey-wide cache clear would disturb
            // live pairs in other communities, so stay pair-scoped.
            stop_legacy_scalar_pid(app, record)?;
            super::super::transport_status::clear_key(app, &pair_key);
            state.clear_agent_session_cache(&pair_key);
        }
        None => {
            stop_legacy_scalar_pid(app, record)?;
            super::super::transport_status::clear_pubkey(app, &record.pubkey);
            state.clear_agent_session_caches(&record.pubkey);
        }
    }
    Ok(())
}

fn stop_managed_agent_process_with<R: tauri::Runtime, T: FnMut(u32) -> Result<(), String>>(
    app: &AppHandle<R>,
    record: &mut ManagedAgentRecord,
    runtimes: &mut HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
    mut terminate: T,
) -> Result<(), String> {
    let keys = managed_agent_runtime_keys(runtimes, &record.pubkey);
    if keys.is_empty() {
        stop_untracked_agent_receipts(app, &record.pubkey, &mut terminate)?;
        stop_legacy_scalar_pid(app, record)?;
        super::super::transport_status::clear_pubkey(app, &record.pubkey);
        return Ok(());
    }

    let prior_runtime_pid = record.runtime_pid;
    let prior_updated_at = record.updated_at.clone();
    let prior_last_stopped_at = record.last_stopped_at.clone();
    let prior_last_exit_code = record.last_exit_code;
    let prior_last_error = record.last_error.clone();
    let prior_last_error_code = record.last_error_code.clone();
    let mut errors = Vec::new();
    for key in keys {
        if let Err(error) = stop_managed_agent_pair(app, record, runtimes, &key, &mut terminate) {
            errors.push(format!("{}: {error}", key.relay_url));
        }
    }

    if errors.is_empty() {
        if let Err(error) = stop_untracked_agent_receipts(app, &record.pubkey, &mut terminate) {
            errors.push(error);
        }
    }

    if errors.is_empty() {
        let now = now_iso();
        record.runtime_pid = None;
        record.updated_at = now.clone();
        record.last_stopped_at = Some(now);
        record.last_error = None;
        record.last_error_code = None;
        super::super::remove_agent_pid_file(app, &record.pubkey);
        super::super::transport_status::clear_pubkey(app, &record.pubkey);
        Ok(())
    } else {
        // A failed pair remains in `runtimes`; keep the record-level recovery
        // fields intact as well.  The caller must persist this record and
        // leave its durable deletion intent unresolved, so a retry can still
        // identify the exact instance rather than creating an orphan.
        record.runtime_pid = prior_runtime_pid;
        record.updated_at = prior_updated_at;
        record.last_stopped_at = prior_last_stopped_at;
        record.last_exit_code = prior_last_exit_code;
        record.last_error = prior_last_error.or_else(|| {
            Some(format!(
                "failed to stop one or more managed-agent runtimes: {}",
                errors.join("; ")
            ))
        });
        record.last_error_code = prior_last_error_code;
        Err(format!(
            "failed to stop one or more managed-agent runtimes: {}",
            errors.join("; ")
        ))
    }
}

pub fn stop_managed_agent_process<R: tauri::Runtime>(
    app: &AppHandle<R>,
    record: &mut ManagedAgentRecord,
    runtimes: &mut HashMap<ManagedAgentRuntimeKey, ManagedAgentPairRuntime>,
) -> Result<(), String> {
    stop_managed_agent_process_with(app, record, runtimes, terminate_process)
}

/// Read pair receipts for one record, fail closed on any live receipt that
/// cannot be proven to belong to this desktop instance, then stop the proven
/// pairs. Startup deletion recovery runs before receipt hydration, so this is
/// the receipt-aware stop seam used when the in-memory runtime map is empty.
fn stop_untracked_agent_receipts<R: tauri::Runtime, T: FnMut(u32) -> Result<(), String>>(
    app: &AppHandle<R>,
    pubkey: &str,
    terminate: &mut T,
) -> Result<(), String> {
    let instance_id = current_instance_id(app);
    let receipts = read_agent_runtime_receipts_for_pubkey(app, pubkey)?;
    let mut owned = Vec::new();
    let mut stale = Vec::new();
    for (path, receipt) in receipts {
        if valid_agent_runtime_receipt(&path, &receipt, &instance_id) {
            owned.push((path, receipt));
        } else if process_is_running(receipt.pid) {
            // A live process with an invalid or foreign receipt is not safe to
            // signal. Keep the local record and durable delete operation so a
            // later retry can recover after the owner proof is repaired.
            return Err(
                "managed-agent runtime receipt could not be validated; deletion remains pending"
                    .into(),
            );
        } else {
            stale.push(path);
        }
    }

    // Preflight every receipt before stopping any pair. A later invalid live
    // receipt must not turn a multi-pair delete into a partially untracked
    // operation merely because an earlier pair was valid.
    for (path, receipt) in owned {
        terminate_runtime_receipt_with(
            &path,
            &receipt,
            &mut *terminate,
            process_is_running,
            super::super::remove_agent_runtime_receipt_path,
        )?;
    }
    for path in stale {
        super::super::remove_agent_runtime_receipt_path(&path);
    }
    Ok(())
}

/// Strictly read receipts attributable to `pubkey`. The regular startup
/// sweep intentionally ignores malformed JSON, but deletion must preserve a
/// retry witness when a target-named receipt is unreadable.
fn read_agent_runtime_receipts_for_pubkey<R: tauri::Runtime>(
    app: &AppHandle<R>,
    pubkey: &str,
) -> Result<Vec<(PathBuf, ManagedAgentRuntimeReceipt)>, String> {
    let dir = super::super::managed_agents_base_dir(app)?.join("agent-pids");
    match fs::symlink_metadata(&dir) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(
                "managed-agent runtime receipt directory is unsafe; deletion remains pending"
                    .into(),
            );
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("managed-agent runtime receipts are unavailable".into()),
    }
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("managed-agent runtime receipts are unavailable".into()),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| "managed-agent runtime receipts are unavailable")?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let claims_pubkey = receipt_path_claims_pubkey(&path, pubkey);
        paths.push((path, claims_pubkey));
    }
    // Check target-named receipts first so a corrupt target cannot be hidden
    // behind unrelated files in a large receipt directory.
    paths.sort_by_key(|(_, claims_pubkey)| !*claims_pubkey);

    let mut receipts = Vec::new();
    for (path, path_claims_pubkey) in paths {
        // `path` came from this exact directory's `read_dir`; retain the
        // lexical containment check because this reader is used by deletion,
        // where a path outside `agent-pids/` must never become a termination
        // target even if a filesystem race changes an entry after discovery.
        if path.parent() != Some(dir.as_path()) {
            if path_claims_pubkey {
                return Err(
                    "managed-agent runtime receipt escaped agent-pids; deletion remains pending"
                        .into(),
                );
            }
            continue;
        }

        // Never follow a receipt symlink or accept a socket/FIFO/directory as
        // a receipt. The target-named case is a durable deletion failure; an
        // unrelated malformed entry can be ignored without widening the
        // target's recovery surface.
        let link_metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) if path_claims_pubkey => {
                return Err(
                    "managed-agent runtime receipt is unreadable; deletion remains pending".into(),
                );
            }
            Err(_) => continue,
        };
        if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
            if path_claims_pubkey {
                return Err(
                    "managed-agent runtime receipt is not a regular file; deletion remains pending"
                        .into(),
                );
            }
            continue;
        }

        // `O_NOFOLLOW` closes the check-then-open symlink race. The descriptor
        // metadata below is the authoritative object check, so a replacement
        // with a directory or other non-regular node is also rejected.
        let mut file = match open_runtime_receipt(&path) {
            Ok(file) => file,
            Err(_) if path_claims_pubkey => {
                return Err(
                    "managed-agent runtime receipt is unreadable; deletion remains pending".into(),
                );
            }
            Err(_) => continue,
        };
        let metadata = match file.metadata() {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) if path_claims_pubkey => {
                return Err(
                    "managed-agent runtime receipt is not a regular file; deletion remains pending"
                        .into(),
                );
            }
            Ok(_) => continue,
            Err(_) if path_claims_pubkey => {
                return Err(
                    "managed-agent runtime receipt is unreadable; deletion remains pending".into(),
                );
            }
            Err(_) => continue,
        };
        if metadata.len() > MAX_RUNTIME_RECEIPT_BYTES {
            if path_claims_pubkey {
                return Err(
                    "managed-agent runtime receipt is too large; deletion remains pending".into(),
                );
            }
            continue;
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        if file
            .by_ref()
            .take(MAX_RUNTIME_RECEIPT_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
        {
            if path_claims_pubkey {
                return Err(
                    "managed-agent runtime receipt is unreadable; deletion remains pending".into(),
                );
            }
            continue;
        }
        if bytes.len() as u64 > MAX_RUNTIME_RECEIPT_BYTES {
            if path_claims_pubkey {
                return Err(
                    "managed-agent runtime receipt is too large; deletion remains pending".into(),
                );
            }
            continue;
        }
        let receipt = match serde_json::from_slice::<ManagedAgentRuntimeReceipt>(&bytes) {
            Ok(receipt) => receipt,
            Err(_) if path_claims_pubkey => {
                return Err(
                    "managed-agent runtime receipt is corrupt; deletion remains pending".into(),
                );
            }
            Err(_) => continue,
        };
        if path_claims_pubkey || receipt.key.pubkey.eq_ignore_ascii_case(pubkey) {
            receipts.push((path, receipt));
        }
    }
    Ok(receipts)
}

fn open_runtime_receipt(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

fn receipt_path_claims_pubkey(path: &Path, pubkey: &str) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".json"))
        .and_then(|stem| stem.split_once("__").map(|(candidate, _)| candidate))
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(pubkey))
}

#[cfg(all(test, unix))]
mod stop_failure_tests {
    use super::*;
    use crate::managed_agents::{
        ManagedAgentProcess, ManagedAgentRuntimeKey, ManagedAgentRuntimeReceipt,
    };
    use std::collections::HashMap;
    use std::process::{Command, Stdio};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tauri::Manager;

    struct HomeGuard {
        home: Option<std::ffi::OsString>,
        xdg_data_home: Option<std::ffi::OsString>,
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            match self.home.take() {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            match self.xdg_data_home.take() {
                Some(value) => std::env::set_var("XDG_DATA_HOME", value),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
        }
    }

    fn app() -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = "xyz.nuncio.crew.stop-red".into();
        let state = crate::app_state::build_app_state();
        tauri::test::mock_builder()
            .manage(state)
            .build(context)
            .expect("build the stop-failure fixture app")
    }

    fn process(child: std::process::Child) -> ManagedAgentProcess {
        ManagedAgentProcess {
            child,
            log_path: std::path::PathBuf::new(),
            spawn_started_at_ms: 1,
            spawn_config: crate::managed_agents::spawn_snapshot::SpawnConfigSnapshot {
                acp_command: "buzz-acp".into(),
                command: "/usr/bin/true".into(),
                args: Vec::new(),
                mcp_command: String::new(),
                env: std::collections::BTreeMap::new(),
                relay_url: "ws://localhost:3000".into(),
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
            },
            setup_mode: false,
            adapter_availability: None,
            start_nonce: "stop-red".into(),
            #[cfg(windows)]
            job: None,
        }
    }

    /// Own a finite receipt fixture and its reaper so an assertion panic cannot
    /// leave a child process behind. The production receipt path intentionally
    /// has no `Child` handle after a restart, so the helper reaps concurrently
    /// while the real receipt stop seam proves ownership and termination.
    struct OwnedReceiptChild {
        pid: u32,
        exited: Arc<AtomicBool>,
        reaper: Option<std::thread::JoinHandle<std::io::Result<std::process::ExitStatus>>>,
    }

    impl OwnedReceiptChild {
        fn spawn(instance_id: &str) -> Self {
            use std::os::unix::process::CommandExt;

            let mut command = Command::new("/bin/sleep");
            command
                .arg("30")
                .env("BUZZ_MANAGED_AGENT", instance_id)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0);
            let mut child = command.spawn().expect("spawn finite receipt fixture");
            let pid = child.id();
            let exited = Arc::new(AtomicBool::new(false));
            let reaper_exited = Arc::clone(&exited);
            let reaper = std::thread::spawn(move || {
                let result = child.wait();
                reaper_exited.store(true, Ordering::SeqCst);
                result
            });
            Self {
                pid,
                exited,
                reaper: Some(reaper),
            }
        }

        fn pid(&self) -> u32 {
            self.pid
        }

        fn join(mut self) {
            self.reaper
                .take()
                .expect("receipt fixture reaper")
                .join()
                .expect("receipt fixture reaper thread")
                .expect("receipt fixture wait");
        }
    }

    impl Drop for OwnedReceiptChild {
        fn drop(&mut self) {
            if !self.exited.load(Ordering::SeqCst) {
                let _ = terminate_process(self.pid);
            }
            if let Some(reaper) = self.reaper.take() {
                let _ = reaper.join();
            }
        }
    }

    #[test]
    fn failed_pair_stop_preserves_record_recovery_fields() {
        let app = app();
        let mut child = Command::new("/usr/bin/true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the short-lived stop fixture");
        child.wait().expect("reap the stop fixture before teardown");

        let mut record = crate::managed_agents::runtime::test_fixtures::fixture(
            crate::managed_agents::RespondTo::OwnerOnly,
            Vec::new(),
            None,
        );
        record.pubkey = "a".repeat(64);
        record.runtime_pid = Some(4242);
        record.last_exit_code = Some(7);
        record.last_error = Some("prior stop error".into());
        let prior_updated_at = record.updated_at.clone();
        let key = ManagedAgentRuntimeKey::new(record.pubkey.clone(), &record.relay_url)
            .expect("fixture runtime key");
        let runtime = ManagedAgentPairRuntime::starting(process(child));
        let mut runtimes = HashMap::from([(key, runtime)]);

        let result = stop_managed_agent_process_with(
            &app.handle().clone(),
            &mut record,
            &mut runtimes,
            |_| Err("injected pair stop failure".into()),
        );

        assert!(result.is_err(), "the injected pair stop must fail");
        // Regression: an aggregate stop failure must preserve the exact
        // record fields that still describe the recoverable runtime.
        assert_eq!(record.runtime_pid, Some(4242));
        assert_eq!(record.updated_at, prior_updated_at);
        assert_eq!(record.last_stopped_at, None);
        assert_eq!(record.last_exit_code, Some(7));
        assert_eq!(record.last_error.as_deref(), Some("prior stop error"));
        assert_eq!(runtimes.len(), 1, "failed runtime remains stoppable");
    }

    #[test]
    fn empty_runtime_map_stops_owned_pair_receipt_before_local_delete() {
        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("temporary app-data root");
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).expect("temporary home");
        let _env_guard = HomeGuard {
            home: std::env::var_os("HOME"),
            xdg_data_home: std::env::var_os("XDG_DATA_HOME"),
        };
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_DATA_HOME", &home);

        let first_app = app();
        let pubkey = "b".repeat(64);
        let key = ManagedAgentRuntimeKey::new(pubkey.clone(), "ws://localhost:3000")
            .expect("fixture pair key");
        let child = OwnedReceiptChild::spawn(&first_app.config().identifier);
        let pid = child.pid();
        let receipt = ManagedAgentRuntimeReceipt {
            key,
            pid,
            desktop_instance_id: first_app.config().identifier.clone(),
            started_at: "now".into(),
        };
        crate::managed_agents::write_agent_runtime_receipt(&first_app.handle(), &receipt)
            .expect("write pair receipt");
        // Startup recovery runs before runtime receipt hydration. Rebuild the
        // app state so the production stop seam is exercised with an empty
        // in-memory runtime map after the prior desktop instance is gone.
        drop(first_app);
        let app = app();
        assert!(app
            .state::<crate::app_state::AppState>()
            .managed_agent_processes
            .lock()
            .expect("lock fresh runtime map")
            .is_empty());

        let mut record = crate::managed_agents::runtime::test_fixtures::fixture(
            crate::managed_agents::RespondTo::OwnerOnly,
            Vec::new(),
            None,
        );
        record.pubkey = pubkey.clone();
        // A fresh AppState has no legacy scalar PID; the receipt is the only
        // recovery witness this restart path may use.
        record.runtime_pid = None;
        record.relay_url = "ws://localhost:3000".into();
        let mut runtimes = HashMap::new();

        stop_managed_agent_process(&app.handle(), &mut record, &mut runtimes)
            .expect("receipt-owned pair must be stopped before deletion");
        child.join();

        assert_eq!(record.runtime_pid, None);
        assert!(
            crate::managed_agents::read_all_agent_runtime_receipts(&app.handle())
                .into_iter()
                .all(|(_, receipt)| receipt.key.pubkey != pubkey),
            "stopped pair receipt must be removed only after the child exits"
        );
    }

    #[test]
    fn empty_runtime_map_refuses_live_foreign_receipt() {
        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("temporary app-data root");
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).expect("temporary home");
        let _env_guard = HomeGuard {
            home: std::env::var_os("HOME"),
            xdg_data_home: std::env::var_os("XDG_DATA_HOME"),
        };
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_DATA_HOME", &home);

        let app = app();
        let pubkey = "c".repeat(64);
        let key = ManagedAgentRuntimeKey::new(pubkey.clone(), "ws://localhost:3000")
            .expect("fixture pair key");
        let child = OwnedReceiptChild::spawn(&app.config().identifier);
        let pid = child.pid();
        let receipt = ManagedAgentRuntimeReceipt {
            key,
            pid,
            desktop_instance_id: "foreign-desktop".into(),
            started_at: "now".into(),
        };
        crate::managed_agents::write_agent_runtime_receipt(&app.handle(), &receipt)
            .expect("write foreign receipt");

        let mut record = crate::managed_agents::runtime::test_fixtures::fixture(
            crate::managed_agents::RespondTo::OwnerOnly,
            Vec::new(),
            None,
        );
        record.pubkey = pubkey.clone();
        // Bind this restart-shaped fixture to the durable pair receipt rather
        // than letting the legacy scalar path mask a receipt-reader failure.
        record.runtime_pid = None;
        let mut runtimes = HashMap::new();

        let error = stop_managed_agent_process(&app.handle(), &mut record, &mut runtimes)
            .expect_err("foreign live receipt must fail closed");
        assert!(error.contains("receipt"));
        assert_eq!(record.runtime_pid, None);
        assert!(
            crate::managed_agents::read_all_agent_runtime_receipts(&app.handle())
                .into_iter()
                .any(|(_, candidate)| candidate.key.pubkey == pubkey),
            "the foreign live receipt remains as a recovery witness"
        );

        drop(child);
    }

    #[test]
    fn target_named_corrupt_receipt_keeps_delete_pending() {
        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("temporary app-data root");
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).expect("temporary home");
        let _env_guard = HomeGuard {
            home: std::env::var_os("HOME"),
            xdg_data_home: std::env::var_os("XDG_DATA_HOME"),
        };
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_DATA_HOME", &home);

        let app = app();
        let pubkey = "d".repeat(64);
        let dir = crate::managed_agents::managed_agents_base_dir(&app.handle())
            .expect("resolve managed-agent data directory")
            .join("agent-pids");
        std::fs::create_dir_all(&dir).expect("create receipt directory");
        std::fs::write(dir.join(format!("{pubkey}__corrupt.json")), b"{")
            .expect("write corrupt target receipt");

        let mut terminate = |_pid: u32| Ok::<(), String>(());
        let error = stop_untracked_agent_receipts(&app.handle(), &pubkey, &mut terminate)
            .expect_err("corrupt target receipt must keep deletion pending");
        assert!(error.contains("corrupt"));
    }

    #[test]
    fn target_named_symlink_receipt_keeps_delete_pending() {
        use std::os::unix::fs::symlink;

        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("temporary app-data root");
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).expect("temporary home");
        let _env_guard = HomeGuard {
            home: std::env::var_os("HOME"),
            xdg_data_home: std::env::var_os("XDG_DATA_HOME"),
        };
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_DATA_HOME", &home);

        let app = app();
        let pubkey = "f".repeat(64);
        let dir = crate::managed_agents::managed_agents_base_dir(&app.handle())
            .expect("resolve managed-agent data directory")
            .join("agent-pids");
        std::fs::create_dir_all(&dir).expect("create receipt directory");
        let outside = temp.path().join("outside-receipt.json");
        std::fs::write(&outside, b"{}").expect("write outside receipt target");
        symlink(&outside, dir.join(format!("{pubkey}__symlink.json")))
            .expect("write symlink receipt");

        let mut terminate = |_pid: u32| Ok::<(), String>(());
        let error = stop_untracked_agent_receipts(&app.handle(), &pubkey, &mut terminate)
            .expect_err("target symlink must keep deletion pending");
        assert!(error.contains("regular file"));
        assert!(
            outside.exists(),
            "reader must not follow the outside target"
        );
    }

    #[test]
    fn target_named_nonregular_receipt_keeps_delete_pending() {
        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("temporary app-data root");
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).expect("temporary home");
        let _env_guard = HomeGuard {
            home: std::env::var_os("HOME"),
            xdg_data_home: std::env::var_os("XDG_DATA_HOME"),
        };
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_DATA_HOME", &home);

        let app = app();
        let pubkey = "g".repeat(64);
        let path = crate::managed_agents::managed_agents_base_dir(&app.handle())
            .expect("resolve managed-agent data directory")
            .join("agent-pids")
            .join(format!("{pubkey}__directory.json"));
        std::fs::create_dir_all(&path).expect("write directory receipt");

        let mut terminate = |_pid: u32| Ok::<(), String>(());
        let error = stop_untracked_agent_receipts(&app.handle(), &pubkey, &mut terminate)
            .expect_err("target directory must keep deletion pending");
        assert!(error.contains("regular file"));
    }

    #[test]
    fn agent_pids_symlink_keeps_delete_pending() {
        use std::os::unix::fs::symlink;

        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("temporary app-data root");
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).expect("temporary home");
        let _env_guard = HomeGuard {
            home: std::env::var_os("HOME"),
            xdg_data_home: std::env::var_os("XDG_DATA_HOME"),
        };
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_DATA_HOME", &home);

        let app = app();
        let base = crate::managed_agents::managed_agents_base_dir(&app.handle())
            .expect("resolve managed-agent data directory");
        let outside = temp.path().join("outside-agent-pids");
        std::fs::create_dir_all(&outside).expect("create outside receipt directory");
        symlink(&outside, base.join("agent-pids")).expect("write agent-pids symlink");

        let mut terminate = |_pid: u32| Ok::<(), String>(());
        let error = stop_untracked_agent_receipts(&app.handle(), &"h".repeat(64), &mut terminate)
            .expect_err("agent-pids symlink must keep deletion pending");
        assert!(error.contains("directory is unsafe"));
    }

    #[test]
    fn target_named_oversized_receipt_is_bounded_and_keeps_delete_pending() {
        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("temporary app-data root");
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).expect("temporary home");
        let _env_guard = HomeGuard {
            home: std::env::var_os("HOME"),
            xdg_data_home: std::env::var_os("XDG_DATA_HOME"),
        };
        std::env::set_var("HOME", &home);
        std::env::set_var("XDG_DATA_HOME", &home);

        let app = app();
        let pubkey = "e".repeat(64);
        let key = ManagedAgentRuntimeKey::new(pubkey.clone(), "ws://localhost:3000")
            .expect("fixture pair key");
        let receipt = ManagedAgentRuntimeReceipt {
            key,
            pid: 999_999_999,
            desktop_instance_id: app.config().identifier.clone(),
            started_at: "x".repeat(MAX_RUNTIME_RECEIPT_BYTES as usize),
        };
        let dir = crate::managed_agents::managed_agents_base_dir(&app.handle())
            .expect("resolve managed-agent data directory")
            .join("agent-pids");
        std::fs::create_dir_all(&dir).expect("create receipt directory");
        let path = dir.join(format!("{pubkey}__oversized.json"));
        let payload = serde_json::to_vec(&receipt).expect("serialize oversized receipt");
        assert!(payload.len() as u64 > MAX_RUNTIME_RECEIPT_BYTES);
        std::fs::write(path, payload).expect("write oversized target receipt");

        let mut terminate = |_pid: u32| Ok::<(), String>(());
        let error = stop_untracked_agent_receipts(&app.handle(), &pubkey, &mut terminate)
            .expect_err("oversized target receipt must keep deletion pending");
        assert!(error.contains("too large"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_preserving_restart_targets_exact_original_relays() {
        let agent = "aa".repeat(32);
        let other = "bb".repeat(32);
        let first = ManagedAgentRuntimeKey::new(&agent, "wss://one.example").unwrap();
        let second = ManagedAgentRuntimeKey::new(&agent, "wss://two.example").unwrap();
        let unrelated = ManagedAgentRuntimeKey::new(other, "wss://fallback.example").unwrap();
        let runtimes = HashMap::from([(first, ()), (second, ()), (unrelated, ())]);

        let mut relays = managed_agent_runtime_relay_urls(&runtimes, &agent);
        relays.sort();
        assert_eq!(
            relays,
            vec![
                "wss://one.example".to_string(),
                "wss://two.example".to_string()
            ]
        );
    }

    #[test]
    fn pair_scoped_selection_targets_only_the_exact_pair() {
        // stop_managed_agent_workspace_pair resolves one key and removes only
        // that map entry: the same agent's pair on another relay and other
        // agents' pairs must survive a pair-scoped stop.
        let agent = "aa".repeat(32);
        let other = "bb".repeat(32);
        let viewed = ManagedAgentRuntimeKey::new(&agent, "wss://one.example").unwrap();
        let elsewhere = ManagedAgentRuntimeKey::new(&agent, "wss://two.example").unwrap();
        let unrelated = ManagedAgentRuntimeKey::new(other, "wss://one.example").unwrap();
        let mut runtimes = HashMap::from([
            (viewed.clone(), ()),
            (elsewhere.clone(), ()),
            (unrelated.clone(), ()),
        ]);

        // Non-canonical spelling of the viewed workspace relay resolves to
        // the same canonical key that spawn stamped.
        let resolved = ManagedAgentRuntimeKey::new(&agent, "WSS://One.Example:443/").unwrap();
        assert_eq!(resolved, viewed);
        assert!(runtimes.remove(&resolved).is_some());
        assert!(runtimes.contains_key(&elsewhere));
        assert!(runtimes.contains_key(&unrelated));
    }

    #[test]
    fn agent_wide_selection_drains_every_pair_only_for_that_agent() {
        let agent = "aa".repeat(32);
        let other = "bb".repeat(32);
        let first = ManagedAgentRuntimeKey::new(&agent, "wss://one.example").unwrap();
        let second = ManagedAgentRuntimeKey::new(&agent, "wss://two.example").unwrap();
        let unrelated = ManagedAgentRuntimeKey::new(other, "wss://one.example").unwrap();
        let runtimes = HashMap::from([(first.clone(), ()), (second.clone(), ()), (unrelated, ())]);

        let mut selected = managed_agent_runtime_keys(&runtimes, &agent);
        selected.sort_by(|left, right| left.relay_url.cmp(&right.relay_url));
        assert_eq!(selected, vec![first, second]);
    }
}
