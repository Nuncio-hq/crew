//! Explicit "Resume publication" after an ambiguous cancellation.
//!
//! Every case here runs the production `drive` against the real temporary
//! SQLite journal fixture, cancels through the same durable CAS the cancel
//! command uses, and reopens the file before the recovery step — so an
//! assertion about what resume may or may not revoke is an assertion about
//! bytes that survived a process restart.

use super::*;
use crate::commands::wiki_publication_commands::{
    admit_cancel_generation, cancel_intent, may_cancel, RETIRED_CANCEL_REFUSAL,
};
use crate::commands::wiki_publication_driver::{may_resume, WikiHeadRetirementProof};
use crate::commands::wiki_publication_record::WikiHeadRetirement;
use crate::commands::wiki_publication_record::WikiPublicationReconciliation;

/// Drive one ordinary attempt whose head ACK is lost, then cancel it. The row
/// is left exactly where the affordance gap appears: unresolved, read-only,
/// `head_attempted`, with no reconciliation proof.
async fn ambiguous_cancellation(journal: &Journal, operation: Operation) -> Operation {
    let id = operation.id.clone();
    journal.lost_head_ack.store(true, Ordering::SeqCst);
    let failed = drive(journal, operation, journal.owner, false, false)
        .await
        .expect("durable failure");
    assert_eq!(failed.status, OperationStatus::Failed);
    assert_eq!(journal.sent.load(Ordering::SeqCst), 1);

    let canceled = journal.request_cancel(&journal.reopened(&id));
    let record = journal.stored_record(&id);
    assert!(record.cancel_requested && record.reconcile_only);
    assert!(record.head_attempted, "the send is genuinely ambiguous");
    assert!(record.reconciliation.is_none());
    canceled
}

/// A read-only reconciliation of that row must submit nothing and must leave
/// the cancellation exactly as the user left it.
async fn reconcile_read_only(journal: &Journal, operation: Operation) -> Operation {
    let id = operation.id.clone();
    let before = journal.sent.load(Ordering::SeqCst);
    let checked = drive(journal, operation, journal.owner, false, true)
        .await
        .expect("read-only reconciliation is always available");
    assert!(!checked.reconciled, "an ambiguous row stays unresolved");
    assert_eq!(
        journal.sent.load(Ordering::SeqCst),
        before,
        "a read-only check never submits"
    );
    let record = journal.stored_record(&id);
    assert!(record.cancel_requested && record.reconcile_only);
    journal.reopened(&id)
}

#[tokio::test]
async fn driver_resume_confirms_a_late_matching_head_without_sending_again() {
    let (journal, operation) = Journal::fixture_for("crew.resume.late");
    let id = operation.id.clone();
    let saved = journal.stored_record(&id);
    let canceled = ambiguous_cancellation(&journal, operation).await;
    let reopened = reconcile_read_only(&journal, canceled).await;

    // The ambiguous write did land after all.
    journal.head.store(HEAD_APPLIED, Ordering::SeqCst);
    let resumed = drive(&journal, reopened, journal.owner, true, false)
        .await
        .expect("an explicit resume may act on the same attempt");

    assert_eq!(resumed.status, OperationStatus::Complete);
    assert!(resumed.reconciled);
    assert_eq!(
        journal.sent.load(Ordering::SeqCst),
        1,
        "a confirmed head must never be republished"
    );
    let record = journal.stored_record(&id);
    assert!(!record.cancel_requested, "resume revoked the cancellation");
    assert!(matches!(
        record.reconciliation,
        Some(WikiPublicationReconciliation::Applied { .. })
    ));
    assert_eq!(record.head, saved.head);
    assert_eq!(record.manifest, saved.manifest);
    assert_eq!(record.pages, saved.pages);
    assert!(journal.reopened(&id).reconciled);
}

