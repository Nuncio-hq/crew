//! Production-seam coverage for the persona cascade coordinator.
//!
//! This drives the private coordinator through a real Tauri `MockRuntime`,
//! unified persona store, owner-operation SQLite journal, and retention
//! tombstone path. It deliberately breaks the tombstone path after the local
//! persona commit so the retry witness and the later recovery are both
//! exercised by the shipped coordinator.

use super::*;
use crate::{
    app_state::{build_app_state, owner_scope::capture},
    managed_agents::{self, AgentDefinition, ManagedAgentRuntimeKey, ManagedAgentRuntimeReceipt},
    owner_operations::{NewOperation, OperationKind, OperationStatus},
};
use buzz_core_pkg::kind::KIND_PERSONA;
use std::collections::BTreeMap;
use tauri::Manager;

const RELAY: &str = "wss://persona-delete-seam.example";

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

fn persona() -> AgentDefinition {
    AgentDefinition {
        id: "persona-delete-seam".into(),
        display_name: "Persona Delete Seam".into(),
        avatar_url: None,
        description: None,
        system_prompt: "Exercise the durable delete coordinator.".into(),
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
}

fn mock_app_with(
    identifier: String,
    keys: Option<nostr::Keys>,
) -> tauri::App<tauri::test::MockRuntime> {
    let state = build_app_state();
    if let Some(keys) = keys {
        *state.keys.lock().unwrap() = keys;
    }
    *state.relay_url_override.lock().unwrap() = Some(RELAY.into());
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = identifier;
    tauri::test::mock_builder()
        .manage(state)
        .build(context)
        .expect("build the production coordinator fixture app")
}

fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
    mock_app_with(
        format!(
            "xyz.nuncio.crew.persona-delete-seam-{}",
            uuid::Uuid::new_v4().simple()
        ),
        None,
    )
}

#[test]
fn production_zero_target_cascade_retries_after_tombstone_failure() {
    let _path_guard = managed_agents::lock_path_mutex();
    let temp = tempfile::tempdir().expect("temporary app-data root");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("temporary home");
    let _env_guard = HomeGuard {
        home: std::env::var_os("HOME"),
        xdg_data_home: std::env::var_os("XDG_DATA_HOME"),
    };
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_DATA_HOME", &home);

    let app = mock_app();
    let persona = persona();
    managed_agents::save_personas(app.handle(), std::slice::from_ref(&persona))
        .expect("persist persona fixture");

    tauri::async_runtime::block_on(async {
        let captured = capture(app.handle().clone())
            .await
            .expect("capture owner/workspace scope");
        let token = captured.token;
        let owner_keys = captured.keys;
        let app_identifier = app.config().identifier.clone();
        let operation = begin_persona_cascade(app.handle(), token.clone(), &persona.id)
            .await
            .expect("reserve the production coordinator");
        let base_dir = managed_agents::managed_agents_base_dir(app.handle())
            .expect("resolve managed-agent data directory");
        let retention_path = base_dir.join("retention");
        std::fs::write(&retention_path, b"retention path failure")
            .expect("inject a retention-open failure");

        let first_attempt =
            resume_persona_cascade(app.handle().clone(), token.clone(), operation.clone(), true)
                .await;
        assert!(
            first_attempt.is_err(),
            "a failed tombstone enqueue must keep the coordinator unresolved"
        );

        let journal = crate::managed_agent_delete::open_journal_store(app.handle())
            .expect("open the production recovery journal");
        let failed = journal
            .load(&operation.scope, &operation.id)
            .expect("load failed coordinator snapshot");
        let failed_payload: Payload = serde_json::from_value(failed.payload.clone())
            .expect("decode failed coordinator payload");
        assert_eq!(failed.status, OperationStatus::Failed);
        assert!(!failed.reconciled);
        assert!(
            !failed_payload
                .cascade
                .as_ref()
                .expect("cascade payload")
                .persona_removed
        );
        assert!(failed_payload.failures > 0);
        assert!(
            crate::managed_agent_delete::pending_in_store(&journal, &failed.resource_key)
                .expect("inspect the unresolved coordinator claim")
        );
        drop(journal);
        assert!(
            managed_agents::load_personas(app.handle())
                .expect("load persona store after failed commit")
                .into_iter()
                .all(|candidate| candidate.id != persona.id),
            "the local persona commit is absent, but its journal retry witness remains"
        );

        std::fs::remove_file(&retention_path).expect("remove injected retention failure");
        std::fs::create_dir_all(&retention_path).expect("restore retention directory");

        // Retry through a newly constructed AppState. A same-process retry can
        // accidentally succeed from hydrated runtime/store state that is not
        // available after the crash boundary this journal is meant to cover.
        drop(app);
        let app = mock_app_with(app_identifier, Some(owner_keys));
        let restarted_token = capture(app.handle().clone())
            .await
            .expect("capture owner/workspace scope after restart")
            .token;
        assert_eq!(
            restarted_token, token,
            "restart must preserve the scope fence"
        );
        resume_persona_cascade(app.handle().clone(), restarted_token.clone(), failed, true)
            .await
            .expect("manual retry must finish the durable coordinator");

        let journal = crate::managed_agent_delete::open_journal_store(app.handle())
            .expect("reopen the production recovery journal");
        let completed = journal
            .load(&operation.scope, &operation.id)
            .expect("load completed coordinator snapshot");
        let completed_payload: Payload = serde_json::from_value(completed.payload.clone())
            .expect("decode completed coordinator payload");
        assert_eq!(completed.status, OperationStatus::Complete);
        assert!(completed.reconciled);
        assert!(
            completed_payload
                .cascade
                .as_ref()
                .expect("completed cascade payload")
                .persona_removed
        );
        assert!(
            !crate::managed_agent_delete::pending_in_store(&journal, &completed.resource_key)
                .expect("inspect completed coordinator claim")
        );
        drop(journal);

        let db_path = managed_agents::retention::scoped_retention_db_path(
            &managed_agents::managed_agents_base_dir(app.handle())
                .expect("resolve restarted managed-agent data directory"),
            RELAY,
            &restarted_token.scope.owner,
        );
        let connection = managed_agents::retention::open_retention_db(&db_path)
            .expect("open the recovered retention scope");
        let tombstone = managed_agents::retention::get_retained_event(
            &connection,
            5,
            &restarted_token.scope.owner,
            &managed_agents::retention::tombstone_retention_d_tag(KIND_PERSONA, &persona.id),
        )
        .expect("read the persona tombstone witness")
        .expect("completed retry must retain a persona tombstone");
        assert!(tombstone.pending_sync);
    });
}

