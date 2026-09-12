use super::*;
use buzz_core_pkg::transport_status::{
    TransportAuthClassification, TransportCode, TransportConnectionAttempt, TransportReceivedAuth,
    TransportRecordEnvelope, TransportRecordV2, TransportState, TransportStatus,
};
use std::io::{self, ErrorKind, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::super::monitor::{Diagnostics, Monitor};

fn ticket() -> ReadTicket {
    ReadTicket {
        key: crate::managed_agents::ManagedAgentRuntimeKey::new("a".repeat(64), "ws://fixture")
            .unwrap(),
        nonce: uuid::Uuid::from_u128(7).to_string(),
        path: PathBuf::from("/fixture/status.json"),
        owner: "b".repeat(64),
        epoch: 0,
        process_id: 7,
        spawn_started_at_ms: 90_000,
        wire_version: 2,
    }
}

fn auth_candidate(ticket: &ReadTicket) -> AuthCandidate {
    let attempt_id = uuid::Uuid::from_u128(8).to_string();
    let record = TransportRecordV2 {
        version: 2,
        runtime_id: ticket.key.runtime_id(),
        start_nonce: ticket.nonce.clone(),
        sequence: 2,
        timestamp_ms: 100_000,
        terminal: true,
        transport: TransportStatus {
            state: TransportState::AuthRejected,
            code: TransportCode::AuthDenied,
            attempts: 1,
            elapsed_ms: 1,
            next_retry_at_ms: None,
            last_error: TransportCode::AuthDenied.message().map(str::to_owned),
        },
        process_id: ticket.process_id,
        spawn_started_at_ms: ticket.spawn_started_at_ms,
        connection_attempt: Some(TransportConnectionAttempt {
            sequence: 1,
            id: attempt_id.clone(),
            started_at_ms: 91_000,
        }),
        received_auth: Some(TransportReceivedAuth {
            auth_event_id: "c".repeat(64),
            attempt_id,
            attempt_sequence: 1,
            accepted: false,
            classification: TransportAuthClassification::CommunityBanned,
            received_at_ms: 92_000,
        }),
    };
    let evidence = ManagedAgentTransportAuthEvidence {
        record,
        native_binding: native_binding(ticket),
        status_path: ticket.path.display().to_string(),
        retired: false,
        failed_exit: false,
        retired_at_ms: None,
    };
    AuthCandidate {
        ticket: ticket.clone(),
        key: ExportKey {
            runtime_id: ticket.key.runtime_id(),
            start_nonce: ticket.nonce.clone(),
            owner: ticket.owner.clone(),
            epoch: ticket.epoch,
            attempt_id: uuid::Uuid::from_u128(8).to_string(),
            attempt_sequence: 1,
            auth_event_id: "c".repeat(64),
            retired: false,
        },
        evidence,
        native_validation_at_ms: 100_000,
    }
}

fn accepted_live_candidate() -> ExportCandidate {
    let ticket = ticket();
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let mut monitor = Monitor::new(ticket.clone(), &diagnostics);
    let now = Instant::now();
    let auth = auth_candidate(&ticket);
    assert!(monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V2(auth.evidence.record.clone())),
        true,
        now,
        100_000,
    ));
    let registration = monitor
        .take_export_candidate_with_enabled(now, true)
        .expect("native registration is due before AUTH evidence");
    assert!(matches!(registration, ExportCandidate::Registration(_)));
    monitor.finish_export(&registration, true, now);
    monitor
        .take_export_candidate_with_enabled(now, true)
        .expect("accepted native AUTH evidence is exportable")
}

#[test]
fn registration_can_export_before_first_health_but_expired_auth_is_fenced() {
    let ticket = ticket();
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let mut monitor = Monitor::new(ticket.clone(), &diagnostics);
    let now = Instant::now();
    let registration = monitor
        .take_export_candidate_with_enabled(now, true)
        .expect("native registration is due before the first health read");
    assert!(matches!(registration, ExportCandidate::Registration(_)));
    assert_eq!(monitor.status().state, TransportState::Unknown);
    assert!(monitor.export_is_current(&registration, now, true));
    monitor.finish_export(&registration, true, now);

    let auth = auth_candidate(&ticket);
    monitor.apply(
        &ticket,
        Ok(TransportRecordEnvelope::V2(auth.evidence.record.clone())),
        true,
        now,
        100_000,
    );
    let auth = monitor
        .take_export_candidate_with_enabled(now, true)
        .expect("accepted AUTH evidence is due after registration");
    assert!(matches!(auth, ExportCandidate::Auth(_)));
    assert!(!monitor.export_is_current(&auth, now + Duration::from_secs(16), true));
    assert!(monitor
        .take_export_candidate_with_enabled(now + Duration::from_secs(16), true)
        .is_none());
}

