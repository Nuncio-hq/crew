//! Head/precondition retirement proof validation, against the exact function
//! `NativeWikiPublication::publish` calls.
//!
//! These tests drive `validate_head_retirement_refusal` — the shipping
//! decision — through the same `HeadRetirementReads` seam the captured runtime
//! implements. Only the two scoped reads are supplied by the fixture; every
//! status, ID-binding, absence and current-head guard under test is production
//! code. There is deliberately no second copy of the decision here: deleting
//! any guard from the runtime must fail these tests.

use super::wiki_publication_driver::WikiPublishError;
use super::wiki_publication_record::{WikiHeadRetirement, WikiPublicationRecord};
use super::wiki_publication_runtime::{validate_head_retirement_refusal, HeadRetirementReads};
use super::wiki_publication_test_fixture as fixture;
use crate::commands::owner_operation_transport::OperationTransportError;
use crate::owner_operations::{Operation, OperationScope};
use nostr::{Event, Keys};
use std::sync::Mutex;

const OTHER_ID: &str = "dd11ee22ff334455667788990011223344556677889900aabbccddeeff001122";

/// One scripted pair of scoped reads.
///
/// `Err` stands for any failure the guarded runtime read can produce — a
/// transport fault or an owner/generation/revision/lease fence that moved
/// between awaits. The runtime cannot distinguish them and neither should the
/// decision: both must stay Unknown.
struct Reads {
    exact: Mutex<Option<Result<Vec<Event>, String>>>,
    current: Mutex<Option<Result<Option<Event>, String>>>,
    exact_requests: Mutex<Vec<String>>,
    current_requests: Mutex<usize>,
}

impl Reads {
    fn new(exact: Result<Vec<Event>, String>, current: Result<Option<Event>, String>) -> Self {
        Self {
            exact: Mutex::new(Some(exact)),
            current: Mutex::new(Some(current)),
            exact_requests: Mutex::new(Vec::new()),
            current_requests: Mutex::new(0),
        }
    }

    /// The ordinary proven shape: the allegedly retired event is gone and the
    /// coordinate carries no head at all.
    fn absent_and_empty() -> Self {
        Self::new(Ok(Vec::new()), Ok(None))
    }
}

impl HeadRetirementReads for Reads {
    async fn read_exact_toc_event(
        &self,
        _operation: &Operation,
        event_id: &str,
    ) -> Result<Vec<Event>, String> {
        self.exact_requests
            .lock()
            .expect("exact requests")
            .push(event_id.to_owned());
        self.exact
            .lock()
            .expect("exact read")
            .take()
            .expect("one exact read per validation")
    }

    async fn read_current_toc(&self, _operation: &Operation) -> Result<Option<Event>, String> {
        *self.current_requests.lock().expect("current requests") += 1;
        self.current
            .lock()
            .expect("current read")
            .take()
            .expect("one current read per validation")
    }
}

fn refusal(reason: &str) -> OperationTransportError {
    OperationTransportError::RelayResponse {
        status: 400,
        reason: reason.to_owned(),
    }
}

struct Fixture {
    keys: Keys,
    record: WikiPublicationRecord,
    operation: Operation,
}

/// A real signed publication. `expected` is the persisted conditional
/// precondition, written through the production builder rather than patched
/// into a signed envelope after the fact.
fn fixture(repo_d: &str, expected: Option<&str>) -> Fixture {
    let keys = Keys::generate();
    let coordinate = fixture::coordinate(&keys, repo_d);
    let record = fixture::record(
        fixture::publication_with_expected(&keys, repo_d, None, expected),
        &coordinate,
        &keys,
    );
    let operation = fixture::operation(
        OperationScope {
            owner: keys.public_key().to_hex(),
            community: "https://relay.example.test".into(),
        },
        uuid::Uuid::new_v4().to_string(),
        &coordinate,
        &record,
        100,
    );
    Fixture {
        keys,
        record,
        operation,
    }
}

