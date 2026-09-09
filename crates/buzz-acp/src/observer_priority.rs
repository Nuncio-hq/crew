//! Cross-channel scheduling only: the publish queue still owns FIFO packing,
//! null barriers, fitting and source-event accounting.

use std::collections::{HashSet, VecDeque};

use crate::observer::ObserverEvent;

const MAX_URGENT_FRAMES: u8 = 2;

#[derive(Default)]
pub(crate) struct ObserverPriority {
    urgent_while_normal_waits: u8,
}

impl ObserverPriority {
    /// Select within the first barrier-delimited prefix. A mixed channel is
    /// urgent until its last urgent entry in that prefix leaves the queue;
    /// its ordinary predecessors still consume slots in causal order.
    pub(crate) fn select_channel(
        &mut self,
        events: &VecDeque<(usize, u64, ObserverEvent)>,
    ) -> Option<Option<String>> {
        let Some((_, _, front)) = events.front() else {
            self.urgent_while_normal_waits = 0;
            return None;
        };
        if front.channel_id.is_none() {
            self.urgent_while_normal_waits = 0;
            return Some(None);
        }

        // Borrowed, slot-local classification cannot retain stale urgency
        // after coalescer flush, overflow, eviction or a completed gather.
        let prefix = events
            .iter()
            .map(|(_, _, event)| event)
            .take_while(|event| event.channel_id.is_some());
        let mut urgent_channels = HashSet::new();
        let mut oldest_urgent = None;
        for event in prefix.clone().filter(|event| is_urgent(event)) {
            if let Some(channel) = event.channel_id.as_deref() {
                oldest_urgent.get_or_insert(channel);
                urgent_channels.insert(channel);
            }
        }
        let oldest_normal = prefix
            .filter_map(|event| event.channel_id.as_deref())
            .find(|channel| !urgent_channels.contains(channel));

        let selected = match (oldest_urgent, oldest_normal) {
            (Some(urgent), Some(normal)) => {
                if self.urgent_while_normal_waits >= MAX_URGENT_FRAMES {
                    self.urgent_while_normal_waits = 0;
                    normal
                } else {
                    self.urgent_while_normal_waits += 1;
                    urgent
                }
            }
            (Some(urgent), None) => {
                self.urgent_while_normal_waits = 0;
                urgent
            }
            (None, _) => {
                self.urgent_while_normal_waits = 0;
                return Some(front.channel_id.clone());
            }
        };
        Some(Some(selected.to_owned()))
    }
}

/// Exact production lifecycle/control classes, not a payload success parser.
/// Raw ACP requests live directly in ObserverEvent.payload (acp.rs::observe).
fn is_urgent(event: &ObserverEvent) -> bool {
    match event.kind.as_str() {
        "turn_started" | "turn_completed" | "turn_error" | "turn_retrying" | "agent_panic" => true,
        "control_result" => {
            event.payload["status"].is_string()
                && matches!(
                    event.payload["type"].as_str(),
                    Some(
                        "cancel_turn"
                            | "switch_model"
                            | "retry_turn"
                            | "guided_handover"
                            | "blind_session_reset"
                    )
                )
        }
        "acp_read" => {
            let id = &event.payload["id"];
            (id.is_string() || id.is_number())
                && event.payload["params"].is_object()
                && matches!(
                    event.payload["method"].as_str(),
                    Some("session/request_permission" | "elicitation/create")
                )
        }
        _ => false,
    }
}
