// Included in lib.rs's crew_thread_cancel_tests module; calls the real control handler.

#[tokio::test]
async fn thread_exact_cancel_releases_lease_only_for_an_accepted_cancel() {
    let channel = Uuid::new_v4();
    let conversation = Uuid::new_v4();
    let mut pool = AgentPool::from_slots(vec![]);
    let mut receiver = thread_exact_cancel_task(&mut pool, channel, conversation, "current");
    let mut queue = thread_exact_cancel_queue(conversation);
    let observer = observer::ObserverHandle::in_process();
    let mut releases = Vec::new();
    for (turn, expected_releases) in [("retired", 0), ("", 0), ("current", 1)] {
        let payload = serde_json::json!({ "channelId": channel.to_string(), "conversationId": conversation.to_string(), "turnId": turn });
        handle_cancel_turn_control_with_release(
            &payload,
            &mut pool,
            &mut queue,
            None,
            Some(&observer),
            |channel, reason| releases.push((channel, reason.to_owned())),
        );
        assert_eq!(
            releases.len(),
            expected_releases,
            "a stale/invalid target must not release the successor's lease"
        );
    }
    assert_eq!(
        releases,
        vec![(Some(channel.to_string()), "cancel".to_owned())]
    );
    assert_eq!(receiver.try_recv().unwrap(), ControlSignal::Cancel);
}
fn thread_exact_cancel_queue(conversation: Uuid) -> EventQueue {
    let mut queue = EventQueue::new(DedupMode::Queue);
    queue.push(QueuedEvent {
        channel_id: conversation,
        event: nostr::EventBuilder::new(nostr::Kind::Custom(9), "newer queued work")
            .sign_with_keys(&nostr::Keys::generate())
            .unwrap(),
        received_at: std::time::Instant::now(),
        prompt_tag: "mention".into(),
        edited_content: None,
        hold_exempt: false,
    });
    queue
}

fn thread_exact_cancel_task(
    pool: &mut AgentPool,
    channel: Uuid,
    conversation: Uuid,
    turn: &str,
) -> tokio::sync::oneshot::Receiver<ControlSignal> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let handle = pool.join_set.spawn(std::future::pending());
    pool.task_map_mut().insert(
        handle.id(),
        pool::TaskMeta {
            agent_index: 0,
            channel_id: Some(conversation),
            routing_channel_id: Some(channel),
            turn_id: turn.into(),
            recoverable_batch: None,
            control_tx: Some(tx),
            steer_tx: None,
            successful_steer_deliveries: HashSet::new(),
        },
    );
    rx
}

fn thread_exact_cancel_invoke(
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    channel: Uuid,
    conversation: Uuid,
    turn: Option<&str>,
) -> serde_json::Value {
    let observer = observer::ObserverHandle::in_process();
    let mut payload = serde_json::json!({
        "type": "cancel_turn", "channelId": channel.to_string(),
        "conversationId": conversation.to_string(), "requestId": "selected-action",
    });
    if let Some(turn) = turn {
        payload["turnId"] = serde_json::json!(turn);
    }
    handle_cancel_turn_control(&payload, pool, queue, None, Some(&observer));
    let frames = observer.snapshot();
    let result = frames
        .iter()
        .find(|frame| frame.kind == "control_result")
        .unwrap();
    assert_eq!(result.payload["requestId"], "selected-action");
    result.payload.clone()
}

#[tokio::test]
async fn thread_exact_cancel_stale_turn_preserves_newer_queued_work() {
    let channel = Uuid::new_v4();
    let conversation = Uuid::new_v4();
    let mut pool = AgentPool::from_slots(vec![]);
    let mut queue = thread_exact_cancel_queue(conversation);
    let result = thread_exact_cancel_invoke(
        &mut pool,
        &mut queue,
        channel,
        conversation,
        Some("retired-a"),
    );
    assert_eq!(result["status"], "no_active_turn");
    assert_eq!(queue.queued_event_count(&conversation), 1);
}

