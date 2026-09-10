//! Native Wiki dispatcher behaviour against a real durable journal.
//!
//! The runtime seam below is the only thing these tests fake: relay head,
//! dependency and publish outcomes. Every journal read and write goes through
//! the shipping `OperationStore` on a temporary SQLite file, opened freshly for
//! each operation exactly as `run_at_path` does in production, so a recovery
//! assertion here also proves the durable transition it depends on.

use super::wiki_publication_driver::{
    drive, WikiDependencyState, WikiHead, WikiPublicationRuntime, WikiPublishError,
};
use super::wiki_publication_record::{WikiPublicationProgress, WikiPublicationRecord};
use super::wiki_publication_test_fixture as fixture;
use crate::owner_operations::{
    CreateResult, Limits, NewOperation, Operation, OperationKind, OperationScope, OperationStatus,
    OperationStore, OperationUpdate,
};
use nostr::{Event, Keys, PublicKey};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicU8, AtomicUsize, Ordering},
    Mutex,
};
use tempfile::TempDir;

pub(super) const HEAD_ORIGINAL: u8 = 0;
pub(super) const HEAD_APPLIED: u8 = 1;
pub(super) const HEAD_MISSING: u8 = 2;
pub(super) const HEAD_CONFLICT: u8 = 3;
pub(super) const DEPENDENCIES_VERIFIED: u8 = 0;
pub(super) const DEPENDENCIES_MISSING_UNTIL_REPAIR: u8 = 1;
pub(super) const DEPENDENCIES_ALWAYS_MISSING: u8 = 2;

/// One temporary owner-operation journal plus the fake relay outcomes.
pub(super) struct Journal {
    _dir: TempDir,
    path: PathBuf,
    pub(super) scope: OperationScope,
    pub(super) owner: PublicKey,
    pub(super) clock: AtomicI64,
    /// Native identity/workspace generation captured when the runtime was
    /// built, mirroring `NativeWikiPublication`'s owner-scope fence.
    captured_generation: AtomicU64,
    pub(super) live_generation: AtomicU64,
    pub(super) head: AtomicU8,
    pub(super) dependencies: AtomicU8,
    pub(super) late_commit_after_head_publish: AtomicBool,
    pub(super) lost_head_ack: AtomicBool,
    pub(super) retire_dependency: AtomicBool,
    /// Flip the owner generation the moment the head publish returns, so the
    /// fence is exercised on both the success and the failure path.
    pub(super) rotate_owner_on_head_publish: AtomicBool,
    pub(super) sent: AtomicUsize,
    pub(super) sent_ids: Mutex<Vec<String>>,
    /// Durable phase read back from SQLite at the instant each head event was
    /// submitted. Capturing it inside the publish seam is the only way to
    /// prove the pre-send CAS landed *before* the send rather than after it.
    pub(super) head_send_phase: Mutex<Vec<(WikiPublicationProgress, bool)>>,
}

impl Journal {
    pub(super) fn fixture() -> (Self, Operation) {
        Self::fixture_for("crew")
    }

