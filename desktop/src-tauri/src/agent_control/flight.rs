//! Per-instrument single-flight + `request_id` idempotency.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::protocol::{ControlError, ControlErrorBody, Instrument};

const DEDUPE_CAP: usize = 256;

#[derive(Clone)]
pub struct CachedOutcome {
    pub result: Option<serde_json::Value>,
    pub error: Option<ControlErrorBody>,
}

#[derive(Default)]
pub struct FlightTable {
    locks: HashMap<(String, Instrument), Arc<Mutex<()>>>,
    dedupe: HashMap<String, CachedOutcome>,
    dedupe_order: Vec<String>,
}

impl FlightTable {
    pub fn lock_for(&mut self, channel_id: &str, instrument: Instrument) -> Arc<Mutex<()>> {
        self.locks
            .entry((channel_id.to_string(), instrument))
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub fn lookup(&self, request_id: &str) -> Option<CachedOutcome> {
        self.dedupe.get(request_id).cloned()
    }

    pub fn remember(&mut self, request_id: String, outcome: CachedOutcome) {
        if let std::collections::hash_map::Entry::Occupied(mut occupied) =
            self.dedupe.entry(request_id.clone())
        {
            occupied.insert(outcome);
            return;
        }
        if self.dedupe_order.len() >= DEDUPE_CAP {
            let old = self.dedupe_order.remove(0);
            self.dedupe.remove(&old);
        }
        self.dedupe_order.push(request_id.clone());
        self.dedupe.insert(request_id, outcome);
    }
}

pub fn aborted(flag: &std::sync::atomic::AtomicBool) -> Result<(), ControlError> {
    if flag.load(std::sync::atomic::Ordering::SeqCst) {
        Err(ControlError::lease_held("human"))
    } else {
        Ok(())
    }
}
