use buzz_core::kind::{KIND_AGENT_USER_INPUT_ANSWER, KIND_AGENT_USER_INPUT_REQUESTED};

use crate::client::{normalize_write_response, BuzzClient};
use crate::error::CliError;
use crate::validate::{parse_uuid, sdk_err, validate_uuid};

fn requesting_agent_from_request_event(
    event: nostr::Event,
    channel: &str,
    request: &str,
    owner_pubkey: &str,
) -> Result<String, CliError> {
    if event.id.to_hex() != request {
        return Err(CliError::Other("user-input request was not found".into()));
    }
    if event.kind.as_u16() as u32 != KIND_AGENT_USER_INPUT_REQUESTED {
        return Err(CliError::Other(
            "user-input request has the wrong event kind".into(),
        ));
    }
    event.verify().map_err(|error| {
        CliError::Other(format!(
            "user-input request failed cryptographic verification: {error}"
        ))
    })?;
    let request_content: buzz_core::user_input::UserInputRequest =
        serde_json::from_str(&event.content)
            .map_err(|_| CliError::Other("user-input request content is malformed".into()))?;
    if request_content.channel_id != channel {
        return Err(CliError::Other(
            "user-input request content targets a different channel".into(),
        ));
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
    if h_tags.len() != 1 || h_tags[0].as_slice().get(1).map(String::as_str) != Some(channel) {
        return Err(CliError::Other(
            "user-input request channel relationship is invalid".into(),
        ));
    }
    if p_tags.len() != 1 || p_tags[0].as_slice().get(1).map(String::as_str) != Some(owner_pubkey) {
        return Err(CliError::Other(
            "user-input request is not intended for the current owner".into(),
        ));
    }
    Ok(event.pubkey.to_hex())
}

async fn requesting_agent_for_request(
    client: &BuzzClient,
    channel: &str,
    request: &str,
) -> Result<String, CliError> {
    let request = request.to_ascii_lowercase();
    let filter = serde_json::json!({
        "ids": [&request],
        "kinds": [KIND_AGENT_USER_INPUT_REQUESTED],
        "#h": [channel],
        "limit": 1
    });
    let events: Vec<nostr::Event> = serde_json::from_str(&client.query(&filter).await?)
        .map_err(|error| CliError::Other(format!("invalid request lookup response: {error}")))?;
    let event = events
        .into_iter()
        .find(|event| event.id.to_hex() == request)
        .ok_or_else(|| CliError::Other("user-input request was not found".into()))?;
    let own_pubkey = client.keys().public_key().to_hex();
    requesting_agent_from_request_event(event, channel, &request, &own_pubkey)
}

/// List durable user-input requests that do not have an answer event yet.
pub async fn cmd_list(client: &BuzzClient, channel: &str) -> Result<(), CliError> {
    validate_uuid(channel)?;
    let filter = serde_json::json!({
        "kinds": [KIND_AGENT_USER_INPUT_REQUESTED],
        "#h": [channel],
        "limit": 100
    });
    let answer_filter = serde_json::json!({
        "kinds": [KIND_AGENT_USER_INPUT_ANSWER],
        "#h": [channel],
        "limit": 500
    });
    let requests: Vec<serde_json::Value> =
        serde_json::from_str(&client.query(&filter).await?).unwrap_or_default();
    let answers: Vec<serde_json::Value> =
        serde_json::from_str(&client.query(&answer_filter).await?).unwrap_or_default();
    let own_pubkey = client.keys().public_key().to_hex();
    let answered: std::collections::HashSet<String> = answers
        .iter()
        .filter(|event| {
            event.get("pubkey").and_then(serde_json::Value::as_str) == Some(own_pubkey.as_str())
        })
        .flat_map(|event| event.get("tags").and_then(|tags| tags.as_array()))
        .filter_map(|tags| {
            tags.iter().find_map(|tag| {
                let tag = tag.as_array()?;
                (tag.first()?.as_str()? == "e").then(|| tag.get(1)?.as_str().map(str::to_owned))?
            })
        })
        .collect();
    let pending: Vec<_> = requests
        .into_iter()
        .filter(|event| {
            event
                .get("id")
                .and_then(|id| id.as_str())
                .is_some_and(|id| !answered.contains(id))
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string(&pending).unwrap_or_else(|_| "[]".into())
    );
    Ok(())
}

/// Publish an owner-authored answer event for a pending request.
pub async fn cmd_answer(
    client: &BuzzClient,
    channel: &str,
    request: &str,
    answers: &str,
) -> Result<(), CliError> {
    let channel_id = parse_uuid(channel)?;
    if request.len() != 64 || !request.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CliError::Usage(
            "request must be a 64-character event id".into(),
        ));
    }
    let value: serde_json::Value = serde_json::from_str(answers)
        .map_err(|e| CliError::Usage(format!("answers must be valid JSON: {e}")))?;
    if !value.is_object() {
        return Err(CliError::Usage("answers must be a JSON object".into()));
    }
    let normalized_channel = channel_id.to_string();
    let requesting_agent =
        requesting_agent_for_request(client, &normalized_channel, request).await?;
    let builder = buzz_sdk::build_agent_user_input_answer(
        channel_id,
        request,
        &requesting_agent,
        &value.to_string(),
    )
    .map_err(sdk_err)?;
    let event = client.sign_event(builder)?;
    let response = client.submit_event(event).await?;
    println!("{}", normalize_write_response(&response));
    Ok(())
}

/// Dispatch user-input commands.
pub async fn dispatch(cmd: crate::UserInputCmd, client: &BuzzClient) -> Result<(), CliError> {
    match cmd {
        crate::UserInputCmd::List { channel } => cmd_list(client, &channel).await,
        crate::UserInputCmd::Answer {
            channel,
            request,
            answers,
        } => cmd_answer(client, &channel, &request, &answers).await,
    }
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
        let event = buzz_sdk::build_agent_user_input_request(
            channel_id,
            &buzz_sdk::ThreadRef {
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
