use buzz_core::kind::{KIND_AGENT_USER_INPUT_ANSWER, KIND_AGENT_USER_INPUT_REQUESTED};

use crate::client::{normalize_write_response, BuzzClient};
use crate::error::CliError;
use crate::validate::{parse_uuid, sdk_err, validate_uuid};

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
    let answered: std::collections::HashSet<String> = answers
        .iter()
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
    let builder = buzz_sdk::build_agent_user_input_answer(channel_id, request, &value.to_string())
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
