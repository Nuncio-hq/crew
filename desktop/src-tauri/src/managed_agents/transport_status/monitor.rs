use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::managed_agents::ManagedAgentRuntimeKey;
use buzz_core_pkg::transport_status::{
    TransportConnectionAttempt, TransportLease, TransportReceivedAuth, TransportRecordEnvelope,
    TransportRecordV2, TransportStatus,
};

use super::export::{auth_evidence_from_record, AuthCandidate, ExportCandidate, ExportState};

const RETIRED_CAPACITY: usize = 256;
const RETIRED_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const FINAL_READ_WINDOW: Duration = Duration::from_secs(15);
const WALL_CLOCK_SKEW_MS: u64 = 5_000;

fn current_wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

fn validate_v2_record(
    record: &TransportRecordEnvelope,
    ticket: &ReadTicket,
    wall_ms: u64,
    retirement_upper_bound_ms: Option<u64>,
    last_attempt: &Option<TransportConnectionAttempt>,
    last_auth: &Option<TransportReceivedAuth>,
    auth_closed: bool,
) -> Result<(), String> {
    let Some(record) = record.v2() else {
        return Ok(());
    };
    if ticket.wire_version != 2 {
        return Err("local transport v2 record is not enabled for this generation".into());
    }
    record.validate_shape()?;
    if record.process_id != ticket.process_id
        || record.spawn_started_at_ms != ticket.spawn_started_at_ms
    {
        return Err("local transport process binding does not match registration".into());
    }
    if record.timestamp_ms > wall_ms.saturating_add(WALL_CLOCK_SKEW_MS) {
        return Err("local transport v2 timestamp is in the future".into());
    }
    if retirement_upper_bound_ms.is_some_and(|upper| record.timestamp_ms > upper) {
        return Err("local transport v2 record was written after retirement".into());
    }
    if let Some(attempt) = &record.connection_attempt {
        if let Some(previous) = last_attempt {
            if attempt.sequence < previous.sequence
                || (attempt.sequence == previous.sequence && attempt != previous)
            {
                return Err("local transport connection attempt is stale".into());
            }
        }
        if let Some(auth) = &record.received_auth {
            if auth_closed && last_attempt.as_ref() == Some(attempt) {
                return Err("local transport AUTH evidence was already retired".into());
            }
            if last_attempt.as_ref() == Some(attempt)
                && last_auth.as_ref().is_some_and(|previous| previous != auth)
            {
                return Err("local transport AUTH evidence changed for an attempt".into());
            }
        }
    } else if last_attempt.is_some() {
        return Err("local transport connection attempt regressed".into());
    } else if record.received_auth.is_some() {
        return Err("local transport AUTH evidence has no attempt".into());
    }
    Ok(())
}

fn is_denial_evidence(record: &TransportRecordV2) -> bool {
    record.received_auth.is_some()
        && record.transport.state == buzz_core_pkg::transport_status::TransportState::AuthRejected
        && record.transport.code == buzz_core_pkg::transport_status::TransportCode::AuthDenied
}

/// Read capability captured from an existing registered generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadTicket {
    pub key: ManagedAgentRuntimeKey,
    pub nonce: String,
    pub path: PathBuf,
    pub owner: String,
    pub epoch: u64,
    pub process_id: u32,
    pub spawn_started_at_ms: u64,
    pub wire_version: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct DiagnosticsProjection {
    pub status: TransportStatus,
    pub failed_exit: bool,
    pub ticket: ReadTicket,
    pub auth_record: Option<TransportRecordV2>,
    pub retired_at_ms: u64,
}

#[derive(Debug)]
pub(crate) struct Monitor {
    ticket: ReadTicket,
    enabled: bool,
    lease: TransportLease,
    diagnostics: Weak<Mutex<Diagnostics>>,
    last_attempt: Option<TransportConnectionAttempt>,
    last_auth: Option<TransportReceivedAuth>,
    auth_closed: bool,
    auth_record: Option<TransportRecordV2>,
    auth_visible: bool,
    auth_validated_at_ms: Option<u64>,
    export: ExportState,
}

