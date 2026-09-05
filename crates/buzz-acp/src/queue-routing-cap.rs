impl EventQueue {
    fn queued_routing_channel(conversation: Uuid, queue: &VecDeque<QueuedEvent>) -> Uuid {
        queue
            .front()
            .and_then(|event| crate::conversation::routing_channel_id(&event.event))
            .unwrap_or(conversation)
    }

    /// Keep thread partitioning from multiplying the existing pending-work cap.
    /// Running and withheld work retain their separate delivery lifecycle; this
    /// bound covers the ready queues, including every restore into those queues.
    fn enforce_routing_channel_cap(&mut self, conversation: Uuid) {
        let Some(queue) = self.queues.get(&conversation) else {
            return;
        };
        let channel = Self::queued_routing_channel(conversation, queue);
        loop {
            let mut total = 0;
            let mut oldest = None;
            for (id, queue) in &self.queues {
                if Self::queued_routing_channel(*id, queue) != channel {
                    continue;
                }
                total += queue.len();
                if let Some(head) = queue.front() {
                    let candidate = (head.received_at, *id);
                    if oldest.is_none_or(|current| candidate < current) {
                        oldest = Some(candidate);
                    }
                }
            }
            if total <= MAX_PENDING_PER_CHANNEL {
                break;
            }
            let Some((_, victim)) = oldest else { break };
            if let Some(queue) = self.queues.get_mut(&victim) {
                queue.pop_front();
                if queue.is_empty() {
                    self.queues.remove(&victim);
                }
            }
            tracing::warn!(
                channel_id = %channel,
                limit = MAX_PENDING_PER_CHANNEL,
                "aggregate per-channel queue cap reached — dropped oldest event"
            );
        }
    }
}

#[cfg(test)]
#[path = "queue-routing-cap-tests.rs"]
mod routing_cap_tests;
