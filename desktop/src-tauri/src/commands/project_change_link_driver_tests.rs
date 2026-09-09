use super::*;
use crate::owner_operations::{
    CreateResult, Limits, NewOperation, OperationKind, OperationScope, OperationStore,
    OperationUpdate,
};
use nostr::{Event, Keys, Tag, Timestamp};
use std::sync::{
    atomic::{AtomicBool, AtomicI64, AtomicU8, Ordering},
    Mutex,
};

struct Runtime {
    _dir: tempfile::TempDir,
    store: Mutex<OperationStore>,
    scope: OperationScope,
    owner: PublicKey,
    clock: AtomicI64,
    current_scope: AtomicBool,
    capability: AtomicBool,
    conditional: AtomicBool,
    eligibility: AtomicBool,
    after_publish_head: AtomicU8,
    head: AtomicU8,
    commits: AtomicBool,
    lost_ack: AtomicBool,
    disable_capability_at_send: AtomicBool,
    switch_after_send: AtomicBool,
    sent: Mutex<Vec<Event>>,
}
impl Runtime {
    fn fixture() -> (Self, Operation) {
        let keys = Keys::generate();
        let owner = keys.public_key();
        let head = buzz_sdk_pkg::builders::build_project_with_tags(
            "body",
            vec![Tag::parse(["d", "project"]).unwrap()],
        )
        .unwrap()
        .custom_created_at(Timestamp::from_secs(1))
        .sign_with_keys(&keys)
        .unwrap();
        let channel = "12345678-1234-4234-8234-123456789abc";
        let tags = super::super::project_change_link::link_channel_tags(&head, owner, channel)
            .unwrap()
            .unwrap();
        let patch = buzz_sdk_pkg::builders::build_project_with_tags(&head.content, tags)
            .unwrap()
            .custom_created_at(Timestamp::from_secs(2))
            .sign_with_keys(&keys)
            .unwrap();
        let coordinate = format!("30621:{}:project", owner.to_hex());
        let record = ProjectLinkRecord {
            version: 1,
            project_coordinate: coordinate.clone(),
            channel_id: Some(channel.into()),
            action: None,
            original_head: head,
            signed_patch: patch,
            attempts: 0,
            publication_attempted: false,
            reconcile_only: false,
            reconciliation: None,
            retry_at: 0,
            last_error: None,
            lease: None,
        };
        let dir = tempfile::tempdir().unwrap();
        let scope = OperationScope {
            owner: owner.to_hex(),
            community: "http://127.0.0.1:3000".into(),
        };
        let mut store = OperationStore::open(
            &dir.path().canonicalize().unwrap().join("recovery.db"),
            Limits::default(),
        )
        .unwrap();
        let created = store
            .create(
                &scope,
                NewOperation {
                    id: uuid::Uuid::new_v4().to_string(),
                    kind: OperationKind::ProjectChange,
                    resource_key: coordinate,
                    payload: serde_json::to_value(record).unwrap(),
                },
                100,
            )
            .unwrap();
        let CreateResult::Created(operation) = created else {
            panic!("fresh fixture")
        };
        (
            Self {
                _dir: dir,
                store: Mutex::new(store),
                scope,
                owner,
                clock: AtomicI64::new(100),
                current_scope: AtomicBool::new(true),
                capability: AtomicBool::new(true),
                conditional: AtomicBool::new(true),
                eligibility: AtomicBool::new(true),
                after_publish_head: AtomicU8::new(1),
                head: AtomicU8::new(0),
                commits: AtomicBool::new(true),
                lost_ack: AtomicBool::new(false),
                disable_capability_at_send: AtomicBool::new(false),
                switch_after_send: AtomicBool::new(false),
                sent: Mutex::new(Vec::new()),
            },
            operation,
        )
    }
    fn load(&self, id: &str) -> Operation {
        self.store.lock().unwrap().load(&self.scope, id).unwrap()
    }
}
impl ProjectLinkRuntime for Runtime {
    fn now(&self) -> Result<i64, String> {
        Ok(self.clock.load(Ordering::SeqCst))
    }
    async fn checkpoint(&self, operation: &Operation) -> Result<(), String> {
        if !self.current_scope.load(Ordering::SeqCst) {
            return Err("stale native scope".into());
        }
        let current = self.load(&operation.id);
        if current.revision != operation.revision || current.payload != operation.payload {
            return Err("stale operation revision".into());
        }
        let record = ProjectLinkRecord::from_operation(&current, self.owner)?;
        if record
            .lease
            .is_none_or(|lease| lease.expires_at <= self.clock.load(Ordering::SeqCst))
        {
            return Err("expired lease".into());
        }
        Ok(())
    }
    async fn save(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
        status: OperationStatus,
        reconciled: bool,
    ) -> Result<Operation, String> {
        if !self.current_scope.load(Ordering::SeqCst) {
            return Err("stale native scope".into());
        }
        self.store
            .lock()
            .unwrap()
            .compare_and_swap(
                &self.scope,
                &operation.id,
                operation.revision,
                OperationUpdate {
                    status,
                    reconciled,
                    payload: serde_json::to_value(record).unwrap(),
                },
                self.clock.load(Ordering::SeqCst),
            )
            .map_err(|error| error.to_string())
    }
    async fn capability(&self, operation: &Operation) -> Result<bool, String> {
        self.checkpoint(operation).await?;
        Ok(self.capability.load(Ordering::SeqCst))
    }
    async fn inspect(
        &self,
        operation: &Operation,
        _: &ProjectLinkRecord,
    ) -> Result<LinkHead, String> {
        self.checkpoint(operation).await?;
        match self.head.load(Ordering::SeqCst) {
            0 => Ok(LinkHead::Original),
            1 => Ok(LinkHead::Applied),
            2 => Ok(LinkHead::Conflict("c".repeat(64))),
            _ => Err("channel eligibility unavailable".into()),
        }
    }
    async fn eligible(&self, operation: &Operation, _: &ProjectLinkRecord) -> Result<(), String> {
        self.checkpoint(operation).await?;
        if self.eligibility.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err("membership removed".into())
        }
    }
    async fn prove_superseded(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
    ) -> Result<Option<LinkHead>, String> {
        self.checkpoint(operation).await?;
        if self.conditional.load(Ordering::SeqCst) {
            self.inspect(operation, record).await.map(Some)
        } else {
            Ok(None)
        }
    }
    async fn publish(
        &self,
        operation: &Operation,
        record: &ProjectLinkRecord,
    ) -> Result<(), String> {
        self.checkpoint(operation).await?;
        self.sent.lock().unwrap().push(record.signed_patch.clone());
        if self.disable_capability_at_send.load(Ordering::SeqCst) {
            self.capability.store(false, Ordering::SeqCst);
            return Err("unsupported: crew-project-channel-link-v1".into());
        }
        if self.commits.load(Ordering::SeqCst) {
            self.head.store(
                self.after_publish_head.load(Ordering::SeqCst),
                Ordering::SeqCst,
            );
        }
        if self.switch_after_send.load(Ordering::SeqCst) {
            self.current_scope.store(false, Ordering::SeqCst);
        }
        if self.lost_ack.load(Ordering::SeqCst) {
            Err("ACK lost".into())
        } else {
            Ok(())
        }
    }
}