#[tokio::test]
async fn driver_resume_retransmits_the_exact_saved_graph_with_its_original_cas() {
    let (journal, operation) = Journal::fixture_for("crew.resume.resend");
    let id = operation.id.clone();
    let saved = journal.stored_record(&id);
    let canceled = ambiguous_cancellation(&journal, operation).await;
    let reopened = reconcile_read_only(&journal, canceled).await;

    // This time the earlier head never landed, so the resume has to submit the
    // persisted graph again.
    journal.lost_head_ack.store(false, Ordering::SeqCst);
    journal
        .late_commit_after_head_publish
        .store(true, Ordering::SeqCst);
    let resumed = drive(&journal, reopened, journal.owner, true, false)
        .await
        .expect("resume submits the persisted attempt");

    assert_eq!(resumed.status, OperationStatus::Complete);
    assert!(resumed.reconciled);
    assert_eq!(journal.sent.load(Ordering::SeqCst), 2);
    assert_eq!(
        journal.sent_ids.lock().expect("sent ids").as_slice(),
        [saved.head.id.to_hex(), saved.head.id.to_hex()],
        "resume replays the exact signed head, never a re-signed one"
    );
    let record = journal.stored_record(&id);
    assert_eq!(record.head, saved.head);
    assert_eq!(record.manifest, saved.manifest);
    assert_eq!(record.pages, saved.pages);
    assert_eq!(record.snapshot_id, saved.snapshot_id);
    assert_eq!(record.source_revision, saved.source_revision);
    assert_eq!(
        record.expected_revision, saved.expected_revision,
        "the conditional precondition is the original one"
    );
}

#[tokio::test]
async fn driver_resume_never_overwrites_a_changed_authoritative_head() {
    let (journal, operation) = Journal::fixture_for("crew.resume.conflict");
    let id = operation.id.clone();
    let saved = journal.stored_record(&id);
    let canceled = ambiguous_cancellation(&journal, operation).await;
    let reopened = reconcile_read_only(&journal, canceled).await;

    journal.head.store(HEAD_CONFLICT, Ordering::SeqCst);
    let resumed = drive(&journal, reopened, journal.owner, true, false)
        .await
        .expect("a conflicting head is a resolvable outcome");

    assert_eq!(resumed.status, OperationStatus::Superseded);
    assert!(resumed.reconciled, "the local claim is released");
    assert_eq!(
        journal.sent.load(Ordering::SeqCst),
        1,
        "a changed head is detected before any write"
    );
    let record = journal.stored_record(&id);
    assert!(matches!(
        record.reconciliation,
        Some(WikiPublicationReconciliation::Superseded {
            current_head_id: Some(_),
            ..
        })
    ));
    assert_eq!(record.head, saved.head);
}

#[tokio::test]
async fn driver_force_read_only_reconcile_never_revokes_cancellation() {
    let (journal, operation) = Journal::fixture_for("crew.resume.readonly");
    let id = operation.id.clone();
    let canceled = ambiguous_cancellation(&journal, operation).await;

    // Even when the caller also asks for an explicit retry, a reconciliation
    // is read-only by construction.
    let checked = drive(&journal, canceled, journal.owner, true, true)
        .await
        .expect("read-only reconciliation");
    assert!(!checked.reconciled);
    assert_eq!(journal.sent.load(Ordering::SeqCst), 1);
    let record = journal.stored_record(&id);
    assert!(
        record.cancel_requested && record.reconcile_only,
        "force_read_only wins over explicit retry"
    );
}

#[tokio::test]
async fn driver_automatic_restart_cannot_resume_a_cancelled_row() {
    let (journal, operation) = Journal::fixture_for("crew.resume.automatic");
    let id = operation.id.clone();
    let canceled = ambiguous_cancellation(&journal, operation).await;
    let reopened = reconcile_read_only(&journal, canceled).await;

    // The app-lifetime worker restarts and finds the row. It never carries an
    // explicit action, so it may only check.
    let checked = drive(&journal, reopened, journal.owner, false, false)
        .await
        .expect("automatic recovery stays read-only");
    assert!(!checked.reconciled);
    assert_eq!(journal.sent.load(Ordering::SeqCst), 1);
    let record = journal.stored_record(&id);
    assert!(record.cancel_requested && record.reconcile_only);
}

