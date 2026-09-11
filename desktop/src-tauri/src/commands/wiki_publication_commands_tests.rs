//! Renderer contract, history selection, and foreground/worker arbitration.

use super::*;
use crate::owner_operations::{Operation, OperationScope, OperationSummary};

fn summary(
    sequence: i64,
    id: u8,
    revision: u64,
    updated_at: i64,
    reconciled: bool,
) -> OperationSummary {
    OperationSummary {
        sequence,
        id: format!("00000000-0000-4000-8000-0000000000{id:02}"),
        kind: OperationKind::WikiPublication,
        resource_key: "30617:owner:repo".into(),
        revision,
        status: if reconciled {
            OperationStatus::Complete
        } else {
            OperationStatus::Reconciling
        },
        reconciled,
        updated_at,
    }
}

#[test]
fn prepare_noop_uses_renderer_field_names() {
    let value = serde_json::to_value(WikiPublicationPrepareResult::Noop {
        head_id: "head".into(),
        source_revision: "git:revision".into(),
    })
    .expect("serialize Wiki prepare no-op");
    assert_eq!(
        value,
        serde_json::json!({
            "result": "noop",
            "headId": "head",
            "sourceRevision": "git:revision"
        })
    );
}

#[test]
fn history_selection_prefers_an_unresolved_claim_over_any_terminal_row() {
    let unresolved = summary(1, 1, 0, 100, false);
    let newer_terminal = summary(99, 2, 40, 5_000, true);
    assert!(projection::prefer_operation(&unresolved, &newer_terminal));
    assert!(!projection::prefer_operation(&newer_terminal, &unresolved));
}

#[test]
fn history_selection_ignores_cross_operation_revision_and_wall_clock() {
    // A long-lived predecessor accumulates local revisions and a late
    // `updated_at`; its successor is a *different* row that starts at revision
    // 0 and may even carry an earlier clock value.
    let predecessor = summary(10, 10, 100, 1_000, true);
    let successor = summary(11, 11, 10, 900, true);
    assert!(projection::prefer_operation(&successor, &predecessor));
    assert!(!projection::prefer_operation(&predecessor, &successor));

    // The same rule holds for an ordinary later Generate operation that has
    // no successor relation to the older row at all.
    let ordinary = summary(12, 12, 0, 1, true);
    assert!(projection::prefer_operation(&ordinary, &predecessor));
    assert!(!projection::prefer_operation(&predecessor, &ordinary));
}

#[test]
fn foreground_decision_drives_the_exact_requested_revision() {
    let operation = row(4, false, OperationStatus::Reconciling);
    assert!(matches!(
        foreground_decision(operation, 4, false),
        Ok(ForegroundDecision::Drive(_))
    ));
}

#[test]
fn foreground_decision_accepts_a_row_the_recovery_worker_already_claimed() {
    // Startup recovery renewed a lease and advanced the revision under the
    // serialized lock. The user's immediate dispatch is still the same action.
    let claimed = row(7, false, OperationStatus::Reconciling);
    match foreground_decision(claimed, 4, false) {
        Ok(ForegroundDecision::Drive(operation)) => assert_eq!(operation.revision, 7),
        _ => panic!("a worker-owned lease advance must remain drivable"),
    }
}

#[test]
fn foreground_decision_reports_a_completed_row_instead_of_a_revision_error() {
    for status in [
        OperationStatus::Complete,
        OperationStatus::Canceled,
        OperationStatus::Superseded,
    ] {
        let settled = row(9, true, status);
        match foreground_decision(settled, 4, false) {
            Ok(ForegroundDecision::Settled(operation)) => {
                assert!(operation.reconciled);
                assert_eq!(operation.status, status);
            }
            _ => panic!("a completed operation must not fail an immediate dispatch"),
        }
    }
}

#[test]
fn foreground_decision_keeps_an_explicit_conflict_for_a_user_cancellation() {
    let cancelled = row(5, false, OperationStatus::Reconciling);
    assert_eq!(
        foreground_decision(cancelled, 4, true).err().as_deref(),
        Some("recovery operation changed; reload")
    );
    // The same cancellation at the exact requested revision is the caller's
    // own view of the row and stays drivable (read-only inside `drive`).
    assert!(matches!(
        foreground_decision(row(4, false, OperationStatus::Reconciling), 4, true),
        Ok(ForegroundDecision::Drive(_))
    ));
}

#[test]
fn foreground_decision_rejects_a_row_of_another_kind() {
    let mut foreign = row(4, false, OperationStatus::Reconciling);
    foreign.kind = OperationKind::ProjectChange;
    assert!(foreground_decision(foreign, 4, false).is_err());
}

fn row(revision: u64, reconciled: bool, status: OperationStatus) -> Operation {
    Operation {
        version: 1,
        scope: OperationScope {
            owner: "a".repeat(64),
            community: "https://one.example".into(),
        },
        id: "00000000-0000-4000-8000-000000000001".into(),
        kind: OperationKind::WikiPublication,
        resource_key: "30617:owner:repo".into(),
        revision,
        created_at: 10,
        updated_at: 20,
        status,
        reconciled,
        payload: serde_json::json!({"step": "exact"}),
    }
}

#[cfg(unix)]
mod store_backed {
    use super::*;
    use crate::owner_operations::{
        CreateResult, Limits, NewOperation, OperationStore, OperationUpdate,
    };

