//! Lost-IPC idempotency for explicit Wiki regeneration.
//!
//! These tests drive the production helper `wiki_publication_regenerate` uses
//! before it captures any source, against a real SQLite journal that is closed
//! and reopened between requests.

use super::*;
use crate::app_state::build_app_state;
use crate::app_state::owner_scope::{capture, OwnerScopeToken};
use crate::commands::wiki_publication_record::WikiPublicationReconciliation;
use crate::commands::wiki_publication_test_fixture as fixture;
use crate::owner_operations::{
    CreateResult, Limits, Operation, OperationStore, WikiSuccessorResult,
};
use nostr::Keys;
use std::path::PathBuf;
use tempfile::TempDir;

fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
    tauri::test::mock_builder()
        .manage(build_app_state())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app")
}

fn wiki_id(number: u128) -> String {
    uuid::Uuid::from_u128(number).hyphenated().to_string()
}

fn created(result: CreateResult) -> Operation {
    match result {
        CreateResult::Created(operation) | CreateResult::Existing(operation) => operation,
    }
}

/// Retirement snapshot exactly as the command builds it: only the recovery
/// fields move, so the store's strict payload fence accepts it.
fn superseded_update(record: &WikiPublicationRecord) -> OperationUpdate {
    let mut retired = record.clone();
    retired.reconcile_only = true;
    retired.reconciliation = Some(WikiPublicationReconciliation::Superseded {
        current_head_id: None,
        retired_dependency_id: None,
    });
    retired.lease = None;
    retired.retry_at = 0;
    retired.last_error = None;
    OperationUpdate {
        status: OperationStatus::Superseded,
        reconciled: true,
        payload: serde_json::to_value(retired).expect("retirement JSON"),
    }
}

struct Fixture {
    _dir: TempDir,
    path: PathBuf,
    token: OwnerScopeToken,
    keys: Keys,
    coordinate: String,
    predecessor: Operation,
    predecessor_record: WikiPublicationRecord,
}

impl Fixture {
    async fn new(app: &tauri::App<tauri::test::MockRuntime>) -> Self {
        let captured = capture(app.handle().clone()).await.expect("native scope");
        let dir = tempfile::tempdir().expect("journal directory");
        let path = dir
            .path()
            .canonicalize()
            .expect("canonical journal path")
            .join("recovery.db");
        let coordinate = fixture::coordinate(&captured.keys, "crew");
        let predecessor_record = fixture::record(
            fixture::publication(&captured.keys, "crew", None),
            &coordinate,
            &captured.keys,
        );
        let mut store = OperationStore::open(&path, Limits::default()).expect("journal");
        let predecessor = created(
            store
                .create(
                    &captured.token.scope,
                    NewOperation {
                        id: wiki_id(1),
                        kind: OperationKind::WikiPublication,
                        resource_key: coordinate.clone(),
                        payload: serde_json::to_value(&predecessor_record)
                            .expect("predecessor JSON"),
                    },
                    100,
                )
                .expect("reserve the retired publication"),
        );
        drop(store);
        Self {
            _dir: dir,
            path,
            token: captured.token,
            keys: captured.keys,
            coordinate,
            predecessor,
            predecessor_record,
        }
    }

    fn store(&self) -> OperationStore {
        OperationStore::open(&self.path, Limits::default()).expect("reopen journal")
    }

    /// Commit the atomic retirement the way the command does, then close the
    /// file so the retry below starts from a cold open.
    fn commit_successor(&self, successor_record: &WikiPublicationRecord) -> WikiSuccessorResult {
        let mut store = self.store();
        store
            .replace_wiki_with_successor(
                &self.token.scope,
                &self.predecessor.id,
                self.predecessor.revision,
                superseded_update(&self.predecessor_record),
                NewOperation {
                    id: wiki_id(2),
                    kind: OperationKind::WikiPublication,
                    resource_key: self.coordinate.clone(),
                    payload: serde_json::to_value(successor_record).expect("successor JSON"),
                },
                101,
            )
            .expect("atomic replacement")
    }

