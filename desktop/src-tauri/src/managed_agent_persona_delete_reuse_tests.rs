//! Production-seam coverage for repeat deletions of one persona coordinate.
//!
//! Persona ids are recycled — an inbound persona reuses its `d` tag as the
//! local id — and a persona can be edited while its deletion is still
//! unresolved. Both drive the same deterministic coordinator id, so both are
//! exercised here through the shipped `delete_persona` / `resume_persona_cascade`
//! entry points rather than a test-only helper.

use super::*;
use crate::{
    app_state::owner_scope::capture,
    managed_agents::{self, AgentDefinition, ManagedAgentRecord},
    owner_operations::{NewOperation, OperationKind},
};
use std::collections::BTreeMap;

const RELAY: &str = "wss://persona-reuse-seam.example";

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
        id: "persona-reuse-seam".into(),
        display_name: "Persona Reuse Seam".into(),
        avatar_url: None,
        description: None,
        system_prompt: "Exercise repeat deletion of one persona coordinate.".into(),
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
        created_at: "2026-09-14T00:00:00Z".into(),
        updated_at: "2026-09-14T00:00:00Z".into(),
    }
}

fn mock_app_with(
    identifier: String,
    keys: Option<nostr::Keys>,
) -> tauri::App<tauri::test::MockRuntime> {
    let state = crate::app_state::build_app_state();
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

fn temp_home() -> (tempfile::TempDir, HomeGuard) {
    let temp = tempfile::tempdir().expect("temporary app-data root");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("temporary home");
    let guard = HomeGuard {
        home: std::env::var_os("HOME"),
        xdg_data_home: std::env::var_os("XDG_DATA_HOME"),
    };
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_DATA_HOME", &home);
    (temp, guard)
}

fn persona_is_present<R: tauri::Runtime>(app: &AppHandle<R>, id: &str) -> bool {
    managed_agents::load_personas(app)
        .expect("load persona store")
        .iter()
        .any(|candidate| candidate.id == id)
}

/// A persona whose id is deleted, then re-imported under the same id, must be
/// really deleted the second time. The coordinator id is derived from the
/// persona id alone, so the completed first coordinator is still on file; if it
/// were resumed, its removal witness would short-circuit the second deletion
/// into a success that removes nothing.
#[test]
fn production_repeat_delete_of_a_recycled_persona_id_removes_the_new_persona() {
    let _path_guard = managed_agents::lock_path_mutex();
    let (_temp, _env_guard) = temp_home();

    // The whole-command future (`delete_persona` → cascade → child deletion)
    // is deeper than the default test-thread stack; the shipped app drives it
    // from the Tauri runtime, not from a test thread.
    let worker = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(repeat_delete_of_a_recycled_persona_id)
        .expect("spawn the deletion worker");
    if let Err(panic) = worker.join() {
        std::panic::resume_unwind(panic);
    }
}

fn repeat_delete_of_a_recycled_persona_id() {
    let app = mock_app_with(
        format!(
            "xyz.nuncio.crew.persona-reuse-seam-{}",
            uuid::Uuid::new_v4().simple()
        ),
        None,
    );
    let persona = persona();
    managed_agents::save_personas(app.handle(), std::slice::from_ref(&persona))
        .expect("persist persona fixture");

    tauri::async_runtime::block_on(async {
        delete_persona(app.handle().clone(), persona.id.clone())
            .await
            .expect("the first deletion must complete");
        assert!(
            !persona_is_present(app.handle(), &persona.id),
            "the first deletion must remove the persona"
        );

        let token = capture(app.handle().clone())
            .await
            .expect("capture owner/workspace scope")
            .token;
        let parent_id = persona_cascade_operation_id(&persona.id);
        let completed = load_scope_operation(app.handle(), &token, &parent_id)
            .expect("load the completed coordinator")
            .expect("the first deletion must leave a durable coordinator");
        assert!(completed.reconciled);

        // A re-received persona event reuses the same `d` tag as the local id,
        // so the second row occupies the coordinate the completed coordinator
        // froze.
        let mut reimported = persona.clone();
        reimported.display_name = "Persona Reuse Seam (re-imported)".into();
        reimported.updated_at = "2026-09-15T00:00:00Z".into();
        managed_agents::save_personas(app.handle(), std::slice::from_ref(&reimported))
            .expect("persist the re-imported persona");

        delete_persona(app.handle().clone(), reimported.id.clone())
            .await
            .expect("the second deletion must complete");
        assert!(
            !persona_is_present(app.handle(), &reimported.id),
            "a reported deletion must actually remove the re-imported persona"
        );
    });
}

/// Editing a persona while its deletion is still unresolved must not brick the
/// retry. The durable fence keeps the persona's stable identity, not its
/// mutable `updated_at`, so the manual retry — itself an explicit
/// re-confirmation — still finishes the cleanup and frees the agent key.
#[test]
fn production_persona_edit_during_pending_cascade_still_retries_to_completion() {
    let _path_guard = managed_agents::lock_path_mutex();
    let (_temp, _env_guard) = temp_home();

    let owner_keys = nostr::Keys::generate();
    let identifier = format!(
        "xyz.nuncio.crew.persona-edit-seam-{}",
        uuid::Uuid::new_v4().simple()
    );
    let app = mock_app_with(identifier, Some(owner_keys));
    let persona = persona();
    managed_agents::save_personas(app.handle(), std::slice::from_ref(&persona))
        .expect("persist linked persona fixture");

    let pubkey = nostr::Keys::generate().public_key().to_hex();
    let mut record: ManagedAgentRecord = persona.clone().into_agent_record();
    record.pubkey = pubkey.clone();
    record.persona_id = Some(persona.id.clone());
    record.relay_url = RELAY.into();
    record.acp_command = "buzz-acp".into();
    record.agent_command = "goose".into();
    managed_agents::save_managed_agents(app.handle(), std::slice::from_ref(&record))
        .expect("persist linked managed-agent fixture");

    tauri::async_runtime::block_on(async {
        let token = capture(app.handle().clone())
            .await
            .expect("capture cascade owner/workspace scope")
            .token;
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

        let failing_key_delete = managed_agents::install_test_agent_key_delete(
            &pubkey,
            Err("keyring unavailable: interaction is not allowed".into()),
        );
        resume_persona_cascade(
            app.handle().clone(),
            token.clone(),
            operation.clone(),
            false,
        )
        .await
        .expect_err("an unreachable keyring must keep the cascade unresolved");
        drop(failing_key_delete);

        // The owner edits the persona from the still-visible library row while
        // the cleanup is pending. Only `updated_at` and the edited fields move;
        // id, d-tag, and created_at are not writable from an edit.
        let mut edited = managed_agents::load_personas(app.handle())
            .expect("load persona store during pending cleanup")
            .into_iter()
            .find(|candidate| candidate.id == persona.id)
            .expect("pending key cleanup retains the persona definition");
        edited.display_name = "Persona Reuse Seam (edited)".into();
        edited.updated_at = "2026-09-16T00:00:00Z".into();
        managed_agents::save_personas(app.handle(), std::slice::from_ref(&edited))
            .expect("persist the mid-cascade edit");

        let failed = load_scope_operation(app.handle(), &token, &parent_id)
            .expect("load the failed coordinator")
            .expect("a failed cascade keeps its coordinator");
        let _key_delete = managed_agents::install_test_agent_key_delete(&pubkey, Ok(()));
        resume_persona_cascade(app.handle().clone(), token.clone(), failed, true)
            .await
            .expect("a manual retry after an edit must finish the cleanup");

        assert!(
            !persona_is_present(app.handle(), &persona.id),
            "the completed retry must remove the edited persona"
        );
        let journal = crate::managed_agent_delete::open_journal_store(app.handle())
            .expect("reopen the recovered cascade journal");
        assert!(
            !pending_in_store(&journal, &pubkey).expect("inspect the completed child claim"),
            "a completed cascade must release the agent key claim"
        );
    });
}