impl Monitor {
    pub fn new(ticket: ReadTicket, diagnostics: &Arc<Mutex<Diagnostics>>) -> Self {
        let now = Instant::now();
        let registered_at_ms = current_wall_ms().max(1);
        Self {
            export: ExportState::new(ticket.clone(), registered_at_ms, now),
            ticket,
            enabled: true,
            lease: TransportLease::default(),
            diagnostics: Arc::downgrade(diagnostics),
            last_attempt: None,
            last_auth: None,
            auth_closed: false,
            auth_record: None,
            auth_visible: false,
            auth_validated_at_ms: None,
        }
    }

    pub fn snapshot(&self) -> Option<ReadTicket> {
        self.enabled.then(|| self.ticket.clone())
    }

    pub fn status(&self) -> &TransportStatus {
        self.lease.status()
    }

    pub fn ticket(&self) -> &ReadTicket {
        &self.ticket
    }

    pub fn take_export_candidate(&mut self, now: Instant) -> Option<ExportCandidate> {
        self.take_export_candidate_with_enabled(now, super::export::enabled())
    }

    /// Internal seam for tests and the production wrapper's eligibility gate.
    pub(crate) fn take_export_candidate_with_enabled(
        &mut self,
        now: Instant,
        export_enabled: bool,
    ) -> Option<ExportCandidate> {
        if !export_enabled {
            return None;
        }
        if let Some(record) = self.auth_record().cloned() {
            if let (Some(native_validation_at_ms), Some(evidence)) = (
                self.auth_validated_at_ms,
                auth_evidence_from_record(&record, &self.ticket, false, false, None),
            ) {
                if let (Some(received), Some(attempt)) = (
                    evidence.record.received_auth.as_ref(),
                    evidence.record.connection_attempt.as_ref(),
                ) {
                    self.export.observe_auth(
                        AuthCandidate {
                            ticket: self.ticket.clone(),
                            key: super::export::ExportKey {
                                runtime_id: self.ticket.key.runtime_id(),
                                start_nonce: self.ticket.nonce.clone(),
                                owner: self.ticket.owner.clone(),
                                epoch: self.ticket.epoch,
                                attempt_id: attempt.id.clone(),
                                attempt_sequence: attempt.sequence,
                                auth_event_id: received.auth_event_id.clone(),
                                retired: false,
                            },
                            evidence,
                            native_validation_at_ms,
                        },
                        now,
                    );
                }
            }
        } else {
            self.export.clear_auth();
        }
        self.export.take_due(now)
    }

    pub fn finish_export(&mut self, candidate: &ExportCandidate, success: bool, now: Instant) {
        self.export.finish(candidate, success, now);
    }

    pub fn export_is_current(
        &mut self,
        candidate: &ExportCandidate,
        now: Instant,
        registered_child: bool,
    ) -> bool {
        if !self.enabled || !self.export.is_pending(candidate) {
            return false;
        }
        if !registered_child {
            if matches!(candidate, ExportCandidate::Auth(_)) {
                self.clear_auth_visibility();
            } else {
                self.export.finish(candidate, false, now);
            }
            return false;
        }
        if matches!(candidate, ExportCandidate::Auth(_)) {
            self.lease.expire(now);
            if self.lease.status().state == buzz_core_pkg::transport_status::TransportState::Unknown
            {
                self.clear_auth_visibility();
                return false;
            }
        }
        true
    }

    pub fn auth_record(&self) -> Option<&TransportRecordV2> {
        (self.auth_visible
            && self.lease.status().state
                == buzz_core_pkg::transport_status::TransportState::AuthRejected
            && self.lease.status().code
                == buzz_core_pkg::transport_status::TransportCode::AuthDenied)
            .then_some(())
            .and(self.auth_record.as_ref())
    }

