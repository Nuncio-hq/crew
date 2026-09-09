//! Scheduling contracts exercised through the real publish queue.
use super::*;

fn event(seq: u64, kind: &str, channel: Option<&str>) -> observer::ObserverEvent {
    observer::ObserverEvent {
        seq,
        timestamp: "2026-09-09T00:00:00Z".into(),
        kind: kind.into(),
        agent_index: Some(0),
        channel_id: channel.map(str::to_owned),
        conversation_id: channel.map(str::to_owned),
        session_id: Some("session-1".into()),
        turn_id: Some("turn-1".into()),
        started_at: None,
        payload: serde_json::json!({}),
    }
}

fn chunk(seq: u64, channel: &str, text: &str) -> observer::ObserverEvent {
    let mut event = event(seq, "acp_read", Some(channel));
    event.payload = serde_json::json!({
        "jsonrpc": "2.0", "method": "session/update", "params": {
            "sessionId": "session-1", "update": {
                "sessionUpdate": "agent_message_chunk", "messageId": "message-1",
                "content": {"type": "text", "text": text}
            }
        }
    });
    event
}

fn seqs(frame: &observer::ObserverEvent) -> Vec<u64> {
    match frame.payload["events"].as_array() {
        Some(events) => events
            .iter()
            .map(|event| event["seq"].as_u64().unwrap())
            .collect(),
        None => vec![frame.seq],
    }
}

#[tokio::test(start_paused = true)]
async fn completion_channel_gets_first_legal_slot_under_three_channel_chunk_backlog() {
    let mut queue = ObserverPublishQueue::default();
    for seq in 1..=9 {
        queue.ingest(chunk(
            seq,
            ["a", "b", "c"][(seq as usize - 1) % 3],
            "stream ",
        ));
    }
    queue.ingest(event(10, "turn_completed", Some("c")));
    assert_eq!(queue.dropped_events, 0);
    let start = tokio::time::Instant::now();
    let mut tick = tokio::time::interval_at(start + OBSERVER_PUBLISH_TICK, OBSERVER_PUBLISH_TICK);
    // An unrestricted slot fixture: every tick permits one queue selection.
    let mut slots = 0;
    let mut completion_slot = None;
    let mut channels = Vec::new();
    while !queue.is_empty() {
        tick.tick().await;
        slots += 1;
        let frame = queue.next_frame().expect("nonempty queue");
        assert!(serialized_len(&frame) <= OBSERVER_MAX_PLAINTEXT_LEN);
        if seqs(&frame).contains(&10) {
            completion_slot = Some(slots);
            assert_eq!(
                seqs(&frame),
                [9, 10],
                "coalesced C text precedes completion"
            );
        }
        channels.push(frame.channel_id);
    }
    assert_eq!(
        slots, 3,
        "existing same-channel batching must remain effective"
    );
    assert_eq!(
        tokio::time::Instant::now() - start,
        OBSERVER_PUBLISH_TICK * 3
    );
    assert_eq!(queue.pending_bytes, 0);
    assert_eq!(queue.dropped_events, 0);
    assert_eq!(
        completion_slot,
        Some(1),
        "completion must use first legal slot; channel order: {channels:?}"
    );
}

fn queue(events: impl IntoIterator<Item = observer::ObserverEvent>) -> ObserverPublishQueue {
    let mut queue = ObserverPublishQueue::default();
    for event in events {
        queue.ingest(event);
    }
    queue
}

fn next(queue: &mut ObserverPublishQueue) -> observer::ObserverEvent {
    let frame = queue.next_frame().expect("pending frame");
    assert!(serialized_len(&frame) <= OBSERVER_MAX_PLAINTEXT_LEN);
    assert_eq!(
        queue.pending_bytes,
        queue
            .events
            .iter()
            .map(|(_, _, event)| serialized_len(event))
            .sum::<usize>()
    );
    frame
}

#[test]
fn oldest_urgent_event_selects_channel_and_keeps_its_causal_predecessors() {
    let mut queue = queue([
        event(1, "acp_read", Some("normal")),
        event(2, "acp_read", Some("later-urgent")),
        event(3, "turn_started", Some("first-urgent")),
        event(4, "turn_completed", Some("later-urgent")),
        event(5, "turn_completed", Some("first-urgent")),
    ]);
    assert_eq!(
        seqs(&next(&mut queue)),
        [3, 5],
        "oldest urgent event, not oldest mixed channel"
    );
    assert_eq!(
        seqs(&next(&mut queue)),
        [2, 4],
        "normal predecessor stays before terminal"
    );
    assert_eq!(seqs(&next(&mut queue)), [1]);
}

