use super::*;
use crate::app_state::{build_app_state, AppState, IdentityStorage};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::Manager;

fn fixture() -> (
    tauri::App<tauri::test::MockRuntime>,
    tempfile::TempDir,
    PathBuf,
) {
    let app = tauri::test::mock_builder()
        .manage(build_app_state())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir
        .path()
        .canonicalize()
        .unwrap()
        .join("owner-operations/recovery.db");
    (app, dir, path)
}

#[tokio::test]
async fn owner_scope_adapter_rejects_stale_input_before_storage_action() {
    let (app, _dir, path) = fixture();
    let mut expected = capture(app.handle().clone()).await.unwrap().token;
    expected.identity_generation += 1;
    let called = Arc::new(AtomicBool::new(false));
    let action_called = called.clone();
    let result = run_at_path(app.handle().clone(), path.clone(), expected, move |_, _| {
        action_called.store(true, Ordering::Release);
        Ok(())
    })
    .await;
    assert!(matches!(result, Err(ref error) if error == STALE));
    assert!(!called.load(Ordering::Acquire));
    assert!(!path.exists());
}

#[tokio::test]
async fn owner_scope_adapter_fences_import_aba_after_real_durable_write() {
    let (app, dir, path) = fixture();
    let initial = capture(app.handle().clone()).await.unwrap();
    let token = initial.token.clone();
    let scope = token.scope.clone();
    let id = uuid::Uuid::new_v4().to_string();
    let saved_id = id.clone();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let handle = app.handle().clone();
    let worker_path = path.clone();
    let pending = tokio::spawn(async move {
        run_at_path(handle, worker_path, token, move |store, scope| {
            let result = store
                .create(
                    scope,
                    NewOperation {
                        id,
                        kind: crate::owner_operations::OperationKind::ProjectChange,
                        resource_key: "project".into(),
                        payload: serde_json::json!({"event":"signed-exact"}),
                    },
                    100,
                )
                .map_err(|error| error.to_string())?;
            entered_tx.send(()).unwrap();
            release_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            Ok(result)
        })
        .await
    });
    entered_rx.await.unwrap();
    {
        let state = app.state::<AppState>();
        let guard = state.identity_mutation.lock().unwrap();
        for keys in [nostr::Keys::generate(), initial.keys] {
            crate::commands::commit_imported_identity(&state, &guard, dir.path(), keys, |_| {
                Ok(IdentityStorage::LocalFile)
            })
            .unwrap();
        }
    }
    release_tx.send(()).unwrap();
    let result = pending.await.unwrap();
    assert!(
        matches!(result, Err(ref error) if error == STALE),
        "old-scope result must not reach consumer"
    );
    let store = OperationStore::open(&path, Limits::default()).unwrap();
    assert_eq!(
        store.load(&scope, &saved_id).unwrap().payload["event"],
        "signed-exact"
    );
}

#[tokio::test]
async fn owner_scope_adapter_returns_native_token_and_scoped_record() {
    let (app, _dir, path) = fixture();
    let token = capture(app.handle().clone()).await.unwrap().token;
    let expected = token.clone();
    let result = run_at_path(app.handle().clone(), path, token, |store, scope| {
        store
            .list(scope, None, 10)
            .map_err(|error| error.to_string())
    })
    .await
    .unwrap();
    assert_eq!(result.token, expected);
    assert!(result.value.is_empty());
}

#[tokio::test]
async fn owner_scope_adapter_resolves_trusted_app_data_alias() {
    let (app, dir, _) = fixture();
    let anchor = dir.path().join("actual-data");
    std::fs::create_dir(&anchor).unwrap();
    let alias = dir.path().join("data-alias");
    std::os::unix::fs::symlink(&anchor, &alias).unwrap();
    let path = journal_path_from_app_data(&alias).unwrap();
    let token = capture(app.handle().clone()).await.unwrap().token;
    run_at_path(app.handle().clone(), path.clone(), token, |store, scope| {
        store
            .list(scope, None, 1)
            .map_err(|error| error.to_string())
    })
    .await
    .unwrap();
    assert_eq!(
        path,
        anchor
            .canonicalize()
            .unwrap()
            .join("owner-operations/recovery.db")
    );
}

#[tokio::test]
async fn owner_scope_adapter_dispatch_binds_revision_scope_and_signer() {
    let (app, _dir, path) = fixture();
    let initial = capture(app.handle().clone()).await.unwrap();
    let token = initial.token.clone();
    let id = uuid::Uuid::new_v4().to_string();
    let mut store = OperationStore::open(&path, Limits::default()).unwrap();
    store
        .create(
            &token.scope,
            NewOperation {
                id: id.clone(),
                kind: crate::owner_operations::OperationKind::ProjectChange,
                resource_key: "dispatch".into(),
                payload: serde_json::json!({"event":"exact"}),
            },
            100,
        )
        .unwrap();
    let mismatch = load_owner_operation_for_dispatch_at_path(
        app.handle().clone(),
        path.clone(),
        token.clone(),
        id.clone(),
        1,
    )
    .await;
    assert!(matches!(mismatch, Err(ref error) if error == "recovery operation changed; reload"));
    let (captured, operation) = load_owner_operation_for_dispatch_at_path(
        app.handle().clone(),
        path.clone(),
        token.clone(),
        id.clone(),
        0,
    )
    .await
    .unwrap();
    assert_eq!(captured.token, token);
    assert_eq!(captured.keys.public_key(), initial.keys.public_key());
    assert_eq!(captured.relay_url, initial.relay_url);
    assert_eq!(operation.scope, token.scope);
    assert_eq!(operation.payload["event"], "exact");
    let mut stale = token;
    stale.identity_generation += 1;
    assert!(
        matches!(load_owner_operation_for_dispatch_at_path(app.handle().clone(), path, stale, id, 0).await, Err(ref error) if error == STALE)
    );
}
