//! Bounded, opt-in native diagnostics for transport provenance.
//!
//! The child record is still the source of health state. This module only
//! formats values that the native monitor has already accepted under an exact
//! generation ticket; it never parses caller-provided JSON.

use std::io::Write;
use std::time::{Duration, Instant};

use buzz_core_pkg::transport_status::{TransportCode, TransportRecordV2, TransportState};

use super::monitor::{DiagnosticsProjection, Monitor, ReadTicket};
use crate::managed_agents::{
    ManagedAgentTransportAuthEvidence, ManagedAgentTransportNativeBinding,
};

pub(crate) const NATIVE_AUTH_EVIDENCE_EXPORT_ENV: &str = "CREW_NATIVE_AUTH_EVIDENCE_EXPORT";
const AUTH_MARKER: &str = "CREW_NATIVE_AUTH_EVIDENCE_V1 ";
const REGISTRATION_MARKER: &str = "CREW_NATIVE_TRANSPORT_REGISTRATION_V1 ";
const MAX_LINE_BYTES: usize = 4096;
const MAX_ATTEMPTS: u8 = 3;
const RETRY_INTERVAL: Duration = Duration::from_secs(5);
const EXPORT_ERROR: &str = "native transport evidence export failed";

/// One marker's deduplication identity. Record renewal sequences are
/// intentionally absent: a renewal must not produce another marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportKey {
    pub runtime_id: String,
    pub start_nonce: String,
    pub owner: String,
    pub epoch: u64,
    pub attempt_id: String,
    pub attempt_sequence: u64,
    pub auth_event_id: String,
    pub retired: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct RegistrationCandidate {
    pub ticket: ReadTicket,
    pub native_registered_at_ms: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct AuthCandidate {
    pub ticket: ReadTicket,
    pub key: ExportKey,
    pub evidence: ManagedAgentTransportAuthEvidence,
    pub native_validation_at_ms: u64,
}

#[derive(Debug, Clone)]
pub(crate) enum ExportCandidate {
    Registration(RegistrationCandidate),
    Auth(AuthCandidate),
}

#[derive(Debug, Clone)]
struct RegistrationTrack {
    candidate: RegistrationCandidate,
    attempts: u8,
    next_retry_at: Instant,
    emitted: bool,
}

#[derive(Debug, Clone)]
struct AuthTrack {
    candidate: AuthCandidate,
    attempts: u8,
    next_retry_at: Instant,
    emitted: bool,
}

#[derive(Debug, Clone)]
struct AuthFence {
    key: ExportKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingMarker {
    Registration,
    Auth(ExportKey),
}

/// Per-generation export state. There is one pending marker slot; a newer
/// attempt supersedes an un-emitted live receipt instead of growing a queue.
#[derive(Debug, Default)]
pub(crate) struct ExportState {
    registration: Option<RegistrationTrack>,
    auth: Option<AuthTrack>,
    auth_fence: Option<AuthFence>,
    pending: Option<PendingMarker>,
}

impl ExportState {
    pub(crate) fn new(ticket: ReadTicket, native_registered_at_ms: u64, now: Instant) -> Self {
        Self {
            registration: Some(RegistrationTrack {
                candidate: RegistrationCandidate {
                    ticket,
                    native_registered_at_ms,
                },
                attempts: 0,
                next_retry_at: now,
                emitted: false,
            }),
            auth: None,
            auth_fence: None,
            pending: None,
        }
    }

    pub(crate) fn rebind(
        &mut self,
        ticket: ReadTicket,
        native_registered_at_ms: u64,
        now: Instant,
    ) {
        *self = Self::new(ticket, native_registered_at_ms, now);
    }

    pub(crate) fn disable(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn clear_auth(&mut self) {
        if let Some(track) = self.auth.take() {
            self.auth_fence = Some(AuthFence {
                key: track.candidate.key,
            });
        }
        if matches!(self.pending, Some(PendingMarker::Auth(_))) {
            self.pending = None;
        }
    }

    pub(crate) fn is_pending(&self, candidate: &ExportCandidate) -> bool {
        match (self.pending.as_ref(), candidate) {
            (Some(PendingMarker::Registration), ExportCandidate::Registration(candidate)) => self
                .registration
                .as_ref()
                .is_some_and(|track| track.candidate.ticket == candidate.ticket),
            (Some(PendingMarker::Auth(key)), ExportCandidate::Auth(candidate)) => {
                key == &candidate.key
                    && self.auth.as_ref().is_some_and(|track| {
                        track.candidate.ticket == candidate.ticket
                            && track.candidate.key == candidate.key
                    })
            }
            _ => false,
        }
    }

    pub(crate) fn observe_auth(&mut self, candidate: AuthCandidate, now: Instant) {
        // The native registration marker is the provenance prefix for every
        // ACK marker. Keep the source record in Monitor until registration has
        // emitted; do not queue a second full line alongside it.
        if self
            .registration
            .as_ref()
            .is_some_and(|track| !track.emitted)
        {
            return;
        }
        if let Some(fence) = self.auth_fence.as_ref() {
            if fence.key == candidate.key {
                // A visibility loss keeps this immutable identity fenced
                // rather than resurrecting a stale full payload.
                return;
            }
        }
        if self
            .auth
            .as_ref()
            .is_some_and(|track| track.emitted && track.candidate.key == candidate.key)
        {
            return;
        }
        if self.pending.as_ref().is_some_and(
            |pending| matches!(pending, PendingMarker::Auth(key) if key != &candidate.key),
        ) {
            self.pending = None;
        }
        if let Some(track) = self
            .auth
            .as_mut()
            .filter(|track| track.candidate.key == candidate.key)
        {
            // Renewals replace the immutable record snapshot and native
            // validation timestamp while preserving the attempt's emission
            // budget and deduplication identity.
            track.candidate = candidate;
            return;
        }
        self.auth_fence = None;
        self.auth = Some(AuthTrack {
            candidate,
            attempts: 0,
            next_retry_at: now,
            emitted: false,
        });
    }

    pub(crate) fn take_due(&mut self, now: Instant) -> Option<ExportCandidate> {
        if self.pending.is_some() {
            return None;
        }
        if let Some(registration) = self.registration.as_mut().filter(|track| {
            !track.emitted && track.attempts < MAX_ATTEMPTS && now >= track.next_retry_at
        }) {
            registration.attempts = registration.attempts.saturating_add(1);
            self.pending = Some(PendingMarker::Registration);
            return Some(ExportCandidate::Registration(
                registration.candidate.clone(),
            ));
        }
        if self
            .registration
            .as_ref()
            .is_some_and(|track| !track.emitted)
        {
            return None;
        }
        let auth = self.auth.as_mut().filter(|track| {
            !track.emitted && track.attempts < MAX_ATTEMPTS && now >= track.next_retry_at
        })?;
        auth.attempts = auth.attempts.saturating_add(1);
        self.pending = Some(PendingMarker::Auth(auth.candidate.key.clone()));
        Some(ExportCandidate::Auth(auth.candidate.clone()))
    }

    pub(crate) fn finish(&mut self, candidate: &ExportCandidate, success: bool, now: Instant) {
        let pending = self.pending.clone();
        match candidate {
            ExportCandidate::Registration(candidate)
                if pending == Some(PendingMarker::Registration)
                    && self
                        .registration
                        .as_ref()
                        .is_some_and(|track| track.candidate.ticket == candidate.ticket) =>
            {
                if success {
                    if let Some(track) = self.registration.as_mut() {
                        track.emitted = true;
                    }
                    self.pending = None;
                } else if let Some(track) = self.registration.as_mut() {
                    self.pending = None;
                    if track.attempts < MAX_ATTEMPTS {
                        track.next_retry_at = now + RETRY_INTERVAL;
                    }
                }
            }
            ExportCandidate::Auth(candidate)
                if pending == Some(PendingMarker::Auth(candidate.key.clone()))
                    && self.auth.as_ref().is_some_and(|track| {
                        track.candidate.key == candidate.key
                            && track.candidate.ticket == candidate.ticket
                    }) =>
            {
                if success {
                    if let Some(track) = self.auth.as_mut() {
                        track.emitted = true;
                    }
                    self.pending = None;
                } else if let Some(track) = self.auth.as_mut() {
                    self.pending = None;
                    if track.attempts < MAX_ATTEMPTS {
                        track.next_retry_at = now + RETRY_INTERVAL;
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn enabled() -> bool {
    std::env::var(NATIVE_AUTH_EVIDENCE_EXPORT_ENV).is_ok_and(|value| value == "1")
}

fn native_binding(ticket: &ReadTicket) -> ManagedAgentTransportNativeBinding {
    ManagedAgentTransportNativeBinding {
        runtime_id: ticket.key.runtime_id(),
        start_nonce: ticket.nonce.clone(),
        process_id: ticket.process_id,
        spawn_started_at_ms: ticket.spawn_started_at_ms,
        owner: ticket.owner.clone(),
        epoch: ticket.epoch,
    }
}

pub(crate) fn auth_evidence_from_record(
    record: &TransportRecordV2,
    ticket: &ReadTicket,
    retired: bool,
    failed_exit: bool,
    retired_at_ms: Option<u64>,
) -> Option<ManagedAgentTransportAuthEvidence> {
    if record.validate_shape().is_err()
        || record.runtime_id != ticket.key.runtime_id()
        || record.start_nonce != ticket.nonce
        || record.process_id != ticket.process_id
        || record.spawn_started_at_ms != ticket.spawn_started_at_ms
        || record.received_auth.is_none()
        || record.transport.state != TransportState::AuthRejected
        || record.transport.code != TransportCode::AuthDenied
        || (retired != failed_exit)
        || (retired && retired_at_ms.is_none())
        || (!retired && retired_at_ms.is_some())
    {
        return None;
    }
    Some(ManagedAgentTransportAuthEvidence {
        record: record.clone(),
        native_binding: native_binding(ticket),
        status_path: ticket.path.display().to_string(),
        retired,
        failed_exit,
        retired_at_ms,
    })
}

pub(crate) fn live_auth_evidence(monitor: &Monitor) -> Option<ManagedAgentTransportAuthEvidence> {
    monitor
        .auth_record()
        .and_then(|record| auth_evidence_from_record(record, monitor.ticket(), false, false, None))
}

pub(crate) fn retired_auth_evidence(
    projection: &DiagnosticsProjection,
) -> Option<ManagedAgentTransportAuthEvidence> {
    projection.auth_record.as_ref().and_then(|record| {
        auth_evidence_from_record(
            record,
            &projection.ticket,
            true,
            projection.failed_exit,
            Some(projection.retired_at_ms),
        )
    })
}

fn validate_registration(candidate: &RegistrationCandidate) -> Result<(), ()> {
    let ticket = &candidate.ticket;
    if candidate.native_registered_at_ms == 0
        || ticket.process_id == 0
        || ticket.spawn_started_at_ms == 0
        || ticket.owner.len() != 64
        || !ticket.owner.bytes().all(|byte| byte.is_ascii_hexdigit())
        || ticket.path.as_os_str().len() > 4096
        || !ticket.path.is_absolute()
    {
        return Err(());
    }
    Ok(())
}

fn validate_auth(candidate: &AuthCandidate) -> Result<(), ()> {
    let evidence = &candidate.evidence;
    let ticket = &candidate.ticket;
    if candidate.native_validation_at_ms == 0
        || evidence.retired != candidate.key.retired
        || evidence.native_binding.runtime_id != ticket.key.runtime_id()
        || evidence.native_binding.start_nonce != ticket.nonce
        || evidence.native_binding.process_id != ticket.process_id
        || evidence.native_binding.spawn_started_at_ms != ticket.spawn_started_at_ms
        || evidence.native_binding.owner != ticket.owner
        || evidence.native_binding.epoch != ticket.epoch
        || evidence.record.received_auth.is_none()
        || evidence.record.validate_shape().is_err()
    {
        return Err(());
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RegistrationLine<'a> {
    version: u32,
    native_registered_at_ms: u64,
    native_binding: &'a ManagedAgentTransportNativeBinding,
    status_path: &'a str,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthLine<'a> {
    version: u32,
    native_validation_at_ms: u64,
    evidence: &'a ManagedAgentTransportAuthEvidence,
}

fn line_for(candidate: &ExportCandidate) -> Result<Vec<u8>, String> {
    let (prefix, body) = match candidate {
        ExportCandidate::Registration(candidate) => {
            validate_registration(candidate).map_err(|_| EXPORT_ERROR)?;
            let binding = native_binding(&candidate.ticket);
            let body = serde_json::to_vec(&RegistrationLine {
                version: 1,
                native_registered_at_ms: candidate.native_registered_at_ms,
                native_binding: &binding,
                status_path: &candidate.ticket.path.to_string_lossy(),
            })
            .map_err(|_| EXPORT_ERROR)?;
            (REGISTRATION_MARKER, body)
        }
        ExportCandidate::Auth(candidate) => {
            validate_auth(candidate).map_err(|_| EXPORT_ERROR)?;
            let body = serde_json::to_vec(&AuthLine {
                version: 1,
                native_validation_at_ms: candidate.native_validation_at_ms,
                evidence: &candidate.evidence,
            })
            .map_err(|_| EXPORT_ERROR)?;
            (AUTH_MARKER, body)
        }
    };
    let mut line = Vec::with_capacity(prefix.len() + body.len() + 1);
    line.extend_from_slice(prefix.as_bytes());
    line.extend_from_slice(&body);
    line.push(b'\n');
    if line.len() > MAX_LINE_BYTES {
        return Err(EXPORT_ERROR.into());
    }
    Ok(line)
}

pub(crate) fn emit(candidate: &ExportCandidate) -> Result<(), String> {
    if !enabled() {
        return Err(EXPORT_ERROR.into());
    }
    let stderr = std::io::stderr();
    let mut sink = stderr.lock();
    emit_to(candidate, &mut sink)
}

/// Write one already-authorized marker through the production sink path.
/// Keeping the sink as an argument lets tests exercise the exact bounded
/// write and flush behavior without redirecting the process's real stderr.
pub(crate) fn emit_to<W: Write>(candidate: &ExportCandidate, mut sink: W) -> Result<(), String> {
    let line = line_for(candidate)?;
    write_line(&line, &mut sink)
}

fn write_line<W: Write>(line: &[u8], sink: &mut W) -> Result<(), String> {
    sink.write_all(line).map_err(|_| EXPORT_ERROR)?;
    sink.flush().map_err(|_| EXPORT_ERROR.to_owned())
}

#[cfg(test)]
mod tests;
