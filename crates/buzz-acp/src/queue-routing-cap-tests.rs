use super::*;
use nostr::{EventBuilder, Keys, Kind, Tag};

fn event(channel: Uuid, root: &str, sequence: usize, now: Instant) -> QueuedEvent {
    let event = EventBuilder::new(Kind::Custom(9), sequence.to_string())
        .tags([
            Tag::parse(["h", &channel.to_string()]).expect("channel"),
            Tag::parse(["e", root, "", "root"]).expect("root"),
        ])
        .sign_with_keys(&Keys::generate())
        .expect("signed event");
    QueuedEvent {
        channel_id: crate::conversation::id_for_event(channel, &event, false),
        event,
        received_at: now + Duration::from_millis(sequence as u64),
        prompt_tag: "test".into(),
        edited_content: None,
        hold_exempt: true,
    }
}

fn count(queue: &EventQueue, channel: Uuid) -> usize {
    queue
        .queues
        .iter()
        .filter(|(id, entries)| EventQueue::queued_routing_channel(**id, entries) == channel)
        .map(|(_, entries)| entries.len())
        .sum()
}

#[test]
fn many_roots_share_one_cap_and_preserve_each_threads_order() {
    let mut queue = EventQueue::new(DedupMode::Queue);
    let channel = Uuid::new_v4();
    let other = Uuid::new_v4();
    let now = Instant::now();
    queue.push(event(other, &"f".repeat(64), 0, now));
    for i in 0..750 {
        queue.push(event(channel, &format!("{:064x}", i % 5), i, now));
    }
    assert_eq!(count(&queue, channel), MAX_PENDING_PER_CHANNEL);
    assert_eq!(count(&queue, other), 1);
    for entries in queue.queues.values() {
        let sequences: Vec<_> = entries
            .iter()
            .map(|e| e.event.content.parse::<usize>().expect("sequence"))
            .collect();
        assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
        if EventQueue::queued_routing_channel(entries[0].channel_id, entries) == channel {
            assert!(sequences.iter().all(|sequence| *sequence >= 250));
        }
    }
}

#[test]
fn restores_cannot_multiply_cap_across_sibling_roots() {
    for retry in [false, true] {
        let mut queue = EventQueue::new(DedupMode::Queue);
        let channel = Uuid::new_v4();
        let now = Instant::now();
        for i in 0..50 {
            queue.push(event(channel, &"a".repeat(64), i, now));
        }
        let batch = queue.flush_next().expect("initial batch");
        for i in 50..550 {
            queue.push(event(channel, &"b".repeat(64), i, now));
        }
        if retry {
            assert!(queue.requeue(batch).is_none());
        } else {
            queue.requeue_preserve_timestamps(batch);
        }
        assert_eq!(count(&queue, channel), MAX_PENDING_PER_CHANNEL);
    }
}

#[test]
fn native_steer_restore_paths_obey_the_same_routing_cap() {
    for expired in [false, true] {
        let mut queue = EventQueue::new(DedupMode::Queue);
        let channel = Uuid::new_v4();
        let now = Instant::now();
        let withheld = event(channel, &"a".repeat(64), 0, now);
        let conversation = withheld.channel_id;
        let event_id = withheld.event.id.to_hex();
        queue.push(withheld);
        assert!(queue.mark_native_steer_pending(conversation, &event_id));
        for i in 1..=MAX_PENDING_PER_CHANNEL {
            queue.push(event(channel, &"b".repeat(64), i, now));
        }
        if expired {
            queue.recover_withheld_for_expired_channel(conversation);
        } else {
            queue.release_native_steer(conversation, &event_id);
        }
        assert_eq!(count(&queue, channel), MAX_PENDING_PER_CHANNEL);
        assert!(!queue.withheld_native_steer.contains_key(&conversation));
        assert!(queue
            .queues
            .values()
            .flatten()
            .all(|event| event.event.content != "0"));
    }
}
