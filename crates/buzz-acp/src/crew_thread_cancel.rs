/// Resolve the cancel_turn outcome after attempting to signal an in-flight turn.
///
/// When nothing is in flight, fall through to draining the conversation's
/// queued events (the hold window / idle case). Keep `no_active_turn` only
/// when there is genuinely nothing to stop.
fn resolve_cancel_turn_outcome(
    fired: bool,
    queue: &mut EventQueue,
    conversation_key: Uuid,
) -> (&'static str, Vec<String>) {
    if fired {
        return ("sent", Vec::new());
    }
    let ids = queue.drain_channel(conversation_key);
    if ids.is_empty() {
        ("no_active_turn", ids)
    } else {
        ("cancelled_queued", ids)
    }
}

/// Cancel an exact turn when present; only an absent turn permits legacy queue drain.
fn handle_cancel_turn_control(
    payload: &serde_json::Value,
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    rest_client: Option<&relay::RestClient>,
    observer: Option<&observer::ObserverHandle>,
) {
    handle_cancel_turn_control_with_release(
        payload,
        pool,
        queue,
        rest_client,
        observer,
        crate::desktop_control::notify_lease_release,
    );
}

/// The cancel handler keeps its release side effect behind the same outcome.
fn handle_cancel_turn_control_with_release(
    payload: &serde_json::Value,
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    rest_client: Option<&relay::RestClient>,
    observer: Option<&observer::ObserverHandle>,
    release_lease: impl FnOnce(Option<String>, &str),
) {
    let Some(channel_id) = payload
        .get("channelId")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<Uuid>().ok())
    else {
        tracing::warn!("observer cancel_turn control frame missing valid channelId");
        return;
    };
    let conversation_id = payload
        .get("conversationId")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<Uuid>().ok());
    let explicit_turn = payload.get("turnId");
    let turn_id = explicit_turn
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty());

    let conversation_key = conversation_id.unwrap_or(channel_id);
    let (status, drained_ids) = if explicit_turn.is_some() {
        // Presence is intentional: malformed explicit targets must never become
        // a broad conversation cancel. A stale turn cannot drain its successor.
        let fired = match (conversation_id, turn_id) {
            (Some(conversation), Some(turn)) => {
                signal_exact_cancel_turn(pool, channel_id, conversation, turn)
            }
            _ => false,
        };
        (if fired { "sent" } else { "no_active_turn" }, Vec::new())
    } else {
        let fired =
            signal_in_flight_task(pool, conversation_key, channel_id, ControlSignal::Cancel);
        resolve_cancel_turn_outcome(fired, queue, conversation_key)
    };
    let drained_count = drained_ids.len();
    if !drained_ids.is_empty() {
        if let Some(rest) = rest_client {
            let rest = rest.clone();
            tokio::spawn(async move {
                pool::clear_reactions(rest, drained_ids).await;
            });
        }
    }
    if let Some(observer) = observer {
        let context = observer::context_for_conversation(
            Some(channel_id),
            conversation_id,
            None,
            turn_id.map(ToOwned::to_owned),
        );
        observer.emit(
            "control_result",
            None,
            &context,
            serde_json::json!({
                "type": "cancel_turn",
                "requestId": payload.get("requestId"),
                "status": status,
                "drainedCount": drained_count,
                "conversationId": conversation_id.map(|id| id.to_string()),
                "turnId": turn_id,
            }),
        );
    }
    if status == "sent" || status == "cancelled_queued" {
        release_lease(Some(channel_id.to_string()), "cancel");
    }
}

/// Signal only the still-current complete target, without routing fallback.
fn signal_exact_cancel_turn(
    pool: &mut AgentPool,
    channel_id: Uuid,
    conversation_id: Uuid,
    turn_id: &str,
) -> bool {
    let sender = pool
        .task_map_mut()
        .values_mut()
        .find(|meta| {
            meta.routing_channel_id == Some(channel_id)
                && meta.channel_id == Some(conversation_id)
                && meta.turn_id == turn_id
        })
        .and_then(|meta| meta.control_tx.take());
    let Some(sender) = sender else {
        return false;
    };
    let sent = sender.send(ControlSignal::Cancel).is_ok();
    if sent {
        tracing::info!(channel = %channel_id, conversation = %conversation_id, turn = %turn_id,
            "cancel signal sent to exact in-flight turn");
    }
    sent
}
