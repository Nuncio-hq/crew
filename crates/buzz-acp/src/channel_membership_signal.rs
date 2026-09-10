//! Subscription readiness is independent of whether an engine is currently running.

use crate::observer::{ObserverContext, ObserverHandle};

pub(crate) struct ChannelMembershipSignal {
    generation: String,
    started_at: String,
    previous_state: Option<MembershipState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MembershipState {
    Unknown,
    Count(usize),
}

impl ChannelMembershipSignal {
    /// Bind the generation to the runtime start nonce so a consumer can fence a
    /// stale frame against the live runtime instead of comparing wall clocks.
    /// An unmanaged harness has no nonce; a local uuid keeps generations
    /// distinct per process without pretending to be a runtime identity.
    pub(crate) fn new(runtime_start_nonce: &str) -> Self {
        let generation = if runtime_start_nonce.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            runtime_start_nonce.to_string()
        };
        Self {
            generation,
            started_at: chrono::Utc::now().to_rfc3339(),
            previous_state: None,
        }
    }

    pub(crate) fn report(&mut self, observer: Option<&ObserverHandle>, channel_count: usize) {
        self.report_state(observer, MembershipState::Count(channel_count));
    }

    pub(crate) fn report_unknown(&mut self, observer: Option<&ObserverHandle>) {
        self.report_state(observer, MembershipState::Unknown);
    }

    fn report_state(&mut self, observer: Option<&ObserverHandle>, state: MembershipState) {
        if self.previous_state == Some(state) {
            return;
        }
        self.previous_state = Some(state);
        if let Some(observer) = observer {
            observer.emit(
                "channel_membership",
                None,
                &ObserverContext::default(),
                serde_json::json!({
                    "channel_count": match state {
                        MembershipState::Unknown => None,
                        MembershipState::Count(count) => Some(count),
                    },
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
        let mut signal = ChannelMembershipSignal::new("");
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
        let mut next = ChannelMembershipSignal::new("");
        next.report(Some(&observer), 1);
        assert_ne!(next.generation, signal.generation);
        assert_eq!(
            observer.snapshot().last().unwrap().payload["channel_count"],
            1
        );
    }

    #[tokio::test]
    async fn unknown_snapshots_are_distinct_from_confirmed_zero() {
        let observer = ObserverHandle::in_process();
        let mut signal = ChannelMembershipSignal::new("");
        signal.report_unknown(Some(&observer));
        signal.report_unknown(Some(&observer));
        signal.report(Some(&observer), 0);
        signal.report(Some(&observer), 0);
        signal.report_unknown(Some(&observer));

        let states: Vec<_> = observer
            .snapshot()
            .iter()
            .map(|event| event.payload["channel_count"].as_u64())
            .collect();
        assert_eq!(states, [None, Some(0), None]);
    }

    /// The published generation must be the runtime start nonce itself, so a
    /// consumer can fence a frame against the live runtime. A self-minted id
    /// would be unrelated to the nonce and fail both assertions.
    #[tokio::test]
    async fn generation_is_the_runtime_start_nonce() {
        let observer = ObserverHandle::in_process();
        let nonce = "runtime-start-nonce-337";
        let mut signal = ChannelMembershipSignal::new(nonce);
        signal.report(Some(&observer), 2);

        assert_eq!(
            observer.snapshot().last().unwrap().payload["generation"],
            nonce
        );
        // A restarted runtime frame carrying the same nonce stays the same
        // generation; only a new nonce is a new generation.
        assert_eq!(ChannelMembershipSignal::new(nonce).generation, nonce);
        assert_ne!(
            ChannelMembershipSignal::new("other-nonce").generation,
            nonce
        );
    }
}