#[test]
fn disabled_native_export_does_not_take_or_emit_a_candidate() {
    let ticket = ticket();
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let mut monitor = Monitor::new(ticket, &diagnostics);
    assert!(monitor.take_export_candidate(Instant::now()).is_none());
    assert!(!enabled());
    assert!(emit(&accepted_live_candidate()).is_err());
}

#[test]
#[ignore = "requires native export env=1 in an isolated test process"]
fn enabled_native_export_uses_production_entry_point() {
    assert!(enabled());
    let ticket = ticket();
    let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
    let mut monitor = Monitor::new(ticket, &diagnostics);
    let candidate = monitor
        .take_export_candidate(Instant::now())
        .expect("opt-in production export entry point should queue registration");
    assert!(matches!(candidate, ExportCandidate::Registration(_)));
}

#[derive(Default)]
struct CaptureSink {
    bytes: Vec<u8>,
    flushes: usize,
}

impl Write for CaptureSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

struct ShortWriteSink {
    inner: CaptureSink,
    width: usize,
}

impl Write for ShortWriteSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let width = self.width.min(bytes.len());
        self.inner.bytes.extend_from_slice(&bytes[..width]);
        Ok(width)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

struct ErrorSink;

impl Write for ErrorSink {
    fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(ErrorKind::BrokenPipe, "fixture sink failed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct FlushErrorSink {
    inner: CaptureSink,
}

impl Write for FlushErrorSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("fixture flush failed"))
    }
}

#[test]
fn native_acceptance_emits_through_the_production_write_sink() {
    let candidate = accepted_live_candidate();
    let mut sink = CaptureSink::default();
    emit_to(&candidate, &mut sink).unwrap();
    assert!(sink.bytes.starts_with(AUTH_MARKER.as_bytes()));
    assert_eq!(sink.bytes.last(), Some(&b'\n'));
    assert_eq!(sink.flushes, 1);
    serde_json::from_slice::<serde_json::Value>(
        &sink.bytes[AUTH_MARKER.len()..sink.bytes.len() - 1],
    )
    .unwrap();
}

#[test]
fn production_sink_rejects_wrong_native_binding_without_touching_sentinel() {
    let mut candidate = auth_candidate(&ticket());
    candidate.evidence.native_binding.process_id += 1;
    let mut sink = CaptureSink {
        bytes: b"SENTINEL".to_vec(),
        flushes: 0,
    };
    assert!(emit_to(&ExportCandidate::Auth(Box::new(candidate)), &mut sink).is_err());
    assert_eq!(sink.bytes, b"SENTINEL");
    assert_eq!(sink.flushes, 0);
}

#[test]
fn production_sink_propagates_write_and_flush_errors() {
    let candidate = accepted_live_candidate();
    assert!(emit_to(&candidate, ErrorSink).is_err());
    assert!(emit_to(
        &candidate,
        FlushErrorSink {
            inner: CaptureSink::default(),
        }
    )
    .is_err());
}

#[test]
fn production_sink_completes_short_writes_with_one_bounded_marker() {
    let candidate = accepted_live_candidate();
    let mut sink = ShortWriteSink {
        inner: CaptureSink::default(),
        width: 1,
    };
    emit_to(&candidate, &mut sink).unwrap();
    assert!(sink.inner.bytes.starts_with(AUTH_MARKER.as_bytes()));
    assert_eq!(sink.inner.bytes.last(), Some(&b'\n'));
    assert_eq!(sink.inner.flushes, 1);
}

#[test]
fn production_sink_rejects_oversize_marker_before_writing() {
    let mut candidate = auth_candidate(&ticket());
    candidate.evidence.status_path = "x".repeat(MAX_LINE_BYTES);
    let mut sink = CaptureSink {
        bytes: b"SENTINEL".to_vec(),
        flushes: 0,
    };
    assert!(emit_to(&ExportCandidate::Auth(Box::new(candidate)), &mut sink).is_err());
    assert_eq!(sink.bytes, b"SENTINEL");
    assert_eq!(sink.flushes, 0);
}

