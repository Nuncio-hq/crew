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

#[test]
fn receipt_directory_scan_counts_unrelated_entries_before_stopping() {
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
    let dir = crate::managed_agents::managed_agents_base_dir(app.handle())
        .expect("managed-agent data directory")
        .join("agent-pids");
    std::fs::create_dir_all(&dir).expect("receipt directory");
    for index in 0..4096 {
        std::fs::write(dir.join(format!("unrelated-{index}.txt")), b"")
            .expect("bounded unrelated fixture entry");
    }
    let mut terminate = |_pid| -> Result<(), String> {
        panic!("an unverified directory must not stop any process")
    };
    let pubkey = "f".repeat(64);
    stop_untracked_agent_receipts(app.handle(), &pubkey, &mut terminate)
        .expect("a complete scan at the entry limit is valid");
    std::fs::write(dir.join("over-limit.txt"), b"").expect("one excess entry");
    let error = stop_untracked_agent_receipts(app.handle(), &pubkey, &mut terminate)
        .expect_err("an incomplete receipt scan must keep deletion pending");
    assert_eq!(
        error,
        "managed-agent runtime receipt scan exceeded its entry limit; deletion remains pending"
    );
}

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
    _ready_dir: tempfile::TempDir,
}

impl OwnedReceiptChild {
    fn spawn(instance_id: &str) -> Self {
        let ready_dir = tempfile::tempdir().expect("temporary receipt readiness directory");
        let ready_path = ready_dir.path().join("ready");
        let mut command =
            crate::managed_agent_delete::receipt_child_command(instance_id, &ready_path);
        let mut child = command.spawn().expect("spawn finite receipt fixture");
        let pid = child.id();
        let exited = Arc::new(AtomicBool::new(false));
        let reaper_exited = Arc::clone(&exited);
        let reaper = std::thread::spawn(move || {
            let result = child.wait();
            reaper_exited.store(true, Ordering::SeqCst);
            result
        });
        let child = Self {
            pid,
            exited,
            reaper: Some(reaper),
            _ready_dir: ready_dir,
        };
        child.wait_until_ready(&ready_path);
        child
    }

    fn pid(&self) -> u32 {
        self.pid
    }

    fn wait_until_ready(&self, ready_path: &std::path::Path) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !ready_path.is_file() {
            assert!(
                !self.exited.load(Ordering::SeqCst),
                "receipt fixture exited before signaling readiness"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "receipt fixture did not signal readiness before the deadline"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn join(mut self) -> std::process::ExitStatus {
        self.reaper
            .take()
            .expect("receipt fixture reaper")
            .join()
            .expect("receipt fixture reaper thread")
            .expect("receipt fixture wait")
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

    let result =
        stop_managed_agent_process_with(&app.handle().clone(), &mut record, &mut runtimes, |_| {
            Err("injected pair stop failure".into())
        });

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
    crate::managed_agents::write_agent_runtime_receipt(first_app.handle(), &receipt)
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

    stop_managed_agent_process(app.handle(), &mut record, &mut runtimes)
        .expect("receipt-owned pair must be stopped before deletion");
    use std::os::unix::process::ExitStatusExt;
    let exit_status = child.join();
    assert!(
        exit_status.signal().is_some(),
        "receipt-owned stop must terminate the live child"
    );

    assert_eq!(record.runtime_pid, None);
    assert!(
        crate::managed_agents::read_all_agent_runtime_receipts(app.handle())
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
    crate::managed_agents::write_agent_runtime_receipt(app.handle(), &receipt)
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

    let error = stop_managed_agent_process(app.handle(), &mut record, &mut runtimes)
        .expect_err("foreign live receipt must fail closed");
    assert!(error.contains("receipt"));
    assert_eq!(record.runtime_pid, None);
    assert!(
        crate::managed_agents::read_all_agent_runtime_receipts(app.handle())
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
    let dir = crate::managed_agents::managed_agents_base_dir(app.handle())
        .expect("resolve managed-agent data directory")
        .join("agent-pids");
    std::fs::create_dir_all(&dir).expect("create receipt directory");
    std::fs::write(dir.join(format!("{pubkey}__corrupt.json")), b"{")
        .expect("write corrupt target receipt");

    let mut terminate = |_pid: u32| Ok::<(), String>(());
    let error = stop_untracked_agent_receipts(app.handle(), &pubkey, &mut terminate)
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
    let dir = crate::managed_agents::managed_agents_base_dir(app.handle())
        .expect("resolve managed-agent data directory")
        .join("agent-pids");
    std::fs::create_dir_all(&dir).expect("create receipt directory");
    let outside = temp.path().join("outside-receipt.json");
    std::fs::write(&outside, b"{}").expect("write outside receipt target");
    symlink(&outside, dir.join(format!("{pubkey}__symlink.json"))).expect("write symlink receipt");

    let mut terminate = |_pid: u32| Ok::<(), String>(());
    let error = stop_untracked_agent_receipts(app.handle(), &pubkey, &mut terminate)
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
    let path = crate::managed_agents::managed_agents_base_dir(app.handle())
        .expect("resolve managed-agent data directory")
        .join("agent-pids")
        .join(format!("{pubkey}__directory.json"));
    std::fs::create_dir_all(&path).expect("write directory receipt");

    let mut terminate = |_pid: u32| Ok::<(), String>(());
    let error = stop_untracked_agent_receipts(app.handle(), &pubkey, &mut terminate)
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
    let base = crate::managed_agents::managed_agents_base_dir(app.handle())
        .expect("resolve managed-agent data directory");
    let outside = temp.path().join("outside-agent-pids");
    std::fs::create_dir_all(&outside).expect("create outside receipt directory");
    symlink(&outside, base.join("agent-pids")).expect("write agent-pids symlink");

    let mut terminate = |_pid: u32| Ok::<(), String>(());
    let error = stop_untracked_agent_receipts(app.handle(), &"h".repeat(64), &mut terminate)
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
    let dir = crate::managed_agents::managed_agents_base_dir(app.handle())
        .expect("resolve managed-agent data directory")
        .join("agent-pids");
    std::fs::create_dir_all(&dir).expect("create receipt directory");
    let path = dir.join(format!("{pubkey}__oversized.json"));
    let payload = serde_json::to_vec(&receipt).expect("serialize oversized receipt");
    assert!(payload.len() as u64 > MAX_RUNTIME_RECEIPT_BYTES);
    std::fs::write(path, payload).expect("write oversized target receipt");

    let mut terminate = |_pid: u32| Ok::<(), String>(());
    let error = stop_untracked_agent_receipts(app.handle(), &pubkey, &mut terminate)
        .expect_err("oversized target receipt must keep deletion pending");
    assert!(error.contains("too large"));
}
