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

/// Forward one Activity Steer request to the exact in-flight task.
///
/// The task map is the native source of truth for routing. A request that no
/// longer matches the selected routing channel, conversation, or turn is
/// settled as `stale_target`; it is never sent to a successor task. The
/// adapter's response is watched asynchronously so the relay control loop is
/// not held open while the selected run reaches its next round boundary.
/// One validated Activity Steer request.
struct ParsedSteerRequest<'a> {
    channel_id: uuid::Uuid,
    conversation_id: uuid::Uuid,
    session_id: &'a str,
    turn_id: &'a str,
    request_id: uuid::Uuid,
    prompt: &'a str,
}

/// Largest steer prompt the observer control accepts, in bytes.
pub(crate) const MAX_STEER_PROMPT_BYTES: usize = 16 * 1024;

/// Longest raw identifier echoed back on a rejected frame, in characters.
const MAX_ECHOED_ID_CHARS: usize = 128;

/// Echo an unvalidated identifier so the caller can still correlate.
fn echo_id(payload: &serde_json::Value, key: &str) -> Option<String> {
    let value = payload.get(key)?.as_str()?;
    Some(
        crate::acp::bound_steer_reason(Some(value))?
            .chars()
            .take(MAX_ECHOED_ID_CHARS)
            .collect(),
    )
}

/// Validate one steer control frame, naming the exact reason on failure.
///
/// Every rejection reason is a fixed string: it is forwarded to the operator,
/// so it must never echo unbounded caller-supplied text.
fn parse_steer_payload(
    payload: &serde_json::Value,
) -> Result<ParsedSteerRequest<'_>, &'static str> {
    let channel_id = payload
        .get("channelId")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<uuid::Uuid>().ok())
        .ok_or("steer request is missing a valid channelId")?;
    let conversation_id = payload
        .get("conversationId")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<uuid::Uuid>().ok())
        .ok_or("steer request is missing a valid conversationId")?;
    let session_id = payload
        .get("sessionId")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or("steer request is missing a sessionId")?;
    let turn_id = payload
        .get("turnId")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or("steer request is missing a turnId")?;
    let request_id = payload
        .get("requestId")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<uuid::Uuid>().ok())
        .ok_or("steer request is missing a valid requestId")?;
    let prompt = payload
        .get("prompt")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or("steer request is missing a prompt")?;
    if prompt.len() > MAX_STEER_PROMPT_BYTES {
        return Err("steer prompt exceeds the 16 KiB limit");
    }
    Ok(ParsedSteerRequest {
        channel_id,
        conversation_id,
        session_id,
        turn_id,
        request_id,
        prompt,
    })
}

/// Classify the adapter's answer to a dispatched strict steer request.
///
/// `dispatched` is the only thing that separates "the queued request was
/// dropped before any write" (replay-safe `stale_target`) from "the request
/// reached the adapter and its answer was lost" (`unconfirmed`, no replay).
/// It is consulted for a lost ack and for the neutral prompt-completed answer
/// alike: the prompt loop only holds a steer after the write succeeded, so a
/// neutral answer to a written request proves nothing about whether the
/// adapter applied it.
pub(crate) fn classify_steer_ack(
    ack: Result<pool::SteerAck, tokio::sync::oneshot::error::RecvError>,
    dispatched: bool,
) -> (String, Option<String>) {
    match ack {
        Ok(pool::SteerAck::Success { .. }) => ("appended".into(), None),
        Ok(pool::SteerAck::Err(pool::SteerError::StrictOutcome { outcome, reason }))
            if matches!(
                outcome.as_str(),
                "stale_target" | "rejected" | "busy" | "expired"
            ) =>
        {
            (outcome, reason)
        }
        // A prompt that ended while the written steer was still pending may
        // already have consumed it. Only an undispatched one is replay-safe.
        Ok(pool::SteerAck::PromptCompletedNeutral) if dispatched => (
            "unconfirmed".into(),
            Some("the selected run ended before it answered the steer".into()),
        ),
        Ok(pool::SteerAck::Err(pool::SteerError::StrictTargetMismatch))
        | Ok(pool::SteerAck::PromptCompletedNeutral)
        | Ok(pool::SteerAck::Err(pool::SteerError::PromptCompleted)) => {
            ("stale_target".into(), None)
        }
        Ok(pool::SteerAck::Err(pool::SteerError::Busy)) => ("busy".into(), None),
        Ok(pool::SteerAck::Err(
            pool::SteerError::StrictUnsupported | pool::SteerError::ExpectedRunIdMissing,
        )) => (
            "rejected".into(),
            Some("selected adapter does not support strict steering".into()),
        ),
        // A malformed echo, unknown adapter outcome, or transport failure
        // does not prove whether the request was applied. Keep replay
        // disabled until the user verifies the run.
        Ok(pool::SteerAck::Err(
            pool::SteerError::StrictResponseMismatch
            | pool::SteerError::StrictOutcome { .. }
            | pool::SteerError::OutcomeRejected { .. }
            | pool::SteerError::Transport(_)
            | pool::SteerError::AgentError { .. },
        )) => (
            "unconfirmed".into(),
            Some("strict steer outcome was unconfirmed".into()),
        ),
        // The ack sender was dropped. If nothing was ever written to the
        // adapter the request cannot have taken effect, so it is safe to
        // retry; only a dropped answer to a written request is unconfirmed.
        Err(_) if !dispatched => (
            "stale_target".into(),
            Some("the selected run ended before the steer was sent".into()),
        ),
        Err(_) => (
            "unconfirmed".into(),
            Some("strict steer response was lost".into()),
        ),
    }
}