async fn validate(
    state: &Fixture,
    reads: &Reads,
    event: &Event,
    error: &OperationTransportError,
) -> Result<Option<super::wiki_publication_driver::WikiHeadRetirementProof>, WikiPublishError> {
    validate_head_retirement_refusal(reads, &state.operation, &state.record, event, error).await
}

#[tokio::test]
async fn exact_head_retirement_refusal_is_proven_once_both_scoped_reads_agree() {
    let state = fixture("crew.proof.head", None);
    let head_id = state.record.head.id.to_hex();
    let reads = Reads::absent_and_empty();

    let proof = validate(
        &state,
        &reads,
        &state.record.head.clone(),
        &refusal(&format!("conflict: wiki-head-retired:{head_id}")),
    )
    .await
    .expect("a validated refusal is not an error")
    .expect("the exact head refusal is proof");

    assert_eq!(
        proof.retirement,
        WikiHeadRetirement::Head {
            head_id: head_id.clone()
        }
    );
    assert_eq!(proof.current_head_id, None);
    assert_eq!(
        *reads.exact_requests.lock().expect("exact requests"),
        vec![head_id],
        "the absence read must name the exact retired head"
    );
    assert_eq!(*reads.current_requests.lock().expect("current requests"), 1);
}

#[tokio::test]
async fn exact_precondition_retirement_binds_the_persisted_expected_revision() {
    let expected = "a".repeat(64);
    let state = fixture("crew.proof.expected", Some(&expected));
    let head_id = state.record.head.id.to_hex();
    // A different head legitimately holds the coordinate now; it is retained.
    let other = fixture::publication(&state.keys, "crew.proof.expected", None);
    let reads = Reads::new(Ok(Vec::new()), Ok(Some(other.head.clone())));

    let proof = validate(
        &state,
        &reads,
        &state.record.head.clone(),
        &refusal(&format!(
            "conflict: wiki-expected-head-retired:{head_id}:{expected}"
        )),
    )
    .await
    .expect("validated")
    .expect("the exact precondition refusal is proof");

    assert_eq!(
        proof.retirement,
        WikiHeadRetirement::ExpectedHead {
            head_id,
            expected_revision: expected.clone()
        }
    );
    assert_eq!(proof.current_head_id, Some(other.head.id.to_hex()));
    assert_eq!(
        *reads.exact_requests.lock().expect("exact requests"),
        vec![expected],
        "the absence read must name the exact retired precondition, not the head"
    );
}

/// Everything that must stay Unknown *before* any scoped read happens.
#[tokio::test]
async fn unproven_refusals_never_reach_a_scoped_read() {
    let expected = "b".repeat(64);
    let state = fixture("crew.proof.reject", Some(&expected));
    let head_id = state.record.head.id.to_hex();
    let head = state.record.head.clone();
    let page = state.record.pages[0].clone();

    let cases: Vec<(&str, Event, OperationTransportError)> = vec![
        (
            "a 500 is never proof",
            head.clone(),
            OperationTransportError::RelayResponse {
                status: 500,
                reason: format!("conflict: wiki-head-retired:{head_id}"),
            },
        ),
        (
            "a non-response transport failure is never proof",
            head.clone(),
            OperationTransportError::NotAttempted("scope changed".into()),
        ),
        (
            "a generic conflict is never upgraded",
            head.clone(),
            refusal("conflict: conditional publication revision changed"),
        ),
        (
            "an older relay's live-head conflict is never upgraded",
            head.clone(),
            refusal("conflict: conditional publication is no longer the live head"),
        ),
        (
            "a dependency retirement is a different fact",
            head.clone(),
            refusal(&format!("conflict: wiki-immutable-retired:{head_id}")),
        ),
        (
            "a foreign head ID does not bind this attempt",
            head.clone(),
            refusal(&format!("conflict: wiki-head-retired:{OTHER_ID}")),
        ),
        (
            "a foreign expected ID does not bind this precondition",
            head.clone(),
            refusal(&format!(
                "conflict: wiki-expected-head-retired:{head_id}:{OTHER_ID}"
            )),
        ),
        (
            "a page refusal can never carry a head proof",
            page,
            refusal(&format!("conflict: wiki-head-retired:{head_id}")),
        ),
    ];

    for (why, event, error) in cases {
        let reads = Reads::new(
            Err("no read expected".into()),
            Err("no read expected".into()),
        );
        assert!(
            validate(&state, &reads, &event, &error)
                .await
                .expect("validated")
                .is_none(),
            "{why}"
        );
        assert!(
            reads.exact_requests.lock().expect("exact").is_empty()
                && *reads.current_requests.lock().expect("current") == 0,
            "{why}: an unproven refusal must not issue scoped reads"
        );
    }
}

