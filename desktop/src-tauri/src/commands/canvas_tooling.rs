//! Owner-signed `tooling` key on the channel canvas ` ```crew ` YAML block.

use serde::{Deserialize, Serialize};

use super::canvas::{ensure_canvas_author, fenced_crew_block};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasTooling {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulator: Option<CanvasSimulatorIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dev_server: Option<CanvasDevServerIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_allowlist: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasSimulatorIntent {
    pub device_type: String,
    pub runtime: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasDevServerIntent {
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_pattern: Option<String>,
}

/// Parse `tooling` from a canvas ` ```crew ` block. Unknown sibling keys are
/// ignored here; assignment updates preserve them via `serde_yaml::Value`.
pub fn parse_canvas_tooling(content: &str) -> Result<Option<CanvasTooling>, String> {
    let Some((_, yaml, _)) = fenced_crew_block(content) else {
        return Ok(None);
    };
    let document: serde_yaml::Value =
        serde_yaml::from_str(&yaml).map_err(|e| format!("invalid crew YAML: {e}"))?;
    let Some(mapping) = document.as_mapping() else {
        return Ok(None);
    };
    let Some(tooling) = mapping.get(serde_yaml::Value::String("tooling".into())) else {
        return Ok(None);
    };
    let parsed: CanvasTooling = serde_yaml::from_value(tooling.clone())
        .map_err(|e| format!("invalid tooling YAML: {e}"))?;
    if parsed
        .simulator
        .as_ref()
        .is_some_and(|s| s.device_type.contains("UDID") || s.runtime.len() > 64)
    {
        return Err("tooling must store intent, not a device UDID".into());
    }
    Ok(Some(parsed))
}

pub fn update_canvas_crew_tooling(
    content: &str,
    tooling: &CanvasTooling,
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
    let value = serde_yaml::to_value(tooling).map_err(|e| format!("serialize tooling: {e}"))?;
    root.insert(serde_yaml::Value::String("tooling".into()), value);
    let yaml = serde_yaml::to_string(&document).map_err(|e| format!("serialize crew YAML: {e}"))?;
    Ok(format!("{prefix}```crew\n{yaml}```{suffix}"))
}

#[tauri::command]
pub async fn get_canvas_tooling(
    channel_id: String,
    state: tauri::State<'_, crate::app_state::AppState>,
) -> Result<Option<CanvasTooling>, String> {
    let events = crate::relay::query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [40100],
            "#h": [channel_id],
            "limit": 1
        })],
    )
    .await?;
    let Some(event) = events.first() else {
        return Ok(None);
    };
    parse_canvas_tooling(&event.content)
}

#[tauri::command]
pub async fn set_canvas_tooling(
    channel_id: String,
    tooling: CanvasTooling,
    state: tauri::State<'_, crate::app_state::AppState>,
) -> Result<serde_json::Value, String> {
    let events = crate::relay::query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [40100],
            "#h": [channel_id],
            "limit": 1
        })],
    )
    .await?;
    let signing_key = state
        .keys
        .lock()
        .map_err(|_| "identity lock poisoned".to_string())?
        .public_key();
    if let Some(event) = events.first() {
        ensure_canvas_author(
            event.pubkey.to_hex().as_str(),
            signing_key.to_hex().as_str(),
        )?;
    }
    let current = events
        .first()
        .map(|event| event.content.as_str())
        .unwrap_or("");
    let updated = update_canvas_crew_tooling(current, &tooling)?;
    let uuid = uuid::Uuid::parse_str(&channel_id)
        .map_err(|_| format!("invalid channel UUID: {channel_id}"))?;
    let result =
        crate::relay::submit_event(crate::events::build_set_canvas(uuid, &updated)?, &state)
            .await?;
    Ok(serde_json::json!({
        "ok": true,
        "event_id": result.event_id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::canvas::update_canvas_crew_assignment;

    const ORIGINAL: &str = "Founder guidance.\n\n```crew\nassignments:\n  old: Research\ndefinitions:\n  Research: Read first.\nrouting:\n  review: Research\ntooling:\n  simulator:\n    deviceType: iPhone 16 Pro\n    runtime: iOS 18\n  extraUnknown: keep-me\n```\n\nClosing notes.";

    #[test]
    fn parse_reads_intent_only() {
        let tooling = parse_canvas_tooling(ORIGINAL)
            .expect("parse")
            .expect("some");
        assert_eq!(
            tooling.simulator.as_ref().map(|s| s.device_type.as_str()),
            Some("iPhone 16 Pro")
        );
        assert_eq!(
            tooling.simulator.as_ref().map(|s| s.runtime.as_str()),
            Some("iOS 18")
        );
    }

    #[test]
    fn assignment_update_preserves_tooling() {
        let updated = update_canvas_crew_assignment(
            ORIGINAL,
            "agent",
            "Code Review",
            "Allowed: inspect code.",
        )
        .expect("assign");
        let tooling = parse_canvas_tooling(&updated)
            .expect("parse")
            .expect("some");
        assert_eq!(tooling.simulator.unwrap().device_type, "iPhone 16 Pro");
        assert!(updated.contains("extraUnknown") || updated.contains("keep-me"));
    }

    #[test]
    fn tooling_update_preserves_roles() {
        let tooling = CanvasTooling {
            simulator: Some(CanvasSimulatorIntent {
                device_type: "iPhone 16".into(),
                runtime: "iOS 18".into(),
            }),
            dev_server: Some(CanvasDevServerIntent {
                command: "pnpm dev --port $PORT".into(),
                ready_pattern: Some("Local:".into()),
            }),
            browser_allowlist: Some(vec!["https://api.stripe.com".into()]),
        };
        let updated = update_canvas_crew_tooling(ORIGINAL, &tooling).expect("write");
        assert!(updated.contains("Research"));
        assert!(updated.contains("pnpm dev --port $PORT"));
        assert!(updated.contains("routing:"));
        assert!(updated.contains("api.stripe.com"));
        assert!(!updated.to_ascii_lowercase().contains("udid"));
    }

    #[test]
    fn parse_reads_browser_allowlist() {
        let yaml = "```crew\ntooling:\n  browserAllowlist:\n    - https://api.stripe.com\n```";
        let tooling = parse_canvas_tooling(yaml).expect("parse").expect("some");
        assert_eq!(
            tooling.browser_allowlist.as_deref(),
            Some(["https://api.stripe.com".to_string()].as_slice())
        );
    }
}
