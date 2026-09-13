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
            session_id: TaskSessionIdentity::default(),
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

fn thread_exact_steer_task(
    pool: &mut AgentPool,
    channel: Uuid,
    conversation: Uuid,
    session: &str,
    turn: &str,
) -> (
    tokio::sync::mpsc::Receiver<pool::SteerRequest>,
    TaskSessionIdentity,
) {
    let (steer_tx, steer_rx) = tokio::sync::mpsc::channel(1);
    let session_identity = TaskSessionIdentity::new(Some(session.into()));
    let handle = pool.join_set.spawn(std::future::pending());
    pool.task_map_mut().insert(
        handle.id(),
        pool::TaskMeta {
            agent_index: 0,
            channel_id: Some(conversation),
            routing_channel_id: Some(channel),
            session_id: session_identity.clone(),
            turn_id: turn.into(),
            recoverable_batch: None,
            control_tx: None,
            steer_tx: Some(steer_tx),
            successful_steer_deliveries: HashSet::new(),
        },
    );
    (steer_rx, session_identity)
}

async fn wait_for_steer_result(
    observer: &observer::ObserverHandle,
    expected_status: &str,
) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(frame) = observer
                .snapshot()
                .into_iter()
                .rev()
                .find(|frame| frame.kind == "control_result")
            {
                assert_eq!(frame.payload["status"], expected_status);
                return frame.payload;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("steer control result must be emitted")
}

#[tokio::test]
async fn thread_exact_steer_crosses_handler_and_pool_with_exact_target() {
    let channel = Uuid::new_v4();
    let conversation = Uuid::new_v4();
    let session = "selected-session";
    let turn = "selected-turn";
    let request_id = Uuid::new_v4();
    let mut pool = AgentPool::from_slots(vec![]);
    let (mut receiver, _) =
        thread_exact_steer_task(&mut pool, channel, conversation, session, turn);
    let observer = observer::ObserverHandle::in_process();

    handle_steer_turn_control(
        &serde_json::json!({
            "type": "steer_turn",
            "channelId": channel,
            "conversationId": conversation,
            "sessionId": session,
            "turnId": turn,
            "requestId": request_id,
            "prompt": "keep the selected run focused"
        }),
        &mut pool,
        Some(&observer),
    );

    let request = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("handler must queue the exact steer")
        .expect("selected task must receive the steer");
    assert_eq!(
        request.prompt_blocks,
        vec!["keep the selected run focused".to_owned()]
    );
    assert_eq!(
        request.strict_target,
        Some(pool::StrictSteerTarget {
            session_id: session.into(),
            turn_id: turn.into(),
            request_id: request_id.to_string(),
        })
    );
    assert!(request
        .ack_tx
        .send(pool::SteerAck::Success {
            session_id: session.into(),
        })
        .is_ok());
    let result = wait_for_steer_result(&observer, "appended").await;
    assert_eq!(result["type"], "steer_turn");
    assert_eq!(result["requestId"], request_id.to_string());
    assert_eq!(result["sessionId"], session);
    assert_eq!(result["turnId"], turn);
}

#[tokio::test]
async fn thread_exact_steer_reports_unsupported_adapter_without_claiming_delivery() {
    let channel = Uuid::new_v4();
    let conversation = Uuid::new_v4();
    let session = "selected-session";
    let turn = "selected-turn";
    let request_id = Uuid::new_v4();
    let mut pool = AgentPool::from_slots(vec![]);
    let (mut receiver, _) =
        thread_exact_steer_task(&mut pool, channel, conversation, session, turn);
    let observer = observer::ObserverHandle::in_process();

    handle_steer_turn_control(
        &serde_json::json!({
            "type": "steer_turn",
            "channelId": channel,
            "conversationId": conversation,
            "sessionId": session,
            "turnId": turn,
            "requestId": request_id,
            "prompt": "keep the selected run focused"
        }),
        &mut pool,
        Some(&observer),
    );

    let request = receiver.recv().await.expect("selected task receives steer");
    assert!(request
        .ack_tx
        .send(pool::SteerAck::Err(pool::SteerError::ExpectedRunIdMissing))
        .is_ok());
    let result = wait_for_steer_result(&observer, "rejected").await;
    assert_eq!(result["requestId"], request_id.to_string());
    assert_eq!(
        result["error"],
        "selected adapter does not support strict steering"
    );
}

#[tokio::test]
async fn pool_exact_steer_rejects_stale_session_before_queueing() {
    let channel = Uuid::new_v4();
    let conversation = Uuid::new_v4();
    let mut pool = AgentPool::from_slots(vec![]);
    let (mut receiver, _) =
        thread_exact_steer_task(&mut pool, channel, conversation, "current-session", "turn");
    let (ack_tx, _ack_rx) = tokio::sync::oneshot::channel();
    let request = pool::SteerRequest {
        prompt_blocks: vec!["stale selection".into()],
        strict_target: Some(pool::StrictSteerTarget {
            session_id: "retired-session".into(),
            turn_id: "turn".into(),
            request_id: Uuid::new_v4().to_string(),
        }),
        ack_tx,
    };

    assert!(matches!(
        pool.send_exact_steer(channel, conversation, "turn", request),
        Err(pool::SteerError::StrictTargetMismatch)
    ));
    assert!(receiver.try_recv().is_err());
}

#[tokio::test]
async fn pool_exact_steer_tracks_session_replacement_and_rejects_retired_target() {
    let channel = Uuid::new_v4();
    let conversation = Uuid::new_v4();
    let turn = "rotated-turn";
    let mut pool = AgentPool::from_slots(vec![]);
    let (mut receiver, session_identity) =
        thread_exact_steer_task(&mut pool, channel, conversation, "old-session", turn);
    session_identity.set("new-session".into());

    let (stale_ack_tx, _stale_ack_rx) = tokio::sync::oneshot::channel();
    let stale_request = pool::SteerRequest {
        prompt_blocks: vec!["before rotation".into()],
        strict_target: Some(pool::StrictSteerTarget {
            session_id: "old-session".into(),
            turn_id: turn.into(),
            request_id: Uuid::new_v4().to_string(),
        }),
        ack_tx: stale_ack_tx,
    };
    assert!(matches!(
        pool.send_exact_steer(channel, conversation, turn, stale_request),
        Err(pool::SteerError::StrictTargetMismatch)
    ));
    assert!(receiver.try_recv().is_err());

    let (ack_tx, _ack_rx) = tokio::sync::oneshot::channel();
    let request = pool::SteerRequest {
        prompt_blocks: vec!["after rotation".into()],
        strict_target: Some(pool::StrictSteerTarget {
            session_id: "new-session".into(),
            turn_id: turn.into(),
            request_id: Uuid::new_v4().to_string(),
        }),
        ack_tx,
    };

    assert!(pool
        .send_exact_steer(channel, conversation, turn, request)
        .is_ok());
    assert!(receiver.recv().await.is_some());
}