#[test]
fn two_urgent_frames_then_oldest_normal_under_repeated_urgent_arrivals() {
    let mut queue = queue([
        event(1, "acp_read", Some("normal-a")),
        event(2, "acp_read", Some("normal-b")),
    ]);
    for (seq, channel) in [(3, "urgent-a"), (4, "urgent-b")] {
        queue.ingest(event(seq, "turn_completed", Some(channel)));
        assert_eq!(next(&mut queue).channel_id.as_deref(), Some(channel));
    }
    queue.ingest(event(5, "turn_error", Some("urgent-c")));
    assert_eq!(
        seqs(&next(&mut queue)),
        [1],
        "two urgent slots force oldest normal"
    );
    assert_eq!(seqs(&next(&mut queue)), [5]);
    queue.ingest(event(6, "turn_started", Some("urgent-d")));
    assert_eq!(seqs(&next(&mut queue)), [6]);
    queue.ingest(event(7, "turn_started", Some("urgent-e")));
    assert_eq!(
        seqs(&next(&mut queue)),
        [2],
        "normal-b cannot starve on a continuing flood"
    );
}

#[test]
fn null_barrier_remains_a_strict_limit_even_for_completion() {
    let mut queue = queue([
        event(1, "acp_read", Some("a")),
        event(2, "agent_panic", None),
        event(3, "turn_completed", Some("c")),
        event(4, "turn_error", None),
        event(5, "turn_completed", Some("c")),
    ]);
    for seq in 1..=5 {
        assert_eq!(
            seqs(&next(&mut queue)),
            [seq],
            "urgent must not cross a null barrier"
        );
    }
}

#[test]
fn urgency_and_normal_waiters_are_classified_only_before_the_barrier() {
    let mut queue = queue([
        event(1, "acp_read", Some("a")),
        event(2, "acp_read", Some("c")),
        event(3, "agent_panic", None),
        event(4, "turn_completed", Some("c")),
    ]);
    assert_eq!(
        seqs(&next(&mut queue)),
        [1],
        "post-barrier terminal cannot promote C's earlier text"
    );
    assert_eq!(seqs(&next(&mut queue)), [2]);
    assert_eq!(seqs(&next(&mut queue)), [3]);
    assert_eq!(seqs(&next(&mut queue)), [4]);

    for (seq, channel) in [(5, "a"), (6, "b"), (7, "c")] {
        queue.ingest(event(seq, "turn_completed", Some(channel)));
    }
    queue.ingest(event(8, "agent_panic", None));
    queue.ingest(event(9, "acp_read", Some("normal-behind-barrier")));
    for seq in 5..=9 {
        assert_eq!(
            seqs(&next(&mut queue)),
            [seq],
            "ineligible normal cannot force a pick across the barrier"
        );
    }
}

#[test]
fn same_channel_over_one_frame_preserves_order_instead_of_promising_one_tick() {
    let mut events = Vec::new();
    for seq in 1..=3 {
        let mut event = event(seq, "acp_read", Some("c"));
        event.payload = serde_json::json!({"text": "x".repeat(30_000)});
        events.push(event);
    }
    events.push(event(4, "turn_completed", Some("c")));
    let mut queue = queue(events);
    assert_eq!(seqs(&next(&mut queue)), [1, 2]);
    assert_eq!(
        seqs(&next(&mut queue)),
        [3, 4],
        "completion waits for its own causal backlog"
    );
    assert!(queue.is_empty());
}

#[test]
fn mixed_channel_becomes_normal_when_its_urgent_event_has_left() {
    let mut queue = queue([event(1, "turn_started", Some("mixed"))]);
    let mut large = event(2, "acp_read", Some("mixed"));
    large.payload = serde_json::json!({"text": "x".repeat(60_000)});
    queue.ingest(large.clone());
    large.seq = 3;
    queue.ingest(large);
    assert_eq!(seqs(&next(&mut queue)), [1, 2]);
    queue.ingest(event(4, "turn_completed", Some("other")));
    assert_eq!(
        seqs(&next(&mut queue)),
        [4],
        "remaining mixed text has no stale urgency"
    );
    assert_eq!(seqs(&next(&mut queue)), [3]);
}