/// A precondition reason cannot be honoured by an attempt that required no
/// exact revision — `expected-revision: absent` has no event identity.
#[tokio::test]
async fn precondition_reason_is_unproven_when_the_attempt_expected_absence() {
    let state = fixture("crew.proof.absent", None);
    let head_id = state.record.head.id.to_hex();
    assert_eq!(state.record.expected_revision, "absent");
    let reads = Reads::new(
        Err("no read expected".into()),
        Err("no read expected".into()),
    );

    assert!(validate(
        &state,
        &reads,
        &state.record.head.clone(),
        &refusal(&format!(
            "conflict: wiki-expected-head-retired:{head_id}:{OTHER_ID}"
        )),
    )
    .await
    .expect("validated")
    .is_none());
    assert!(reads.exact_requests.lock().expect("exact").is_empty());
}

/// Either scoped read failing — transport fault or a moved
/// owner/generation/revision/lease fence — keeps the claim retryable.
#[tokio::test]
async fn a_failed_scoped_read_is_unknown_and_never_proof() {
    let state = fixture("crew.proof.readfail", None);
    let head_id = state.record.head.id.to_hex();
    let error = refusal(&format!("conflict: wiki-head-retired:{head_id}"));

    let first = Reads::new(Err("active owner or workspace changed".into()), Ok(None));
    match validate(&state, &first, &state.record.head.clone(), &error).await {
        Err(WikiPublishError::Unknown(reason)) => {
            assert!(reason.contains("changed"), "the exact reason is preserved")
        }
        other => panic!("a failed absence read must be Unknown: {}", other.is_ok()),
    }
    assert_eq!(*first.current_requests.lock().expect("current"), 0);

    let second = Reads::new(
        Ok(Vec::new()),
        Err("Wiki publication worker lease expired.".into()),
    );
    match validate(&state, &second, &state.record.head.clone(), &error).await {
        Err(WikiPublishError::Unknown(reason)) => assert!(reason.contains("lease")),
        other => panic!("a failed current read must be Unknown: {}", other.is_ok()),
    }
}

/// A live copy of the allegedly retired event refutes the claim outright.
#[tokio::test]
async fn a_live_allegedly_retired_event_is_unproven() {
    let state = fixture("crew.proof.live", None);
    let head_id = state.record.head.id.to_hex();
    let reads = Reads::new(Ok(vec![state.record.head.clone()]), Ok(None));

    assert!(validate(
        &state,
        &reads,
        &state.record.head.clone(),
        &refusal(&format!("conflict: wiki-head-retired:{head_id}")),
    )
    .await
    .expect("validated")
    .is_none());
    assert_eq!(
        *reads.current_requests.lock().expect("current"),
        0,
        "a refuted absence must not continue to the current-head read"
    );
}

/// Item 6: terminal metadata compatibility, through the production validators.
mod terminal_metadata {
    use super::*;
    use crate::owner_operations::OperationStatus;

    /// The record as-is: a writable, never-attempted `Preparing` row.
    fn payload(state: &Fixture) -> serde_json::Value {
        serde_json::to_value(&state.record).expect("record JSON")
    }

    /// The record shaped as an actual settlement, which is the only shape a
    /// head-retirement proof can legitimately appear on.
    fn settled_payload(state: &Fixture) -> serde_json::Value {
        let mut record = state.record.clone();
        record.head_attempted = true;
        record.progress = super::super::wiki_publication_record::WikiPublicationProgress::Head;
        record.reconcile_only = true;
        serde_json::to_value(&record).expect("record JSON")
    }