    pub fn inspection_failed(&mut self) {
        self.lease.unavailable();
        self.clear_auth_visibility();
    }

    pub fn expire(&mut self, now: Instant) -> bool {
        let previous = self.lease.status().clone();
        self.lease.expire(now);
        if self.lease.status().state == buzz_core_pkg::transport_status::TransportState::Unknown {
            self.clear_auth_visibility();
        }
        previous != *self.lease.status()
    }

    pub fn disable(&mut self) -> bool {
        if !self.enabled {
            return false;
        }
        self.enabled = false;
        self.ticket.epoch = self.ticket.epoch.saturating_add(1);
        self.lease.unavailable();
        self.clear_auth_visibility();
        self.export.disable();
        true
    }

    pub fn enable(&mut self, owner: &str) -> Result<bool, String> {
        if self.ticket.owner != owner {
            return Err("transport generation belongs to another native owner".into());
        }
        let changed = !self.enabled;
        if changed {
            self.ticket.epoch = self
                .ticket
                .epoch
                .checked_add(1)
                .ok_or("transport eligibility generation exhausted")?;
            self.enabled = true;
            self.lease.unavailable();
            self.export.rebind(
                self.ticket.clone(),
                current_wall_ms().max(1),
                Instant::now(),
            );
        }
        Ok(changed)
    }

    pub fn belongs_to(&self, owner: &str, relay_url: &str) -> bool {
        self.ticket.owner == owner && self.ticket.key.relay_url == relay_url
    }

    pub fn apply(
        &mut self,
        ticket: &ReadTicket,
        record: Result<TransportRecordEnvelope, String>,
        registered_alive: bool,
        now: Instant,
        wall_ms: u64,
    ) -> bool {
        if !self.enabled || ticket != &self.ticket {
            return false;
        }
        let previous = self.lease.status().clone();
        if !registered_alive {
            self.lease.unavailable();
            self.clear_auth_visibility();
            return previous != *self.lease.status();
        }
        if record
            .as_ref()
            .ok()
            .is_some_and(|record| record.version() == 2 && self.ticket.wire_version != 2)
        {
            self.lease.unavailable();
            self.clear_auth_visibility();
            return previous != *self.lease.status();
        }
        match record {
            Ok(record) => {
                if self.validate_v2(&record, wall_ms).is_err()
                    || self
                        .lease
                        .observe_envelope(
                            record.clone(),
                            &self.ticket.key.runtime_id(),
                            &self.ticket.nonce,
                            now,
                            wall_ms,
                            false,
                        )
                        .is_err()
                {
                    self.lease.unavailable();
                    self.clear_auth_visibility();
                } else if self.lease.status().state
                    == buzz_core_pkg::transport_status::TransportState::Unknown
                {
                    // A same-sequence read after the monotonic lease expired is
                    // accepted for sequence bookkeeping but must not revive
                    // derived AUTH evidence without a fresh writer advance.
                    self.clear_auth_visibility();
                } else {
                    self.accept_v2(record, wall_ms);
                }
            }
            Err(_) => {
                self.lease.unavailable();
                self.clear_auth_visibility();
            }
        }
        previous != *self.lease.status()
    }

    pub fn retire(&mut self, failed_exit: bool, now: Instant) {
        if !self.enabled {
            return;
        }
        if let Some(cache) = self.diagnostics.upgrade() {
            // Derived cache mutations remain validated by exact ticket on apply.
            // Recovering a poisoned diagnostic lock cannot establish a process.
            let mut cache = cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let auth_record = if failed_exit {
                self.auth_record().cloned()
            } else {
                None
            };
            cache.retire(
                self.ticket.clone(),
                failed_exit,
                now,
                std::mem::take(&mut self.lease),
                auth_record,
                self.auth_validated_at_ms,
                self.last_attempt.clone(),
                self.last_auth.clone(),
                self.auth_closed,
                std::mem::take(&mut self.export),
            );
        }
    }

