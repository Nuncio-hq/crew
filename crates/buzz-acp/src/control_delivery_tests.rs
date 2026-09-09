//! A completed prompt can drop its control receiver before finalization
//! removes TaskMeta. Exercise the actual control handlers in that window.
use super::*;

fn in_flight() -> (
    AgentPool,
    Uuid,
    Uuid,
    tokio::sync::oneshot::Receiver<ControlSignal>,
) {
    let mut pool = AgentPool::from_slots(vec![]);
    let channel = Uuid::new_v4();
    let conversation = Uuid::new_v4();
    let (control_tx, control_rx) = tokio::sync::oneshot::channel();
    let task = pool.join_set.spawn(std::future::pending::<()>());
    pool.task_map_mut().insert(
        task.id(),
        pool::TaskMeta {
            agent_index: 0,
            channel_id: Some(conversation),
            routing_channel_id: Some(channel),
            turn_id: "finishing-turn".into(),
            recoverable_batch: None,
            control_tx: Some(control_tx),
            steer_tx: None,
            successful_steer_deliveries: HashSet::new(),
        },
    );
    (pool, channel, conversation, control_rx)
}

fn control(channel: Uuid, conversation: Uuid, exact: bool) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "channelId":channel, "conversationId":conversation,
        "requestId":"request-1", "modelId":"candidate-model"
    });
    if exact {
        payload["turnId"] = "finishing-turn".into();
    }
    payload
}

fn enqueue(queue: &mut EventQueue, conversation: Uuid) {
    queue.push(QueuedEvent {
        channel_id: conversation,
        event: nostr::EventBuilder::text_note("queued work")
            .sign_with_keys(&nostr::Keys::generate())
            .expect("fixture event"),
        received_at: std::time::Instant::now(),
        prompt_tag: "mention".into(),
        edited_content: None,
        hold_exempt: false,
    });
}

fn result(observer: &observer::ObserverHandle, expected_status: &str) -> serde_json::Value {
    let snapshot = observer.snapshot();
    let event = snapshot.last().expect("control result emitted");
    assert_eq!(event.kind, "control_result");
    assert_eq!(event.payload["requestId"], "request-1");
    assert_eq!(event.payload["status"], expected_status);
    event.payload.clone()
}

#[tokio::test]
async fn closed_receiver_cancel_reports_no_active_turn_for_exact_and_conversation_targets() {
    for exact in [true, false] {
        let (mut pool, channel, conversation, rx) = in_flight();
        drop(rx);
        let mut queue = EventQueue::new(DedupMode::Queue);
        let observer = observer::ObserverHandle::in_process();
        handle_cancel_turn_control(
            &control(channel, conversation, exact),
            &mut pool,
            &mut queue,
            None,
            Some(&observer),
        );
        assert_eq!(result(&observer, "no_active_turn")["drainedCount"], 0);
    }
}

#[tokio::test]
async fn closed_receiver_cancel_drains_only_its_queued_conversation() {
    for exact in [true, false] {
        let (mut pool, channel, conversation, rx) = in_flight();
        drop(rx);
        let sibling = Uuid::new_v4();
        let mut queue =
            EventQueue::new(DedupMode::Queue).with_dispatch_hold(Duration::from_secs(2));
        enqueue(&mut queue, conversation);
        enqueue(&mut queue, sibling);
        let observer = observer::ObserverHandle::in_process();
        handle_cancel_turn_control(
            &control(channel, conversation, exact),
            &mut pool,
            &mut queue,
            None,
            Some(&observer),
        );
        assert_eq!(result(&observer, "cancelled_queued")["drainedCount"], 1);
        assert_eq!(queue.queued_event_count(&conversation), 0);
        assert_eq!(queue.queued_event_count(&sibling), 1);
    }
}

#[tokio::test]
async fn closed_receiver_switch_reports_existing_turn_ending_fallback() {
    for exact in [true, false] {
        let (mut pool, channel, conversation, rx) = in_flight();
        drop(rx);
        let observer = observer::ObserverHandle::in_process();
        handle_switch_model_control(
            &control(channel, conversation, exact),
            &mut pool,
            Some(&observer),
        );
        assert_eq!(
            result(&observer, "turn_ending")["modelId"],
            "candidate-model"
        );
    }
}

#[tokio::test]
async fn open_receiver_cancel_sends_once_and_preserves_queued_work_until_fallback() {
    for exact in [true, false] {
        let (mut pool, channel, conversation, rx) = in_flight();
        let mut queue = EventQueue::new(DedupMode::Queue);
        enqueue(&mut queue, conversation);
        let observer = observer::ObserverHandle::in_process();
        let payload = control(channel, conversation, exact);
        handle_cancel_turn_control(&payload, &mut pool, &mut queue, None, Some(&observer));
        assert_eq!(result(&observer, "sent")["drainedCount"], 0);
        assert_eq!(rx.await.expect("control received"), ControlSignal::Cancel);
        assert_eq!(queue.queued_event_count(&conversation), 1);
        handle_cancel_turn_control(&payload, &mut pool, &mut queue, None, Some(&observer));
        assert_eq!(result(&observer, "cancelled_queued")["drainedCount"], 1);
        assert_eq!(queue.queued_event_count(&conversation), 0);
    }
}

#[tokio::test]
async fn open_receiver_switch_sends_once_with_request_correlation() {
    for exact in [true, false] {
        let (mut pool, channel, conversation, rx) = in_flight();
        let observer = observer::ObserverHandle::in_process();
        let payload = control(channel, conversation, exact);
        handle_switch_model_control(&payload, &mut pool, Some(&observer));
        result(&observer, "sent");
        assert_eq!(
            rx.await.expect("control received"),
            ControlSignal::SwitchModel {
                model_id: "candidate-model".into(),
                request_id: Some("request-1".into())
            }
        );
        handle_switch_model_control(&payload, &mut pool, Some(&observer));
        result(&observer, "turn_ending");
    }
}