    fn reconciliation(payload: &mut serde_json::Value, value: serde_json::Value) {
        payload
            .as_object_mut()
            .expect("record object")
            .insert("reconciliation".into(), value);
    }

    fn parsed(payload: &serde_json::Value) -> Result<WikiPublicationRecord, String> {
        serde_json::from_value::<WikiPublicationRecord>(payload.clone())
            .map_err(|error| error.to_string())
    }

    /// A journal written before this field existed must load, prove nothing,
    /// and authorize no release on its own.
    #[tokio::test]
    async fn old_superseded_payload_without_head_retirement_loads_as_none() {
        let state = fixture("crew.compat.old", None);
        let mut value = payload(&state);
        reconciliation(
            &mut value,
            serde_json::json!({
                "proof": "superseded",
                "current_head_id": null,
                "retired_dependency_id": null
            }),
        );
        let record = parsed(&value).expect("an older payload still deserializes");
        record
            .validate_intent(state.keys.public_key())
            .expect("intent");
        record
            .validate_projection(state.keys.public_key())
            .expect("projection");
        match record.reconciliation {
            Some(
                super::super::wiki_publication_record::WikiPublicationReconciliation::Superseded {
                    head_retirement,
                    ..
                },
            ) => assert!(
                head_retirement.is_none(),
                "an absent optional field is never evidence"
            ),
            other => panic!("expected Superseded: {other:?}"),
        }
    }

    #[tokio::test]
    async fn valid_head_and_precondition_metadata_pass_both_validators() {
        let head_state = fixture("crew.compat.head", None);
        let mut value = settled_payload(&head_state);
        reconciliation(
            &mut value,
            serde_json::json!({
                "proof": "superseded",
                "current_head_id": null,
                "retired_dependency_id": null,
                "head_retirement": {
                    "retired": "head",
                    "head_id": head_state.record.head.id.to_hex()
                }
            }),
        );
        let record = parsed(&value).expect("valid head metadata");
        record
            .validate_intent(head_state.keys.public_key())
            .expect("intent");
        record
            .validate_projection(head_state.keys.public_key())
            .expect("projection");

        let expected = "c".repeat(64);
        let expected_state = fixture("crew.compat.expected", Some(&expected));
        let mut value = settled_payload(&expected_state);
        reconciliation(
            &mut value,
            serde_json::json!({
                "proof": "superseded",
                "current_head_id": null,
                "retired_dependency_id": null,
                "head_retirement": {
                    "retired": "expected-head",
                    "head_id": expected_state.record.head.id.to_hex(),
                    "expected_revision": expected
                }
            }),
        );
        let record = parsed(&value).expect("valid precondition metadata");
        record
            .validate_intent(expected_state.keys.public_key())
            .expect("intent");
    }

    /// Metadata that does not bind this exact attempt is rejected by the
    /// production validators rather than trusted.
    #[tokio::test]
    async fn unbound_or_malformed_head_retirement_metadata_is_rejected() {
        let expected = "d".repeat(64);
        let state = fixture("crew.compat.reject", Some(&expected));
        let head_id = state.record.head.id.to_hex();
        let absent = fixture("crew.compat.absent", None);

        let cases: Vec<(&str, &Fixture, serde_json::Value)> = vec![
            (
                "a foreign head ID",
                &state,
                serde_json::json!({"retired": "head", "head_id": OTHER_ID}),
            ),
            (
                "a foreign expected revision",
                &state,
                serde_json::json!({
                    "retired": "expected-head",
                    "head_id": head_id,
                    "expected_revision": OTHER_ID
                }),
            ),
            (
                "a precondition proof on an absent-expecting attempt",
                &absent,
                serde_json::json!({
                    "retired": "expected-head",
                    "head_id": absent.record.head.id.to_hex(),
                    "expected_revision": OTHER_ID
                }),
            ),
            (
                "a malformed head ID",
                &state,
                serde_json::json!({"retired": "head", "head_id": "not-hex"}),
            ),
        ];

        for (why, owner_state, retirement) in cases {
            let mut value = settled_payload(owner_state);
            reconciliation(
                &mut value,
                serde_json::json!({
                    "proof": "superseded",
                    "current_head_id": null,
                    "retired_dependency_id": null,
                    "head_retirement": retirement
                }),
            );
            let record = parsed(&value).expect("shape parses");
            assert!(
                record
                    .validate_intent(owner_state.keys.public_key())
                    .is_err(),
                "{why} must not validate as intent"
            );
            assert!(
                record
                    .validate_projection(owner_state.keys.public_key())
                    .is_err(),
                "{why} must not validate as a projection"
            );
        }

        // An unknown tagged variant is refused at deserialization, so it can
        // never reach a validator or a durable row.
        let mut value = payload(&state);
        reconciliation(
            &mut value,
            serde_json::json!({
                "proof": "superseded",
                "current_head_id": null,
                "retired_dependency_id": null,
                "head_retirement": {"retired": "invented", "head_id": head_id}
            }),
        );
        assert!(
            parsed(&value).is_err(),
            "unknown proof variants are refused"
        );
    }