#[tokio::test]
async fn driver_resume_whose_claim_cas_fails_sends_nothing() {
    let (journal, operation) = Journal::fixture_for("crew.resume.lostcas");
    let id = operation.id.clone();
    let canceled = ambiguous_cancellation(&journal, operation).await;
    // Another window advances the same row through the cancel seam, so the
    // snapshot this resume holds is stale.
    let advanced = journal.request_cancel(&journal.reopened(&id));
    assert!(advanced.revision > canceled.revision);

    let error = drive(&journal, canceled, journal.owner, true, false)
        .await
        .expect_err("a stale resume must not claim the row");
    assert!(
        error.contains("changed"),
        "expected an explicit CAS conflict, got {error}"
    );
    assert_eq!(
        journal.sent.load(Ordering::SeqCst),
        1,
        "a failed resume CAS submits nothing"
    );
    let record = journal.stored_record(&id);
    assert!(
        record.cancel_requested && record.reconcile_only,
        "a failed save leaves the cancellation durably in place"
    );
}

/// Cancel is the other half of the Regenerate-only contract: it must refuse a
/// typed retirement proof outright, before it can read the relay or write the
/// journal, instead of replacing the proof with `CanceledBeforeHead` and
/// reconciling the row.
#[tokio::test]
async fn cancel_refuses_a_typed_retired_dependency_and_preserves_its_proof() {
    let (journal, operation) = Journal::fixture_for("crew.cancel.retired");
    let id = operation.id.clone();
    journal.head.store(HEAD_MISSING, Ordering::SeqCst);
    journal
        .dependencies
        .store(DEPENDENCIES_MISSING_UNTIL_REPAIR, Ordering::SeqCst);
    journal.retire_dependency.store(true, Ordering::SeqCst);
    let retired = drive(&journal, operation, journal.owner, false, false)
        .await
        .expect("typed retirement proof is recorded");
    assert!(
        !retired.reconciled,
        "the retirement keeps its unresolved claim"
    );
    let sent = journal.sent.load(Ordering::SeqCst);
    let before = journal.stored_record(&id);
    assert!(!may_cancel(&before));

    // Exactly the durable row a Cancel click loads, through the command's own
    // production admission seam. Register a newer foreground generation first
    // so the test is falsifiable: moving the cancellation signal above the
    // durable refusal would flip this token.
    let durable = journal.reopened(&id);
    let generation_key =
        crate::wiki_worker::generation_cancel_key(&durable.scope.community, &durable.resource_key);
    let token = crate::wiki_worker::begin_generation_cancel(&generation_key)
        .expect("active generation token");
    let refusal = admit_cancel_generation(&durable, &generation_key)
        .expect_err("Cancel must refuse a retired row");
    assert_eq!(refusal, RETIRED_CANCEL_REFUSAL);
    assert!(
        !token.load(Ordering::Acquire),
        "refused Cancel did not stop Regenerate"
    );
    crate::wiki_worker::finish_generation_cancel(&generation_key, &token);

    // The refusal happens before the runtime is built, so nothing was read
    // from the relay and nothing was written to the journal.
    let after = journal.stored_record(&id);
    assert_eq!(
        after.reconciliation, before.reconciliation,
        "the typed proof survives a refused Cancel"
    );
    assert!(after.reconcile_only && !after.cancel_requested);
    assert_eq!(after.head, before.head);
    assert_eq!(after.manifest, before.manifest);
    assert_eq!(after.pages, before.pages);
    let row = journal.reopened(&id);
    assert_eq!(row.revision, retired.revision, "no durable mutation");
    assert!(!row.reconciled, "the claim Regenerate needs is retained");
    assert_eq!(journal.sent.load(Ordering::SeqCst), sent);
}