    pub(super) fn fixture_for(repo_d: &str) -> (Self, Operation) {
        let keys = Keys::generate();
        let owner = keys.public_key();
        let dir = tempfile::tempdir().expect("journal directory");
        let path = dir
            .path()
            .canonicalize()
            .expect("canonical journal path")
            .join("recovery.db");
        let scope = OperationScope {
            owner: owner.to_hex(),
            community: "https://relay.example.test".into(),
        };
        let coordinate = fixture::coordinate(&keys, repo_d);
        let record = fixture::record(
            fixture::publication(&keys, repo_d, None),
            &coordinate,
            &keys,
        );
        let mut store = OperationStore::open(&path, Limits::default()).expect("journal");
        let operation = match store
            .create(
                &scope,
                NewOperation {
                    id: uuid::Uuid::new_v4().to_string(),
                    kind: OperationKind::WikiPublication,
                    resource_key: coordinate,
                    payload: serde_json::to_value(&record).expect("record JSON"),
                },
                100,
            )
            .expect("reserve the publication before any relay effect")
        {
            CreateResult::Created(operation) | CreateResult::Existing(operation) => operation,
        };
        drop(store);
        let journal = Self {
            _dir: dir,
            path,
            scope,
            owner,
            clock: AtomicI64::new(100),
            captured_generation: AtomicU64::new(0),
            live_generation: AtomicU64::new(0),
            head: AtomicU8::new(HEAD_ORIGINAL),
            dependencies: AtomicU8::new(DEPENDENCIES_VERIFIED),
            late_commit_after_head_publish: AtomicBool::new(false),
            lost_head_ack: AtomicBool::new(false),
            retire_dependency: AtomicBool::new(false),
            rotate_owner_on_head_publish: AtomicBool::new(false),
            sent: AtomicUsize::new(0),
            sent_ids: Mutex::new(Vec::new()),
            head_send_phase: Mutex::new(Vec::new()),
        };
        (journal, operation)
    }

    fn store(&self) -> OperationStore {
        OperationStore::open(&self.path, Limits::default()).expect("reopen journal")
    }

    /// Close and reopen the SQLite file, the way a restarted process does,
    /// then read the durable row back.
    pub(super) fn reopened(&self, id: &str) -> Operation {
        self.store().load(&self.scope, id).expect("durable row")
    }

    pub(super) fn stored_record(&self, id: &str) -> WikiPublicationRecord {
        let operation = self.reopened(id);
        let record: WikiPublicationRecord =
            serde_json::from_value(operation.payload).expect("durable record");
        record
            .validate_projection(self.owner)
            .expect("durable record stays valid");
        record
    }

    /// Persist an out-of-band user decision through the same durable CAS the
    /// cancel command uses.
    pub(super) fn request_cancel(&self, operation: &Operation) -> Operation {
        let mut record =
            WikiPublicationRecord::from_operation(operation, self.owner).expect("stored fixture");
        record.cancel_requested = true;
        record.reconcile_only = true;
        record.lease = None;
        self.store()
            .compare_and_swap(
                &self.scope,
                &operation.id,
                operation.revision,
                OperationUpdate {
                    status: OperationStatus::Reconciling,
                    reconciled: false,
                    payload: serde_json::to_value(record).expect("cancel JSON"),
                },
                self.clock.load(Ordering::SeqCst),
            )
            .expect("durable cancellation")
    }

    pub(super) fn rotate_owner(&self) {
        self.live_generation.fetch_add(1, Ordering::SeqCst);
    }

    fn scope_is_current(&self) -> bool {
        self.captured_generation.load(Ordering::SeqCst)
            == self.live_generation.load(Ordering::SeqCst)
    }
}

impl WikiPublicationRuntime for Journal {
    fn now(&self) -> Result<i64, String> {
        Ok(self.clock.load(Ordering::SeqCst))
    }

    async fn checkpoint(&self, operation: &Operation) -> Result<(), String> {
        if !self.scope_is_current() {
            return Err(crate::app_state::owner_scope::OWNER_SCOPE_STALE.into());
        }
        let current = self.reopened(&operation.id);
        if current.revision != operation.revision
            || current.payload != operation.payload
            || current.status != operation.status
            || current.reconciled
        {
            return Err("Wiki publication changed before dispatch.".into());
        }
        let lease = current
            .payload
            .get("lease")
            .cloned()
            .map(serde_json::from_value::<super::wiki_publication_record::WikiPublicationLease>)
            .transpose()
            .map_err(|_| "Invalid Wiki publication lease metadata.".to_string())?;
        if lease.is_none_or(|lease| lease.expires_at <= self.clock.load(Ordering::SeqCst)) {
            return Err("Wiki publication worker lease expired.".into());
        }
        Ok(())
    }

