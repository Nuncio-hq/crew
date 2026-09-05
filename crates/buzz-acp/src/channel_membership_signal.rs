//! Subscription readiness is independent of whether an engine is currently running.

use crate::observer::{ObserverContext, ObserverHandle};

pub(crate) struct ChannelMembershipSignal {
    generation: String,
    started_at: String,
    previous_count: Option<usize>,
}

impl ChannelMembershipSignal {
    pub(crate) fn new() -> Self {
        Self {
            generation: uuid::Uuid::new_v4().to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            previous_count: None,
        }
    }

    pub(crate) fn report(&mut self, observer: Option<&ObserverHandle>, channel_count: usize) {
        if self.previous_count == Some(channel_count) {
            return;
        }
        self.previous_count = Some(channel_count);
        if let Some(observer) = observer {
            observer.emit(
                "channel_membership",
                None,
                &ObserverContext::default(),
                serde_json::json!({
                    "channel_count": channel_count,
                    "generation": self.generation,
                    "generation_started_at": self.started_at,
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn startup_and_membership_changes_publish_ordered_readiness() {
        let observer = ObserverHandle::in_process();
        let mut signal = ChannelMembershipSignal::new();
        for count in [0, 0, 1, 2, 0] {
            signal.report(Some(&observer), count);
        }
        let events = observer.snapshot();
        let counts: Vec<_> = events
            .iter()
            .map(|e| e.payload["channel_count"].as_u64().unwrap())
            .collect();
        assert_eq!(counts, [0, 1, 2, 0]);
        assert!(events.windows(2).all(|pair| pair[0].seq < pair[1].seq));
        assert!(events.iter().all(
            |e| e.kind == "channel_membership" && e.payload["generation"] == signal.generation
        ));
        assert!(chrono::DateTime::parse_from_rfc3339(&signal.started_at).is_ok());
        let mut next = ChannelMembershipSignal::new();
        next.report(Some(&observer), 1);
        assert_ne!(next.generation, signal.generation);
        assert_eq!(
            observer.snapshot().last().unwrap().payload["channel_count"],
            1
        );
    }
}
