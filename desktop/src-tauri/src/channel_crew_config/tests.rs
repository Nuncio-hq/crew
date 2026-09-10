use std::sync::{
    atomic::{AtomicI64, AtomicUsize, Ordering},
    Mutex,
};

use super::{
    driver::{resume, Backend},
    record::{Outcome, Payload},
};
use crate::owner_operations::{
    CreateResult, Limits, NewOperation, Operation, OperationKind, OperationScope, OperationStatus,
    OperationStore, OperationUpdate,
};
use nostr::{Event, Keys};

pub(super) struct Fixture {
    pub(super) keys: Keys,
    _directory: tempfile::TempDir,
    store: Mutex<OperationStore>,
    scope: OperationScope,
    now: AtomicI64,
    events: Mutex<Vec<Event>>,
    sends: Mutex<Vec<String>>,
    reject_announcement: AtomicUsize,
    lose_canvas_ack: AtomicUsize,
    lose_notice_ack: AtomicUsize,
    persist_count: AtomicUsize,
    fail_persist_at: AtomicUsize,
    remote_after_canvas: AtomicUsize,
    remote_after_notice: AtomicUsize,
    scope_stale: AtomicUsize,
    switch_after_canvas: AtomicUsize,
}

impl Fixture {
    pub(super) fn new() -> (Self, Operation) {
        let directory = tempfile::tempdir().unwrap();
        let mut store = OperationStore::open(
            &directory.path().canonicalize().unwrap().join("recovery.db"),
            Limits::default(),
        )
        .unwrap();
        let keys = Keys::generate();
        let channel = uuid::Uuid::new_v4();
        let scope = OperationScope {
            owner: keys.public_key().to_hex(),
            community: "http://fixture.invalid".into(),
        };
        let canvas = crate::events::build_set_canvas(
            channel,
            "```crew\ndefinitions: {Review: Inspect}\n```",
        )
        .unwrap()
        .sign_with_keys(&keys)
        .unwrap();
        let notice = format!(
            "AGENT-WORKING-AGREEMENT: configuration snapshot recorded. Canvas: {}",
            canvas.id
        );
        let announcement = crate::events::build_message(
            channel,
            &notice,
            None,
            &[],
            &[],
            &[],
            &[],
            &[],
            None,
            &scope.community,
        )
        .unwrap()
        .sign_with_keys(&keys)
        .unwrap();
        let payload = Payload {
            version: 1,
            relay_url: scope.community.clone(),
            channel_id: channel.to_string(),
            expected_head: None,
            canvas,
            announcement,
            canvas_attempted: false,
            canvas_acknowledged: false,
            announcement_attempted: false,
            announcement_acknowledged: false,
            failures: 0,
            next_retry_at: None,
            lease: None,
            outcome: Outcome::NotCommitted,
            draft: None,
            cleanup_members: None,
        };
        let operation = match store
            .create(
                &scope,
                NewOperation {
                    id: uuid::Uuid::new_v4().to_string(),
                    kind: OperationKind::ChannelCrewConfig,
                    resource_key: channel.to_string(),
                    payload: serde_json::to_value(payload).unwrap(),
                },
                1000,
            )
            .unwrap()
        {
            CreateResult::Created(operation) => operation,
            _ => panic!("fresh operation"),
        };
        (
            Self {
                keys,
                _directory: directory,
                store: Mutex::new(store),
                scope,
                now: AtomicI64::new(1000),
                events: Mutex::new(Vec::new()),
                sends: Mutex::new(Vec::new()),
                reject_announcement: AtomicUsize::new(0),
                lose_canvas_ack: AtomicUsize::new(0),
                lose_notice_ack: AtomicUsize::new(0),
                persist_count: AtomicUsize::new(0),
                fail_persist_at: AtomicUsize::new(0),
                remote_after_canvas: AtomicUsize::new(0),
                remote_after_notice: AtomicUsize::new(0),
                scope_stale: AtomicUsize::new(0),
                switch_after_canvas: AtomicUsize::new(0),
            },
            operation,
        )
    }

    pub(super) fn load(&self, id: &str) -> Operation {
        self.store.lock().unwrap().load(&self.scope, id).unwrap()
    }
}