#[test]
fn exact_lifecycle_kinds_are_urgent_but_arbitrary_done_text_is_not() {
    for kind in [
        "turn_started",
        "turn_completed",
        "turn_error",
        "turn_retrying",
        "agent_panic",
    ] {
        let mut queue = queue([
            event(1, "acp_read", Some("normal")),
            event(2, kind, Some("urgent")),
        ]);
        assert_eq!(seqs(&next(&mut queue)), [2], "{kind}");
    }
    for kind in [
        "done",
        "almost_turn_completed",
        "turn_liveness",
        "acp_parse_error",
    ] {
        let mut queue = queue([
            event(1, "acp_read", Some("normal")),
            event(2, kind, Some("other")),
        ]);
        assert_eq!(seqs(&next(&mut queue)), [1], "{kind} must remain normal");
    }
}

#[test]
fn permission_and_input_requests_require_exact_read_method_and_request_shape() {
    for method in ["session/request_permission", "elicitation/create"] {
        for id in [serde_json::json!(17), serde_json::json!("request-1")] {
            let mut request = event(2, "acp_read", Some("urgent"));
            request.payload =
                serde_json::json!({"jsonrpc":"2.0", "id":id, "method":method, "params":{}});
            let mut queue = queue([event(1, "acp_read", Some("normal")), request]);
            assert_eq!(seqs(&next(&mut queue)), [2], "{method} request");
        }
        for (kind, payload) in [
            (
                "acp_write",
                serde_json::json!({"id":1,"method":method,"params":{}}),
            ),
            ("acp_read", serde_json::json!({"method":method,"params":{}})),
            (
                "acp_read",
                serde_json::json!({"id":null,"method":method,"params":{}}),
            ),
            (
                "acp_read",
                serde_json::json!({"id":{},"method":method,"params":{}}),
            ),
            (
                "acp_read",
                serde_json::json!({"id":1,"method":method,"params":null}),
            ),
            (
                "acp_read",
                serde_json::json!({"id":1,"method":format!("{method}/done"),"params":{}}),
            ),
        ] {
            let mut request = event(2, kind, Some("other"));
            request.payload = payload;
            let mut queue = queue([event(1, "acp_read", Some("normal")), request]);
            assert_eq!(seqs(&next(&mut queue)), [1], "non-request must stay normal");
        }
    }
}

#[test]
fn known_control_results_are_urgent_without_promoting_arbitrary_payload_text() {
    for control in [
        "cancel_turn",
        "switch_model",
        "retry_turn",
        "guided_handover",
        "blind_session_reset",
    ] {
        let mut result = event(2, "control_result", Some("urgent"));
        result.payload = serde_json::json!({"type":control,"status":"failure"});
        let mut queue = queue([event(1, "acp_read", Some("normal")), result]);
        assert_eq!(
            seqs(&next(&mut queue)),
            [2],
            "{control}, including failures"
        );
    }
    for (kind, payload) in [
        (
            "control_result",
            serde_json::json!({"type":"unknown","status":"done"}),
        ),
        ("control_result", serde_json::json!({"type":"cancel_turn"})),
        (
            "acp_read",
            serde_json::json!({"type":"cancel_turn","status":"done"}),
        ),
    ] {
        let mut result = event(2, kind, Some("other"));
        result.payload = payload;
        let mut queue = queue([event(1, "acp_read", Some("normal")), result]);
        assert_eq!(seqs(&next(&mut queue)), [1]);
    }
}

#[test]
fn fairness_counter_resets_without_normal_waiters_and_after_empty_queue() {
    for drain_empty in [false, true] {
        let mut queue = queue([
            event(1, "acp_read", Some("normal")),
            event(2, "turn_completed", Some("urgent-a")),
            event(3, "turn_completed", Some("urgent-b")),
        ]);
        assert_eq!(seqs(&next(&mut queue)), [2]);
        assert_eq!(seqs(&next(&mut queue)), [3]);
        assert_eq!(seqs(&next(&mut queue)), [1]);
        if drain_empty {
            assert!(queue.next_frame().is_none());
        }
        queue.ingest(event(4, "turn_completed", Some("urgent-c")));
        assert_eq!(seqs(&next(&mut queue)), [4]);
        queue.ingest(event(5, "acp_read", Some("normal")));
        queue.ingest(event(6, "turn_completed", Some("urgent-d")));
        assert_eq!(seqs(&next(&mut queue)), [6]);
        queue.ingest(event(7, "turn_completed", Some("urgent-e")));
        assert_eq!(
            seqs(&next(&mut queue)),
            [7],
            "uncontended urgent slot must not charge new normal waiters"
        );
    }
}

