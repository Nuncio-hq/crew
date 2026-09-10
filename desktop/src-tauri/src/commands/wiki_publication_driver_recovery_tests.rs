//! Recovery-side dispatcher invariants, on the same real journal fixture.

use super::*;
use crate::commands::wiki_publication_commands::projected_job;
use crate::commands::wiki_publication_record::{WikiHeadRetirement, WikiPublicationReconciliation};

fn retirement_proof(record: &WikiPublicationRecord) -> Option<String> {
    match &record.reconciliation {
        Some(WikiPublicationReconciliation::ImmutableDependencyRetired { dependency_id }) => {
            Some(dependency_id.clone())
        }
        _ => None,
    }
}

/// A dependency that never comes back must never be reported as success, no
/// matter how many attempts the driver makes.
#[tokio::test]
async fn driver_never_completes_while_an_immutable_dependency_is_missing() {
    let (journal, operation) = Journal::fixture_for("crew.repair");
    let id = operation.id.clone();
    journal.head.store(HEAD_MISSING, Ordering::SeqCst);
    journal
        .dependencies
        .store(DEPENDENCIES_ALWAYS_MISSING, Ordering::SeqCst);

    let result = drive(&journal, operation, journal.owner, false, false)
        .await
        .expect("a missing dependency is a durable failure");
    assert_eq!(result.status, OperationStatus::Failed);
    assert!(!result.reconciled);
    let record = journal.stored_record(&id);
    assert!(
        record
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("disappeared")),
        "the durable failure must name the missing dependency check"
    );
    assert!(!record.reconcile_only, "an ordinary retry stays available");
    let durable = journal.reopened(&id);
    assert!(!durable.reconciled);
    assert_ne!(durable.status, OperationStatus::Complete);
}

/// A retry must reuse the exact persisted envelopes; recovery may never sign a
/// replacement page, manifest, or head.
#[tokio::test]
async fn driver_retains_exact_signed_event_ids_across_failure_and_reopen() {
    let (journal, operation) = Journal::fixture();
    let id = operation.id.clone();
    let before = journal.stored_record(&id);
    journal.lost_head_ack.store(true, Ordering::SeqCst);

    let failed = drive(&journal, operation, journal.owner, false, false)
        .await
        .expect("durable failure");
    assert_eq!(failed.status, OperationStatus::Failed);

    let after = journal.stored_record(&id);
    assert_eq!(after.head, before.head);
    assert_eq!(after.manifest, before.manifest);
    assert_eq!(after.pages, before.pages);
    assert_eq!(after.snapshot_id, before.snapshot_id);
    assert_eq!(after.source_revision, before.source_revision);
    assert_eq!(after.expected_revision, before.expected_revision);
    assert_eq!(
        journal.sent_ids.lock().expect("sent ids").as_slice(),
        [before.head.id.to_hex()],
        "only the exact persisted head may be sent"
    );
}

/// Five automatic attempts, then the row waits for an explicit user retry.
#[tokio::test]
async fn driver_bounds_automatic_attempts_at_five_before_requiring_an_explicit_retry() {
    let (journal, operation) = Journal::fixture();
    let id = operation.id.clone();
    journal.lost_head_ack.store(true, Ordering::SeqCst);

    let mut current = operation;
    for attempt in 1..=5_u8 {
        // The dispatcher schedules a backoff; recovery only runs a row once it
        // is due, so advance the fixture clock to that time.
        journal.clock.store(
            journal.stored_record(&id).retry_at.max(100),
            Ordering::SeqCst,
        );
        current = drive(&journal, current, journal.owner, false, false)
            .await
            .expect("bounded automatic attempt");
        assert_eq!(journal.stored_record(&id).attempts, attempt);
        assert!(!current.reconciled);
    }
    journal
        .clock
        .store(journal.stored_record(&id).retry_at, Ordering::SeqCst);
    assert!(
        drive(&journal, current.clone(), journal.owner, false, false)
            .await
            .is_err(),
        "a sixth automatic attempt must be refused"
    );
    assert_eq!(journal.sent.load(Ordering::SeqCst), 5);

    // An explicit retry resets the window and sends again.
    let retried = drive(&journal, current, journal.owner, true, false)
        .await
        .expect("explicit retry");
    assert_eq!(journal.stored_record(&id).attempts, 1);
    assert!(!retried.reconciled);
    assert_eq!(journal.sent.load(Ordering::SeqCst), 6);
}