    /// The fence that matters: terminal proof on a still-writable row.
    ///
    /// `operation.reconciled` alone is not enough — a payload could carry a
    /// head-retirement proof on a `Preparing`, never-attempted, not-read-only
    /// row, which `from_operation` would happily hand to the dispatcher to
    /// re-send. The record validator must refuse that shape outright, leaving
    /// the durable claim retryable and the stored evidence untouched.
    #[tokio::test]
    async fn terminal_proof_on_a_writable_row_is_refused_before_dispatch() {
        use super::super::wiki_publication_record::WikiPublicationProgress;
        let state = fixture("crew.compat.writable", None);
        let head_id = state.record.head.id.to_hex();
        let proof = serde_json::json!({
            "proof": "superseded",
            "current_head_id": null,
            "retired_dependency_id": null,
            "head_retirement": {"retired": "head", "head_id": head_id}
        });

        // Positive control: the settled shape validates, so each negative
        // below isolates exactly one changed requirement.
        let mut settled = settled_payload(&state);
        reconciliation(&mut settled, proof.clone());
        parsed(&settled)
            .expect("settled shape parses")
            .validate_intent(state.keys.public_key())
            .expect("the settled control must validate");

        // Vary ONE requirement at a time from that valid settled record.
        // `progress` moves to `Manifest`, never `Preparing`, so the older
        // head_attempted/Preparing guard cannot mask removal of the new check.
        // Each case differs from the validated control in EXACTLY one field,
        // so removing any one production requirement leaves its own case
        // passing where it must fail.
        type Mutate = Box<dyn Fn(&mut WikiPublicationRecord)>;
        let cases: Vec<(&str, Mutate)> = vec![
            (
                "never attempted",
                Box::new(|record: &mut WikiPublicationRecord| record.head_attempted = false),
            ),
            (
                "still writable",
                Box::new(|record: &mut WikiPublicationRecord| record.reconcile_only = false),
            ),
            (
                "a non-head phase",
                Box::new(|record: &mut WikiPublicationRecord| {
                    record.progress = WikiPublicationProgress::Manifest;
                }),
            ),
        ];

        for (why, mutate) in cases {
            let mut control = state.record.clone();
            control.head_attempted = true;
            control.progress = WikiPublicationProgress::Head;
            control.reconcile_only = true;
            let mut record = control.clone();
            mutate(&mut record);
            // Prove the isolation rather than assuming it: exactly one of the
            // three terminal requirements may differ from the control.
            let differing = usize::from(record.head_attempted != control.head_attempted)
                + usize::from(record.reconcile_only != control.reconcile_only)
                + usize::from(record.progress != control.progress);
            assert_eq!(differing, 1, "{why}: exactly one field may vary");
            let mut value = serde_json::to_value(&record).expect("record JSON");
            reconciliation(&mut value, proof.clone());
            let parsed_record = parsed(&value).expect("shape parses");
            assert!(
                parsed_record
                    .validate_intent(state.keys.public_key())
                    .is_err(),
                "{why}: terminal proof must not validate as intent"
            );
            assert!(
                parsed_record
                    .validate_projection(state.keys.public_key())
                    .is_err(),
                "{why}: terminal proof must not validate as a projection"
            );
            let mut operation = state.operation.clone();
            operation.payload = value;
            assert!(
                WikiPublicationRecord::from_operation(&operation, state.keys.public_key()).is_err(),
                "{why}: dispatch must refuse it too"
            );
        }
    }