/// Ordinary cancellation is untouched by that guard.
#[tokio::test]
async fn cancel_still_accepts_an_ordinary_unresolved_attempt() {
    let (journal, operation) = Journal::fixture_for("crew.cancel.ordinary");
    let id = operation.id.clone();
    let fresh = cancel_intent(&journal.reopened(&id)).expect("a fresh row may be canceled");
    assert!(fresh.reconciliation.is_none());
    assert!(may_cancel(&fresh));

    // And after an ambiguous send, which is the case Cancel exists for.
    let _canceled = ambiguous_cancellation(&journal, operation).await;
    let ambiguous = journal.reopened(&id);
    assert!(
        cancel_intent(&ambiguous).is_ok(),
        "an ambiguous cancellation is not a typed retirement"
    );
}

#[tokio::test]
async fn driver_typed_retirement_proof_is_not_cleared_by_an_explicit_resume() {
    let (journal, operation) = Journal::fixture_for("crew.resume.retired");
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
    let sent = journal.sent.load(Ordering::SeqCst);
    let proof = journal.stored_record(&id).reconciliation;
    assert!(matches!(
        proof,
        Some(WikiPublicationReconciliation::ImmutableDependencyRetired { .. })
    ));

    let after = drive(&journal, journal.reopened(&id), journal.owner, true, false)
        .await
        .expect("an explicit retry on a retired row is a read-only check");
    assert!(!after.reconciled, "retirement still needs Regenerate");
    assert_eq!(
        journal.sent.load(Ordering::SeqCst),
        sent,
        "a retired dependency must never be resubmitted by retry or resume"
    );
    let record = journal.stored_record(&id);
    assert_eq!(record.reconciliation, proof, "the typed proof is preserved");
    assert!(record.reconcile_only, "the row stays Regenerate-only");
}

/// Item 4: a canceled ambiguous attempt whose head reads Missing stays
/// unresolved and silent under Reconcile and automatic restart; only explicit
/// Resume can reach the relay, and it may then obtain a retirement proof.
#[tokio::test]
async fn canceled_missing_reconcile_sends_nothing_then_resume_settles_on_proof() {
    let (journal, operation) = Journal::fixture_for("crew.cancel.missing");
    let id = operation.id.clone();
    let saved = journal.stored_record(&id);
    let head_id = saved.head.id.to_hex();
    let canceled = ambiguous_cancellation(&journal, operation).await;

    // The coordinate now reads empty. Absence alone proves nothing.
    journal.head.store(HEAD_MISSING, Ordering::SeqCst);
    let sent_after_cancel = journal.sent.load(Ordering::SeqCst);
    let reopened = reconcile_read_only(&journal, canceled).await;
    assert_eq!(journal.sent.load(Ordering::SeqCst), sent_after_cancel);

    // An automatic restart is equally read-only and equally unresolved.
    let automatic = drive(&journal, reopened, journal.owner, false, false)
        .await
        .expect("automatic recovery stays read-only");
    assert!(!automatic.reconciled, "absence never releases the claim");
    assert_eq!(journal.sent.load(Ordering::SeqCst), sent_after_cancel);
    let still = journal.stored_record(&id);
    assert!(still.cancel_requested && still.reconcile_only);
    assert!(still.reconciliation.is_none(), "no invented retirement");

    // The owner explicitly resumes; the relay now refuses with proof.
    *journal.retire_head.lock().expect("head retirement") = Some(WikiHeadRetirementProof {
        retirement: WikiHeadRetirement::Head {
            head_id: head_id.clone(),
        },
        current_head_id: None,
    });
    let settled = drive(&journal, journal.reopened(&id), journal.owner, true, false)
        .await
        .expect("explicit resume can obtain the proof");
    assert_eq!(settled.status, OperationStatus::Superseded);
    assert!(settled.reconciled);
    let record = journal.stored_record(&id);
    assert!(matches!(
        record.reconciliation,
        Some(WikiPublicationReconciliation::Superseded {
            head_retirement: Some(WikiHeadRetirement::Head { .. }),
            ..
        })
    ));
    assert_eq!(record.head, saved.head, "the signed graph is unchanged");
    assert_eq!(record.manifest, saved.manifest);
    assert_eq!(record.pages, saved.pages);
    assert_eq!(record.expected_revision, saved.expected_revision);
}