/// An owner replacement that lands while the head publish is in flight must
/// fence the result, on both the success and the failure path, and must not
/// write a terminal state under the new identity.
#[tokio::test]
async fn driver_fences_owner_generation_aba_after_awaiting_success_and_error() {
    for lost_ack in [false, true] {
        let (journal, operation) = Journal::fixture();
        let id = operation.id.clone();
        journal.lost_head_ack.store(lost_ack, Ordering::SeqCst);
        journal
            .rotate_owner_on_head_publish
            .store(true, Ordering::SeqCst);
        journal
            .late_commit_after_head_publish
            .store(true, Ordering::SeqCst);

        let error = drive(&journal, operation, journal.owner, false, false)
            .await
            .expect_err("a replaced owner must fence the awaited result");
        assert_eq!(error, crate::app_state::owner_scope::OWNER_SCOPE_STALE);
        let durable = journal.reopened(&id);
        assert!(
            !durable.reconciled,
            "a stale scope must not commit a terminal state"
        );
        assert_ne!(durable.status, OperationStatus::Complete);
        assert_eq!(journal.sent.load(Ordering::SeqCst), 1);
    }
}

/// The typed retirement proof is the only regeneration authorization, and it
/// is recorded for every live head classification.
#[tokio::test]
async fn driver_typed_retirement_proof_covers_original_desired_and_absent_head() {
    for head in [HEAD_ORIGINAL, HEAD_MISSING, HEAD_APPLIED] {
        let (journal, operation) = Journal::fixture();
        let id = operation.id.clone();
        journal.head.store(head, Ordering::SeqCst);
        journal
            .dependencies
            .store(DEPENDENCIES_MISSING_UNTIL_REPAIR, Ordering::SeqCst);
        journal.retire_dependency.store(true, Ordering::SeqCst);

        let result = drive(&journal, operation, journal.owner, false, false)
            .await
            .expect("head classification is representable");
        let record = journal.stored_record(&id);
        if head == HEAD_APPLIED {
            // The runtime already classified the desired head as applied, so
            // nothing is published and no dependency can be retired.
            assert_eq!(result.status, OperationStatus::Complete);
            assert!(retirement_proof(&record).is_none());
            continue;
        }
        assert_eq!(result.status, OperationStatus::Reconciling);
        assert!(!result.reconciled, "a retired dependency stays unresolved");
        assert!(record.reconcile_only);
        let dependency_id = retirement_proof(&record).expect("typed retirement proof");
        assert!(std::iter::once(&record.manifest)
            .chain(record.pages.iter())
            .any(|event| event.id.to_hex() == dependency_id));
    }
}

/// D-079: a local revision fence cannot revoke an already-admitted request, so
/// both relay commit orders must leave an explicit recovery path and neither
/// may strand a permanent claim.
#[tokio::test]
async fn driver_resolves_both_relay_commit_orders_without_stranding_a_claim() {
    // Old send commits first: this attempt's conditional write loses the CAS
    // and the row is superseded, reconciled, with the winning head recorded.
    let (journal, operation) = Journal::fixture();
    let id = operation.id.clone();
    journal.head.store(HEAD_CONFLICT, Ordering::SeqCst);
    let superseded = drive(&journal, operation, journal.owner, false, false)
        .await
        .expect("a conflicting head is a resolvable outcome");
    assert_eq!(superseded.status, OperationStatus::Superseded);
    assert!(
        superseded.reconciled,
        "a lost CAS must release the local claim"
    );
    assert_eq!(
        journal.sent.load(Ordering::SeqCst),
        0,
        "a conflicting head must be detected before any write"
    );
    let record = journal.stored_record(&id);
    assert!(matches!(
        record.reconciliation,
        Some(WikiPublicationReconciliation::Superseded {
            current_head_id: Some(_),
            ..
        })
    ));
    assert!(journal.reopened(&id).reconciled);

    // New send commits first: the relay keeps this exact head, so the earlier
    // precondition can never overwrite it and the row completes.
    let (winner, operation) = Journal::fixture_for("crew.winner");
    let winner_id = operation.id.clone();
    winner
        .late_commit_after_head_publish
        .store(true, Ordering::SeqCst);
    let applied = drive(&winner, operation, winner.owner, false, false)
        .await
        .expect("the exact head is retained");
    assert_eq!(applied.status, OperationStatus::Complete);
    assert!(applied.reconciled);
    let applied_record = winner.stored_record(&winner_id);
    assert!(matches!(
        applied_record.reconciliation,
        Some(WikiPublicationReconciliation::Applied { .. })
    ));
}