impl Backend for Fixture {
    fn now(&self) -> Result<i64, String> {
        Ok(self.now.load(Ordering::SeqCst))
    }
    async fn check_scope(&self) -> Result<(), String> {
        if self.scope_stale.load(Ordering::SeqCst) > 0 {
            Err("fixture owner changed".into())
        } else {
            Ok(())
        }
    }
    async fn persist(
        &self,
        operation: &Operation,
        payload: &Payload,
        status: OperationStatus,
        reconciled: bool,
    ) -> Result<Operation, String> {
        let count = self.persist_count.fetch_add(1, Ordering::SeqCst) + 1;
        if count == self.fail_persist_at.load(Ordering::SeqCst) {
            return Err("fixture persistence failed".into());
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
                    payload: serde_json::to_value(payload).unwrap(),
                },
                self.now()?,
            )
            .map_err(|error| error.to_string())
    }
    async fn latest(&self, _payload: &Payload) -> Result<Option<String>, String> {
        let mut events: Vec<_> = self
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.kind.as_u16() == 40100)
            .cloned()
            .collect();
        events.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(events.first().map(|event| event.id.to_hex()))
    }
    async fn contains(&self, _payload: &Payload, event: &Event) -> Result<bool, String> {
        Ok(self
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|seen| seen.id == event.id))
    }
    async fn publish(&self, payload: &Payload, event: &Event) -> Result<(), String> {
        self.sends.lock().unwrap().push(event.id.to_hex());
        if event.kind.as_u16() != 40100 && self.reject_announcement.load(Ordering::SeqCst) > 0 {
            return Err("fixture announcement rejected".into());
        }
        let mut events = self.events.lock().unwrap();
        if !events.iter().any(|seen| seen.id == event.id) {
            events.push(event.clone());
        }
        if (event.kind.as_u16() == 40100 && self.remote_after_canvas.load(Ordering::SeqCst) > 0)
            || (event.kind.as_u16() == 9 && self.remote_after_notice.load(Ordering::SeqCst) > 0)
        {
            let remote = crate::events::build_set_canvas(
                uuid::Uuid::parse_str(&payload.channel_id).unwrap(),
                "remote newer canvas",
            )
            .unwrap()
            .custom_created_at(nostr::Timestamp::from(event.created_at.as_secs() + 100))
            .sign_with_keys(&Keys::generate())
            .unwrap();
            events.push(remote);
        }
        if event.kind.as_u16() == 40100 && self.switch_after_canvas.load(Ordering::SeqCst) > 0 {
            self.scope_stale.store(1, Ordering::SeqCst);
        }
        if event.kind.as_u16() == 40100 && self.lose_canvas_ack.load(Ordering::SeqCst) > 0 {
            return Err("fixture lost ACK".into());
        }
        if event.kind.as_u16() == 9 && self.lose_notice_ack.load(Ordering::SeqCst) > 0 {
            return Err("fixture lost notice ACK".into());
        }
        Ok(())
    }
}

#[tokio::test]
async fn channel_crew_recovery_new_remote_head_suppresses_notice_and_false_applied() {
    for after_notice in [false, true] {
        let (fixture, operation) = Fixture::new();
        if after_notice {
            fixture.remote_after_notice.store(1, Ordering::SeqCst);
        } else {
            fixture.remote_after_canvas.store(1, Ordering::SeqCst);
        }
        let id = operation.id.clone();
        let result = resume(&fixture, operation, false).await.unwrap();
        assert_eq!(result.outcome, Outcome::Superseded);
        assert_eq!(
            fixture.sends.lock().unwrap().len(),
            if after_notice { 2 } else { 1 }
        );
        assert!(fixture.load(&id).reconciled);
    }
}

#[tokio::test]
async fn channel_crew_recovery_ack_crash_boundaries_keep_exact_events_on_reopen() {
    for fail_at in [4, 5, 8, 9, 10] {
        let (fixture, operation) = Fixture::new();
        let id = operation.id.clone();
        fixture.fail_persist_at.store(fail_at, Ordering::SeqCst);
        let partial = resume(&fixture, operation, false).await.unwrap();
        assert_ne!(
            partial.outcome,
            Outcome::Applied,
            "failed completion persist cannot report applied"
        );
        *fixture.store.lock().unwrap() = OperationStore::open(
            &fixture
                ._directory
                .path()
                .canonicalize()
                .unwrap()
                .join("recovery.db"),
            Limits::default(),
        )
        .unwrap();
        fixture.now.store(1060, Ordering::SeqCst);
        let result = resume(&fixture, fixture.load(&id), false).await.unwrap();
        assert_eq!(result.outcome, Outcome::Applied);
        assert_eq!(fixture.sends.lock().unwrap().len(), 2);
    }
}

