use super::*;
use crate::app_state::{
    build_app_state, owner_scope::CapturedOwnerScope, AppState, IdentityStorage,
};
use crate::managed_agents::recap_capability::{
    RecapExecutableIdentity, RecapGuarantees, RecapRuntimeReadyProof, RecapSelection,
};
use crate::owner_operations::OperationScope;
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
use tauri::Manager;
use tokio::net::TcpListener;

use super::storage::recap_key_digest;

static REGISTRY_TEST_MUTEX: Mutex<()> = Mutex::new(());

struct RecapTestApp {
    app: tauri::App<tauri::test::MockRuntime>,
    app_data_dir: PathBuf,
}

impl RecapTestApp {
    fn new() -> Self {
        let identifier = format!(
            "xyz.nuncio.crew.test.recap-{}",
            uuid::Uuid::new_v4().simple()
        );
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = identifier;
        let app = tauri::test::mock_builder()
            .manage(build_app_state())
            .build(context)
            .unwrap();
        let app_data_dir = app.path().app_data_dir().unwrap();
        assert!(!app_data_dir.exists());
        Self { app, app_data_dir }
    }
}

impl std::ops::Deref for RecapTestApp {
    type Target = tauri::App<tauri::test::MockRuntime>;

    fn deref(&self) -> &Self::Target {
        &self.app
    }
}

impl Drop for RecapTestApp {
    fn drop(&mut self) {
        if self.app_data_dir.exists() {
            std::fs::remove_dir_all(&self.app_data_dir).unwrap();
        }
    }
}

struct RecapTestHooksGuard;

impl Drop for RecapTestHooksGuard {
    fn drop(&mut self) {
        clear_test_source_barrier();
        clear_test_commit_barrier();
        clear_test_settings_load_barrier();
        clear_test_settings_persist_barrier();
        crate::managed_agents::recap_ownership::clear_test_runtime_proofs();
        crate::managed_agents::recap_ownership::set_test_recap_base(None);
        crate::managed_agents::recap_service::clear_test_execution_observers();
    }
}

fn test_source(root_event_id: &str) -> ThreadSource {
    ThreadSource {
        prompt: "fixture recap input".to_string(),
        manifest_hash: "a".repeat(64),
        event_ids: vec![root_event_id.to_string()],
        omitted_message_count: 0,
        source_overflow: false,
        oldest_included_event_id: Some(root_event_id.to_string()),
        newest_included_event_id: Some(root_event_id.to_string()),
    }
}

fn test_executable(root: &std::path::Path, name: &str) -> RecapExecutableIdentity {
    let path = root.join(name);
    std::fs::write(
            &path,
            b"#!/bin/sh\nprintf '%s' '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"fixture recap\",\"modelUsage\":{\"fixture-model\":{}}}'\n",
        )
        .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let fingerprint = hex::encode(Sha256::digest(std::fs::read(&path).unwrap()));
    RecapExecutableIdentity {
        resolved_path: path.canonicalize().unwrap(),
        version: "fixture-1".to_string(),
        fingerprint,
        platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
    }
}

fn test_proof(executable: RecapExecutableIdentity) -> RecapRuntimeReadyProof {
    RecapRuntimeReadyProof::for_test(
        "claude",
        executable,
        RecapSelection {
            model: "fixture-model".to_string(),
            profile: None,
            profile_digest: None,
            profile_identity: None,
            auth_available: true,
        },
        RecapGuarantees {
            one_shot: true,
            tool_isolation: true,
            state_isolation: true,
            process_containment: true,
        },
    )
}

fn assert_no_recap_runs(base: &std::path::Path) {
    let runs = base.join("recap-runs");
    if let Ok(entries) = std::fs::read_dir(runs) {
        assert!(
            entries.filter_map(Result::ok).next().is_none(),
            "stale pre-spawn rejection must clean its owned run"
        );
    }
}

async fn switch_identity<R: tauri::Runtime>(
    app: &tauri::App<R>,
    directory: &std::path::Path,
    keys: nostr::Keys,
) {
    let state = app.state::<AppState>();
    let guard = state.identity_mutation.lock().unwrap();
    crate::commands::commit_imported_identity(&state, &guard, directory, keys, |_| {
        Ok(IdentityStorage::LocalFile)
    })
    .unwrap();
}

