use super::*;
use buzz_core_pkg::transport_status::{
    TransportAuthClassification, TransportCode, TransportConnectionAttempt, TransportReceivedAuth,
    TransportRecord, TransportRecordEnvelope, TransportRecordV2, TransportState,
};

fn ticket(index: usize) -> ReadTicket {
    ReadTicket {
        key: ManagedAgentRuntimeKey::new(format!("{index:064x}"), "ws://fixture").unwrap(),
        nonce: uuid::Uuid::new_v4().simple().to_string(),
        path: PathBuf::from("/fixture/status.json"),
        owner: "a".repeat(64),
        epoch: 0,
        process_id: 1,
        spawn_started_at_ms: 1,
        wire_version: 1,
    }
}
fn record(ticket: &ReadTicket, terminal: bool) -> TransportRecord {
    TransportRecord {
        version: 1,
        runtime_id: ticket.key.runtime_id(),
        start_nonce: ticket.nonce.clone(),
        sequence: 1,
        timestamp_ms: 100_000,
        terminal,
        transport: TransportStatus {
            state: if terminal {
                TransportState::Exhausted
            } else {
                TransportState::Connected
            },
            code: TransportCode::None,
            attempts: 0,
            elapsed_ms: 0,
            next_retry_at_ms: None,
            last_error: None,
        },
    }
}

fn v2_ticket(index: usize) -> ReadTicket {
    let mut ticket = ticket(index);
    ticket.process_id = 7;
    ticket.spawn_started_at_ms = 90_000;
    ticket.wire_version = 2;
    ticket
}

fn denial_record(
    ticket: &ReadTicket,
    sequence: u64,
    attempt_sequence: u64,
    terminal: bool,
) -> TransportRecordV2 {
    let attempt_id = uuid::Uuid::from_u128(0x1234 + u128::from(attempt_sequence)).to_string();
    TransportRecordV2 {
        version: 2,
        runtime_id: ticket.key.runtime_id(),
        start_nonce: ticket.nonce.clone(),
        sequence,
        timestamp_ms: 100_000,
        terminal,
        transport: TransportStatus {
            state: TransportState::AuthRejected,
            code: TransportCode::AuthDenied,
            attempts: 1,
            elapsed_ms: 10,
            next_retry_at_ms: None,
            last_error: TransportCode::AuthDenied.message().map(str::to_owned),
        },
        process_id: ticket.process_id,
        spawn_started_at_ms: ticket.spawn_started_at_ms,
        connection_attempt: Some(TransportConnectionAttempt {
            sequence: attempt_sequence,
            id: attempt_id.clone(),
            started_at_ms: 91_000,
        }),
        received_auth: Some(TransportReceivedAuth {
            auth_event_id: "a".repeat(64),
            attempt_id,
            attempt_sequence,
            accepted: false,
            classification: TransportAuthClassification::CommunityBanned,
            received_at_ms: 92_000,
        }),
    }
}

#[test]
fn late_read_cannot_recache_after_disable_or_reenable() {
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let ticket = ticket(1);
    let mut monitor = Monitor::new(ticket.clone(), &diagnostics);
    assert!(monitor.disable());
    assert!(!monitor.disable(), "repeated leave must be idempotent");
    assert!(!monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V1(record(&ticket, false))),
        true,
        Instant::now(),
        100_000
    ));
    assert_eq!(monitor.status().state, TransportState::Unknown);
    assert!(monitor.enable(&ticket.owner).unwrap());
    assert!(
        !monitor.enable(&ticket.owner).unwrap(),
        "repeated rejoin must be idempotent"
    );
    assert!(!monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V1(record(&ticket, false))),
        true,
        Instant::now(),
        100_000
    ));
    assert_eq!(monitor.status().state, TransportState::Unknown);
    let current = monitor.snapshot().unwrap();
    assert!(monitor.apply(
        &current,
        Ok(TransportRecordEnvelope::V1(record(&current, false))),
        true,
        Instant::now(),
        100_000
    ));
}

#[test]
fn owner_change_cannot_reenable_previous_owners_generation() {
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let mut monitor = Monitor::new(ticket(1), &diagnostics);
    monitor.disable();
    assert!(monitor.enable(&"b".repeat(64)).is_err());
    assert!(monitor.snapshot().is_none());
}