    async fn save(
        &self,
        operation: &Operation,
        record: &WikiPublicationRecord,
        status: OperationStatus,
        reconciled: bool,
    ) -> Result<Operation, String> {
        if !self.scope_is_current() {
            return Err(crate::app_state::owner_scope::OWNER_SCOPE_STALE.into());
        }
        record.validate_intent(self.owner)?;
        self.store()
            .compare_and_swap(
                &self.scope,
                &operation.id,
                operation.revision,
                OperationUpdate {
                    status,
                    reconciled,
                    payload: serde_json::to_value(record).map_err(|error| error.to_string())?,
                },
                self.clock.load(Ordering::SeqCst),
            )
            .map_err(|error| error.to_string())
    }

    async fn capability(&self, operation: &Operation) -> Result<bool, String> {
        self.checkpoint(operation).await?;
        Ok(true)
    }

    async fn inspect(
        &self,
        operation: &mut Operation,
        _record: &mut WikiPublicationRecord,
        _worker: &str,
    ) -> Result<WikiHead, String> {
        self.checkpoint(operation).await?;
        Ok(match self.head.load(Ordering::SeqCst) {
            HEAD_ORIGINAL => WikiHead::Original,
            HEAD_APPLIED => WikiHead::Applied,
            HEAD_MISSING => WikiHead::Missing,
            HEAD_CONFLICT => WikiHead::Conflict("c".repeat(64)),
            _ => return Err("invalid fixture head".into()),
        })
    }

    async fn dependencies(
        &self,
        operation: &mut Operation,
        _record: &mut WikiPublicationRecord,
        _worker: &str,
    ) -> Result<WikiDependencyState, String> {
        self.checkpoint(operation).await?;
        Ok(
            if self.dependencies.load(Ordering::SeqCst) == DEPENDENCIES_VERIFIED {
                WikiDependencyState::Verified
            } else {
                WikiDependencyState::Missing
            },
        )
    }

    async fn publish(
        &self,
        operation: &Operation,
        record: &WikiPublicationRecord,
        event: &Event,
    ) -> Result<(), WikiPublishError> {
        self.checkpoint(operation)
            .await
            .map_err(WikiPublishError::Unknown)?;
        self.sent.fetch_add(1, Ordering::SeqCst);
        self.sent_ids
            .lock()
            .expect("sent ids")
            .push(event.id.to_hex());
        if event.id == record.head.id {
            // Read the durable row back *now*, while the head is going out, so
            // a test can prove the pre-send CAS already recorded the phase.
            let durable = self.stored_record(&operation.id);
            self.head_send_phase
                .lock()
                .expect("head send phase")
                .push((durable.progress, durable.head_attempted));
            if self.late_commit_after_head_publish.load(Ordering::SeqCst) {
                self.head.store(HEAD_APPLIED, Ordering::SeqCst);
            }
            if self.rotate_owner_on_head_publish.load(Ordering::SeqCst) {
                self.rotate_owner();
            }
            if self.lost_head_ack.load(Ordering::SeqCst) {
                return Err(WikiPublishError::Unknown("ACK lost".into()));
            }
        } else {
            if self.retire_dependency.load(Ordering::SeqCst) {
                return Err(WikiPublishError::ImmutableDependencyRetired {
                    event_id: event.id.to_hex(),
                });
            }
            if self.dependencies.load(Ordering::SeqCst) != DEPENDENCIES_ALWAYS_MISSING {
                self.dependencies
                    .store(DEPENDENCIES_VERIFIED, Ordering::SeqCst);
            }
        }
        Ok(())
    }
}