    fn validate_v2(&self, record: &TransportRecordEnvelope, wall_ms: u64) -> Result<(), String> {
        validate_v2_record(
            record,
            &self.ticket,
            wall_ms,
            None,
            &self.last_attempt,
            &self.last_auth,
            self.auth_closed,
        )
    }

    fn clear_auth_visibility(&mut self) {
        self.auth_visible = false;
        self.auth_record = None;
        self.auth_validated_at_ms = None;
        self.export.clear_auth();
    }

    fn accept_v2(&mut self, record: TransportRecordEnvelope, wall_ms: u64) {
        let Some(record) = record.v2() else {
            self.clear_auth_visibility();
            return;
        };
        if let Some(attempt) = &record.connection_attempt {
            if self.last_attempt.as_ref() != Some(attempt) {
                self.auth_closed = false;
                self.last_auth = None;
                self.clear_auth_visibility();
                // AUTH evidence belongs to the attempt that produced it. A
                // new attempt must start with no receipt, even when its first
                // health snapshot has not received an ACK yet.
                self.auth_record = None;
            }
            self.last_attempt = Some(attempt.clone());
        }
        // A successful health snapshot is the writer's reset boundary. Keep
        // the attempt identity so a later snapshot cannot regress to `None`,
        // but consume every receipt and fence replay of the prior denial.
        let success_reset = record.transport.state
            == buzz_core_pkg::transport_status::TransportState::Connected
            && record.transport.code == buzz_core_pkg::transport_status::TransportCode::None;
        if success_reset {
            self.last_auth = None;
            self.auth_closed = true;
            self.clear_auth_visibility();
            return;
        }
        match &record.received_auth {
            Some(auth) => {
                self.last_auth = Some(auth.clone());
                self.auth_closed = false;
                let denial = is_denial_evidence(record);
                if !denial {
                    self.clear_auth_visibility();
                }
                self.auth_record = denial.then(|| record.clone());
                self.auth_visible = denial;
                if denial {
                    self.auth_validated_at_ms = Some(wall_ms);
                }
            }
            None => {
                let had_receipt =
                    self.auth_closed || self.last_auth.is_some() || self.auth_record.is_some();
                self.auth_closed = had_receipt;
                self.clear_auth_visibility();
            }
        }
    }
}

#[derive(Debug)]
struct Retired {
    ticket: ReadTicket,
    retired_at: Instant,
    retired_at_ms: u64,
    failed_exit: bool,
    lease: TransportLease,
    final_applied: bool,
    auth_record: Option<TransportRecordV2>,
    auth_validated_at_ms: Option<u64>,
    last_attempt: Option<TransportConnectionAttempt>,
    last_auth: Option<TransportReceivedAuth>,
    auth_closed: bool,
    export: ExportState,
}

/// Bounded derived diagnostics; never a source of active process membership.
#[derive(Debug, Default)]
pub(crate) struct Diagnostics {
    entries: HashMap<ManagedAgentRuntimeKey, Retired>,
}