async fn switch_identity_and_workspace<R: tauri::Runtime>(
    app: &tauri::App<R>,
    directory: &std::path::Path,
    keys: nostr::Keys,
    relay_url: &str,
) {
    let state = app.state::<AppState>();
    let _workspace_guard = state.workspace_apply_lock.clone().lock_owned().await;
    state
        .workspace_apply_generation
        .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    let guard = state.identity_mutation.lock().unwrap();
    crate::commands::commit_imported_identity(&state, &guard, directory, keys, |_| {
        Ok(IdentityStorage::LocalFile)
    })
    .unwrap();
    *state.relay_url_override.lock().unwrap() = Some(relay_url.to_string());
}

fn event(content: impl Into<String>, created_at: u64, channel_id: &str) -> nostr::Event {
    nostr::EventBuilder::new(nostr::Kind::Custom(9), content)
        .tags([nostr::Tag::parse(["h", channel_id]).unwrap()])
        .custom_created_at(nostr::Timestamp::from_secs(created_at))
        .sign_with_keys(&nostr::Keys::generate())
        .unwrap()
}

#[tokio::test]
async fn cancelled_source_read_stops_before_first_relay_request() {
    let state = build_app_state();
    let keys = nostr::Keys::generate();
    let owner = keys.public_key().to_hex();
    let scope = CapturedOwnerScope {
        token: crate::app_state::owner_scope::OwnerScopeToken {
            scope: OperationScope {
                owner,
                community: "http://127.0.0.1:9".to_string(),
            },
            workspace_generation: 0,
            identity_generation: 0,
        },
        keys,
        relay_url: "ws://127.0.0.1:9".to_string(),
    };
    let cancelled = AtomicBool::new(true);
    let result = collect_thread_source_with_cancel(
        &state,
        &scope,
        "550e8400-e29b-41d4-a716-446655440000",
        &"a".repeat(64),
        Some(&cancelled),
    )
    .await;
    assert!(matches!(result, Err(error) if error == "cancelled"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_source_read_aborts_a_pending_relay_request() {
    let _serial = REGISTRY_TEST_MUTEX.lock().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let accepted = Arc::new(tokio::sync::Notify::new());
    let server_accepted = accepted.clone();
    let server = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        server_accepted.notify_one();
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    });
    let state = build_app_state();
    *state.relay_url_override.lock().unwrap() = Some(format!("ws://{address}"));
    let keys = nostr::Keys::generate();
    let owner = keys.public_key().to_hex();
    let scope = CapturedOwnerScope {
        token: crate::app_state::owner_scope::OwnerScopeToken {
            scope: OperationScope {
                owner,
                community: format!("http://{address}"),
            },
            workspace_generation: 0,
            identity_generation: 0,
        },
        keys,
        relay_url: format!("ws://{address}"),
    };
    let cancelled = AtomicBool::new(false);
    let root_event_id = "a".repeat(64);
    let started = std::time::Instant::now();
    let query = collect_thread_source_with_cancel(
        &state,
        &scope,
        "550e8400-e29b-41d4-a716-446655440000",
        &root_event_id,
        Some(&cancelled),
    );
    tokio::pin!(query);
    let accepted_wait = accepted.notified();
    tokio::pin!(accepted_wait);
    let result = tokio::select! {
        result = &mut query => result,
        _ = &mut accepted_wait => {
            cancelled.store(true, Ordering::Release);
            query.await
        },
    };
    server.abort();
    assert!(matches!(result, Err(error) if error == "cancelled"));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(1),
        "cancellation must not wait for the relay request's long timeout"
    );
}