#[test]
fn urgent_eviction_leaves_no_stale_channel_priority() {
    let mut queue = queue([event(1, "turn_started", Some("evicted"))]);
    for seq in 2..=90 {
        let mut item = event(seq, "acp_read", Some("normal"));
        item.payload = serde_json::json!({"text":"x".repeat(60_000)});
        queue.ingest(item);
    }
    queue.ingest(event(91, "acp_read", Some("evicted")));
    assert!(
        queue.dropped_events > 0,
        "fixture must evict the urgent source"
    );
    assert!(queue.events.iter().all(|(_, _, event)| event.seq != 1));
    assert!(queue.total_pending_bytes() <= OBSERVER_PENDING_QUEUE_MAX_BYTES);
    assert_eq!(next(&mut queue).channel_id.as_deref(), Some("normal"));
    let retained_sources: u64 = queue.events.iter().map(|(_, sources, _)| sources).sum();
    assert_eq!(
        queue.dropped_events + retained_sources + 1,
        91,
        "one source published; eviction remains in source units"
    );
}

#[test]
fn pending_coalescer_flush_and_oversized_control_keep_budget_and_source_counts() {
    let mut queue = queue([event(1, "acp_read", Some("normal"))]);
    let mut control = event(2, "control_result", Some("urgent"));
    control.payload = serde_json::json!({"type":"guided_handover","status":"failure","error":"x".repeat(100_000)});
    queue.ingest(control);
    queue.ingest(chunk(3, "tail", "one"));
    queue.ingest(chunk(4, "tail", "two"));
    assert!(
        !queue.coalescer.pending.is_empty(),
        "fixture reaches pending buffer"
    );
    assert_eq!(seqs(&next(&mut queue)), [2]);
    assert!(
        queue.coalescer.pending.is_empty(),
        "selection flushes pending chunks"
    );
    assert_eq!(seqs(&next(&mut queue)), [1]);
    let tail = next(&mut queue);
    assert_eq!(seqs(&tail), [4]);
    assert_eq!(
        tail.payload["params"]["update"]["content"]["text"],
        "onetwo"
    );
    assert_eq!(queue.dropped_events, 0);
    assert_eq!(queue.total_pending_bytes(), 0);
}

#[tokio::test(start_paused = true)]
async fn production_publisher_keeps_urgent_frames_on_the_existing_tick() {
    let observer = observer::ObserverHandle::in_process();
    let rx = observer.subscribe();
    drop(observer);
    let keys = nostr::Keys::generate();
    let owner = nostr::Keys::generate();
    let (publisher, mut received) = RelayEventPublisher::test_pair();
    let started = tokio::time::Instant::now();
    let task = tokio::spawn(run_relay_observer_publisher(
        vec![
            event(1, "acp_read", Some("a")),
            event(2, "acp_read", Some("b")),
            event(3, "turn_completed", Some("c")),
        ],
        rx,
        publisher,
        keys.clone(),
        keys.public_key().to_hex(),
        owner.public_key().to_hex(),
        owner.public_key(),
    ));
    // This transport accepts every frame. The production timer, not the test,
    // determines when next_frame may run (including the first legal slot).
    for expected_slot in 1..=3 {
        let signed = received.recv().await.expect("published frame");
        let payload: serde_json::Value =
            decrypt_observer_payload(&owner, &signed).expect("valid encrypted observer frame");
        assert_eq!(
            tokio::time::Instant::now() - started,
            OBSERVER_PUBLISH_TICK * expected_slot
        );
        if expected_slot == 1 {
            assert_eq!(payload["kind"], "turn_completed");
        }
    }
    task.await.expect("publisher exited cleanly");
    assert!(received.recv().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn failed_publisher_preserves_paced_best_effort_drain() {
    let observer = observer::ObserverHandle::in_process();
    let rx = observer.subscribe();
    drop(observer);
    let keys = nostr::Keys::generate();
    let owner = nostr::Keys::generate();
    let (publisher, received) = RelayEventPublisher::test_pair();
    drop(received);
    // Close the forwarding task before the measured run, so every attempt
    // fails at the existing publisher boundary. This does not claim an ACK.
    publisher
        .publish_event(
            nostr::EventBuilder::text_note("fixture")
                .sign_with_keys(&keys)
                .expect("fixture event"),
        )
        .await
        .expect("first forwarding enqueue");
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }
    let started = tokio::time::Instant::now();
    run_relay_observer_publisher(
        vec![
            event(1, "acp_read", Some("a")),
            event(2, "turn_completed", Some("b")),
            event(3, "turn_error", Some("c")),
        ],
        rx,
        publisher,
        keys.clone(),
        keys.public_key().to_hex(),
        owner.public_key().to_hex(),
        owner.public_key(),
    )
    .await;
    assert_eq!(
        tokio::time::Instant::now() - started,
        OBSERVER_PUBLISH_TICK * 3,
        "failure must not bypass the pacer or make the queue retry indefinitely"
    );
}
