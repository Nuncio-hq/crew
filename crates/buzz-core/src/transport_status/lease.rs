//! Native monotonic freshness for an already-owned generation's status record.
use super::{TransportRecord, TransportState, TransportStatus, LEASE_SECONDS};
use std::time::{Duration, Instant};

/// Derived reader state; this never establishes process ownership.
#[derive(Debug)]
pub struct TransportLease {
    status: TransportStatus,
    last_record: Option<TransportRecord>,
    last_advance: Option<Instant>,
    retired: bool,
}

impl Default for TransportLease {
    fn default() -> Self {
        Self {
            status: TransportStatus::unknown(),
            last_record: None,
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
        let result = self.observe_owned(record, runtime_id, nonce, now, wall_ms, retired);
        if result.is_err() {
            self.unavailable();
        }
        result
    }

    fn observe_owned(
        &mut self,
        record: TransportRecord,
        runtime_id: &str,
        nonce: &str,
        now: Instant,
        wall_ms: u64,
        retired: bool,
    ) -> Result<(), String> {
        if record.version != 1
            || record.sequence == 0
            || !record.transport.has_safe_error()
            || record.runtime_id != runtime_id
            || record.start_nonce != nonce
        {
            return Err("local transport status does not match the owned generation".into());
        }
        if (retired && !record.terminal)
            || (record.terminal
                && !matches!(
                    record.transport.state,
                    TransportState::Exhausted
                        | TransportState::AuthRejected
                        | TransportState::Unknown
                ))
        {
            return Err("exited generation has no final transport failure diagnostic".into());
        }
        if let Some(previous) = &self.last_record {
            if record.sequence < previous.sequence
                || (record.sequence == previous.sequence && record != *previous)
            {
                return Err("local transport status sequence is stale or inconsistent".into());
            }
            if record.sequence == previous.sequence {
                self.status = record.transport;
                self.retired = retired;
                self.expire(now);
                return Ok(());
            }
        } else if wall_ms.saturating_sub(record.timestamp_ms) >= LEASE_SECONDS * 1000
            || record.timestamp_ms > wall_ms.saturating_add(5000)
        {
            return Err("initial local transport status timestamp is stale or invalid".into());
        }
        self.status = record.transport.clone();
        self.last_record = Some(record);
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
