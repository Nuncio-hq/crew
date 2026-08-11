use crate::{app_state::AppState, events, relay::submit_event};

/// Publish a durable assignment announcement in the target channel.
pub(crate) async fn publish_assignment_announcement(
    state: &AppState,
    channel_id: &str,
    content: &str,
) -> Result<String, String> {
    let channel = uuid::Uuid::parse_str(channel_id)
        .map_err(|_| format!("invalid channel UUID: {channel_id}"))?;
    let relay_base = crate::relay::relay_api_base_url_with_override(state);
    let builder = events::build_message(
        channel,
        content.trim(),
        None,
        &[],
        &[],
        &[],
        &[],
        &[],
        &relay_base,
    )?;
    submit_event(builder, state)
        .await
        .map(|result| result.event_id)
        .map_err(|error| format!("assignment announcement publish failed: {error}"))
}
