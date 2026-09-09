use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use crate::managed_agents::ManagedAgentRuntimeKey;
use buzz_core_pkg::transport_status::{TransportLease, TransportRecord, TransportStatus};

const RETIRED_CAPACITY: usize = 256;
const RETIRED_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const FINAL_READ_WINDOW: Duration = Duration::from_secs(15);

/// Read capability captured from an existing registered generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadTicket {
    pub key: ManagedAgentRuntimeKey,
    pub nonce: String,
    pub path: PathBuf,
    pub owner: String,
    pub epoch: u64,
}

#[derive(Debug)]
pub(crate) struct Monitor {
    ticket: ReadTicket,
    enabled: bool,
    lease: TransportLease,
    diagnostics: Weak<Mutex<Diagnostics>>,
}

impl Monitor {
    pub fn new(ticket: ReadTicket, diagnostics: &Arc<Mutex<Diagnostics>>) -> Self {
        Self {
            ticket,
            enabled: true,
            lease: TransportLease::default(),
            diagnostics: Arc::downgrade(diagnostics),
        }
    }

    pub fn snapshot(&self) -> Option<ReadTicket> {
        self.enabled.then(|| self.ticket.clone())
    }

    pub fn status(&self) -> &TransportStatus {
        self.lease.status()
    }

    pub fn inspection_failed(&mut self) {
        self.lease.unavailable();
    }

    pub fn expire(&mut self, now: Instant) -> bool {
        let previous = self.lease.status().clone();
        self.lease.expire(now);
        previous != *self.lease.status()
    }

    pub fn disable(&mut self) -> bool {
        if !self.enabled {
            return false;
        }
        self.enabled = false;
        self.ticket.epoch = self.ticket.epoch.saturating_add(1);
        self.lease.unavailable();
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
        }
        Ok(changed)
    }

    pub fn belongs_to(&self, owner: &str, relay_url: &str) -> bool {
        self.ticket.owner == owner && self.ticket.key.relay_url == relay_url
    }

    pub fn apply(
        &mut self,
        ticket: &ReadTicket,
        record: Result<TransportRecord, String>,
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
            return previous != *self.lease.status();
        }
        match record {
            Ok(record) => {
                let _ = self.lease.observe(
                    record,
                    &self.ticket.key.runtime_id(),
                    &self.ticket.nonce,
                    now,
                    wall_ms,
                    false,
                );
            }
            Err(_) => self.lease.unavailable(),
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
            cache.retire(
                self.ticket.clone(),
                failed_exit,
                now,
                std::mem::take(&mut self.lease),
            );
        }
    }
}

#[derive(Debug)]
struct Retired {
    ticket: ReadTicket,
    retired_at: Instant,
    failed_exit: bool,
    lease: TransportLease,
    final_applied: bool,
}

/// Bounded derived diagnostics; never a source of active process membership.
#[derive(Debug, Default)]
pub(crate) struct Diagnostics {
    entries: HashMap<ManagedAgentRuntimeKey, Retired>,
}

impl Diagnostics {
    fn retire(
        &mut self,
        ticket: ReadTicket,
        failed_exit: bool,
        now: Instant,
        mut lease: TransportLease,
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
                failed_exit,
                lease,
                final_applied: false,
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

    pub fn projection(
        &self,
        key: &ManagedAgentRuntimeKey,
        owner: &str,
        now: Instant,
    ) -> Option<(TransportStatus, bool)> {
        let entry = self.entries.get(key)?;
        (entry.ticket.owner == owner
            && now.saturating_duration_since(entry.retired_at) < RETIRED_TTL)
            .then(|| (entry.lease.status().clone(), entry.failed_exit))
    }

    pub fn apply(
        &mut self,
        ticket: &ReadTicket,
        record: TransportRecord,
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
        if entry
            .lease
            .observe(
                record,
                &ticket.key.runtime_id(),
                &ticket.nonce,
                now,
                wall_ms,
                true,
            )
            .is_ok()
        {
            entry.final_applied = true;
        }
        previous != *entry.lease.status()
    }
}

#[cfg(test)]
mod tests;