    fn wiki_id(number: u128) -> String {
        uuid::Uuid::from_u128(number).hyphenated().to_string()
    }

    fn payload(marker: &str) -> serde_json::Value {
        serde_json::json!({
            "marker": marker,
            "reconciliation": null,
            "reconcile_only": false,
            "retry_at": 0,
            "last_error": null,
            "lease": null
        })
    }

    fn intent(id: u128, resource: &str, marker: &str) -> NewOperation {
        NewOperation {
            id: wiki_id(id),
            kind: OperationKind::WikiPublication,
            resource_key: resource.into(),
            payload: payload(marker),
        }
    }

    fn superseded(mut value: serde_json::Value) -> OperationUpdate {
        let object = value.as_object_mut().expect("payload object");
        object.insert(
            "reconciliation".into(),
            serde_json::json!({"proof": "superseded", "current_head_id": null}),
        );
        object.insert("reconcile_only".into(), serde_json::json!(true));
        OperationUpdate {
            status: OperationStatus::Superseded,
            reconciled: true,
            payload: value,
        }
    }

    fn created(result: CreateResult) -> Operation {
        match result {
            CreateResult::Created(operation) | CreateResult::Existing(operation) => operation,
        }
    }

    fn find<'a>(summaries: &'a [OperationSummary], id: &str) -> &'a OperationSummary {
        summaries
            .iter()
            .find(|summary| summary.id == id)
            .expect("listed summary")
    }

    /// The projection's ordering key must come from the durable store, not
    /// from a hand-written model of it.
    #[test]
    fn history_selection_follows_real_store_insertion_order_after_replacement() {
        let dir = tempfile::tempdir().expect("journal directory");
        let path = dir
            .path()
            .canonicalize()
            .expect("canonical journal path")
            .join("recovery.db");
        let mut store = OperationStore::open(&path, Limits::default()).expect("journal");
        let scope = OperationScope {
            owner: "a".repeat(64),
            community: "https://one.example".into(),
        };
        let resource = "30617:owner:repo";

        // A predecessor that lived long enough to accumulate local revisions
        // and a late wall clock.
        let predecessor = created(
            store
                .create(&scope, intent(1, resource, "old"), 1_000)
                .expect("create predecessor"),
        );
        let mut predecessor = predecessor;
        for step in 0..3 {
            predecessor = store
                .compare_and_swap(
                    &scope,
                    &predecessor.id,
                    predecessor.revision,
                    OperationUpdate {
                        status: OperationStatus::Reconciling,
                        reconciled: false,
                        payload: payload(&format!("old-{step}")),
                    },
                    1_000,
                )
                .expect("advance predecessor");
        }
        // The successor is reserved with a *backwards* clock.
        let replacement = store
            .replace_wiki_with_successor(
                &scope,
                &predecessor.id,
                predecessor.revision,
                superseded(predecessor.payload.clone()),
                intent(2, resource, "new"),
                900,
            )
            .expect("atomic replacement");
        let successor = store
            .compare_and_swap(
                &scope,
                &replacement.successor.id,
                replacement.successor.revision,
                OperationUpdate {
                    status: OperationStatus::Complete,
                    reconciled: true,
                    payload: replacement.successor.payload.clone(),
                },
                900,
            )
            .expect("successor completes");

        let summaries = store.list(&scope, None, 100).expect("durable summaries");
        let old = find(&summaries, &predecessor.id);
        let new = find(&summaries, &successor.id);
        assert!(old.revision > new.revision, "revision favours the old row");
        assert!(
            new.sequence > old.sequence,
            "the durable insertion sequence must order the successor last"
        );
        assert!(projection::prefer_operation(new, old));
        assert!(!projection::prefer_operation(old, new));
    }

    /// An ordinary later Generate for the same coordinate wins even when the
    /// older row carries a higher wall clock and revision.
    #[test]
    fn history_selection_prefers_a_later_ordinary_generate_over_an_older_clock() {
        let dir = tempfile::tempdir().expect("journal directory");
        let path = dir
            .path()
            .canonicalize()
            .expect("canonical journal path")
            .join("recovery.db");
        let mut store = OperationStore::open(&path, Limits::default()).expect("journal");
        let scope = OperationScope {
            owner: "b".repeat(64),
            community: "https://one.example".into(),
        };
        let resource = "30617:owner:repo";

        let first = created(
            store
                .create(&scope, intent(11, resource, "first"), 5_000)
                .expect("first"),
        );
        let first = store
            .compare_and_swap(
                &scope,
                &first.id,
                first.revision,
                OperationUpdate {
                    status: OperationStatus::Complete,
                    reconciled: true,
                    payload: first.payload.clone(),
                },
                5_000,
            )
            .expect("first completes");
        let second = created(
            store
                .create(&scope, intent(12, resource, "second"), 900)
                .expect("second"),
        );
        let second = store
            .compare_and_swap(
                &scope,
                &second.id,
                second.revision,
                OperationUpdate {
                    status: OperationStatus::Complete,
                    reconciled: true,
                    payload: second.payload.clone(),
                },
                900,
            )
            .expect("second completes");

        let summaries = store.list(&scope, None, 100).expect("durable summaries");
        let older = find(&summaries, &first.id);
        let newer = find(&summaries, &second.id);
        assert!(
            older.updated_at > newer.updated_at,
            "wall clock favours the older row"
        );
        assert!(projection::prefer_operation(newer, older));
        assert!(!projection::prefer_operation(older, newer));
    }
}