#[tokio::test]
async fn thread_exact_cancel_stale_turn_preserves_replacement_and_queue() {
    let channel = Uuid::new_v4();
    let conversation = Uuid::new_v4();
    let mut pool = AgentPool::from_slots(vec![]);
    let mut receiver = thread_exact_cancel_task(&mut pool, channel, conversation, "replacement-b");
    let mut queue = thread_exact_cancel_queue(conversation);
    let result = thread_exact_cancel_invoke(
        &mut pool,
        &mut queue,
        channel,
        conversation,
        Some("retired-a"),
    );
    assert_eq!(result["status"], "no_active_turn");
    assert_eq!(queue.queued_event_count(&conversation), 1);
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn thread_exact_cancel_known_turn_requires_channel_and_conversation() {
    for wrong_channel in [true, false] {
        let channel = Uuid::new_v4();
        let conversation = Uuid::new_v4();
        let mut pool = AgentPool::from_slots(vec![]);
        let mut receiver = thread_exact_cancel_task(&mut pool, channel, conversation, "current");
        let request_channel = if wrong_channel {
            Uuid::new_v4()
        } else {
            channel
        };
        let request_conversation = if wrong_channel {
            conversation
        } else {
            Uuid::new_v4()
        };
        let mut queue = thread_exact_cancel_queue(request_conversation);
        let result = thread_exact_cancel_invoke(
            &mut pool,
            &mut queue,
            request_channel,
            request_conversation,
            Some("current"),
        );
        assert_eq!(result["status"], "no_active_turn");
        assert_eq!(queue.queued_event_count(&request_conversation), 1);
        assert!(matches!(
            receiver.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
    }
}

#[tokio::test]
async fn thread_exact_cancel_closed_receiver_is_not_sent_and_does_not_drain() {
    let channel = Uuid::new_v4();
    let conversation = Uuid::new_v4();
    let mut pool = AgentPool::from_slots(vec![]);
    drop(thread_exact_cancel_task(
        &mut pool,
        channel,
        conversation,
        "current",
    ));
    let mut queue = thread_exact_cancel_queue(conversation);
    let result = thread_exact_cancel_invoke(
        &mut pool,
        &mut queue,
        channel,
        conversation,
        Some("current"),
    );
    assert_eq!(result["status"], "no_active_turn");
    assert_eq!(queue.queued_event_count(&conversation), 1);
}

#[tokio::test]
async fn thread_exact_cancel_success_preserves_other_queued_work() {
    let channel = Uuid::new_v4();
    let conversation = Uuid::new_v4();
    let mut pool = AgentPool::from_slots(vec![]);
    let mut receiver = thread_exact_cancel_task(&mut pool, channel, conversation, "current");
    let mut queue = thread_exact_cancel_queue(conversation);
    let result = thread_exact_cancel_invoke(
        &mut pool,
        &mut queue,
        channel,
        conversation,
        Some("current"),
    );
    assert_eq!(result["status"], "sent");
    assert_eq!(receiver.try_recv().unwrap(), ControlSignal::Cancel);
    assert_eq!(queue.queued_event_count(&conversation), 1);
}

#[tokio::test]
async fn thread_exact_cancel_without_turn_keeps_conversation_drain_compatibility() {
    let channel = Uuid::new_v4();
    let conversation = Uuid::new_v4();
    let mut pool = AgentPool::from_slots(vec![]);
    let mut queue = thread_exact_cancel_queue(conversation);
    let result = thread_exact_cancel_invoke(&mut pool, &mut queue, channel, conversation, None);
    assert_eq!(result["status"], "cancelled_queued");
    assert_eq!(queue.queued_event_count(&conversation), 0);
}

#[tokio::test]
async fn thread_exact_cancel_malformed_explicit_turn_never_uses_legacy_fallback() {
    for turn_value in [
        serde_json::json!(""),
        serde_json::json!("  "),
        serde_json::json!(false),
        serde_json::json!(42),
        serde_json::json!(null),
        serde_json::json!({}),
    ] {
        let channel = Uuid::new_v4();
        let conversation = Uuid::new_v4();
        let mut pool = AgentPool::from_slots(vec![]);
        let mut receiver = thread_exact_cancel_task(&mut pool, channel, conversation, "current");
        let mut queue = thread_exact_cancel_queue(conversation);
        let observer = observer::ObserverHandle::in_process();
        let payload = serde_json::json!({"type": "cancel_turn", "channelId": channel.to_string(), "conversationId": conversation.to_string(), "turnId": turn_value, "requestId": "malformed"});
        handle_cancel_turn_control(&payload, &mut pool, &mut queue, None, Some(&observer));
        assert_eq!(
            queue.queued_event_count(&conversation),
            1,
            "malformed explicit target cannot drain"
        );
        assert!(
            matches!(
                receiver.try_recv(),
                Err(tokio::sync::oneshot::error::TryRecvError::Empty)
            ),
            "malformed explicit target cannot signal current task"
        );
        assert!(observer
            .snapshot()
            .iter()
            .filter(|event| event.kind == "control_result")
            .all(|event| event.payload["status"] != "sent"
                && event.payload["status"] != "cancelled_queued"));
    }
}