#[test]
fn stopped_process_cannot_accept_live_record_even_for_matching_nonce() {
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let ticket = ticket(1);
    let mut monitor = Monitor::new(ticket.clone(), &diagnostics);
    assert!(!monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V1(record(&ticket, false))),
        false,
        Instant::now(),
        100_000
    ));
    assert_eq!(monitor.status().state, TransportState::Unknown);
}

#[test]
fn v2_capable_native_accepts_legacy_v1_health_records() {
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let ticket = v2_ticket(2);
    let mut monitor = Monitor::new(ticket.clone(), &diagnostics);
    assert!(monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V1(record(&ticket, false))),
        true,
        Instant::now(),
        100_000,
    ));
    assert_eq!(monitor.status().state, TransportState::Connected);
}

#[test]
fn retirement_cache_is_bounded_and_expires_monotonically() {
    let now = Instant::now();
    let mut cache = Diagnostics::default();
    for index in 0..257 {
        cache.retire(
            ticket(index),
            true,
            now + Duration::from_secs(index as u64),
            TransportLease::default(),
            None,
            None,
            None,
            None,
            false,
            ExportState::default(),
        );
    }
    assert_eq!(cache.entries.len(), RETIRED_CAPACITY);
    assert!(!cache.entries.contains_key(&ticket(0).key));
    cache.prune(now + Duration::from_secs(256) + RETIRED_TTL);
    assert!(cache.entries.is_empty());
}

#[test]
fn clear_or_replacement_invalidates_a_retired_read_token() {
    let now = Instant::now();
    let old = ticket(1);
    let mut cache = Diagnostics::default();
    cache.retire(
        old.clone(),
        true,
        now,
        TransportLease::default(),
        None,
        None,
        None,
        None,
        false,
        ExportState::default(),
    );
    cache.clear_key(&old.key);
    assert!(!cache.apply(
        &old,
        TransportRecordEnvelope::V1(record(&old, true)),
        false,
        now,
        100_000,
    ));
    let replacement = ticket(1);
    cache.retire(
        replacement,
        true,
        now,
        TransportLease::default(),
        None,
        None,
        None,
        None,
        false,
        ExportState::default(),
    );
    assert!(!cache.apply(
        &old,
        TransportRecordEnvelope::V1(record(&old, true)),
        false,
        now,
        100_000,
    ));
}

#[test]
fn retired_read_cannot_overwrite_new_live_generation_or_outlive_window() {
    let now = Instant::now();
    let old = ticket(1);
    let mut cache = Diagnostics::default();
    cache.retire(
        old.clone(),
        true,
        now,
        TransportLease::default(),
        None,
        None,
        None,
        None,
        false,
        ExportState::default(),
    );
    assert!(!cache.apply(
        &old,
        TransportRecordEnvelope::V1(record(&old, true)),
        true,
        now,
        100_000,
    ));
    assert!(cache.pending(now + FINAL_READ_WINDOW).is_empty());
    assert!(!cache.apply(
        &old,
        TransportRecordEnvelope::V1(record(&old, true)),
        false,
        now + FINAL_READ_WINDOW,
        100_000
    ));
}

#[test]
fn disabled_generation_cannot_create_retirement_metadata() {
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let mut monitor = Monitor::new(ticket(1), &diagnostics);
    monitor.disable();
    monitor.retire(true, Instant::now());
    assert!(diagnostics.lock().unwrap().entries.is_empty());
}

#[test]
fn expired_same_sequence_cannot_reexpose_auth_evidence_but_newer_record_can() {
    let now = Instant::now();
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let ticket = v2_ticket(4);
    let mut monitor = Monitor::new(ticket.clone(), &diagnostics);
    let first = denial_record(&ticket, 1, 1, false);
    assert!(monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V2(first.clone())),
        true,
        now,
        100_000,
    ));
    assert!(monitor.auth_record().is_some());
    monitor.expire(now + Duration::from_secs(16));
    assert!(monitor.auth_record().is_none());
    assert!(!monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V2(first)),
        true,
        now + Duration::from_secs(16),
        100_000,
    ));
    assert!(monitor.auth_record().is_none());
    let newer = denial_record(&ticket, 2, 1, false);
    assert!(monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V2(newer)),
        true,
        now + Duration::from_secs(17),
        100_001,
    ));
    assert!(monitor.auth_record().is_some());
}