/// The common path: a freshly prepared snapshot whose immutable dependencies
/// are *already* present never enters the repair branch, so it is the only
/// path that has to record the conditional-head phase on its own. The durable
/// record must carry `Head` + `head_attempted` before the head is submitted —
/// a bare `head_attempted` on a `Preparing` row is unrepresentable and the
/// save refuses it, which would strand this attempt before its head write.
#[tokio::test]
async fn driver_records_the_head_phase_durably_before_sending_the_head() {
    let (journal, operation) = Journal::fixture_for("crew.headphase");
    let id = operation.id.clone();
    let before = journal.stored_record(&id);
    assert!(matches!(
        before.progress,
        WikiPublicationProgress::Preparing
    ));
    assert!(!before.head_attempted);
    assert_eq!(
        journal.dependencies.load(Ordering::SeqCst),
        DEPENDENCIES_VERIFIED,
        "this fixture starts with every dependency already present"
    );
    journal
        .late_commit_after_head_publish
        .store(true, Ordering::SeqCst);

    let result = drive(&journal, operation, journal.owner, false, false)
        .await
        .expect("an already verified snapshot reaches its head write");
    assert_eq!(result.status, OperationStatus::Complete);
    assert!(result.reconciled);

    // Captured inside the publish seam, from the reopened SQLite row: the
    // phase was durable *before* the event went out, not only afterwards.
    assert_eq!(
        journal
            .head_send_phase
            .lock()
            .expect("head send phase")
            .as_slice(),
        [(WikiPublicationProgress::Head, true)],
        "the pre-send CAS must land before the head is submitted"
    );

    // Exactly the original signed head, exactly once.
    assert_eq!(journal.sent.load(Ordering::SeqCst), 1);
    assert_eq!(
        journal.sent_ids.lock().expect("sent ids").as_slice(),
        [before.head.id.to_hex()],
        "only the persisted head may be sent"
    );

    // The signed graph and its conditional precondition are untouched, and the
    // reopened record still validates.
    let after = journal.stored_record(&id);
    assert_eq!(after.head, before.head);
    assert_eq!(after.manifest, before.manifest);
    assert_eq!(after.pages, before.pages);
    assert_eq!(after.snapshot_id, before.snapshot_id);
    assert_eq!(after.source_revision, before.source_revision);
    assert_eq!(after.expected_revision, before.expected_revision);
    assert!(matches!(after.progress, WikiPublicationProgress::Head));
    assert!(after.head_attempted);
    assert!(journal.reopened(&id).reconciled);
}

#[tokio::test]
async fn driver_records_retired_dependency_as_unresolved_regeneration_proof() {
    let (journal, operation) = Journal::fixture();
    let id = operation.id.clone();
    journal.head.store(HEAD_MISSING, Ordering::SeqCst);
    journal
        .dependencies
        .store(DEPENDENCIES_MISSING_UNTIL_REPAIR, Ordering::SeqCst);
    journal.retire_dependency.store(true, Ordering::SeqCst);

    let result = drive(&journal, operation, journal.owner, false, false)
        .await
        .expect("retirement proof is durably recorded");
    assert_eq!(result.status, OperationStatus::Reconciling);
    assert!(!result.reconciled);

    // Reopen the SQLite journal the way a restarted process would.
    let durable = journal.reopened(&id);
    assert_eq!(durable, result);
    let record = journal.stored_record(&id);
    assert!(record.reconcile_only);
    assert_eq!(record.retry_at, 0);
    let dependency_id = match record.reconciliation {
        Some(super::wiki_publication_record::WikiPublicationReconciliation::ImmutableDependencyRetired {
            dependency_id,
        }) => dependency_id,
        other => panic!("expected a typed retirement proof, got {other:?}"),
    };
    assert!(
        std::iter::once(&record.manifest)
            .chain(record.pages.iter())
            .any(|event| event.id.to_hex() == dependency_id),
        "the proof must name one of the stored immutable dependencies"
    );
}