impl Diagnostics {
    #[allow(clippy::too_many_arguments)]
    fn retire(
        &mut self,
        ticket: ReadTicket,
        failed_exit: bool,
        now: Instant,
        mut lease: TransportLease,
        auth_record: Option<TransportRecordV2>,
        auth_validated_at_ms: Option<u64>,
        last_attempt: Option<TransportConnectionAttempt>,
        last_auth: Option<TransportReceivedAuth>,
        auth_closed: bool,
        export: ExportState,
    ) {
        self.prune(now);
        self.entries.remove(&ticket.key);
        if self.entries.len() >= RETIRED_CAPACITY {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.retired_at)
                .map(|(key, _)| key.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        lease.unavailable();
        self.entries.insert(
            ticket.key.clone(),
            Retired {
                ticket,
                retired_at: now,
                retired_at_ms: current_wall_ms(),
                failed_exit,
                lease,
                final_applied: false,
                auth_record,
                auth_validated_at_ms,
                last_attempt,
                last_auth,
                auth_closed,
                export,
            },
        );
    }

    pub fn protected_nonce(&self, key: &ManagedAgentRuntimeKey, now: Instant) -> Option<String> {
        self.entries
            .get(key)
            .filter(|entry| now.saturating_duration_since(entry.retired_at) < RETIRED_TTL)
            .map(|entry| entry.ticket.nonce.clone())
    }

    pub fn clear_key(&mut self, key: &ManagedAgentRuntimeKey) {
        self.entries.remove(key);
    }

    pub fn clear_pubkey(&mut self, pubkey: &str) {
        self.entries
            .retain(|key, _| !key.pubkey.eq_ignore_ascii_case(pubkey));
    }

    pub fn clear_owner_relay(&mut self, owner: &str, relay_url: &str) {
        self.entries
            .retain(|key, entry| entry.ticket.owner != owner || key.relay_url != relay_url);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn keys_for_owner(&self, owner: &str, now: Instant) -> Vec<ManagedAgentRuntimeKey> {
        self.entries
            .iter()
            .filter(|(_, entry)| {
                entry.ticket.owner == owner
                    && now.saturating_duration_since(entry.retired_at) < RETIRED_TTL
            })
            .map(|(key, _)| key.clone())
            .collect()
    }

    pub fn prune(&mut self, now: Instant) {
        self.entries
            .retain(|_, entry| now.saturating_duration_since(entry.retired_at) < RETIRED_TTL);
    }

    pub fn pending(&self, now: Instant) -> Vec<ReadTicket> {
        self.entries
            .values()
            .filter(|entry| {
                !entry.final_applied
                    && now.saturating_duration_since(entry.retired_at) < FINAL_READ_WINDOW
            })
            .map(|entry| entry.ticket.clone())
            .collect()
    }

    pub fn export_tickets(&self, owner: &str, now: Instant) -> Vec<ReadTicket> {
        self.entries
            .values()
            .filter(|entry| {
                entry.failed_exit
                    && entry.final_applied
                    && entry.ticket.owner == owner
                    && now.saturating_duration_since(entry.retired_at) < RETIRED_TTL
            })
            .map(|entry| entry.ticket.clone())
            .collect()
    }

    pub fn projection(
        &self,
        key: &ManagedAgentRuntimeKey,
        owner: &str,
        now: Instant,
    ) -> Option<DiagnosticsProjection> {
        let entry = self.entries.get(key)?;
        (entry.ticket.owner == owner
            && now.saturating_duration_since(entry.retired_at) < RETIRED_TTL)
            .then(|| DiagnosticsProjection {
                status: entry.lease.status().clone(),
                failed_exit: entry.failed_exit,
                ticket: entry.ticket.clone(),
                auth_record: (entry.final_applied
                    && entry.failed_exit
                    && entry.lease.status().state
                        == buzz_core_pkg::transport_status::TransportState::AuthRejected
                    && entry.lease.status().code
                        == buzz_core_pkg::transport_status::TransportCode::AuthDenied)
                    .then(|| entry.auth_record.clone())
                    .flatten(),
                retired_at_ms: entry.retired_at_ms,
            })
    }

    pub fn apply(
        &mut self,
        ticket: &ReadTicket,
        record: TransportRecordEnvelope,
        newer_registered: bool,
        now: Instant,
        wall_ms: u64,
    ) -> bool {
        let Some(entry) = self.entries.get_mut(&ticket.key) else {
            return false;
        };
        if newer_registered
            || &entry.ticket != ticket
            || entry.final_applied
            || now.saturating_duration_since(entry.retired_at) >= FINAL_READ_WINDOW
        {
            return false;
        }
        let previous = entry.lease.status().clone();
        if record.version() == 2 && ticket.wire_version != 2 {
            entry.lease.unavailable();
            return previous != *entry.lease.status();
        }
        if record.version() != 1
            && validate_v2_record(
                &record,
                ticket,
                wall_ms,
                Some(entry.retired_at_ms),
                &entry.last_attempt,
                &entry.last_auth,
                entry.auth_closed,
            )
            .is_err()
        {
            entry.lease.unavailable();
            return previous != *entry.lease.status();
        }
        let denial = record.v2().is_some_and(is_denial_evidence);
        if entry
            .lease
            .observe_envelope(
                record.clone(),
                &ticket.key.runtime_id(),
                &ticket.nonce,
                now,
                wall_ms,
                true,
            )
            .is_ok()
        {
            entry.auth_record = record.v2().filter(|_| denial).cloned();
            entry.auth_validated_at_ms = denial.then_some(wall_ms);
            if !denial {
                entry.export.clear_auth();
            }
            entry.final_applied = true;
        } else {
            entry.lease.unavailable();
            entry.auth_record = None;
            entry.auth_validated_at_ms = None;
            entry.export.clear_auth();
        }
        previous != *entry.lease.status()
    }

    pub fn take_export_candidate(
        &mut self,
        ticket: &ReadTicket,
        now: Instant,
    ) -> Option<ExportCandidate> {
        let entry = self.entries.get_mut(&ticket.key)?;
        if &entry.ticket != ticket || !entry.final_applied || !entry.failed_exit {
            return None;
        }
        if let Some(record) = entry.auth_record.as_ref() {
            if let (Some(native_validation_at_ms), Some(evidence)) = (
                entry.auth_validated_at_ms,
                super::export::auth_evidence_from_record(
                    record,
                    &entry.ticket,
                    true,
                    true,
                    Some(entry.retired_at_ms),
                ),
            ) {
                if let (Some(received), Some(attempt)) = (
                    evidence.record.received_auth.as_ref(),
                    evidence.record.connection_attempt.as_ref(),
                ) {
                    entry.export.observe_auth(
                        super::export::AuthCandidate {
                            ticket: entry.ticket.clone(),
                            key: super::export::ExportKey {
                                runtime_id: entry.ticket.key.runtime_id(),
                                start_nonce: entry.ticket.nonce.clone(),
                                owner: entry.ticket.owner.clone(),
                                epoch: entry.ticket.epoch,
                                attempt_id: attempt.id.clone(),
                                attempt_sequence: attempt.sequence,
                                auth_event_id: received.auth_event_id.clone(),
                                retired: true,
                            },
                            evidence,
                            native_validation_at_ms,
                        },
                        now,
                    );
                }
            }
        }
        entry.export.take_due(now)
    }

    pub fn finish_export(&mut self, candidate: &ExportCandidate, success: bool, now: Instant) {
        let ticket = match candidate {
            ExportCandidate::Registration(candidate) => &candidate.ticket,
            ExportCandidate::Auth(candidate) => &candidate.ticket,
        };
        if let Some(entry) = self.entries.get_mut(&ticket.key) {
            if &entry.ticket == ticket {
                entry.export.finish(candidate, success, now);
            }
        }
    }

    pub fn export_is_current(&self, candidate: &ExportCandidate, now: Instant) -> bool {
        let ticket = match candidate {
            ExportCandidate::Registration(candidate) => &candidate.ticket,
            ExportCandidate::Auth(candidate) => &candidate.ticket,
        };
        self.entries.get(&ticket.key).is_some_and(|entry| {
            &entry.ticket == ticket
                && entry.failed_exit
                && entry.final_applied
                && now.saturating_duration_since(entry.retired_at) < RETIRED_TTL
                && entry.export.is_pending(candidate)
        })
    }
}

#[cfg(test)]
mod tests;