/// Forward one Activity Steer request to the exact in-flight task.
///
/// The task map is the native source of truth for routing. A request that no
/// longer matches the selected routing channel, conversation, or turn is
/// settled as `stale_target`; it is never sent to a successor task. The
/// adapter's response is watched asynchronously so the relay control loop is
/// not held open while the selected run reaches its next round boundary.
///
/// Every frame, including a malformed one, leaves exactly one terminal
/// `control_result`: a dropped frame would strand the caller's control
/// latched on a request that can never resolve.
pub(crate) fn handle_steer_turn_control(
    payload: &serde_json::Value,
    pool: &mut AgentPool,
    observer: Option<&observer::ObserverHandle>,
) {
    let parsed = match parse_steer_payload(payload) {
        Ok(parsed) => parsed,
        Err(reason) => {
            tracing::warn!("observer steer_turn control frame rejected: {reason}");
            emit_rejected_steer_frame(observer, payload, reason);
            return;
        }
    };
    let ParsedSteerRequest {
        channel_id,
        conversation_id,
        session_id,
        turn_id,
        request_id,
        prompt,
    } = parsed;

    let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
    let dispatched = pool::SteerDispatchMarker::default();
    let request = pool::SteerRequest {
        prompt_blocks: vec![prompt.to_owned()],
        strict_target: Some(pool::StrictSteerTarget {
            session_id: session_id.to_owned(),
            turn_id: turn_id.to_owned(),
            request_id: request_id.to_string(),
        }),
        ack_tx,
        dispatched: dispatched.clone(),
    };
    match pool.send_exact_steer(channel_id, conversation_id, turn_id, request) {
        Ok(()) => {
            let observer = observer.cloned();
            let session_id = session_id.to_owned();
            let turn_id = turn_id.to_owned();
            tokio::spawn(async move {
                let ack = ack_rx.await;
                let (status, error) = classify_steer_ack(ack, dispatched.was_dispatched());
                if let Some(observer) = observer {
                    emit_steer_result(
                        Some(&observer),
                        channel_id,
                        conversation_id,
                        &session_id,
                        &turn_id,
                        request_id,
                        &status,
                        error.as_deref(),
                    );
                }
            });
        }
        Err(pool::SteerError::Busy) => {
            emit_steer_result(
                observer,
                channel_id,
                conversation_id,
                session_id,
                turn_id,
                request_id,
                "busy",
                None,
            );
        }
        Err(pool::SteerError::StrictTargetMismatch) => {
            emit_steer_result(
                observer,
                channel_id,
                conversation_id,
                session_id,
                turn_id,
                request_id,
                "stale_target",
                None,
            );
        }
        Err(pool::SteerError::StrictUnsupported) => emit_steer_result(
            observer,
            channel_id,
            conversation_id,
            session_id,
            turn_id,
            request_id,
            "rejected",
            Some("selected adapter does not support strict steering"),
        ),
        Err(_) => emit_steer_result(
            observer,
            channel_id,
            conversation_id,
            session_id,
            turn_id,
            request_id,
            "rejected",
            Some("strict steer was not sent"),
        ),
    }
}

/// Answer a frame that never became a routable request.
///
/// Identifiers are echoed raw and bounded: the caller correlates on string
/// equality, and a frame whose own requestId is unusable still deserves a
/// terminal answer rather than silence.
fn emit_rejected_steer_frame(
    observer: Option<&observer::ObserverHandle>,
    payload: &serde_json::Value,
    reason: &str,
) {
    let Some(observer) = observer else {
        return;
    };
    let channel_id = echo_id(payload, "channelId");
    let conversation_id = echo_id(payload, "conversationId");
    let session_id = echo_id(payload, "sessionId");
    let turn_id = echo_id(payload, "turnId");
    let context = observer::context_for_conversation(
        channel_id
            .as_deref()
            .and_then(|value| value.parse::<uuid::Uuid>().ok()),
        conversation_id
            .as_deref()
            .and_then(|value| value.parse::<uuid::Uuid>().ok()),
        session_id.clone(),
        turn_id.clone(),
    );
    observer.emit(
        "control_result",
        None,
        &context,
        serde_json::json!({
            "type": "steer_turn",
            "requestId": echo_id(payload, "requestId"),
            "status": "rejected",
            "channelId": channel_id,
            "conversationId": conversation_id,
            "sessionId": session_id,
            "turnId": turn_id,
            "error": reason,
        }),
    );
}

#[allow(clippy::too_many_arguments)] // Correlated result identity remains explicit at the boundary.
fn emit_steer_result(
    observer: Option<&observer::ObserverHandle>,
    channel_id: uuid::Uuid,
    conversation_id: uuid::Uuid,
    session_id: &str,
    turn_id: &str,
    request_id: uuid::Uuid,
    status: &str,
    error: Option<&str>,
) {
    let Some(observer) = observer else {
        return;
    };
    let context = observer::context_for_conversation(
        Some(channel_id),
        Some(conversation_id),
        Some(session_id.to_owned()),
        Some(turn_id.to_owned()),
    );
    observer.emit(
        "control_result",
        None,
        &context,
        serde_json::json!({
            "type": "steer_turn",
            "requestId": request_id.to_string(),
            "status": status,
            "channelId": channel_id.to_string(),
            "conversationId": conversation_id.to_string(),
            "sessionId": session_id,
            "turnId": turn_id,
            "error": error,
        }),
    );
}
