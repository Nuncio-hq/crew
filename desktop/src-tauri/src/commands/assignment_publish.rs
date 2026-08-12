use crate::{app_state::AppState, events, relay::submit_event};

pub(crate) fn assignment_announcement_content(
    agent_pubkey: &str,
    label: &str,
    definition: &str,
) -> String {
    format!(
        "AGENT-WORKING-AGREEMENT: assigned {agent_pubkey} to role `{label}` in this channel.\n\n{definition}"
    )
}

/// Publish a durable assignment announcement in the target channel.
pub(crate) async fn publish_assignment_announcement(
    state: &AppState,
    channel_id: &str,
    content: &str,
) -> Result<String, String> {
    let channel = parse_channel_id(channel_id)?;
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
        None,
        &relay_base,
    )?;
    submit_event(builder, state)
        .await
        .map(|result| result.event_id)
        .map_err(|error| format!("assignment announcement publish failed: {error}"))
}

fn parse_channel_id(channel_id: &str) -> Result<uuid::Uuid, String> {
    uuid::Uuid::parse_str(channel_id).map_err(|_| format!("invalid channel UUID: {channel_id}"))
}

#[cfg(test)]
mod tests {
    use super::{assignment_announcement_content, parse_channel_id};

    #[test]
    fn assignment_announcement_contains_assignment_details() {
        let content = assignment_announcement_content("agent", "reviewer", "Review code.");
        assert!(content.contains("AGENT-WORKING-AGREEMENT"));
        assert!(content.contains("reviewer"));
        assert!(content.contains("Review code."));
    }

    #[test]
    fn assignment_announcement_rejects_invalid_channel_before_publish() {
        let error = parse_channel_id("not-a-uuid").expect_err("invalid channel must propagate");
        assert!(error.contains("invalid channel UUID"));
    }
}