#[tokio::test]
async fn driver_lost_head_ack_with_late_commit_reconciles_without_republish() {
    let (journal, operation) = Journal::fixture();
    let id = operation.id.clone();
    journal.lost_head_ack.store(true, Ordering::SeqCst);
    journal
        .late_commit_after_head_publish
        .store(true, Ordering::SeqCst);

    let result = drive(&journal, operation, journal.owner, false, false)
        .await
        .expect("late head commit reconciles");
    assert_eq!(result.status, OperationStatus::Complete);
    assert!(result.reconciled);
    assert_eq!(journal.sent.load(Ordering::SeqCst), 1);
    let durable = journal.reopened(&id);
    assert!(durable.reconciled);
    assert_eq!(durable.status, OperationStatus::Complete);
}

#[tokio::test]
async fn driver_cancelled_unknown_head_resolves_after_late_commit() {
    let (journal, operation) = Journal::fixture();
    let id = operation.id.clone();
    journal.lost_head_ack.store(true, Ordering::SeqCst);
    let failed = drive(&journal, operation, journal.owner, false, false)
        .await
        .expect("durable failure");
    assert_eq!(failed.status, OperationStatus::Failed);
    assert!(!failed.reconciled);

    // The user cancels while the admitted head write is still ambiguous.
    let canceled = journal.request_cancel(&journal.reopened(&id));
    journal.head.store(HEAD_APPLIED, Ordering::SeqCst);
    let resolved = drive(&journal, canceled, journal.owner, false, true)
        .await
        .expect("read-only late commit reconciliation");
    assert_eq!(resolved.status, OperationStatus::Complete);
    assert!(resolved.reconciled);
    assert_eq!(
        journal.sent.load(Ordering::SeqCst),
        1,
        "cancellation must never re-send the ambiguous head"
    );
    assert!(journal.reopened(&id).reconciled);
}

#[tokio::test]
async fn driver_repairs_missing_dependencies_after_a_recorded_head_attempt() {
    let (journal, operation) = Journal::fixture();
    let id = operation.id.clone();
    journal.head.store(HEAD_MISSING, Ordering::SeqCst);
    journal
        .dependencies
        .store(DEPENDENCIES_MISSING_UNTIL_REPAIR, Ordering::SeqCst);
    journal
        .late_commit_after_head_publish
        .store(true, Ordering::SeqCst);
    let mut record =
        WikiPublicationRecord::from_operation(&operation, journal.owner).expect("stored fixture");
    record.head_attempted = true;
    record.progress = WikiPublicationProgress::Head;
    let operation = journal
        .save(&operation, &record, OperationStatus::Pending, false)
        .await
        .expect("persist attempted head");

    let result = drive(&journal, operation, journal.owner, true, false)
        .await
        .expect("dependency repair");
    assert_eq!(result.status, OperationStatus::Complete);
    assert!(result.reconciled);
    assert!(journal.sent.load(Ordering::SeqCst) >= 3);
    assert!(journal.reopened(&id).reconciled);
}

#[tokio::test]
async fn driver_caps_automatic_attempts_but_allows_read_only_reconciliation() {
    let (journal, operation) = Journal::fixture();
    let id = operation.id.clone();
    let mut record =
        WikiPublicationRecord::from_operation(&operation, journal.owner).expect("stored fixture");
    record.attempts = 5;
    record.retry_at = 200;
    let operation = journal
        .save(&operation, &record, OperationStatus::Failed, false)
        .await
        .expect("persist attempt limit");

    assert!(
        drive(&journal, operation.clone(), journal.owner, false, false)
            .await
            .is_err(),
        "the automatic attempt window is exhausted"
    );
    let reconciled = drive(&journal, operation, journal.owner, false, true)
        .await
        .expect("read-only reconciliation remains available");
    assert_eq!(reconciled.status, OperationStatus::Reconciling);
    assert!(!reconciled.reconciled);
    assert_eq!(journal.sent.load(Ordering::SeqCst), 0);
    assert!(!journal.reopened(&id).reconciled);
}

#[path = "wiki_publication_driver_recovery_tests.rs"]
mod recovery;

#[path = "wiki_publication_resume_tests.rs"]
mod resume;