    async fn lookup(
        &self,
        app: &tauri::App<tauri::test::MockRuntime>,
        revision: u64,
    ) -> Result<Option<WikiPublicationJob>, String> {
        committed_regeneration_job(
            app.handle().clone(),
            self.path.clone(),
            self.token.clone(),
            self.predecessor.id.clone(),
            revision,
        )
        .await
    }
}

#[tokio::test]
async fn regenerate_recovers_its_committed_successor_after_a_lost_ipc_response() {
    let app = mock_app();
    let state = Fixture::new(&app).await;
    let successor_record = fixture::record(
        fixture::publication(&state.keys, "crew", None),
        &state.coordinate,
        &state.keys,
    );
    assert_ne!(
        state.predecessor_record.snapshot_id, successor_record.snapshot_id,
        "regeneration must build a fresh immutable snapshot"
    );

    // Before any replacement, the request has nothing to recover and must fall
    // through to real source capture.
    assert!(state
        .lookup(&app, state.predecessor.revision)
        .await
        .expect("lookup")
        .is_none());

    let replacement = state.commit_successor(&successor_record);
    let recovered = state
        .lookup(&app, state.predecessor.revision)
        .await
        .expect("lookup")
        .expect("the committed successor is recoverable after a lost response");
    assert_eq!(recovered.id, replacement.successor.id);
    assert_eq!(recovered.snapshot_id, successor_record.snapshot_id);
    assert_eq!(recovered.resource_key, state.coordinate);
    assert!(!recovered.reconciled);

    // A fast worker may finish the successor before the caller retries; the
    // now terminal row is still the one valid outcome of this request.
    let completed = state
        .store()
        .compare_and_swap(
            &state.token.scope,
            &replacement.successor.id,
            replacement.successor.revision,
            OperationUpdate {
                status: OperationStatus::Complete,
                reconciled: true,
                payload: replacement.successor.payload.clone(),
            },
            102,
        )
        .expect("successor completes");
    let after_completion = state
        .lookup(&app, state.predecessor.revision)
        .await
        .expect("lookup")
        .expect("a completed successor is still recoverable");
    assert_eq!(after_completion.id, completed.id);
    assert!(after_completion.reconciled);
    assert_eq!(after_completion.status, OperationStatus::Complete);
}

#[tokio::test]
async fn regenerate_lookup_binds_the_requested_pre_retirement_revision() {
    let app = mock_app();
    let state = Fixture::new(&app).await;
    let successor_record = fixture::record(
        fixture::publication(&state.keys, "crew", None),
        &state.coordinate,
        &state.keys,
    );
    state.commit_successor(&successor_record);

    // The link records the post-retirement revision. Only `requested + 1` may
    // resolve it; any other revision is a different request.
    assert_eq!(
        state
            .store()
            .load(&state.token.scope, &state.predecessor.id)
            .expect("retired predecessor")
            .revision,
        state.predecessor.revision + 1
    );
    // `revision + 1` is the *post*-retirement revision a naive caller might
    // reload and resend; it is not the request this relation belongs to.
    for wrong in [
        state.predecessor.revision + 1,
        state.predecessor.revision + 7,
    ] {
        let error = state
            .lookup(&app, wrong)
            .await
            .expect_err("a different revision must never adopt this successor");
        assert!(
            error.contains("changed"),
            "expected an explicit conflict, got {error}"
        );
    }
    let overflow = state.lookup(&app, u64::MAX).await;
    assert!(
        overflow.is_err(),
        "an overflowing revision must fail closed"
    );
}

#[tokio::test]
async fn regenerate_lookup_ignores_an_unrelated_predecessor() {
    let app = mock_app();
    let state = Fixture::new(&app).await;
    let unrelated = committed_regeneration_job(
        app.handle().clone(),
        state.path.clone(),
        state.token.clone(),
        wiki_id(99),
        0,
    )
    .await
    .expect("lookup");
    assert!(
        unrelated.is_none(),
        "an operation with no recorded relation must never inherit one"
    );
}
