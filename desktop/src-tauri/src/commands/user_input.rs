use tauri::State;

use crate::{
    app_state::AppState,
    relay::{submit_event, SubmitEventResponse},
};

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
    let builder = buzz_sdk_pkg::build_agent_user_input_answer(
        channel_uuid,
        &request_event_id,
        &answers.to_string(),
    )
    .map_err(|error| error.to_string())?;
    submit_event(builder, &state).await
}