#[test]
fn registration_retry_blocks_auth_until_registration_succeeds() {
    let now = Instant::now();
    let ticket = ticket();
    let auth = auth_candidate(&ticket);
    let mut state = ExportState::new(ticket, 90_001, now);
    let first = state.take_due(now).expect("registration is initially due");
    assert!(matches!(first, ExportCandidate::Registration(_)));
    state.observe_auth(auth.clone(), now);
    state.finish(&first, false, now);
    assert!(state.take_due(now + Duration::from_secs(1)).is_none());
    let retry = state
        .take_due(now + RETRY_INTERVAL)
        .expect("registration retry is due");
    assert!(matches!(retry, ExportCandidate::Registration(_)));
    state.finish(&retry, true, now + RETRY_INTERVAL);
    state.observe_auth(auth, now + RETRY_INTERVAL);
    assert!(matches!(
        state.take_due(now + RETRY_INTERVAL),
        Some(ExportCandidate::Auth(_))
    ));
}

#[test]
fn live_renewals_deduplicate_and_retired_final_phase_can_emit() {
    let now = Instant::now();
    let ticket = ticket();
    let mut state = ExportState::new(ticket.clone(), 90_001, now);
    let registration = state.take_due(now).unwrap();
    state.finish(&registration, true, now);

    let live = auth_candidate(&ticket);
    state.observe_auth(live.clone(), now);
    let live_marker = state.take_due(now).unwrap();
    state.finish(&live_marker, true, now);

    let mut renewal = live.clone();
    renewal.evidence.record.sequence = 3;
    renewal.native_validation_at_ms += 1;
    state.observe_auth(renewal, now + Duration::from_secs(1));
    assert!(state.take_due(now + Duration::from_secs(1)).is_none());

    let mut final_candidate = live;
    final_candidate.key.retired = true;
    final_candidate.evidence.retired = true;
    final_candidate.evidence.failed_exit = true;
    final_candidate.evidence.retired_at_ms = Some(100_001);
    state.observe_auth(final_candidate, now + Duration::from_secs(2));
    assert!(matches!(
        state.take_due(now + Duration::from_secs(2)),
        Some(ExportCandidate::Auth(candidate)) if candidate.key.retired
    ));
}

#[test]
fn reenable_requeues_registration_disable_cancels_pending_and_new_attempt_supersedes_old() {
    let now = Instant::now();
    let ticket = ticket();
    let mut state = ExportState::new(ticket.clone(), 90_001, now);
    let _registration = state.take_due(now).unwrap();
    state.disable();
    assert!(state.take_due(now).is_none());

    state.rebind(ticket.clone(), 90_002, now);
    let registration = state.take_due(now).unwrap();
    state.finish(&registration, true, now);

    let first = auth_candidate(&ticket);
    state.observe_auth(first, now);
    let old = state.take_due(now).unwrap();
    let mut superseding = auth_candidate(&ticket);
    let superseding_attempt_id = uuid::Uuid::from_u128(9).to_string();
    let superseding_auth_event_id = "d".repeat(64);
    superseding.key.attempt_id = superseding_attempt_id.clone();
    superseding.key.attempt_sequence = 2;
    superseding.key.auth_event_id = superseding_auth_event_id.clone();
    if let Some(attempt) = superseding.evidence.record.connection_attempt.as_mut() {
        attempt.id = superseding_attempt_id.clone();
        attempt.sequence = 2;
    }
    if let Some(received) = superseding.evidence.record.received_auth.as_mut() {
        received.attempt_id = superseding_attempt_id;
        received.attempt_sequence = 2;
        received.auth_event_id = superseding_auth_event_id;
    }
    superseding.key.retired = true;
    superseding.evidence.retired = true;
    superseding.evidence.failed_exit = true;
    superseding.evidence.retired_at_ms = Some(100_001);
    state.observe_auth(superseding, now);
    assert!(!state.is_pending(&old));
    assert!(matches!(
        state.take_due(now),
        Some(ExportCandidate::Auth(candidate)) if candidate.key.retired
    ));
}

#[test]
fn cleared_auth_receipt_cannot_be_replayed() {
    let now = Instant::now();
    let ticket = ticket();
    let auth = auth_candidate(&ticket);
    let mut state = ExportState::new(ticket, 90_001, now);
    let registration = state.take_due(now).unwrap();
    state.finish(&registration, true, now);
    state.observe_auth(auth.clone(), now);
    let pending = state.take_due(now).unwrap();
    state.finish(&pending, false, now);
    state.clear_auth();
    state.observe_auth(auth, now + RETRY_INTERVAL);
    assert!(state.take_due(now + RETRY_INTERVAL).is_none());
}