#[test]
fn denial_on_a_new_attempt_after_a_no_ack_snapshot_is_accepted() {
    let now = Instant::now();
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let ticket = v2_ticket(6);
    let mut monitor = Monitor::new(ticket.clone(), &diagnostics);
    let first = denial_record(&ticket, 1, 1, false);
    assert!(monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V2(first)),
        true,
        now,
        100_000,
    ));
    assert!(monitor.auth_record().is_some());

    let mut connecting = denial_record(&ticket, 2, 2, false);
    connecting.transport = TransportStatus {
        state: TransportState::Connecting,
        code: TransportCode::None,
        attempts: 2,
        elapsed_ms: 0,
        next_retry_at_ms: None,
        last_error: None,
    };
    connecting.received_auth = None;
    assert!(monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V2(connecting)),
        true,
        now + Duration::from_secs(1),
        100_001,
    ));
    assert!(monitor.auth_record().is_none());

    let second = denial_record(&ticket, 3, 2, false);
    assert!(monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V2(second)),
        true,
        now + Duration::from_secs(2),
        100_002,
    ));
    assert!(monitor.auth_record().is_some());
}

#[test]
fn success_reset_consumes_receipt_and_fences_same_attempt_replay() {
    let now = Instant::now();
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let ticket = v2_ticket(7);
    let mut monitor = Monitor::new(ticket.clone(), &diagnostics);
    let first = denial_record(&ticket, 1, 1, false);
    assert!(monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V2(first.clone())),
        true,
        now,
        100_000,
    ));
    assert!(monitor.auth_record().is_some());

    let mut success = first.clone();
    success.sequence = 2;
    success.transport = TransportStatus {
        state: TransportState::Connected,
        code: TransportCode::None,
        attempts: 0,
        elapsed_ms: 0,
        next_retry_at_ms: None,
        last_error: None,
    };
    // Exercise the stale receipt shape too: native consumes it at the
    // success/reset boundary and does not let it reopen the wrapper later.
    assert!(success.received_auth.is_some());
    assert!(monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V2(success)),
        true,
        now + Duration::from_secs(1),
        100_001,
    ));
    assert!(monitor.auth_record().is_none());

    let mut replay = first;
    replay.sequence = 3;
    assert!(monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V2(replay)),
        true,
        now + Duration::from_secs(2),
        100_002,
    ));
    assert_eq!(monitor.status().state, TransportState::Unknown);
    assert!(monitor.auth_record().is_none());
}

#[test]
fn retired_matching_terminal_record_is_the_only_evidence_source() {
    let now = Instant::now();
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let ticket = v2_ticket(5);
    let mut monitor = Monitor::new(ticket.clone(), &diagnostics);
    let terminal = denial_record(&ticket, 1, 1, true);
    assert!(monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V2(terminal.clone())),
        true,
        now,
        100_000,
    ));
    monitor.retire(true, now + Duration::from_secs(1));
    let before = diagnostics
        .lock()
        .unwrap()
        .projection(&ticket.key, &ticket.owner, now)
        .unwrap();
    assert!(before.auth_record.is_none());
    let mut changed_ack = terminal.clone();
    changed_ack
        .received_auth
        .as_mut()
        .expect("fixture has AUTH evidence")
        .auth_event_id = "b".repeat(64);
    assert!(!diagnostics.lock().unwrap().apply(
        &ticket,
        TransportRecordEnvelope::V2(changed_ack),
        false,
        now + Duration::from_secs(1),
        100_000,
    ));
    let unavailable = diagnostics
        .lock()
        .unwrap()
        .projection(&ticket.key, &ticket.owner, now)
        .unwrap();
    assert_eq!(unavailable.status.state, TransportState::Unknown);
    assert_eq!(unavailable.status.code, TransportCode::StatusUnavailable);
    assert!(unavailable.auth_record.is_none());
    assert!(diagnostics.lock().unwrap().apply(
        &ticket,
        TransportRecordEnvelope::V2(terminal),
        false,
        now + Duration::from_secs(1),
        100_000,
    ));
    let after = diagnostics
        .lock()
        .unwrap()
        .projection(&ticket.key, &ticket.owner, now)
        .unwrap();
    assert_eq!(after.status.state, TransportState::AuthRejected);
    assert_eq!(after.status.code, TransportCode::AuthDenied);
    assert!(after.auth_record.is_some());
}