#[cfg(unix)]
#[test]
fn production_linked_child_receipt_failure_persists_cascade_and_fresh_recovery_finishes() {
    use std::os::unix::process::ExitStatusExt;

    let _path_guard = managed_agents::lock_path_mutex();
    let temp = tempfile::tempdir().expect("temporary app-data root");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("temporary home");
    let _env_guard = HomeGuard {
        home: std::env::var_os("HOME"),
        xdg_data_home: std::env::var_os("XDG_DATA_HOME"),
    };
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_DATA_HOME", &home);

    let owner_keys = nostr::Keys::generate();
    let identifier = format!(
        "xyz.nuncio.crew.persona-child-seam-{}",
        uuid::Uuid::new_v4().simple()
    );
    let app = mock_app_with(identifier.clone(), Some(owner_keys.clone()));
    let persona = persona();
    managed_agents::save_personas(app.handle(), std::slice::from_ref(&persona))
        .expect("persist linked persona fixture");

    let pubkey = nostr::Keys::generate().public_key().to_hex();
    let key_delete = managed_agents::install_test_agent_key_delete(&pubkey, Ok(()));
    let mut record = persona.clone().into_agent_record();
    record.pubkey = pubkey.clone();
    record.persona_id = Some(persona.id.clone());
    record.relay_url = RELAY.into();
    record.acp_command = "buzz-acp".into();
    record.agent_command = "goose".into();
    managed_agents::save_managed_agents(app.handle(), std::slice::from_ref(&record))
        .expect("persist linked managed-agent fixture");

    let child = crate::managed_agent_delete::OwnedReceiptChild::spawn(&identifier);
    let receipt_key = ManagedAgentRuntimeKey::new(pubkey.clone(), &record.relay_url)
        .expect("fixture runtime key");
    let receipt = ManagedAgentRuntimeReceipt {
        key: receipt_key.clone(),
        pid: child.pid(),
        desktop_instance_id: identifier.clone(),
        started_at: "2026-09-13T00:00:00Z".into(),
    };
    managed_agents::write_agent_runtime_receipt(&app.handle(), &receipt)
        .expect("persist linked child receipt");
    let receipt_path = managed_agents::managed_agents_base_dir(app.handle())
        .expect("resolve runtime receipt directory")
        .join("agent-pids")
        .join(format!("{}.json", receipt.key.runtime_id()));
    let receipt_bytes = std::fs::read(&receipt_path).expect("read valid receipt bytes");
    assert!(managed_agents::process_is_running(child.pid()));
    assert!(managed_agents::valid_agent_runtime_receipt(
        &receipt_path,
        &receipt,
        &identifier
    ));
    let receipt_target = temp.path().join("linked-owned-receipt-target.json");
    std::fs::write(&receipt_target, &receipt_bytes).expect("write symlink receipt target");
    std::fs::remove_file(&receipt_path).expect("remove regular receipt before replacement");
    std::os::unix::fs::symlink(&receipt_target, &receipt_path)
        .expect("replace receipt with symlink fixture");

    tauri::async_runtime::block_on(async {
        let captured = capture(app.handle().clone())
            .await
            .expect("capture cascade owner/workspace scope");
        let token = captured.token;
        let target_payload = Payload::new(RecordFence::capture(&record), Vec::new(), &pubkey)
            .expect("build linked child payload");
        let parent_id = persona_cascade_operation_id(&persona.id);
        let (parent_payload, children) = build_persona_cascade_payload(
            &persona,
            &parent_id,
            std::slice::from_ref(&target_payload),
        )
        .expect("build production persona cascade payload");
        let parent = NewOperation {
            id: parent_id.clone(),
            kind: OperationKind::ManagedAgentDelete,
            resource_key: parent_payload.fence.pubkey.clone(),
            payload: serde_json::to_value(parent_payload).expect("encode cascade coordinator"),
        };
        let child_id = children
            .first()
            .expect("production cascade must prepare one child")
            .id
            .clone();
        let mut operations = Vec::with_capacity(children.len() + 1);
        operations.push(parent);
        operations.extend(children);
        let mut journal = crate::managed_agent_delete::open_journal_store(app.handle())
            .expect("open production cascade journal");
        let committed = journal
            .create_managed_agent_delete_batch(
                &token.scope,
                operations,
                native_now().expect("clock"),
            )
            .expect("atomically reserve parent and child operations");
        let operation = committed
            .into_iter()
            .find(|candidate| candidate.id == parent_id)
            .expect("load committed cascade coordinator");
        drop(journal);

        // The public begin path also performs relay channel discovery. This
        // fixture deliberately has no relay/provider dependency; it uses the
        // same production payload builder and atomic journal admission before
        // invoking the production coordinator and recovery scanner.
        let expected_stop_error =
            "managed-agent runtime receipt is not a regular file; deletion remains pending";
        let first_attempt = resume_persona_cascade(
            app.handle().clone(),
            token.clone(),
            operation.clone(),
            false,
        )
        .await;
        assert_eq!(
            first_attempt.expect_err("strict child receipt failure must propagate"),
            expected_stop_error
        );

        let journal = crate::managed_agent_delete::open_journal_store(app.handle())
            .expect("reopen failed cascade journal");
        let failed_parent = journal
            .load(&operation.scope, &operation.id)
            .expect("load failed cascade coordinator");
        let failed_parent_payload: Payload = serde_json::from_value(failed_parent.payload.clone())
            .expect("decode failed cascade coordinator");
        let failed_child = journal
            .load(&operation.scope, &child_id)
            .expect("load failed cascade child");
        let failed_child_payload: Payload = serde_json::from_value(failed_child.payload.clone())
            .expect("decode failed cascade child");
        assert_eq!(failed_parent.status, OperationStatus::Failed);
        assert!(!failed_parent.reconciled);
        assert_eq!(failed_child.status, OperationStatus::Failed);
        assert!(!failed_child.reconciled);
        assert!(!failed_child_payload.local_removed);
        assert!(!failed_child_payload.key_removed);
        assert_eq!(
            key_delete.calls(),
            0,
            "failed stop must retain the agent key"
        );
        assert!(failed_child_payload.failures > 0);
        assert_eq!(
            failed_child_payload.last_error.as_deref(),
            Some(expected_stop_error)
        );
        let failed_cascade = failed_parent_payload
            .cascade
            .as_ref()
            .expect("failed coordinator cascade payload");
        assert!(!failed_cascade.persona_removed);
        assert!(!failed_cascade.targets[0].settled);
        assert!(pending_in_store(&journal, &pubkey).expect("inspect unresolved child claim"));
        assert!(
            managed_agents::load_personas(app.handle())
                .expect("load retained persona")
                .iter()
                .any(|candidate| candidate.id == persona.id),
            "failed cascade must retain the persona until the child is recovered"
        );
        assert!(
            managed_agents::load_managed_agents(app.handle())
                .expect("load retained linked record")
                .iter()
                .any(|candidate| candidate.pubkey == pubkey),
            "failed cascade must retain the linked managed-agent record"
        );
        assert!(std::fs::symlink_metadata(&receipt_path)
            .expect("failed cascade must retain receipt path")
            .file_type()
            .is_symlink());
        assert!(managed_agents::process_is_running(child.pid()));
        drop(journal);

        std::fs::remove_file(&receipt_path).expect("remove failed receipt symlink");
        managed_agents::write_agent_runtime_receipt(&app.handle(), &receipt)
            .expect("restore regular receipt for fresh recovery");
        assert!(managed_agents::valid_agent_runtime_receipt(
            &receipt_path,
            &receipt,
            &identifier
        ));
        assert!(managed_agents::process_is_running(child.pid()));

        drop(app);
        let app = mock_app_with(identifier.clone(), Some(owner_keys));
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

        recover(&app.handle())
            .await
            .expect("fresh AppState recovery must retry the cascade");
        let exit_status = child.join();
        assert!(
            exit_status.signal().is_some(),
            "fresh recovery must terminate the live linked child"
        );

        let journal = crate::managed_agent_delete::open_journal_store(app.handle())
            .expect("open recovered cascade journal");
        let completed_parent = journal
            .load(&operation.scope, &operation.id)
            .expect("load completed cascade coordinator");
        let completed_parent_payload: Payload =
            serde_json::from_value(completed_parent.payload.clone())
                .expect("decode completed cascade coordinator");
        let completed_child = journal
            .load(&operation.scope, &child_id)
            .expect("load completed cascade child");
        let completed_child_payload: Payload = serde_json::from_value(completed_child.payload)
            .expect("decode completed cascade child");
        assert_eq!(completed_parent.status, OperationStatus::Complete);
        assert!(completed_parent.reconciled);
        assert_eq!(completed_child.status, OperationStatus::Complete);
        assert!(completed_child.reconciled);
        assert!(completed_child_payload.local_removed);
        assert!(completed_child_payload.key_removed);
        assert!(completed_child_payload.tombstone_enqueued);
        let completed_cascade = completed_parent_payload
            .cascade
            .as_ref()
            .expect("completed coordinator cascade payload");
        assert!(completed_cascade.persona_removed);
        assert!(completed_cascade.targets[0].settled);
        assert!(!pending_in_store(&journal, &pubkey).expect("inspect completed child claim"));
        assert!(
            managed_agents::load_personas(app.handle())
                .expect("load removed persona store")
                .iter()
                .all(|candidate| candidate.id != persona.id),
            "fresh recovery must finalize the persona tombstone"
        );
        assert!(
            managed_agents::load_managed_agents(app.handle())
                .expect("load removed linked record store")
                .iter()
                .all(|candidate| candidate.pubkey != pubkey),
            "fresh recovery must remove the linked managed-agent record"
        );
        assert!(
            managed_agents::read_all_agent_runtime_receipts(app.handle())
                .into_iter()
                .all(|(_, candidate)| candidate.key.pubkey != pubkey),
            "fresh recovery must retire the linked child receipt"
        );
        assert_eq!(
            std::fs::symlink_metadata(&receipt_path)
                .expect_err("fresh recovery must remove the receipt path")
                .kind(),
            std::io::ErrorKind::NotFound
        );
        assert_eq!(
            std::fs::read(&receipt_target).expect("failed symlink target remains readable"),
            receipt_bytes,
            "receipt cleanup must not change the failed symlink target"
        );
        let db_path = managed_agents::retention::scoped_retention_db_path(
            &managed_agents::managed_agents_base_dir(app.handle())
                .expect("resolve recovered managed-agent data directory"),
            RELAY,
            &restarted_token.scope.owner,
        );
        let connection = managed_agents::retention::open_retention_db(&db_path)
            .expect("open recovered retention scope");
        let tombstone = managed_agents::retention::get_retained_event(
            &connection,
            5,
            &restarted_token.scope.owner,
            &managed_agents::retention::tombstone_retention_d_tag(KIND_PERSONA, &persona.id),
        )
        .expect("read recovered persona tombstone")
        .expect("fresh cascade recovery must retain a persona tombstone");
        assert!(tombstone.pending_sync);
        assert_eq!(
            key_delete.calls(),
            1,
            "fresh recovery must use the scoped credential cleanup seam once"
        );
    });
}
