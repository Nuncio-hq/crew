//! Input lease: free → agent-held → human-held → free.
//!
//! Humans always win instantly. In-flight agent actions abort with `lease_held`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::protocol::{ControlError, Instrument};

pub const HUMAN_RELEASE_MS: u64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseState {
    Free,
    AgentHeld,
    HumanHeld,
}

#[derive(Debug, Clone)]
pub struct Lease {
    pub state: LeaseState,
    pub channel_id: String,
    pub instrument: Instrument,
    pub agent_name: Option<String>,
    pub human_since_ms: Option<u64>,
    pub abort: Arc<AtomicBool>,
}

impl Lease {
    fn free(channel_id: String, instrument: Instrument) -> Self {
        Self {
            state: LeaseState::Free,
            channel_id,
            instrument,
            agent_name: None,
            human_since_ms: None,
            abort: Arc::new(AtomicBool::new(false)),
        }
    }

    #[allow(dead_code)]
    pub fn holder_label(&self) -> String {
        match self.state {
            LeaseState::Free => "free".into(),
            LeaseState::AgentHeld => self.agent_name.clone().unwrap_or_else(|| "agent".into()),
            LeaseState::HumanHeld => "human".into(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseView {
    pub channel_id: String,
    pub instrument: String,
    pub state: String,
    pub agent_name: Option<String>,
    pub human_held_until_ms: Option<u64>,
}

#[derive(Default)]
pub struct LeaseMap {
    inner: HashMap<(String, Instrument), Lease>,
}

impl LeaseMap {
    fn key(channel_id: &str, instrument: Instrument) -> (String, Instrument) {
        (channel_id.to_string(), instrument)
    }

    fn entry(&mut self, channel_id: &str, instrument: Instrument) -> &mut Lease {
        self.inner
            .entry(Self::key(channel_id, instrument))
            .or_insert_with(|| Lease::free(channel_id.to_string(), instrument))
    }

    pub fn view_all(&self, _now_ms: u64) -> Vec<LeaseView> {
        self.inner
            .values()
            .filter(|lease| lease.state != LeaseState::Free)
            .map(|lease| LeaseView {
                channel_id: lease.channel_id.clone(),
                instrument: match lease.instrument {
                    Instrument::Browser => "browser".into(),
                    Instrument::Sim => "sim".into(),
                },
                state: match lease.state {
                    LeaseState::Free => "free".into(),
                    LeaseState::AgentHeld => "agentHeld".into(),
                    LeaseState::HumanHeld => "humanHeld".into(),
                },
                agent_name: lease.agent_name.clone(),
                human_held_until_ms: lease.human_since_ms.map(|since| since + HUMAN_RELEASE_MS),
            })
            .collect()
    }

    /// First input tool in a turn takes the lease. Human-held fails closed.
    pub fn acquire_agent(
        &mut self,
        channel_id: &str,
        instrument: Instrument,
        agent_name: Option<&str>,
    ) -> Result<Arc<AtomicBool>, ControlError> {
        let lease = self.entry(channel_id, instrument);
        match lease.state {
            LeaseState::HumanHeld => Err(ControlError::lease_held("human")),
            LeaseState::AgentHeld => {
                if lease.abort.load(Ordering::SeqCst) {
                    return Err(ControlError::lease_held("human"));
                }
                Ok(Arc::clone(&lease.abort))
            }
            LeaseState::Free => {
                lease.state = LeaseState::AgentHeld;
                lease.agent_name = agent_name.map(str::to_string);
                lease.human_since_ms = None;
                lease.abort.store(false, Ordering::SeqCst);
                Ok(Arc::clone(&lease.abort))
            }
        }
    }

    /// Any human interaction preempts immediately and aborts in-flight work.
    pub fn preempt_human(&mut self, channel_id: &str, instrument: Instrument, now_ms: u64) {
        let lease = self.entry(channel_id, instrument);
        lease.state = LeaseState::HumanHeld;
        lease.human_since_ms = Some(now_ms);
        lease.abort.store(true, Ordering::SeqCst);
    }

    pub fn note_human_input(&mut self, channel_id: &str, instrument: Instrument, now_ms: u64) {
        let lease = self.entry(channel_id, instrument);
        if lease.state == LeaseState::HumanHeld {
            lease.human_since_ms = Some(now_ms);
        } else {
            self.preempt_human(channel_id, instrument, now_ms);
        }
    }

    pub fn release_human(&mut self, channel_id: &str, instrument: Instrument) {
        let lease = self.entry(channel_id, instrument);
        if lease.state == LeaseState::HumanHeld {
            *lease = Lease::free(channel_id.to_string(), instrument);
        }
    }

    pub fn release_turn(&mut self, channel_id: &str) {
        for instrument in [Instrument::Browser, Instrument::Sim] {
            let lease = self.entry(channel_id, instrument);
            if lease.state == LeaseState::AgentHeld {
                lease.abort.store(true, Ordering::SeqCst);
                *lease = Lease::free(channel_id.to_string(), instrument);
            }
        }
    }

    pub fn tick(&mut self, now_ms: u64) {
        let keys: Vec<(String, Instrument)> = self.inner.keys().cloned().collect();
        for key in keys {
            let Some(lease) = self.inner.get_mut(&key) else {
                continue;
            };
            if lease.state != LeaseState::HumanHeld {
                continue;
            }
            let Some(since) = lease.human_since_ms else {
                continue;
            };
            if now_ms.saturating_sub(since) >= HUMAN_RELEASE_MS {
                let (channel, instrument) = key;
                *lease = Lease::free(channel, instrument);
            }
        }
    }

    #[cfg(test)]
    pub fn get(&self, channel_id: &str, instrument: Instrument) -> LeaseState {
        self.inner
            .get(&Self::key(channel_id, instrument))
            .map(|lease| lease.state)
            .unwrap_or(LeaseState::Free)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_control::protocol::Instrument;

    #[test]
    fn holder_label_for_agent() {
        let mut map = LeaseMap::default();
        let _ = map.acquire_agent("ch", Instrument::Browser, Some("Hermes"));
        let label = map
            .inner
            .get(&LeaseMap::key("ch", Instrument::Browser))
            .map(|lease| lease.holder_label())
            .unwrap_or_default();
        assert_eq!(label, "Hermes");
    }
}
