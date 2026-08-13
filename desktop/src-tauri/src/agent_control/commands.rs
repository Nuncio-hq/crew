//! Tauri commands for lease banners and origin decisions.

use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

use super::protocol::Instrument;
use super::server::AgentControlHandle;

const EVENT: &str = "agent-control";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaseInput {
    channel_id: String,
    instrument: String,
}

fn parse_instrument(raw: &str) -> Instrument {
    if raw.eq_ignore_ascii_case("sim") {
        Instrument::Sim
    } else {
        Instrument::Browser
    }
}

async fn emit_ui(app: &AppHandle, handle: &AgentControlHandle) {
    let views = handle.runtime.lease_views().await;
    let overlay = handle.runtime.overlay.lock().await.frames.last().cloned();
    let pending = handle.runtime.pending_origin.lock().await.clone();
    let _ = app.emit(
        EVENT,
        serde_json::json!({
            "leases": views,
            "overlay": overlay,
            "pendingOrigin": pending,
        }),
    );
}

#[tauri::command]
pub async fn agent_control_status(
    handle: State<'_, AgentControlHandle>,
) -> Result<serde_json::Value, String> {
    let views = handle.runtime.lease_views().await;
    let overlay = handle.runtime.overlay.lock().await.frames.last().cloned();
    Ok(serde_json::json!({
        "leases": views,
        "overlay": overlay,
        "pendingOrigin": handle.runtime.pending_origin.lock().await.clone(),
    }))
}

#[tauri::command]
pub async fn agent_control_take_over(
    input: LeaseInput,
    app: AppHandle,
    handle: State<'_, AgentControlHandle>,
) -> Result<serde_json::Value, String> {
    let now = *handle.runtime.now_ms.lock().await;
    handle.runtime.leases.lock().await.preempt_human(
        &input.channel_id,
        parse_instrument(&input.instrument),
        now,
    );
    emit_ui(&app, &handle).await;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn agent_control_release(
    input: LeaseInput,
    app: AppHandle,
    handle: State<'_, AgentControlHandle>,
) -> Result<serde_json::Value, String> {
    handle
        .runtime
        .leases
        .lock()
        .await
        .release_human(&input.channel_id, parse_instrument(&input.instrument));
    emit_ui(&app, &handle).await;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
pub async fn agent_control_note_human(
    input: LeaseInput,
    app: AppHandle,
    handle: State<'_, AgentControlHandle>,
) -> Result<serde_json::Value, String> {
    let now = *handle.runtime.now_ms.lock().await;
    handle.runtime.leases.lock().await.note_human_input(
        &input.channel_id,
        parse_instrument(&input.instrument),
        now,
    );
    emit_ui(&app, &handle).await;
    Ok(serde_json::json!({ "ok": true }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginDecisionInput {
    channel_id: String,
    origin: String,
    decision: String,
}

#[tauri::command]
pub async fn agent_control_origin_decision(
    input: OriginDecisionInput,
    handle: State<'_, AgentControlHandle>,
) -> Result<serde_json::Value, String> {
    use super::origin::OriginDecision;
    let parsed = match input.decision.as_str() {
        "allow_once" => OriginDecision::AllowOnce,
        "allow_domain" => OriginDecision::AllowDomain,
        _ => OriginDecision::Deny,
    };
    handle.runtime.set_elicitation(parsed).await;
    if parsed == OriginDecision::AllowDomain {
        handle
            .runtime
            .origin
            .lock()
            .await
            .grant_domain(&input.channel_id, &input.origin);
        handle
            .runtime
            .canvas_writes
            .lock()
            .await
            .push((input.channel_id, input.origin));
    } else if parsed == OriginDecision::AllowOnce {
        handle
            .runtime
            .origin
            .lock()
            .await
            .grant_once(&input.channel_id, &input.origin);
    }
    Ok(serde_json::json!({ "ok": true }))
}

pub fn apply_control_env(
    command: &mut std::process::Command,
    handle: &super::server::AgentControlHandle,
) {
    if let Some(url) = handle.control_url() {
        command.env(super::protocol::ENV_CONTROL_URL, url);
    }
    command.env(super::protocol::ENV_CONTROL_TOKEN, &handle.token);
}

/// Inject control URL/token from app state. One call site in spawn — no lease I/O.
pub fn apply_control_env_from_app(command: &mut std::process::Command, app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(control) = app.try_state::<super::server::AgentControlHandle>() {
        apply_control_env(command, &control);
    }
}
