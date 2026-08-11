use tauri::State;

use crate::{
    app_state::AppState,
    events,
    relay::{query_relay, submit_event},
};

/// Update one founder-authored assignment while preserving all other canvas
/// prose and unknown Crew keys for forward compatibility.
pub(crate) fn update_canvas_crew_assignment(
    content: &str,
    agent_pubkey: &str,
    label: &str,
    definition: &str,
) -> Result<String, String> {
    let (prefix, yaml, suffix) = match fenced_crew_block(content) {
        Some(parts) => parts,
        None => (
            content.to_string(),
            "assignments: {}\ndefinitions: {}\n".to_string(),
            String::new(),
        ),
    };
    let mut document: serde_yaml::Value =
        serde_yaml::from_str(&yaml).map_err(|e| format!("invalid crew YAML: {e}"))?;
    let root = document
        .as_mapping_mut()
        .ok_or_else(|| "crew YAML must be a mapping".to_string())?;
    {
        let assignments = root
            .entry(serde_yaml::Value::String("assignments".into()))
            .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()))
            .as_mapping_mut()
            .ok_or_else(|| "crew assignments must be a mapping".to_string())?;
        assignments.insert(
            serde_yaml::Value::String(agent_pubkey.trim().to_string()),
            serde_yaml::Value::String(label.trim().to_string()),
        );
    }
    {
        let definitions = root
            .entry(serde_yaml::Value::String("definitions".into()))
            .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()))
            .as_mapping_mut()
            .ok_or_else(|| "crew definitions must be a mapping".to_string())?;
        definitions.insert(
            serde_yaml::Value::String(label.trim().to_string()),
            serde_yaml::Value::String(definition.to_string()),
        );
    }
    let yaml =
        serde_yaml::to_string(&document).map_err(|e| format!("serialize crew YAML: {e}"))?;
    let updated = format!("{prefix}```crew\n{yaml}```{suffix}");
    buzz_core_pkg::crew_role::parse_canvas_assignments(&updated).map_err(|e| e.to_string())?;
    Ok(updated)
}

fn fenced_crew_block(content: &str) -> Option<(String, String, String)> {
    let start = content.find("```crew")?;
    let body_start = content[start..].find('\n').map(|offset| start + offset + 1)?;
    let end = content[body_start..].find("```").map(|offset| body_start + offset)?;
    Some((
        content[..start].to_string(),
        content[body_start..end].to_string(),
        content[end + 3..].to_string(),
    ))
}

/// Read the most recent canvas event (kind:40100) for a channel.
#[tauri::command]
pub async fn get_canvas(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let events = query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [40100],
            "#h": [channel_id],
            "limit": 1
        })],
    )
    .await?;

    let Some(event) = events.first() else {
        // Explicit nulls: the TS caller distinguishes "no canvas yet" from
        // "canvas exists" via `updated_at`/`author`, so these keys must be
        // present (absent keys deserialize as `undefined`, not `null`).
        return Ok(serde_json::json!({
            "content": "",
            "event_id": null,
            "updated_at": null,
            "author": null,
        }));
    };

    Ok(serde_json::json!({
        "content": event.content,
        "event_id": event.id.to_hex(),
        "updated_at": event.created_at.as_secs(),
        "author": event.pubkey.to_hex(),
    }))
}

#[tauri::command]
pub async fn set_canvas(
    channel_id: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let uuid = uuid::Uuid::parse_str(&channel_id)
        .map_err(|_| format!("invalid channel UUID: {channel_id}"))?;
    let builder = events::build_set_canvas(uuid, &content)?;
    let result = submit_event(builder, &state).await?;

    Ok(serde_json::json!({
        "ok": true,
        "event_id": result.event_id,
    }))
}

/// Assign one agent a founder-authored role in a channel's canvas.
#[tauri::command]
pub async fn assign_channel_agent_role(
    channel_id: String,
    agent_pubkey: String,
    label: String,
    definition: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let events = query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [40100],
            "#h": [channel_id],
            "limit": 1
        })],
    )
    .await?;
    let current = events
        .first()
        .map(|event| event.content.as_str())
        .unwrap_or("");
    let updated =
        update_canvas_crew_assignment(current, &agent_pubkey, &label, &definition)?;
    let uuid = uuid::Uuid::parse_str(&channel_id)
        .map_err(|_| format!("invalid channel UUID: {channel_id}"))?;
    let canvas_result = submit_event(events::build_set_canvas(uuid, &updated)?, &state).await?;
    let announcement =
        crate::commands::assignment_announcement_content(&agent_pubkey, label.trim(), &definition);
    let announcement_event_id =
        crate::commands::publish_assignment_announcement(&state, &channel_id, &announcement)
            .await?;

    Ok(serde_json::json!({
        "ok": true,
        "canvas_event_id": canvas_result.event_id,
        "announcement_event_id": announcement_event_id,
    }))
}

#[cfg(test)]
mod tests {
    use super::update_canvas_crew_assignment;

    #[test]
    fn crew_assignment_update_preserves_canvas_prose() {
        let original = "Founder guidance.\n\n```crew\nassignments:\n  old: Research\ndefinitions:\n  Research: Read first.\nrouting:\n  review: Research\n```\n\nClosing notes.";
        let updated = update_canvas_crew_assignment(
            original,
            "agent",
            "Code Review",
            "Allowed: inspect code.\nNot allowed: merge.",
        )
        .expect("valid crew block");
        assert!(updated.starts_with("Founder guidance.\n\n"));
        assert!(updated.ends_with("\n\nClosing notes."));
        assert!(updated.contains("Code Review"));
        assert!(updated.contains("routing:"));
    }
}