    /// A terminal reconciled row is never reopened for dispatch, so terminal
    /// metadata cannot become a writable claim.
    #[tokio::test]
    async fn a_reconciled_terminal_row_is_refused_for_dispatch() {
        let state = fixture("crew.compat.terminal", None);
        let mut operation = state.operation.clone();
        operation.reconciled = true;
        operation.status = OperationStatus::Superseded;
        assert!(
            WikiPublicationRecord::from_operation(&operation, state.keys.public_key()).is_err(),
            "a reconciled row must not be dispatchable"
        );
    }

    /// The dependency proof stays a separate fact from the head proof.
    #[tokio::test]
    async fn dependency_and_head_proofs_are_retained_independently() {
        let state = fixture("crew.compat.both", None);
        let mut value = settled_payload(&state);
        reconciliation(
            &mut value,
            serde_json::json!({
                "proof": "superseded",
                "current_head_id": null,
                "retired_dependency_id": state.record.manifest.id.to_hex(),
                "head_retirement": {
                    "retired": "head",
                    "head_id": state.record.head.id.to_hex()
                }
            }),
        );
        let record = parsed(&value).expect("both proofs");
        record
            .validate_intent(state.keys.public_key())
            .expect("both proofs are independently valid");
    }
}

/// The current `_toc` contradicting the claim, in both forms: the desired head
/// H being live means the attempt landed, and the allegedly retired event
/// being live — E for the precondition form — means it was never retired.
#[tokio::test]
async fn a_contradictory_current_head_or_expected_revision_is_unproven() {
    let head_state = fixture("crew.proof.currenth", None);
    let head_id = head_state.record.head.id.to_hex();
    let reads = Reads::new(Ok(Vec::new()), Ok(Some(head_state.record.head.clone())));
    assert!(
        validate(
            &head_state,
            &reads,
            &head_state.record.head.clone(),
            &refusal(&format!("conflict: wiki-head-retired:{head_id}")),
        )
        .await
        .expect("validated")
        .is_none(),
        "a live desired head means the attempt landed, not that it is retired"
    );

    // The precondition form: the coordinate's current head *is* the exact
    // revision the relay claimed was retired. E and H must share the owner,
    // kind and `d` coordinate — a foreign-owner E could never survive the
    // production `query_head` owner check to reach this guard at all, so a
    // fixture built from unrelated keys would test nothing.
    let repo_d = "crew.proof.currente";
    let keys = Keys::generate();
    let predecessor = fixture::publication(&keys, repo_d, None);
    let expected = predecessor.head.id.to_hex();
    let coordinate = fixture::coordinate(&keys, repo_d);
    let record = fixture::record(
        fixture::publication_with_expected(&keys, repo_d, None, Some(&expected)),
        &coordinate,
        &keys,
    );
    let expected_state = Fixture {
        operation: fixture::operation(
            OperationScope {
                owner: keys.public_key().to_hex(),
                community: "https://relay.example.test".into(),
            },
            uuid::Uuid::new_v4().to_string(),
            &coordinate,
            &record,
            100,
        ),
        record,
        keys,
    };
    assert_eq!(
        expected_state.record.head.pubkey, predecessor.head.pubkey,
        "E and H must belong to the same repository owner"
    );
    let expected_head_id = expected_state.record.head.id.to_hex();
    let reads = Reads::new(Ok(Vec::new()), Ok(Some(predecessor.head.clone())));
    assert!(
        validate(
            &expected_state,
            &reads,
            &expected_state.record.head.clone(),
            &refusal(&format!(
                "conflict: wiki-expected-head-retired:{expected_head_id}:{expected}"
            )),
        )
        .await
        .expect("validated")
        .is_none(),
        "a live expected revision contradicts its own retirement claim"
    );
}
