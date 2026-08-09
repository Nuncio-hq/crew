use tauri::State;

use crate::{
    app_state::AppState,
    relay::{query_relay, submit_event, SubmitEventResponse},
};

fn requesting_agent_from_request_event(
    event: nostr::Event,
    channel_id: &str,
    request_event_id: &str,
    owner_pubkey: &str,
) -> Result<String, String> {
    if event.id.to_hex() != request_event_id {
        return Err("user-input request was not found".to_string());
    }
    if event.kind.as_u16() as u32 != buzz_core_pkg::kind::KIND_AGENT_USER_INPUT_REQUESTED {
        return Err("user-input request has the wrong event kind".to_string());
    }
    event.verify().map_err(|error| {
        format!("user-input request failed cryptographic verification: {error}")
    })?;
    let request_content: buzz_core_pkg::user_input::UserInputRequest =
        serde_json::from_str(&event.content)
            .map_err(|_| "user-input request content is malformed".to_string())?;
    if request_content.channel_id != channel_id {
        return Err("user-input request content targets a different channel".to_string());
    }
    let h_tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|value| value == "h"))
        .collect();
    let p_tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|value| value == "p"))
        .collect();
    if h_tags.len() != 1 || h_tags[0].as_slice().get(1).map(String::as_str) != Some(channel_id) {
        return Err("user-input request channel relationship is invalid".to_string());
    }
    if p_tags.len() != 1 || p_tags[0].as_slice().get(1).map(String::as_str) != Some(owner_pubkey) {
        return Err("user-input request is not intended for the current owner".to_string());
    }
    Ok(event.pubkey.to_hex())
}

async fn requesting_agent_for_request(
    state: &AppState,
    channel_id: &str,
    request_event_id: &str,
) -> Result<String, String> {
    let request_event_id = request_event_id.to_ascii_lowercase();
    let filter = serde_json::json!({
        "ids": [&request_event_id],
        "kinds": [buzz_core_pkg::kind::KIND_AGENT_USER_INPUT_REQUESTED],
        "#h": [channel_id],
        "limit": 1,
    });
    let events = query_relay(state, &[filter]).await?;
    let event = events
        .into_iter()
        .find(|event| event.id.to_hex() == request_event_id)
        .ok_or_else(|| "user-input request was not found".to_string())?;
    let owner_pubkey = state.signing_keys()?.public_key().to_hex();
    requesting_agent_from_request_event(event, channel_id, &request_event_id, &owner_pubkey)
}

/// Publish an owner-authored answer to a durable agent question.
#[tauri::command]
pub async fn send_channel_user_input_answer(
    channel_id: String,
    request_event_id: String,
    answers: serde_json::Value,
    state: State<'_, AppState>,
) -> Result<SubmitEventResponse, String> {
    let channel_uuid = uuid::Uuid::parse_str(&channel_id)
        .map_err(|_| format!("invalid channel UUID: {channel_id}"))?;
    if !answers.is_object() {
        return Err("answers must be a JSON object".to_string());
    }
    let requesting_agent =
        requesting_agent_for_request(&state, &channel_uuid.to_string(), &request_event_id).await?;
    let builder = buzz_sdk_pkg::build_agent_user_input_answer(
        channel_uuid,
        &request_event_id,
        &requesting_agent,
        &answers.to_string(),
    )
    .map_err(|error| error.to_string())?;
    submit_event(builder, &state).await
}

#[cfg(test)]
mod tests {
    use super::requesting_agent_from_request_event;

    #[test]
    fn request_lookup_binds_channel_owner_and_requesting_agent() {
        let channel_id = uuid::Uuid::new_v4();
        let agent = nostr::Keys::generate();
        let owner = nostr::Keys::generate();
        let content = serde_json::json!({
            "request_id": "request",
            "session_id": "session",
            "turn_id": "turn",
            "channel_id": channel_id.to_string(),
            "tool_call_id": null,
            "engine": "codex",
            "message": "Choose",
            "questions": [],
        })
        .to_string();
        let trigger = nostr::EventId::from_hex(&"a".repeat(64)).unwrap();
        let event = buzz_sdk_pkg::build_agent_user_input_request(
            channel_id,
            &buzz_sdk_pkg::ThreadRef {
                root_event_id: trigger,
                parent_event_id: trigger,
            },
            &owner.public_key().to_hex(),
            &content,
        )
        .unwrap()
        .sign_with_keys(&agent)
        .unwrap();
        let request_event_id = event.id.to_hex();

        assert_eq!(
            requesting_agent_from_request_event(
                event.clone(),
                &channel_id.to_string(),
                &request_event_id,
                &owner.public_key().to_hex(),
            )
            .unwrap(),
            agent.public_key().to_hex()
        );
        assert!(requesting_agent_from_request_event(
            event,
            &channel_id.to_string(),
            &request_event_id,
            &nostr::Keys::generate().public_key().to_hex(),
        )
        .is_err());
    }
}
