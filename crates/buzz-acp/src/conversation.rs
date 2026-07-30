use nostr::Event;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Return the scheduler/session identity for an inbound event.
///
/// Channel UUIDs remain the identity for DMs. Channel messages use their
/// NIP-10 root event, or their own event ID when starting a new thread. This
/// lets multiple threads in one channel occupy independent pool slots without
/// changing the queue and session-state APIs that already accept UUIDs.
pub fn id_for_event(channel_id: Uuid, event: &Event, is_dm: bool) -> Uuid {
    if is_dm {
        return channel_id;
    }

    let root = crate::queue::parse_thread_tags(event)
        .root_event_id
        .unwrap_or_else(|| event.id.to_hex());
    deterministic_id(channel_id, &root)
}

/// Recover the real channel UUID carried by a NIP-29 event.
///
/// Tests and legacy callers may construct events without an `h` tag, so users
/// should fall back to the queue key when this returns `None`.
pub fn routing_channel_id(event: &Event) -> Option<Uuid> {
    event.tags.iter().find_map(|tag| {
        let parts = tag.as_slice();
        (parts.first().map(String::as_str) == Some("h"))
            .then(|| parts.get(1))
            .flatten()
            .and_then(|value| Uuid::parse_str(value).ok())
    })
}

fn deterministic_id(channel_id: Uuid, root_event_id: &str) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(b"buzz-acp-conversation-v1");
    digest.update(channel_id.as_bytes());
    digest.update(root_event_id.as_bytes());
    let hash = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use nostr::{EventBuilder, Keys, Kind, Tag};

    use super::*;
    use crate::config::DedupMode;
    use crate::queue::{EventQueue, QueuedEvent};

    fn event(tags: Vec<Tag>) -> Event {
        EventBuilder::new(Kind::TextNote, "task")
            .tags(tags)
            .sign_with_keys(&Keys::generate())
            .expect("test event signs")
    }

    #[test]
    fn top_level_channel_events_get_distinct_conversation_ids() {
        let channel = Uuid::new_v4();
        let first = event(vec![]);
        let second = event(vec![]);

        assert_ne!(
            id_for_event(channel, &first, false),
            id_for_event(channel, &second, false)
        );
    }

    #[test]
    fn replies_to_same_root_share_a_conversation_id() {
        let channel = Uuid::new_v4();
        let root = event(vec![]);
        let root_id = root.id.to_hex();
        let reply_tag = Tag::parse(["e", root_id.as_str(), "", "reply"]).expect("valid reply tag");
        let first = event(vec![reply_tag.clone()]);
        let second = event(vec![reply_tag]);

        assert_eq!(
            id_for_event(channel, &first, false),
            id_for_event(channel, &second, false)
        );
    }

    #[test]
    fn dm_events_keep_channel_identity() {
        let channel = Uuid::new_v4();
        assert_eq!(id_for_event(channel, &event(vec![]), true), channel);
    }

    #[test]
    fn two_threads_in_one_channel_can_be_in_flight_together() {
        let channel = Uuid::new_v4();
        let channel_tag =
            Tag::parse(["h", channel.to_string().as_str()]).expect("valid channel tag");
        let first = event(vec![channel_tag.clone()]);
        let second = event(vec![channel_tag]);
        let first_id = id_for_event(channel, &first, false);
        let second_id = id_for_event(channel, &second, false);
        let mut queue = EventQueue::new(DedupMode::Queue);

        for (conversation, event) in [(first_id, first), (second_id, second)] {
            assert!(queue.push(QueuedEvent {
                channel_id: conversation,
                event,
                received_at: Instant::now(),
                prompt_tag: "mention".to_string(),
            }));
        }

        let first_batch = queue.flush_next().expect("first thread flushes");
        let second_batch = queue
            .flush_next()
            .expect("second thread flushes while first is in flight");

        assert_ne!(first_batch.channel_id, second_batch.channel_id);
        assert_eq!(first_batch.routing_channel_id(), channel);
        assert_eq!(second_batch.routing_channel_id(), channel);
    }
}