#[tokio::test]
async fn recap_scope_snapshot_keeps_a_path_and_rejects_aba_token_reuse() {
    let _serial = REGISTRY_TEST_MUTEX.lock().unwrap();
    let _hooks = RecapTestHooksGuard;
    let app = RecapTestApp::new();
    let first = capture_recap_scope(app.handle()).await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let state = app.state::<AppState>();
    let first_keys = first.0.keys.clone();
    {
        let guard = state.identity_mutation.lock().unwrap();
        crate::commands::commit_imported_identity(
            &state,
            &guard,
            directory.path(),
            nostr::Keys::generate(),
            |_| Ok(IdentityStorage::LocalFile),
        )
        .unwrap();
    }
    let second = capture_recap_scope(app.handle()).await.unwrap();
    assert_ne!(
        first.3, second.3,
        "owner B must use a different retention DB"
    );
    {
        let guard = state.identity_mutation.lock().unwrap();
        crate::commands::commit_imported_identity(
            &state,
            &guard,
            directory.path(),
            first_keys,
            |_| Ok(IdentityStorage::LocalFile),
        )
        .unwrap();
    }
    let aba = capture_recap_scope(app.handle()).await.unwrap();
    assert_eq!(first.3, aba.3, "A-B-A returns to A's retention path");
    assert_ne!(
        first.0.token, aba.0.token,
        "ABA must still fence the old run"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generate_recap_uses_captured_a_proof_when_b_is_active() {
    let _serial = REGISTRY_TEST_MUTEX.lock().unwrap();
    let _hooks = RecapTestHooksGuard;
    let app = RecapTestApp::new();
    let state = app.state::<AppState>();
    *state.relay_url_override.lock().unwrap() = Some("ws://127.0.0.1:9".to_string());
    let first = capture_recap_scope(app.handle()).await.unwrap();
    let base = first.4.clone();
    let directory = tempfile::tempdir().unwrap();
    let executable_root = tempfile::tempdir().unwrap();
    let executable_a = test_executable(executable_root.path(), "claude-a");
    assert!(super::super::recap_capability::verify_executable(&executable_a).is_ok());
    let proof_a = test_proof(executable_a);
    let proof_b = test_proof(test_executable(executable_root.path(), "claude-b"));
    let runtimes = runtime_inventory_from_proof(Some(proof_a.clone()));
    let capability_fingerprint = runtimes
        .iter()
        .find(|runtime| runtime.id == "claude")
        .and_then(|runtime| runtime.capability_fingerprint.clone())
        .unwrap();
    let settings = RecapSettings {
        version: 1,
        mode: RecapMode::Manual,
        runtime_id: Some("claude".to_string()),
        requested_model: Some("fixture-model".to_string()),
        profile_ref: None,
        capability_fingerprint: Some(capability_fingerprint),
        bounds: default_bounds(),
    };
    let first_keys = first.0.keys.clone();
    let root_event_id = "a".repeat(64);

    crate::managed_agents::recap_ownership::set_test_recap_base(Some(base.clone()));
    crate::managed_agents::recap_ownership::clear_test_runtime_proofs();
    crate::managed_agents::recap_ownership::set_test_runtime_proof(
        first.3.clone(),
        proof_a.clone(),
    );
    write_settings(app.handle(), &first.1, &first.2, &settings).unwrap();
    crate::managed_agents::recap_service::clear_test_execution_observers();
    let (after_assert, release) = install_test_source_barrier(test_source(&root_event_id));

    let root_event_id_for_generation = root_event_id.clone();
    let (result, ()) = tokio::join!(
        generate_thread_recap_for_runtime(
            app.handle().clone(),
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
            root_event_id_for_generation,
            "generation-a".to_string(),
            app.state(),
        ),
        async {
            await_test_signal(after_assert, "A/B source assertion").await;
            switch_identity(&app, directory.path(), nostr::Keys::generate()).await;
            let second = capture_recap_scope(app.handle()).await.unwrap();
            crate::managed_agents::recap_ownership::set_test_runtime_proof(second.3, proof_b);
            release.notify_one();
        },
    );
    clear_test_source_barrier();
    crate::managed_agents::recap_ownership::clear_test_runtime_proofs();
    crate::managed_agents::recap_ownership::set_test_recap_base(None);

    assert_eq!(result, Err("runtime_not_ready".to_string()));
    assert_eq!(
        crate::managed_agents::recap_service::observed_test_scoped_proofs(),
        vec![proof_a],
        "the blocking service must reload the captured A retention scope, not active B"
    );
    assert_eq!(
        crate::managed_agents::recap_service::observed_test_provider_launches(),
        0,
        "a stale owner scope must fail before the provider is invoked"
    );
    assert!(
        !base.join("recaps").exists(),
        "a stale generation must not persist a recap artifact"
    );
    assert_no_recap_runs(&base);
    assert_ne!(
        first_keys.public_key(),
        app.state::<AppState>().keys.lock().unwrap().public_key()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generate_recap_rejects_aba_after_returning_to_same_owner() {
    let _serial = REGISTRY_TEST_MUTEX.lock().unwrap();
    let _hooks = RecapTestHooksGuard;
    let app = RecapTestApp::new();
    let state = app.state::<AppState>();
    *state.relay_url_override.lock().unwrap() = Some("ws://127.0.0.1:9".to_string());
    let first = capture_recap_scope(app.handle()).await.unwrap();
    let base = first.4.clone();
    let directory = tempfile::tempdir().unwrap();
    let executable_root = tempfile::tempdir().unwrap();
    let executable_a = test_executable(executable_root.path(), "claude-a");
    assert!(super::super::recap_capability::verify_executable(&executable_a).is_ok());
    let proof_a = test_proof(executable_a);
    let runtimes = runtime_inventory_from_proof(Some(proof_a.clone()));
    let capability_fingerprint = runtimes
        .iter()
        .find(|runtime| runtime.id == "claude")
        .and_then(|runtime| runtime.capability_fingerprint.clone())
        .unwrap();
    let settings = RecapSettings {
        version: 1,
        mode: RecapMode::Manual,
        runtime_id: Some("claude".to_string()),
        requested_model: Some("fixture-model".to_string()),
        profile_ref: None,
        capability_fingerprint: Some(capability_fingerprint),
        bounds: default_bounds(),
    };
    let first_keys = first.0.keys.clone();
    let root_event_id = "b".repeat(64);

    crate::managed_agents::recap_ownership::set_test_recap_base(Some(base.clone()));
    crate::managed_agents::recap_ownership::clear_test_runtime_proofs();
    crate::managed_agents::recap_ownership::set_test_runtime_proof(
        first.3.clone(),
        proof_a.clone(),
    );
    write_settings(app.handle(), &first.1, &first.2, &settings).unwrap();
    crate::managed_agents::recap_service::clear_test_execution_observers();
    let (after_assert, release) = install_test_source_barrier(test_source(&root_event_id));

    let root_event_id_for_generation = root_event_id.clone();
    let (result, ()) = tokio::join!(
        generate_thread_recap_for_runtime(
            app.handle().clone(),
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
            root_event_id_for_generation,
            "generation-aba".to_string(),
            app.state(),
        ),
        async {
            await_test_signal(after_assert, "ABA source assertion").await;
            switch_identity(&app, directory.path(), nostr::Keys::generate()).await;
            switch_identity(&app, directory.path(), first_keys.clone()).await;
            release.notify_one();
        },
    );
    clear_test_source_barrier();
    crate::managed_agents::recap_ownership::clear_test_runtime_proofs();
    crate::managed_agents::recap_ownership::set_test_recap_base(None);

    assert_eq!(result, Err("runtime_not_ready".to_string()));
    assert_eq!(
        crate::managed_agents::recap_service::observed_test_provider_launches(),
        0,
        "returning to the same owner must not make the stale A generation current"
    );
    assert_eq!(
        crate::managed_agents::recap_service::observed_test_scoped_proofs(),
        vec![proof_a]
    );
    assert_no_recap_runs(&base);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generate_recap_rejects_completion_after_workspace_switch_before_commit() {
    let _serial = REGISTRY_TEST_MUTEX.lock().unwrap();
    let _hooks = RecapTestHooksGuard;
    let app = RecapTestApp::new();
    let state = app.state::<AppState>();
    *state.relay_url_override.lock().unwrap() = Some("ws://127.0.0.1:9".to_string());
    let first = capture_recap_scope(app.handle()).await.unwrap();
    let base = first.4.clone();
    let directory = tempfile::tempdir().unwrap();
    let executable_root = tempfile::tempdir().unwrap();
    let proof_a = test_proof(test_executable(executable_root.path(), "claude-a"));
    let runtimes = runtime_inventory_from_proof(Some(proof_a.clone()));
    let capability_fingerprint = runtimes
        .iter()
        .find(|runtime| runtime.id == "claude")
        .and_then(|runtime| runtime.capability_fingerprint.clone())
        .unwrap();
    let settings = RecapSettings {
        version: 1,
        mode: RecapMode::Manual,
        runtime_id: Some("claude".to_string()),
        requested_model: Some("fixture-model".to_string()),
        profile_ref: None,
        capability_fingerprint: Some(capability_fingerprint),
        bounds: default_bounds(),
    };
    let root_event_id = "c".repeat(64);

    crate::managed_agents::recap_ownership::set_test_recap_base(Some(base.clone()));
    crate::managed_agents::recap_ownership::clear_test_runtime_proofs();
    crate::managed_agents::recap_ownership::set_test_runtime_proof(first.3.clone(), proof_a);
    write_settings(app.handle(), &first.1, &first.2, &settings).unwrap();
    crate::managed_agents::recap_service::clear_test_execution_observers();
    let (after_assert, source_release) = install_test_source_barrier(test_source(&root_event_id));
    let (after_provider, commit_release) = install_test_commit_barrier();

    let root_event_id_for_generation = root_event_id.clone();
    let (result, ()) = tokio::join!(
        generate_thread_recap_for_runtime(
            app.handle().clone(),
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
            root_event_id_for_generation,
            "generation-before-commit".to_string(),
            app.state(),
        ),
        async {
            await_test_signal(after_assert, "pre-commit source assertion").await;
            source_release.notify_one();
            await_test_signal(after_provider, "post-provider completion").await;
            switch_identity_and_workspace(
                &app,
                directory.path(),
                nostr::Keys::generate(),
                "ws://127.0.0.1:10",
            )
            .await;
            commit_release.notify_one();
        },
    );
    clear_test_source_barrier();
    clear_test_commit_barrier();
    crate::managed_agents::recap_ownership::clear_test_runtime_proofs();
    crate::managed_agents::recap_ownership::set_test_recap_base(None);

    assert_eq!(
        result,
        Err(crate::app_state::owner_scope::OWNER_SCOPE_STALE.to_string())
    );
    assert_eq!(
        crate::managed_agents::recap_service::observed_test_provider_launches(),
        1,
        "the provider must complete before the final persistence fence runs"
    );
    assert!(
        !base.join("recaps").exists(),
        "a completion after an owner/workspace switch must not persist"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_save_cancels_generation_registered_before_settings_load() {
    let _serial = REGISTRY_TEST_MUTEX.lock().unwrap();
    let _hooks = RecapTestHooksGuard;
    let app = RecapTestApp::new();
    let state = app.state::<AppState>();
    *state.relay_url_override.lock().unwrap() = Some("ws://127.0.0.1:9".to_string());
    let first = capture_recap_scope(app.handle()).await.unwrap();
    let executable_root = tempfile::tempdir().unwrap();
    let proof_a = test_proof(test_executable(executable_root.path(), "claude-a"));
    let runtimes = runtime_inventory_from_proof(Some(proof_a.clone()));
    let capability_fingerprint = runtimes
        .iter()
        .find(|runtime| runtime.id == "claude")
        .and_then(|runtime| runtime.capability_fingerprint.clone())
        .unwrap();
    let settings = RecapSettings {
        version: 1,
        mode: RecapMode::Manual,
        runtime_id: Some("claude".to_string()),
        requested_model: Some("fixture-model".to_string()),
        profile_ref: None,
        capability_fingerprint: Some(capability_fingerprint),
        bounds: default_bounds(),
    };

    crate::managed_agents::recap_ownership::set_test_recap_base(Some(first.4.clone()));
    crate::managed_agents::recap_ownership::clear_test_runtime_proofs();
    crate::managed_agents::recap_ownership::set_test_runtime_proof(first.3.clone(), proof_a);
    write_settings(app.handle(), &first.1, &first.2, &settings).unwrap();
    crate::managed_agents::recap_service::clear_test_execution_observers();
    let (after_register, release) = install_test_settings_load_barrier();

    let (result, save_result) = tokio::join!(
        generate_thread_recap_for_runtime(
            app.handle().clone(),
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
            "d".repeat(64),
            "generation-settings-race".to_string(),
            app.state(),
        ),
        async {
            await_test_signal(after_register, "generation registration").await;
            let result = save_recap_settings_for_runtime(app.handle().clone(), settings).await;
            release.notify_one();
            result
        },
    );

    clear_test_settings_load_barrier();
    crate::managed_agents::recap_ownership::clear_test_runtime_proofs();
    crate::managed_agents::recap_ownership::set_test_recap_base(None);

    assert!(save_result.is_ok());
    assert_eq!(result, Err("cancelled".to_string()));
    assert_eq!(
        crate::managed_agents::recap_service::observed_test_provider_launches(),
        0,
        "a settings save racing generation startup must cancel the registered run"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_save_rejects_aba_before_persisting_or_cancelling() {
    let _serial = REGISTRY_TEST_MUTEX.lock().unwrap();
    let _hooks = RecapTestHooksGuard;
    let app = RecapTestApp::new();
    let state = app.state::<AppState>();
    *state.relay_url_override.lock().unwrap() = Some("ws://127.0.0.1:9".to_string());
    let first = capture_recap_scope(app.handle()).await.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let executable_root = tempfile::tempdir().unwrap();
    let proof_a = test_proof(test_executable(executable_root.path(), "claude-a"));
    let runtimes = runtime_inventory_from_proof(Some(proof_a.clone()));
    let capability_fingerprint = runtimes
        .iter()
        .find(|runtime| runtime.id == "claude")
        .and_then(|runtime| runtime.capability_fingerprint.clone())
        .unwrap();
    let initial_settings = RecapSettings {
        version: 1,
        mode: RecapMode::Manual,
        runtime_id: Some("claude".to_string()),
        requested_model: Some("fixture-model".to_string()),
        profile_ref: None,
        capability_fingerprint: Some(capability_fingerprint),
        bounds: default_bounds(),
    };
    let stale_save = RecapSettings {
        mode: RecapMode::Off,
        ..initial_settings.clone()
    };
    let channel_id = "550e8400-e29b-41d4-a716-446655440000";
    let root_event_id = "e".repeat(64);

    crate::managed_agents::recap_ownership::set_test_recap_base(Some(first.4.clone()));
    crate::managed_agents::recap_ownership::clear_test_runtime_proofs();
    crate::managed_agents::recap_ownership::set_test_runtime_proof(first.3.clone(), proof_a);
    write_settings(app.handle(), &first.1, &first.2, &initial_settings).unwrap();
    let generation_key = generation_key(
        &first.2,
        &first.1,
        channel_id,
        &root_event_id,
        &first.0.token,
    );
    let generation = register_generation(&generation_key, "generation-save-aba").unwrap();
    let (after_capture, release) = install_test_settings_persist_barrier();
    let save_task = tokio::spawn(save_recap_settings_for_runtime(
        app.handle().clone(),
        stale_save,
    ));

    await_test_signal(after_capture, "settings-persist capture").await;
    switch_identity(&app, directory.path(), nostr::Keys::generate()).await;
    switch_identity(&app, directory.path(), first.0.keys.clone()).await;
    release.notify_one();
    let save_result = save_task.await.unwrap();

    unregister_generation(&generation_key, "generation-save-aba");
    assert_eq!(
        save_result,
        Err(crate::app_state::owner_scope::OWNER_SCOPE_STALE.to_string())
    );
    assert!(
        !generation.load(Ordering::Acquire),
        "a stale settings save must not cancel a newer generation in the returned A scope"
    );
    let persisted = load_settings(app.handle(), &first.1, &first.2).unwrap();
    assert_eq!(persisted.settings.mode, RecapMode::Manual);
}

#[test]
fn settings_default_off_and_bounds_are_fixed() {
    let settings = default_settings();
    assert_eq!(settings.mode, RecapMode::Off);
    assert_eq!(settings.bounds, default_bounds());
}

#[test]
fn corrupt_saved_settings_keep_safe_off_and_surface_repair_error() {
    let loaded = decode_settings(b"{not-json");
    assert_eq!(loaded.settings.mode, RecapMode::Off);
    assert_eq!(loaded.error.as_deref(), Some("invalid_settings"));
}

#[test]
fn generation_registry_fences_stale_cancellation() {
    let _guard = REGISTRY_TEST_MUTEX.lock().unwrap();
    let key = "test-channel\0test-root";
    let generation = register_generation(key, "g-1").unwrap();
    assert!(generation_is_current(key, "g-1"));
    assert!(!generation_is_current(key, "g-2"));
    unregister_generation(key, "g-2");
    assert!(generation_is_current(key, "g-1"));
    generation.store(true, Ordering::Release);
    unregister_generation(key, "g-1");
    assert!(!generation_is_current(key, "g-1"));
}

#[test]
fn settings_change_cancels_only_the_current_owner_scope() {
    let _guard = REGISTRY_TEST_MUTEX.lock().unwrap();
    let owner_generation =
        register_generation("https://relay\0viewer\0channel\0root", "g-owner").unwrap();
    let other_generation =
        register_generation("https://relay\0other\0channel\0root", "g-other").unwrap();
    cancel_scope_generations("https://relay", "viewer");
    assert!(owner_generation.load(Ordering::Acquire));
    assert!(!other_generation.load(Ordering::Acquire));
    unregister_generation("https://relay\0viewer\0channel\0root", "g-owner");
    unregister_generation("https://relay\0other\0channel\0root", "g-other");
}

#[test]
fn generation_guard_unregisters_on_every_drop_path() {
    let _guard = REGISTRY_TEST_MUTEX.lock().unwrap();
    let key = "guard-relay\0guard-viewer\0channel\0root";
    {
        let generation = GenerationGuard::register(key, "g-guard").unwrap();
        assert!(generation_is_current(key, "g-guard"));
        generation.cancelled().store(true, Ordering::Release);
    }
    assert!(!generation_is_current(key, "g-guard"));
}

#[test]
fn cache_key_binds_source_and_settings_fingerprints() {
    let base = recap_key_digest("https://relay", "viewer", "channel", "root", "a", "b");
    assert_ne!(
        base,
        recap_key_digest("https://relay", "viewer", "channel", "root", "changed", "b")
    );
    assert_ne!(
        base,
        recap_key_digest("https://relay", "viewer", "channel", "root", "a", "changed")
    );
}

#[test]
fn unavailable_saved_selection_remains_recoverable_in_settings() {
    let settings = RecapSettings {
        mode: RecapMode::Manual,
        runtime_id: Some("claude".to_string()),
        capability_fingerprint: Some("old".to_string()),
        ..default_settings()
    };
    let runtimes = vec![RecapRuntimeOption {
        id: "claude".to_string(),
        label: "Claude".to_string(),
        kind: "cli".to_string(),
        availability: "unsupported".to_string(),
        reason: Some("runtime_not_ready".to_string()),
        capability_fingerprint: None,
        profiles: Vec::new(),
        models: Vec::new(),
    }];
    let (recovered, valid) = recoverable_settings(settings.clone(), &runtimes);
    assert!(!valid);
    assert_eq!(recovered, settings);
    assert_eq!(recovered.mode, RecapMode::Manual);
}

#[test]
fn ids_and_generation_names_are_canonical_and_bounded() {
    assert_eq!(
        canonical_channel_id("550e8400-e29b-41d4-a716-446655440000").unwrap(),
        "550e8400-e29b-41d4-a716-446655440000"
    );
    assert!(canonical_event_id(&"a".repeat(64)).is_ok());
    assert!(!valid_generation_id("../escape"));
    assert!(!valid_generation_id(
        &"x".repeat(MAX_GENERATION_ID_BYTES + 1)
    ));
}

#[test]
fn source_keeps_root_and_newest_fitting_whole_messages() {
    let channel_id = "550e8400-e29b-41d4-a716-446655440000";
    let root = event("root", 1, channel_id);
    let older = event("older", 2, channel_id);
    let newest_too_large = event(
        "x".repeat(super::super::recap_adapter::RECAP_INPUT_LIMIT),
        3,
        channel_id,
    );
    let root_id = root.id.to_hex();
    let source =
        build_thread_source(vec![root, older, newest_too_large], channel_id, &root_id).unwrap();
    assert_eq!(source.event_ids.len(), 2);
    assert_eq!(source.omitted_message_count, 1);
    assert!(source.prompt.contains("root"));
    assert!(source.prompt.contains("older"));
    assert!(!source.prompt.contains(&"x".repeat(1024)));
}

#[test]
fn source_cap_preserves_newest_page_instead_of_oldest_page() {
    let channel_id = "550e8400-e29b-41d4-a716-446655440000";
    let root = event("root", 1, channel_id);
    let root_id = root.id.to_hex();
    let mut events = vec![root];
    for created_at in 2..=301 {
        events.push(event(format!("reply-{created_at}"), created_at, channel_id));
    }

    let source = build_thread_source(events, channel_id, &root_id).unwrap();
    assert_eq!(source.event_ids.len(), MAX_SOURCE_EVENTS);
    assert!(source.omitted_message_count > 0);
    assert!(source.prompt.contains("reply-301"));
    assert!(!source.prompt.lines().any(|line| line.ends_with(" reply-2")));
    assert_eq!(
        source.newest_included_event_id.as_deref(),
        source.event_ids.last().map(String::as_str)
    );
}

#[test]
fn source_scan_ceiling_marks_prefix_truncation_and_keeps_observed_tail() {
    let channel_id = "550e8400-e29b-41d4-a716-446655440000";
    let mut replies = Vec::new();
    let mut overflow = false;
    for page_index in 0..17 {
        let start = page_index * MAX_SOURCE_EVENTS + 1;
        let page = (start..start + MAX_SOURCE_EVENTS)
            .map(|created_at| event(format!("reply-{created_at}"), created_at as u64, channel_id))
            .collect();
        let (_, page_overflow) = append_source_scan_page(&mut replies, page);
        if page_overflow {
            overflow = true;
            break;
        }
    }

    assert!(overflow);
    assert_eq!(replies.len(), MAX_SOURCE_SCAN_EVENTS);
    assert_eq!(
        replies.first().map(|event| event.content.as_str()),
        Some("reply-257")
    );
    assert_eq!(
        replies.last().map(|event| event.content.as_str()),
        Some("reply-4352")
    );
}

#[test]
fn source_overflow_is_carried_into_the_manifest_input() {
    let channel_id = "550e8400-e29b-41d4-a716-446655440000";
    let root = event("root", 1, channel_id);
    let root_id = root.id.to_hex();
    let source = build_thread_source_with_overflow(vec![root], channel_id, &root_id, true).unwrap();
    assert!(source.source_overflow);
    assert!(source.manifest_hash.len() == 64);
}

#[test]
fn source_overflow_can_never_report_a_current_recap() {
    assert_eq!(
        recap_status(true, true, true, true),
        ("stale", Some("source_overflow"))
    );
    assert_eq!(recap_status(false, true, true, true), ("current", None));
}

#[test]
fn manual_settings_require_explicit_model_and_bound_profile() {
    let supported = RecapRuntimeOption {
        id: "hermes".to_string(),
        label: "Hermes Agent".to_string(),
        kind: "hermes".to_string(),
        availability: "supported".to_string(),
        reason: None,
        capability_fingerprint: Some("fingerprint".to_string()),
        profiles: vec![RecapProfileOption {
            id: "scout".to_string(),
            label: "scout".to_string(),
        }],
        models: vec!["hermes-low".to_string()],
    };
    let missing_model = RecapSettings {
        mode: RecapMode::Manual,
        runtime_id: Some("hermes".to_string()),
        profile_ref: Some("scout".to_string()),
        capability_fingerprint: Some("fingerprint".to_string()),
        ..default_settings()
    };
    assert_eq!(
        validate_settings(&missing_model, std::slice::from_ref(&supported)),
        Err("missing_selection".to_string())
    );

    let wrong_profile = RecapSettings {
        requested_model: Some("hermes-low".to_string()),
        profile_ref: Some("other".to_string()),
        ..missing_model
    };
    assert_eq!(
        validate_settings(&wrong_profile, std::slice::from_ref(&supported)),
        Err("profile_mismatch".to_string())
    );
}

#[test]
fn source_manifest_changes_for_content_tags_and_scope() {
    let channel_id = "550e8400-e29b-41d4-a716-446655440000";
    let root = event("root", 1, channel_id);
    let root_id = root.id.to_hex();
    let first = build_thread_source(vec![root.clone()], channel_id, &root_id).unwrap();
    let changed = event("changed", 1, channel_id);
    let changed_id = changed.id.to_hex();
    let changed_source = build_thread_source(vec![changed], channel_id, &changed_id).unwrap();
    assert_ne!(first.manifest_hash, changed_source.manifest_hash);
    let other_scope =
        build_thread_source(vec![root], "6ba7b810-9dad-11d1-80b4-00c04fd430c8", &root_id).unwrap();
    assert_ne!(first.manifest_hash, other_scope.manifest_hash);
}