/// R2: a conflict resolution replaces the whole reconciliation value, so the
/// typed immutable-dependency proof must be carried into the new Superseded
/// proof — in the durable row, through reopen, and through the public job
/// projection the renderer actually consumes.
#[tokio::test]
async fn driver_conflict_preserves_an_existing_typed_dependency_retirement() {
    let (journal, operation) = Journal::fixture_for("crew.preserve.dependency");
    let id = operation.id.clone();
    journal.head.store(HEAD_MISSING, Ordering::SeqCst);
    journal
        .dependencies
        .store(DEPENDENCIES_MISSING_UNTIL_REPAIR, Ordering::SeqCst);
    journal.retire_dependency.store(true, Ordering::SeqCst);
    let retired = drive(&journal, operation, journal.owner, false, false)
        .await
        .expect("typed retirement proof is recorded");
    assert!(!retired.reconciled);
    let before = journal.stored_record(&id);
    let dependency_id = retirement_proof(&before).expect("typed dependency proof");

    // A competing head now wins the coordinate. Reconciling that conflict is
    // where the proof used to be silently dropped.
    journal.head.store(HEAD_CONFLICT, Ordering::SeqCst);
    let settled = drive(&journal, journal.reopened(&id), journal.owner, false, true)
        .await
        .expect("read-only reconciliation resolves the conflict");
    assert_eq!(settled.status, OperationStatus::Superseded);
    assert!(settled.reconciled);

    let after = journal.stored_record(&id);
    match &after.reconciliation {
        Some(WikiPublicationReconciliation::Superseded {
            current_head_id: Some(_),
            retired_dependency_id: Some(preserved),
            ..
        }) => assert_eq!(
            *preserved, dependency_id,
            "the dependency proof is retained"
        ),
        other => panic!("expected a Superseded proof carrying the dependency: {other:?}"),
    }
    // The same fact must survive a journal reopen and reach the projection.
    let reopened = journal.reopened(&id);
    assert!(reopened.reconciled);
    let job = projected_job(&reopened).expect("terminal projection");
    assert_eq!(
        job.retired_dependency_id.as_deref(),
        Some(&dependency_id[..])
    );
    // The signed graph and recorded progress are untouched by reconciliation.
    assert_eq!(after.head, before.head);
    assert_eq!(after.manifest, before.manifest);
    assert_eq!(after.pages, before.pages);
    assert_eq!(after.progress, before.progress);
    assert_eq!(after.head_attempted, before.head_attempted);
}

/// R1: a validated head-retirement proof settles the operation terminally in
/// one guarded CAS, releasing the resource claim so a fresh Generate can take
/// it — with no further network step after the proof.
#[tokio::test]
async fn driver_head_retirement_proof_settles_superseded_and_releases_the_claim() {
    let (journal, operation) = Journal::fixture_for("crew.head.retired");
    let id = operation.id.clone();
    let before = journal.stored_record(&id);
    let head_id = before.head.id.to_hex();
    *journal.retire_head.lock().expect("head retirement") = Some(WikiHeadRetirementProof {
        retirement: WikiHeadRetirement::Head {
            head_id: head_id.clone(),
        },
        current_head_id: None,
    });

    let settled = drive(&journal, operation, journal.owner, false, false)
        .await
        .expect("a proven retired head is terminal");
    assert_eq!(settled.status, OperationStatus::Superseded);
    assert!(settled.reconciled, "the unresolved claim is released");
    assert_eq!(
        journal.sent.load(Ordering::SeqCst),
        1,
        "the proof arrives from the head send; nothing is sent afterwards"
    );

    let after = journal.stored_record(&id);
    match &after.reconciliation {
        Some(WikiPublicationReconciliation::Superseded {
            head_retirement: Some(WikiHeadRetirement::Head { head_id: proven }),
            ..
        }) => assert_eq!(*proven, head_id, "the proof binds this exact signed head"),
        other => panic!("expected a head-retirement proof: {other:?}"),
    }
    assert!(journal.reopened(&id).reconciled, "terminal after reopen");
    // The persisted signed graph is never replaced by settling.
    assert_eq!(after.head, before.head);
    assert_eq!(after.manifest, before.manifest);
    assert_eq!(after.pages, before.pages);
}