/// Item 4: with `expected-revision: absent` and a head that was never
/// accepted, absence stays unresolved and Resume simply applies H under the
/// original precondition — it must never receive an invented retirement.
#[tokio::test]
async fn absent_precondition_never_accepted_head_resumes_under_its_original_cas() {
    let (journal, operation) = Journal::fixture_for("crew.cancel.absent");
    let id = operation.id.clone();
    let saved = journal.stored_record(&id);
    assert_eq!(saved.expected_revision, "absent");
    let canceled = ambiguous_cancellation(&journal, operation).await;

    journal.head.store(HEAD_MISSING, Ordering::SeqCst);
    let reopened = reconcile_read_only(&journal, canceled).await;
    assert!(
        journal.stored_record(&id).reconciliation.is_none(),
        "an absent precondition with no history is not retirement"
    );

    // Resume re-sends the exact saved head, which now lands.
    journal.lost_head_ack.store(false, Ordering::SeqCst);
    journal
        .late_commit_after_head_publish
        .store(true, Ordering::SeqCst);
    let sent_before = journal.sent.load(Ordering::SeqCst);
    let applied = drive(&journal, reopened, journal.owner, true, false)
        .await
        .expect("resume may publish the exact head");
    assert_eq!(applied.status, OperationStatus::Complete);
    assert!(applied.reconciled);
    assert_eq!(
        journal.sent.load(Ordering::SeqCst),
        sent_before + 1,
        "exactly one further send, of the persisted head"
    );
    let record = journal.stored_record(&id);
    assert!(matches!(
        record.reconciliation,
        Some(WikiPublicationReconciliation::Applied { .. })
    ));
    assert_eq!(record.head, saved.head);
    assert_eq!(
        record.expected_revision, saved.expected_revision,
        "the original CAS precondition is reused verbatim"
    );
}

#[tokio::test]
async fn driver_resume_eligibility_requires_an_explicit_action_and_no_typed_proof() {
    let (journal, operation) = Journal::fixture_for("crew.resume.matrix");
    let id = operation.id.clone();
    let ordinary = journal.stored_record(&id);
    assert!(
        !may_resume(&ordinary, true, false),
        "an ordinary retryable row has nothing to revoke"
    );

    let _canceled = ambiguous_cancellation(&journal, operation).await;
    let ambiguous = journal.stored_record(&id);
    assert!(may_resume(&ambiguous, true, false));
    assert!(
        !may_resume(&ambiguous, false, false),
        "an automatic scan never revokes a cancellation"
    );
    assert!(
        !may_resume(&ambiguous, true, true),
        "a read-only reconciliation never revokes a cancellation"
    );

    let mut proven = ambiguous.clone();
    proven.reconciliation = Some(WikiPublicationReconciliation::ImmutableDependencyRetired {
        dependency_id: proven.manifest.id.to_hex(),
    });
    assert!(
        !may_resume(&proven, true, false),
        "a typed retirement proof stays Regenerate-only"
    );
    proven.reconciliation = Some(WikiPublicationReconciliation::Superseded {
        current_head_id: Some("c".repeat(64)),
        retired_dependency_id: None,
        head_retirement: None,
    });
    assert!(
        !may_resume(&proven, true, false),
        "a proven outcome is already decided"
    );
}
