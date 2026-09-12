//! Native monotonic freshness for an already-owned generation's status record.
use super::{
    TransportRecord, TransportRecordEnvelope, TransportState, TransportStatus, LEASE_SECONDS,
};
use std::time::{Duration, Instant};

/// Derived reader state; this never establishes process ownership.
#[derive(Debug)]
pub struct TransportLease {
    status: TransportStatus,
    last_record: Option<TransportRecordEnvelope>,
    accepted_wire_version: Option<u32>,
    last_advance: Option<Instant>,
    retired: bool,
}

impl Default for TransportLease {
    fn default() -> Self {
        Self {
            status: TransportStatus::unknown(),
            last_record: None,
            accepted_wire_version: None,
            last_advance: None,
            retired: false,
        }
    }
}

impl TransportLease {
    /// Current trustworthy diagnostic or actionable unknown.
    pub fn status(&self) -> &TransportStatus {
        &self.status
    }

    /// Invalidate the projection after a missing or unreadable record.
    pub fn unavailable(&mut self) {
        self.status = TransportStatus::unknown();
    }

    /// Apply a record for an identity already authorized by the native owner.
    pub fn observe(
        &mut self,
        record: TransportRecord,
        runtime_id: &str,
        nonce: &str,
        now: Instant,
        wall_ms: u64,
        retired: bool,
    ) -> Result<(), String> {
        self.observe_envelope(
            TransportRecordEnvelope::V1(record),
            runtime_id,
            nonce,
            now,
            wall_ms,
            retired,
        )
    }

    /// Apply a strict v1/v2 record for an identity already authorized by the
    /// native owner. v2-specific process/attempt checks happen at that owner;
    /// this method preserves the shared generation, timestamp, terminal,
    /// monotonic sequence, and per-generation wire-version checks for both
    /// wire versions.
    pub fn observe_envelope(
        &mut self,
        record: TransportRecordEnvelope,
        runtime_id: &str,
        nonce: &str,
        now: Instant,
        wall_ms: u64,
        retired: bool,
    ) -> Result<(), String> {
        let result = self.observe_owned(record, runtime_id, nonce, now, wall_ms, retired);
        if result.is_err() {
            self.unavailable();
        }
        result
    }

    fn observe_owned(
        &mut self,
        record: TransportRecordEnvelope,
        runtime_id: &str,
        nonce: &str,
        now: Instant,
        wall_ms: u64,
        retired: bool,
    ) -> Result<(), String> {
        if !matches!(&record, TransportRecordEnvelope::V1(record) if record.version == 1)
            && !matches!(&record, TransportRecordEnvelope::V2(record) if record.version == 2)
        {
            return Err("local transport status version is unsupported".into());
        }
        if record.sequence() == 0
            || !record.transport().has_safe_error()
            || record.runtime_id() != runtime_id
            || record.start_nonce() != nonce
        {
            return Err("local transport status does not match the owned generation".into());
        }
        if let Some(v2) = record.v2() {
            v2.validate_shape()?;
        }
        if let Some(previous_version) = self.accepted_wire_version {
            if previous_version != record.version() {
                return Err("local transport status wire version changed for a generation".into());
            }
        }
        if (retired && !record.terminal())
            || (record.terminal()
                && !matches!(
                    record.transport().state,
                    TransportState::Exhausted
                        | TransportState::AuthRejected
                        | TransportState::Unknown
                ))
        {
            return Err("exited generation has no final transport failure diagnostic".into());
        }
        if let Some(previous) = &self.last_record {
            if record.sequence() < previous.sequence()
                || (record.sequence() == previous.sequence() && record != *previous)
            {
                return Err("local transport status sequence is stale or inconsistent".into());
            }
            if record.sequence() == previous.sequence() {
                self.status = record.transport().clone();
                self.retired = retired;
                self.expire(now);
                return Ok(());
            }
        } else if wall_ms.saturating_sub(record.timestamp_ms()) >= LEASE_SECONDS * 1000
            || record.timestamp_ms() > wall_ms.saturating_add(5000)
        {
            return Err("initial local transport status timestamp is stale or invalid".into());
        }
        let version = record.version();
        self.status = record.transport().clone();
        self.last_record = Some(record);
        self.accepted_wire_version = Some(version);
        self.last_advance = Some(now);
        self.retired = retired;
        Ok(())
    }

    /// Expire live status using monotonic time, never frontend polling time.
    /// Retired records are instead bounded by the native retirement cache TTL.
    pub fn expire(&mut self, now: Instant) {
        if !self.retired
            && self.last_advance.is_none_or(|last| {
                now.saturating_duration_since(last) >= Duration::from_secs(LEASE_SECONDS)
            })
        {
            self.unavailable();
        }
    }
}

#[cfg(test)]
mod tests;