#[path = "project_change_link_capability_tests.rs"]
mod capability_tests;

#[tokio::test]
async fn project_link_driver_lost_ack_requires_exact_readback_and_finishes_once() {
    let (runtime, operation) = Runtime::fixture();
    runtime.lost_ack.store(true, Ordering::SeqCst);
    let expected_event = operation.payload["signed_patch"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let result = drive(&runtime, operation, runtime.owner, false)
        .await
        .unwrap();
    assert_eq!(result.status, OperationStatus::Complete);
    assert!(result.reconciled);
    assert_eq!(runtime.sent.lock().unwrap()[0].id.to_hex(), expected_event);
    assert_eq!(runtime.sent.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn project_link_driver_scope_change_retains_attempt_and_takeover_reads_before_replay() {
    let (runtime, operation) = Runtime::fixture();
    let id = operation.id.clone();
    runtime.switch_after_send.store(true, Ordering::SeqCst);
    assert!(drive(&runtime, operation, runtime.owner, false)
        .await
        .is_err());
    let persisted = runtime.load(&id);
    assert_eq!(persisted.status, OperationStatus::Pending);
    assert!(!persisted.reconciled);
    runtime.current_scope.store(true, Ordering::SeqCst);
    runtime.clock.store(161, Ordering::SeqCst);
    let result = drive(&runtime, persisted, runtime.owner, false)
        .await
        .unwrap();
    assert!(result.reconciled);
    assert_eq!(
        runtime.sent.lock().unwrap().len(),
        1,
        "takeover must read exact committed head before any replay"
    );
}

#[tokio::test]
async fn project_link_driver_unknown_outcome_stays_durable_with_same_envelope() {
    let (runtime, operation) = Runtime::fixture();
    runtime.commits.store(false, Ordering::SeqCst);
    runtime.lost_ack.store(true, Ordering::SeqCst);
    let expected = operation.payload["signed_patch"].clone();
    let result = drive(&runtime, operation, runtime.owner, false)
        .await
        .unwrap();
    assert_eq!(result.status, OperationStatus::Failed);
    assert!(!result.reconciled);
    assert_eq!(result.payload["signed_patch"], expected);
    assert!(result.payload["last_error"]
        .as_str()
        .unwrap()
        .contains("ACK lost"));
}

#[tokio::test]
async fn project_link_driver_pre_send_conflict_never_publishes() {
    let (runtime, operation) = Runtime::fixture();
    runtime.head.store(2, Ordering::SeqCst);
    let result = drive(&runtime, operation, runtime.owner, false)
        .await
        .unwrap();
    assert_eq!(result.status, OperationStatus::Superseded);
    assert!(result.reconciled);
    assert!(runtime.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn project_link_driver_capability_failures_are_bounded_and_manual_retry_keeps_intent() {
    let (runtime, mut operation) = Runtime::fixture();
    runtime.capability.store(false, Ordering::SeqCst);
    let expected = operation.payload["signed_patch"].clone();
    for attempt in 1..=5 {
        operation = drive(&runtime, operation, runtime.owner, false)
            .await
            .unwrap();
        assert_eq!(operation.payload["attempts"], attempt);
        runtime.clock.fetch_add(301, Ordering::SeqCst);
    }
    assert!(drive(&runtime, operation.clone(), runtime.owner, false)
        .await
        .is_err());
    assert!(runtime.sent.lock().unwrap().is_empty());
    runtime.capability.store(true, Ordering::SeqCst);
    let result = drive(&runtime, operation, runtime.owner, true)
        .await
        .unwrap();
    assert!(result.reconciled);
    assert_eq!(result.payload["signed_patch"], expected);
}

#[tokio::test]
async fn project_link_driver_active_lease_and_stale_revision_cannot_dispatch() {
    let (runtime, operation) = Runtime::fixture();
    let mut record = ProjectLinkRecord::from_operation(&operation, runtime.owner).unwrap();
    renew(&mut record, 100, &uuid::Uuid::new_v4().to_string()).unwrap();
    let claimed = runtime
        .save(&operation, &record, OperationStatus::Reconciling, false)
        .await
        .unwrap();
    assert!(drive(&runtime, claimed, runtime.owner, false)
        .await
        .is_err());
    assert!(drive(&runtime, operation, runtime.owner, false)
        .await
        .is_err());
    assert!(runtime.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn project_link_driver_invalid_journal_cannot_authorize_a_different_resource() {
    let (runtime, mut operation) = Runtime::fixture();
    operation.resource_key.push_str("-other");
    assert!(drive(&runtime, operation, runtime.owner, false)
        .await
        .is_err());
    assert!(runtime.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn project_link_driver_missing_membership_prevents_send() {
    let (runtime, operation) = Runtime::fixture();
    runtime.eligibility.store(false, Ordering::SeqCst);
    let result = drive(&runtime, operation, runtime.owner, false)
        .await
        .unwrap();
    assert!(!result.reconciled);
    assert!(runtime.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn project_link_applied_readback_survives_capability_removal() {
    let (runtime, operation) = Runtime::fixture();
    runtime.head.store(1, Ordering::SeqCst);
    runtime.capability.store(false, Ordering::SeqCst);
    runtime.eligibility.store(false, Ordering::SeqCst);
    let result = drive(&runtime, operation, runtime.owner, false)
        .await
        .unwrap();
    assert!(
        result.reconciled,
        "exact applied head must reconcile without new-write capability"
    );
    assert!(runtime.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn project_link_pre_send_conflict_releases_claim_for_reviewed_successor() {
    let (runtime, operation) = Runtime::fixture();
    runtime.head.store(2, Ordering::SeqCst);
    let result = drive(&runtime, operation, runtime.owner, false)
        .await
        .unwrap();
    assert!(runtime.sent.lock().unwrap().is_empty());
    let successor = runtime
        .store
        .lock()
        .unwrap()
        .create(
            &runtime.scope,
            NewOperation {
                id: uuid::Uuid::new_v4().to_string(),
                kind: OperationKind::ProjectChange,
                resource_key: result.resource_key.clone(),
                payload: result.payload.clone(),
            },
            101,
        )
        .unwrap();
    assert!(
        matches!(successor, CreateResult::Created(_)),
        "pre-send conflict must not leave an undispatchable permanent resource claim"
    );
}

#[tokio::test]
async fn project_link_post_send_conflict_stays_read_only_until_conditional_evidence() {
    let (runtime, operation) = Runtime::fixture();
    runtime.after_publish_head.store(2, Ordering::SeqCst);
    runtime.lost_ack.store(true, Ordering::SeqCst);
    let retained = drive(&runtime, operation, runtime.owner, false)
        .await
        .unwrap();
    assert_eq!(retained.status, OperationStatus::Superseded);
    assert!(!retained.reconciled);
    assert_eq!(retained.payload["publication_attempted"], true);
    assert_eq!(retained.payload["reconcile_only"], true);
    runtime.capability.store(false, Ordering::SeqCst);
    runtime.conditional.store(false, Ordering::SeqCst);
    runtime.eligibility.store(false, Ordering::SeqCst);
    let unknown = drive(&runtime, retained, runtime.owner, true)
        .await
        .unwrap();
    assert!(
        !unknown.reconciled,
        "absent conditional proof cannot release an attempted claim"
    );
    runtime.conditional.store(true, Ordering::SeqCst);
    let resolved = drive(&runtime, unknown, runtime.owner, true).await.unwrap();
    assert!(resolved.reconciled);
    assert_eq!(
        resolved.payload["reconciliation"]["proof"],
        "conditional-conflict"
    );
    assert_eq!(
        runtime.sent.lock().unwrap().len(),
        1,
        "read-only recovery never republishes"
    );
}

#[tokio::test]
async fn project_link_reconcile_only_survives_a_reconciling_status_on_restart() {
    let (runtime, operation) = Runtime::fixture();
    let mut record = ProjectLinkRecord::from_operation(&operation, runtime.owner).unwrap();
    record.publication_attempted = true;
    record.reconcile_only = true;
    record.lease = None;
    let restarted = runtime
        .save(&operation, &record, OperationStatus::Reconciling, false)
        .await
        .unwrap();
    let result = drive(&runtime, restarted, runtime.owner, true)
        .await
        .unwrap();
    assert!(!result.reconciled);
    assert!(
        runtime.sent.lock().unwrap().is_empty(),
        "durable read-only intent must not depend on transient status"
    );
}
