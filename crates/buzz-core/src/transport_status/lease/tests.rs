use super::*;
use crate::transport_status::{
    TransportCode, TransportRecordEnvelope, TransportRecordV2, TransportState,
};
use std::time::Duration;

fn record(sequence: u64) -> TransportRecord {
    TransportRecord {
        version: 1,
        runtime_id: "fixture-runtime".into(),
        start_nonce: "fixture-nonce".into(),
        sequence,
        timestamp_ms: 100_000,
        terminal: false,
        transport: TransportStatus {
            state: TransportState::Connected,
            code: TransportCode::None,
            attempts: 0,
            elapsed_ms: 0,
            next_retry_at_ms: None,
            last_error: None,
        },
    }
}

fn record_v2(sequence: u64) -> TransportRecordV2 {
    TransportRecordV2 {
        version: 2,
        runtime_id: "fixture-runtime".into(),
        start_nonce: "fixture-nonce".into(),
        sequence,
        timestamp_ms: 100_000,
        terminal: false,
        transport: record(sequence).transport,
        process_id: 7,
        spawn_started_at_ms: 90_000,
        connection_attempt: None,
        received_auth: None,
    }
}

fn observe(
    lease: &mut TransportLease,
    record: TransportRecord,
    now: Instant,
) -> Result<(), String> {
    lease.observe(
        record,
        "fixture-runtime",
        "fixture-nonce",
        now,
        100_000,
        false,
    )
}

#[test]
fn identical_rereads_do_not_extend_the_monotonic_lease() {
    let now = Instant::now();
    let mut lease = TransportLease::default();
    observe(&mut lease, record(1), now).unwrap();
    observe(&mut lease, record(1), now + Duration::from_secs(14)).unwrap();
    lease.expire(now + Duration::from_secs(15));
    assert_eq!(lease.status().state, TransportState::Unknown);
}

#[test]
fn advancing_sequence_recovers_after_stale_connected_record() {
    let now = Instant::now();
    let mut lease = TransportLease::default();
    observe(&mut lease, record(1), now).unwrap();
    lease.expire(now + Duration::from_secs(16));
    assert_eq!(lease.status().state, TransportState::Unknown);
    observe(&mut lease, record(2), now + Duration::from_secs(17)).unwrap();
    assert_eq!(lease.status().state, TransportState::Connected);
    lease.expire(now + Duration::from_secs(31));
    assert_eq!(lease.status().state, TransportState::Connected);
    lease.expire(now + Duration::from_secs(32));
    assert_eq!(lease.status().state, TransportState::Unknown);
}

#[test]
fn wrong_identity_or_nonce_never_changes_the_projection() {
    let now = Instant::now();
    for field in ["identity", "nonce"] {
        let mut lease = TransportLease::default();
        let mut wrong = record(1);
        if field == "identity" {
            wrong.runtime_id = "other-relay-or-agent".into();
        } else {
            wrong.start_nonce = "previous-start".into();
        }
        assert!(observe(&mut lease, wrong, now).is_err());
        assert_eq!(lease.status().state, TransportState::Unknown);
    }
}

#[test]
fn old_or_changed_same_sequence_is_rejected() {
    let now = Instant::now();
    let mut lease = TransportLease::default();
    observe(&mut lease, record(2), now).unwrap();
    assert!(observe(&mut lease, record(1), now).is_err());
    let mut changed = record(2);
    changed.transport.state = TransportState::Degraded;
    assert!(observe(&mut lease, changed, now).is_err());
    assert_eq!(lease.status().state, TransportState::Unknown);
}

#[test]
fn first_read_rejects_stale_future_and_untrusted_error_payloads() {
    let now = Instant::now();
    for variant in ["stale", "future", "secret", "version", "zero"] {
        let mut lease = TransportLease::default();
        let mut invalid = record(1);
        match variant {
            "stale" => invalid.timestamp_ms = 80_000,
            "future" => invalid.timestamp_ms = 120_000,
            "secret" => invalid.transport.last_error = Some("relay token".into()),
            "version" => invalid.version = 2,
            _ => invalid.sequence = 0,
        }
        assert!(observe(&mut lease, invalid, now).is_err(), "{variant}");
        assert_eq!(lease.status().state, TransportState::Unknown);
    }
}

#[test]
fn retired_read_requires_an_explicit_final_failure_record() {
    let now = Instant::now();
    let mut lease = TransportLease::default();
    assert!(lease
        .observe(
            record(1),
            "fixture-runtime",
            "fixture-nonce",
            now,
            100_000,
            true
        )
        .is_err());
    let mut final_record = record(2);
    final_record.terminal = true;
    final_record.transport.state = TransportState::Exhausted;
    lease
        .observe(
            final_record,
            "fixture-runtime",
            "fixture-nonce",
            now,
            100_000,
            true,
        )
        .unwrap();
    assert_eq!(lease.status().state, TransportState::Exhausted);
}

#[test]
fn read_failure_recovers_without_renewing_same_sequence() {
    let now = Instant::now();
    let mut lease = TransportLease::default();
    observe(&mut lease, record(1), now).unwrap();
    lease.unavailable();
    assert_eq!(lease.status().state, TransportState::Unknown);
    observe(&mut lease, record(1), now + Duration::from_secs(10)).unwrap();
    assert_eq!(lease.status().state, TransportState::Connected);
    lease.expire(now + Duration::from_secs(15));
    assert_eq!(lease.status().state, TransportState::Unknown);
}

#[test]
fn accepted_wire_version_cannot_change_within_one_generation() {
    let now = Instant::now();
    let mut lease = TransportLease::default();
    lease
        .observe_envelope(
            TransportRecordEnvelope::V2(record_v2(1)),
            "fixture-runtime",
            "fixture-nonce",
            now,
            100_000,
            false,
        )
        .unwrap();
    assert!(lease
        .observe(
            record(2),
            "fixture-runtime",
            "fixture-nonce",
            now,
            100_000,
            false,
        )
        .is_err());
    // The v2 writer may continue after the rejected v1 receipt, but the
    // old v2 record cannot be mistaken for a new wire-version negotiation.
    lease
        .observe_envelope(
            TransportRecordEnvelope::V2(record_v2(2)),
            "fixture-runtime",
            "fixture-nonce",
            now + Duration::from_secs(1),
            100_000,
            false,
        )
        .unwrap();
    assert_eq!(lease.status().state, TransportState::Connected);
}
