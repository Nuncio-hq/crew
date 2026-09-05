//! Per-turn ACP tool lifetimes extend silence tolerance, never the hard cap.
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::time::Instant;

#[derive(Default)]
pub(super) struct ToolProgress {
    deadlines: HashMap<String, Instant>,
    terminal_ids: HashSet<String>,
}

impl ToolProgress {
    pub(super) fn observe(
        &mut self,
        msg: &serde_json::Value,
        session: &str,
        now: Instant,
        budget: Duration,
    ) {
        if msg["method"] != "session/update" || msg["params"]["sessionId"] != session {
            return;
        }
        let update = &msg["params"]["update"];
        let Some(id) = update["toolCallId"].as_str().filter(|id| !id.is_empty()) else {
            return;
        };
        let kind = update["sessionUpdate"].as_str();
        if !matches!(kind, Some("tool_call" | "tool_call_update")) {
            return;
        }
        match update["status"].as_str() {
            Some("completed" | "failed") => {
                self.deadlines.remove(id);
                self.terminal_ids.insert(id.to_owned());
            }
            None | Some("pending" | "in_progress") if kind == Some("tool_call") => {
                if self.terminal_ids.contains(id) {
                    return;
                }
                // A duplicate start or partial progress cannot renew the lifetime.
                self.deadlines.entry(id.to_owned()).or_insert(now + budget);
            }
            _ => {}
        }
    }

    pub(super) fn deadline(&self, ordinary: Instant) -> Instant {
        // Keep expired entries until terminal notification or turn end: deleting
        // them would let a repeated start renew a tool that never completed.
        self.deadlines
            .values()
            .copied()
            .fold(ordinary, Instant::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_foreign_and_terminal_starts_do_not_extend_silence() {
        let now = Instant::now();
        let mut tools = ToolProgress::default();
        for (session, id, status) in [
            ("foreign", "tool", "in_progress"),
            ("test", "", "pending"),
            ("test", "tool", "completed"),
            ("test", "tool", "failed"),
            ("test", "tool", "unknown"),
        ] {
            let frame = serde_json::json!({"method":"session/update","params":{"sessionId":session,"update":{"sessionUpdate":"tool_call","toolCallId":id,"status":status}}});
            tools.observe(&frame, "test", now, Duration::from_secs(2400));
        }
        tools.observe(
            &serde_json::json!({}),
            "test",
            now,
            Duration::from_secs(2400),
        );
        assert_eq!(tools.deadline(now), now);
    }

    #[test]
    fn expired_tool_and_partial_updates_cannot_renew_lifetime() {
        let now = Instant::now();
        let mut tools = ToolProgress::default();
        let frame = |kind| serde_json::json!({"method":"session/update","params":{"sessionId":"test","update":{"sessionUpdate":kind,"toolCallId":"tool","status":"in_progress"}}});
        tools.observe(&frame("tool_call"), "test", now, Duration::from_secs(2400));
        let later = now + Duration::from_secs(2500);
        tools.observe(
            &frame("tool_call"),
            "test",
            later,
            Duration::from_secs(2400),
        );
        tools.observe(
            &frame("tool_call_update"),
            "test",
            later,
            Duration::from_secs(2400),
        );
        assert_eq!(tools.deadline(later), later);
    }
    #[test]
    fn terminal_tool_cannot_be_restarted_by_replayed_start() {
        let now = Instant::now();
        let frame = |kind, status| serde_json::json!({"method":"session/update","params":{"sessionId":"test","update":{"sessionUpdate":kind,"toolCallId":"tool","status":status}}});
        for start_first in [true, false] {
            let mut tools = ToolProgress::default();
            if start_first {
                tools.observe(
                    &frame("tool_call", "in_progress"),
                    "test",
                    now,
                    Duration::from_secs(2400),
                );
            }
            tools.observe(
                &frame("tool_call_update", "completed"),
                "test",
                now,
                Duration::from_secs(2400),
            );
            tools.observe(
                &frame("tool_call", "in_progress"),
                "test",
                now,
                Duration::from_secs(2400),
            );
            assert_eq!(
                tools.deadline(now),
                now,
                "terminal ID must stay retired within the turn"
            );
        }
    }
    #[test]
    fn parallel_tools_keep_independent_fixed_deadlines() {
        let now = tokio::time::Instant::now();
        let budget = std::time::Duration::from_secs(2400);
        let mut tools = ToolProgress::default();
        let frame = |id: &str, kind: &str, status: &str| serde_json::json!({"method":"session/update","params":{"sessionId":"test","update":{"sessionUpdate":kind,"toolCallId":id,"status":status}}});
        tools.observe(&frame("a", "tool_call", "in_progress"), "test", now, budget);
        tools.observe(
            &frame("b", "tool_call", "pending"),
            "test",
            now + std::time::Duration::from_secs(10),
            budget,
        );
        tools.observe(
            &frame("a", "tool_call_update", "completed"),
            "test",
            now,
            budget,
        );
        tools.observe(
            &frame("b", "tool_call", "in_progress"),
            "test",
            now + std::time::Duration::from_secs(30),
            budget,
        );
        tools.observe(&serde_json::json!({"method":"session/update","params":{"sessionId":"test","update":{"sessionUpdate":"agent_message_chunk"}}}), "test", now, budget);
        assert_eq!(
            tools.deadline(now),
            now + budget + std::time::Duration::from_secs(10)
        );
        tools.observe(
            &frame("b", "tool_call_update", "failed"),
            "test",
            now,
            budget,
        );
        assert_eq!(tools.deadline(now), now);
    }
}
