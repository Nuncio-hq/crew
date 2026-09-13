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
    managed_agents::{self, AgentDefinition},
    owner_operations::OperationStatus,
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

fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
    let state = build_app_state();
    *state.relay_url_override.lock().unwrap() = Some(RELAY.into());
    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.config_mut().identifier = format!(
        "xyz.nuncio.crew.persona-delete-seam-{}",
        uuid::Uuid::new_v4().simple()
    );
    tauri::test::mock_builder()
        .manage(state)
        .build(context)
        .expect("build the production coordinator fixture app")
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
        let token = capture(app.handle().clone())
            .await
            .expect("capture owner/workspace scope")
            .token;
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
        resume_persona_cascade(app.handle().clone(), token.clone(), failed, true)
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
            &base_dir,
            RELAY,
            &token.scope.owner,
        );
        let connection = managed_agents::retention::open_retention_db(&db_path)
            .expect("open the recovered retention scope");
        let tombstone = managed_agents::retention::get_retained_event(
            &connection,
            5,
            &token.scope.owner,
            &managed_agents::retention::tombstone_retention_d_tag(KIND_PERSONA, &persona.id),
        )
        .expect("read the persona tombstone witness")
        .expect("completed retry must retain a persona tombstone");
        assert!(tombstone.pending_sync);
    });
}
