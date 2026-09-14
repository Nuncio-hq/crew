use super::*;
use crate::app_state::{build_app_state, owner_scope::capture};
use crate::commands::owner_operations::{
    owner_operation_create_at_path, owner_operation_load_at_path,
};
use crate::commands::wiki_publication_commands::WikiPublicationBuildInput;
use crate::commands::wiki_publication_native_reads::{NativeClock, NativeJournal};
use crate::commands::wiki_publication_record::WikiPublicationRecord;
use crate::commands::wiki_publication_test_fixture as fixture;
use crate::commands::wiki_publication_worker::{release, reserve, run_due_with_context};
use crate::owner_operations::CreateResult;

#[tokio::test]
async fn wiki_generation_claim_survives_live_worker_and_settles_after_interruption() {
    let app = tauri::test::mock_builder()
        .manage(build_app_state())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    let handle = app.handle().clone();
    let captured = capture(handle.clone()).await.expect("scope");
    let coordinate = fixture::coordinate(&captured.keys, "generation-restart");
    let id = uuid::Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("temp");
    let path = dir
        .path()
        .canonicalize()
        .expect("canonical temp path")
        .join("operations")
        .join("recovery.db");
    let registration =
        GenerationRegistration::begin(&captured.token, &id, &coordinate).expect("registration");
    let result = owner_operation_create_at_path(
        handle.clone(),
        path.clone(),
        captured.token.clone(),
        WikiGenerationRecord::new_operation(id.clone(), coordinate.clone()).expect("intent"),
    )
    .await
    .expect("create");
    let operation = match result.value {
        CreateResult::Created(operation) => operation,
        _ => panic!("new operation"),
    };
    let record = WikiGenerationRecord::read(&operation)
        .expect("valid")
        .expect("generation");
    assert_eq!(record.job(&operation).progress, "generation");
    assert!(
        WikiPublicationRecord::from_operation(&operation, captured.keys.public_key()).is_err(),
        "an incomplete generation can never dispatch as a signed publication"
    );
    // Install worker state, then remove its brief reservation. The actual
    // generation registration, not a timer lease, keeps the live row safe.
    reserve(&handle, &captured.token, &coordinate);
    release(&handle, &captured.token, &coordinate);
    run_due_with_context(
        handle.clone(),
        captured.token.clone(),
        NativeJournal::Path(path.clone()),
        NativeClock::System,
    )
    .await
    .expect("live tick");
    let live = owner_operation_load_at_path(
        handle.clone(),
        path.clone(),
        captured.token.clone(),
        id.clone(),
        None,
    )
    .await
    .expect("read")
    .value;
    assert_eq!(live.revision, 0);
    assert!(!live.reconciled);
    drop(registration);
    run_due_with_context(
        handle.clone(),
        captured.token.clone(),
        NativeJournal::Path(path.clone()),
        NativeClock::System,
    )
    .await
    .expect("restart tick");
    let interrupted = owner_operation_load_at_path(
        handle.clone(),
        path.clone(),
        captured.token.clone(),
        id,
        None,
    )
    .await
    .expect("read")
    .value;
    assert_eq!(interrupted.status, OperationStatus::Canceled);
    assert!(interrupted.reconciled);
    assert!(WikiGenerationRecord::read(&interrupted)
        .expect("valid")
        .expect("generation")
        .error
        .expect("reason")
        .contains("interrupted"));
    let late = complete_generation_at_path(
        handle,
        path,
        captured.token,
        &operation,
        WikiPublicationBuildInput {
            owner: captured.keys.public_key().to_hex(),
            repo_d: "generation-restart".into(),
            generation: fixture::generation(),
            cadence: "manual".into(),
            expected_revision: None,
            created_at: 10,
            keys: captured.keys,
        },
    )
    .await;
    assert!(
        late.is_err(),
        "a late runtime completion must not resurrect a canceled generation"
    );
}

#[tokio::test]
async fn wiki_generation_promotes_the_same_claim_and_rejects_late_cancel() {
    let app = tauri::test::mock_builder()
        .manage(build_app_state())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    let handle = app.handle().clone();
    let captured = capture(handle.clone()).await.expect("scope");
    let coordinate = fixture::coordinate(&captured.keys, "generation-promotion");
    let id = uuid::Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("temp");
    let path = dir
        .path()
        .canonicalize()
        .expect("canonical temp path")
        .join("operations")
        .join("recovery.db");
    let created = owner_operation_create_at_path(
        handle.clone(),
        path.clone(),
        captured.token.clone(),
        WikiGenerationRecord::new_operation(id.clone(), coordinate).expect("intent"),
    )
    .await
    .expect("create");
    let original = match created.value {
        CreateResult::Created(operation) => operation,
        _ => panic!("new"),
    };
    let completed = complete_generation_at_path(
        handle.clone(),
        path.clone(),
        captured.token.clone(),
        &original,
        WikiPublicationBuildInput {
            owner: captured.keys.public_key().to_hex(),
            repo_d: "generation-promotion".into(),
            generation: fixture::generation(),
            cadence: "manual".into(),
            expected_revision: None,
            created_at: 10,
            keys: captured.keys.clone(),
        },
    )
    .await
    .expect("promote")
    .value;
    assert_eq!(completed.id, id);
    assert_eq!(completed.revision, 1);
    assert!(!completed.reconciled);
    assert!(WikiGenerationRecord::read(&completed)
        .expect("tag check")
        .is_none());
    WikiPublicationRecord::from_operation(&completed, captured.keys.public_key())
        .expect("valid signed graph");
    assert!(
        finish(handle, path, captured.token, &original, Some("cancel"))
            .await
            .is_err(),
        "stale generation cancel cannot overwrite the signed graph"
    );
}
#[tokio::test]
async fn wiki_generation_cancel_signals_only_after_durable_cas() {
    let app = tauri::test::mock_builder()
        .manage(build_app_state())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    let handle = app.handle().clone();
    let captured = capture(handle.clone()).await.expect("scope");
    let coordinate = fixture::coordinate(&captured.keys, "generation-promotion");
    let id = uuid::Uuid::new_v4().to_string();
    let dir = tempfile::tempdir().expect("temp");
    let path = dir
        .path()
        .canonicalize()
        .expect("canonical temp path")
        .join("operations")
        .join("recovery.db");
    let created = owner_operation_create_at_path(
        handle.clone(),
        path.clone(),
        captured.token.clone(),
        WikiGenerationRecord::new_operation(id.clone(), coordinate).expect("intent"),
    )
    .await
    .expect("create");
    let original = match created.value {
        CreateResult::Created(operation) => operation,
        _ => panic!("new"),
    };
    let registration = GenerationRegistration::begin(&captured.token, &id, &original.resource_key)
        .expect("register");
    let canceled = cancel(
        handle.clone(),
        path.clone(),
        captured.token.clone(),
        &original,
    )
    .await
    .expect("cancel")
    .value;
    assert!(registration.token.load(Ordering::Acquire));
    assert!(canceled.reconciled);
    assert_eq!(canceled.status, OperationStatus::Canceled);
    drop(registration);
    // A rejected duplicate CAS must not signal a newly registered process.
    let replacement = GenerationRegistration::begin(&captured.token, &id, &original.resource_key)
        .expect("replacement");
    assert!(cancel(handle, path, captured.token, &original)
        .await
        .is_err());
    assert!(!replacement.token.load(Ordering::Acquire));
}