#[tokio::test]
async fn channel_crew_recovery_lost_notice_ack_does_not_publish_twice() {
    let (fixture, operation) = Fixture::new();
    let id = operation.id.clone();
    fixture.lose_notice_ack.store(1, Ordering::SeqCst);
    assert_eq!(
        resume(&fixture, operation, false).await.unwrap().outcome,
        Outcome::CanvasCommittedAnnouncementPending
    );
    fixture.now.store(1005, Ordering::SeqCst);
    assert_eq!(
        resume(&fixture, fixture.load(&id), false)
            .await
            .unwrap()
            .outcome,
        Outcome::Applied
    );
    assert_eq!(fixture.sends.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn channel_crew_recovery_durable_lease_and_cas_fence_duplicate_workers() {
    let (fixture, operation) = Fixture::new();
    let stale = operation.clone();
    let mut payload: Payload = serde_json::from_value(operation.payload.clone()).unwrap();
    payload.lease = Some(super::record::Lease {
        worker: uuid::Uuid::new_v4().to_string(),
        expires_at: 1060,
    });
    let leased = fixture
        .persist(&operation, &payload, OperationStatus::Reconciling, false)
        .await
        .unwrap();
    assert!(resume(&fixture, leased.clone(), true).await.is_err());
    assert!(fixture.sends.lock().unwrap().is_empty());
    fixture.now.store(1060, Ordering::SeqCst);
    assert_eq!(
        resume(&fixture, leased, false).await.unwrap().outcome,
        Outcome::Applied
    );
    assert!(resume(&fixture, stale, false).await.is_err());
    assert_eq!(fixture.sends.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn channel_crew_recovery_scope_switch_after_send_retains_uncertain_intent() {
    let (fixture, operation) = Fixture::new();
    let id = operation.id.clone();
    fixture.switch_after_canvas.store(1, Ordering::SeqCst);
    assert!(resume(&fixture, operation, false).await.is_err());
    let saved = fixture.load(&id);
    let payload: Payload = serde_json::from_value(saved.payload.clone()).unwrap();
    assert!(payload.canvas_attempted);
    assert!(!saved.reconciled);
    assert_eq!(fixture.sends.lock().unwrap().len(), 1);
    fixture.scope_stale.store(0, Ordering::SeqCst);
    fixture.now.store(1060, Ordering::SeqCst);
    assert_eq!(
        resume(&fixture, saved, false).await.unwrap().outcome,
        Outcome::Applied
    );
    assert_eq!(fixture.sends.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn channel_crew_recovery_failed_initial_persist_has_no_external_effect() {
    let (fixture, operation) = Fixture::new();
    fixture.fail_persist_at.store(1, Ordering::SeqCst);
    assert!(resume(&fixture, operation, false).await.is_err());
    assert!(fixture.sends.lock().unwrap().is_empty());
}

#[tokio::test]
async fn channel_crew_recovery_commits_one_canvas_and_one_unmentioned_notice() {
    let (fixture, operation) = Fixture::new();
    let id = operation.id.clone();
    let result = resume(&fixture, operation, false).await.unwrap();
    assert_eq!(result.outcome, Outcome::Applied);
    assert_eq!(fixture.sends.lock().unwrap().len(), 2);
    assert!(fixture.load(&id).reconciled);
    for event in fixture.events.lock().unwrap().iter() {
        assert!(!event
            .tags
            .iter()
            .any(|tag| tag.as_slice().first().is_some_and(|key| key == "p")));
    }
}

#[tokio::test]
async fn channel_crew_recovery_lost_canvas_ack_reconciles_without_resigning() {
    let (fixture, operation) = Fixture::new();
    let id = operation.id.clone();
    fixture.lose_canvas_ack.store(1, Ordering::SeqCst);
    let first = resume(&fixture, operation, false).await.unwrap();
    assert_eq!(first.outcome, Outcome::CommitUncertain);
    fixture.now.store(1005, Ordering::SeqCst);
    let recovered = resume(&fixture, fixture.load(&id), false).await.unwrap();
    assert_eq!(recovered.outcome, Outcome::Applied);
    assert_eq!(
        fixture.sends.lock().unwrap().len(),
        2,
        "one canvas attempt and one notice"
    );
}

#[tokio::test]
async fn channel_crew_recovery_announcement_failure_stays_durable_and_bounded() {
    let (fixture, operation) = Fixture::new();
    let id = operation.id.clone();
    fixture.reject_announcement.store(1, Ordering::SeqCst);
    let first = resume(&fixture, operation, false).await.unwrap();
    assert_eq!(first.outcome, Outcome::CanvasCommittedAnnouncementPending);
    assert_eq!(first.automatic_retry_at, Some(1005));
    let attempts = fixture.sends.lock().unwrap().len();
    assert!(resume(&fixture, fixture.load(&id), false).await.is_err());
    assert_eq!(fixture.sends.lock().unwrap().len(), attempts);
    for time in [1005, 1015, 1035, 1075] {
        fixture.now.store(time, Ordering::SeqCst);
        resume(&fixture, fixture.load(&id), false).await.unwrap();
    }
    let saved: Payload = serde_json::from_value(fixture.load(&id).payload).unwrap();
    assert_eq!(saved.failures, 5);
    assert!(saved.next_retry_at.is_none());
    assert!(!fixture.load(&id).reconciled);
    assert!(resume(&fixture, fixture.load(&id), false).await.is_err());
    fixture.reject_announcement.store(0, Ordering::SeqCst);
    assert_eq!(
        resume(&fixture, fixture.load(&id), true)
            .await
            .unwrap()
            .outcome,
        Outcome::Applied
    );
}
