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

#[cfg(unix)]
struct OwnedReceiptChild {
    pid: u32,
    exited: std::sync::Arc<std::sync::atomic::AtomicBool>,
    reaper: Option<std::thread::JoinHandle<std::io::Result<std::process::ExitStatus>>>,
}

#[cfg(unix)]
struct HomeGuard {
    home: Option<std::ffi::OsString>,
    xdg_data_home: Option<std::ffi::OsString>,
}

#[cfg(unix)]
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

#[cfg(unix)]
impl OwnedReceiptChild {
    fn spawn(instance_id: &str) -> Self {
        use std::os::unix::process::CommandExt;
        let mut command = std::process::Command::new("/bin/sleep");
        command
            .arg("30")
            .env("BUZZ_MANAGED_AGENT", instance_id)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .process_group(0);
        let mut child = command.spawn().expect("spawn live receipt fixture");
        let pid = child.id();
        let exited = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reaper_exited = std::sync::Arc::clone(&exited);
        let reaper = std::thread::spawn(move || {
            let result = child.wait();
            reaper_exited.store(true, std::sync::atomic::Ordering::SeqCst);
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

    fn join(mut self) -> std::process::ExitStatus {
        self.reaper
            .take()
            .expect("live receipt reaper")
            .join()
            .expect("join live receipt reaper")
            .expect("wait for live receipt child")
    }
}

#[cfg(unix)]
impl Drop for OwnedReceiptChild {
    fn drop(&mut self) {
        if !self.exited.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = crate::managed_agents::terminate_process(self.pid);
        }
        if let Some(reaper) = self.reaper.take() {
            let _ = reaper.join();
        }
    }
}

#[cfg(unix)]
fn direct_delete_test_app(
    identifier: String,
    keys: nostr::Keys,
) -> tauri::App<tauri::test::MockRuntime> {
    let state = crate::app_state::build_app_state();
    *state.keys.lock().unwrap() = keys;
    *state.relay_url_override.lock().unwrap() = Some("wss://direct-delete-seam.example".into());
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = identifier;
    tauri::test::mock_builder()
        .manage(state)
        .build(context)
        .expect("build direct-delete fixture app")
}

#[cfg(unix)]
#[test]
fn production_live_receipt_failure_persists_failed_delete_and_fresh_restart_recovers() {
    use crate::{
        app_state::owner_scope::capture,
        managed_agents::{
            self, AgentDefinition, ManagedAgentRuntimeKey, ManagedAgentRuntimeReceipt,
        },
        owner_operations::OperationStatus,
    };
    use std::collections::BTreeMap;
    use tauri::Manager;

    let _path_guard = managed_agents::lock_path_mutex();
    let temp = tempfile::tempdir().expect("temporary app-data root");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("temporary home");
    let _home_guard = HomeGuard {
        home: std::env::var_os("HOME"),
        xdg_data_home: std::env::var_os("XDG_DATA_HOME"),
    };
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_DATA_HOME", &home);

    let owner_keys = nostr::Keys::generate();
    let identifier = format!(
        "xyz.nuncio.crew.direct-delete-seam-{}",
        uuid::Uuid::new_v4().simple()
    );
    let app = direct_delete_test_app(identifier.clone(), owner_keys.clone());
    let mut record = AgentDefinition {
        id: "direct-delete-seam".into(),
        display_name: "Direct Delete Seam".into(),
        avatar_url: None,
        description: None,
        system_prompt: "Exercise durable direct deletion recovery.".into(),
        runtime: None,
        model: None,
        provider: None,
        name_pool: Vec::new(),
        is_builtin: false,
        is_active: true,
        shared: false,
        source_team: None,
        source_team_persona_slug: None,
        catalog_source: None,
        team_catalog_source: None,
        env_vars: BTreeMap::new(),
        respond_to: None,
        respond_to_allowlist: Vec::new(),
        parallelism: None,
        created_at: "2026-09-13T00:00:00Z".into(),
        updated_at: "2026-09-13T00:00:00Z".into(),
    }
    .into_agent_record();
    let pubkey = nostr::Keys::generate().public_key().to_hex();
    record.pubkey = pubkey.clone();
    record.relay_url = "wss://direct-delete-seam.example".into();
    record.acp_command = "buzz-acp".into();
    record.agent_command = "goose".into();
    managed_agents::save_managed_agents(app.handle(), std::slice::from_ref(&record))
        .expect("persist direct-delete record");

    let child = OwnedReceiptChild::spawn(&identifier);
    let receipt_key = ManagedAgentRuntimeKey::new(pubkey.clone(), &record.relay_url)
        .expect("fixture runtime key");
    let receipt = ManagedAgentRuntimeReceipt {
        key: receipt_key.clone(),
        pid: child.pid(),
        desktop_instance_id: identifier.clone(),
        started_at: "2026-09-13T00:00:00Z".into(),
    };
    managed_agents::write_agent_runtime_receipt(&app.handle(), &receipt)
        .expect("persist live pair receipt");
    let receipt_path = managed_agents::managed_agents_base_dir(app.handle())
        .expect("resolve runtime receipt directory")
        .join("agent-pids")
        .join(format!("{}.json", receipt.key.runtime_id()));
    let receipt_bytes = std::fs::read(&receipt_path).expect("read valid receipt bytes");
    assert!(managed_agents::process_is_running(child.pid()));
    assert!(managed_agents::process_has_buzz_marker(
        child.pid(),
        &identifier
    ));
    assert!(managed_agents::valid_agent_runtime_receipt(
        &receipt_path,
        &receipt,
        &identifier
    ));
    let receipt_target = temp.path().join("owned-receipt-target.json");
    std::fs::write(&receipt_target, &receipt_bytes).expect("write symlink receipt target");
    std::fs::remove_file(&receipt_path).expect("remove regular receipt before replacement");
    std::os::unix::fs::symlink(&receipt_target, &receipt_path)
        .expect("replace receipt with symlink fixture");
    assert!(std::fs::symlink_metadata(&receipt_path)
        .expect("inspect replaced receipt")
        .file_type()
        .is_symlink());

    tauri::async_runtime::block_on(async {
        let captured = capture(app.handle().clone())
            .await
            .expect("capture delete owner/workspace scope");
        let token = captured.token;
        let payload = Payload::new(RecordFence::capture(&record), Vec::new(), &pubkey)
            .expect("build direct-delete payload");
        let mut journal = open_journal_store(app.handle()).expect("open delete journal");
        let new_operation = NewOperation {
            id: uuid::Uuid::new_v4().to_string(),
            kind: OperationKind::ManagedAgentDelete,
            resource_key: pubkey.clone(),
            payload: serde_json::to_value(payload).expect("encode direct-delete payload"),
        };
        let operation = match create_native_claim(&mut journal, &token.scope, new_operation)
            .expect("claim direct-delete operation")
        {
            CreateResult::Created(operation) | CreateResult::Existing(operation) => operation,
        };
        drop(journal);

        let expected_stop_error =
            "managed-agent runtime receipt is not a regular file; deletion remains pending";
        let first_attempt = resume(
            app.handle().clone(),
            token.clone(),
            operation.clone(),
            false,
        )
        .await;
        assert_eq!(
            first_attempt.expect_err("strict receipt failure must propagate"),
            expected_stop_error
        );
        let journal = open_journal_store(app.handle()).expect("reopen failed delete journal");
        let failed = journal
            .load(&operation.scope, &operation.id)
            .expect("load failed direct-delete operation");
        let failed_payload: Payload = serde_json::from_value(failed.payload.clone())
            .expect("decode failed direct-delete payload");
        assert_eq!(failed.status, OperationStatus::Failed);
        assert!(!failed.reconciled);
        assert!(!failed_payload.local_removed);
        assert!(failed_payload.failures > 0);
        assert_eq!(
            failed_payload.last_error.as_deref(),
            Some(expected_stop_error)
        );
        assert!(pending_in_store(&journal, &pubkey).expect("inspect pending delete claim"));
        assert!(
            managed_agents::load_managed_agents(app.handle())
                .expect("load retained record")
                .iter()
                .any(|candidate| candidate.pubkey == pubkey),
            "a failed stop must retain the exact record for recovery"
        );
        assert!(std::fs::symlink_metadata(&receipt_path)
            .expect("failed delete must retain receipt path")
            .file_type()
            .is_symlink());
        assert!(
            receipt_target.exists(),
            "symlink target must remain untouched"
        );
        assert!(managed_agents::process_is_running(child.pid()));
        drop(journal);

        std::fs::remove_file(&receipt_path).expect("remove failed receipt symlink");
        managed_agents::write_agent_runtime_receipt(&app.handle(), &receipt)
            .expect("restore regular receipt for fresh recovery");
        assert!(std::fs::symlink_metadata(&receipt_path)
            .expect("inspect restored receipt")
            .file_type()
            .is_file());
        assert!(managed_agents::valid_agent_runtime_receipt(
            &receipt_path,
            &receipt,
            &identifier
        ));
        assert!(managed_agents::process_is_running(child.pid()));
        drop(app);
        let app = direct_delete_test_app(identifier, owner_keys);
        assert!(app
            .state::<crate::app_state::AppState>()
            .managed_agent_processes
            .lock()
            .expect("lock fresh runtime map")
            .is_empty());
        let restarted_token = capture(app.handle().clone())
            .await
            .expect("capture fresh restart scope")
            .token;
        assert_eq!(restarted_token, token, "restart must preserve owner scope");
        assert!(managed_agents::process_is_running(child.pid()));

        let app_handle = app.handle();
        recover(&app_handle)
            .await
            .expect("fresh AppState recovery must retry direct deletion");
        use std::os::unix::process::ExitStatusExt;
        let exit_status = child.join();
        assert!(
            exit_status.signal().is_some(),
            "fresh recovery must terminate the live receipt child"
        );

        let journal = open_journal_store(app.handle()).expect("open recovered delete journal");
        let completed = journal
            .load(&operation.scope, &operation.id)
            .expect("load recovered direct-delete operation");
        let completed_payload: Payload = serde_json::from_value(completed.payload)
            .expect("decode recovered direct-delete payload");
        assert_eq!(completed.status, OperationStatus::Complete);
        assert!(completed.reconciled);
        assert!(completed_payload.local_removed);
        assert!(completed_payload.key_removed);
        assert!(completed_payload.tombstone_enqueued);
        assert!(!pending_in_store(&journal, &pubkey).expect("inspect completed delete claim"));
        assert!(
            managed_agents::load_managed_agents(app.handle())
                .expect("load removed record store")
                .iter()
                .all(|candidate| candidate.pubkey != pubkey),
            "fresh recovery must remove the original record after stopping it"
        );
        assert!(
            managed_agents::read_all_agent_runtime_receipts(app.handle())
                .into_iter()
                .all(|(_, receipt)| receipt.key.pubkey != pubkey),
            "fresh recovery must retire the consumed runtime receipt"
        );
        assert_eq!(
            std::fs::symlink_metadata(&receipt_path)
                .expect_err("fresh recovery must remove the receipt path")
                .kind(),
            std::io::ErrorKind::NotFound
        );
        assert_eq!(
            std::fs::read(&receipt_target).expect("failed symlink target must remain readable"),
            receipt_bytes,
            "receipt cleanup must not change the failed symlink target"
        );
    });
}
