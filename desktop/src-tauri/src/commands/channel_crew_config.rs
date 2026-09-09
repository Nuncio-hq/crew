//! Thin IPC for the Crew-owned canvas preparation and durable recovery domain.
use crate::app_state::owner_scope::OwnerScopeToken;
use buzz_core_pkg::crew_role::CrewConfigDraft;
use tauri::AppHandle;

pub(crate) async fn channel_crew_transport_publish<
    F: std::future::Future<Output = Result<(), String>>,
>(
    state: &crate::app_state::AppState,
    origin: String,
    keys: nostr::Keys,
    event: &nostr::Event,
    guard: F,
) -> Result<crate::relay::SubmitEventResponse, String> {
    super::owner_operation_transport::OwnerOperationTransport::captured(state, origin, keys, None)
        .map_err(|error| error.to_string())?
        .publish(event, guard)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn channel_crew_transport_query<
    F: std::future::Future<Output = Result<(), String>>,
>(
    state: &crate::app_state::AppState,
    origin: String,
    keys: nostr::Keys,
    filter: serde_json::Value,
    guard: F,
) -> Result<Vec<nostr::Event>, String> {
    super::owner_operation_transport::OwnerOperationTransport::captured(state, origin, keys, None)
        .map_err(|error| error.to_string())?
        .query(filter, guard)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn save_channel_crew_config(
    app: AppHandle,
    expected: OwnerScopeToken,
    channel_id: String,
    expected_canvas_event_id: Option<String>,
    draft: CrewConfigDraft,
) -> Result<super::ScopedOperationResult<crate::channel_crew_config::CrewSaveResult>, String> {
    crate::channel_crew_config::save(app, expected, channel_id, expected_canvas_event_id, draft)
        .await
}

#[tauri::command]
pub(crate) async fn retry_channel_crew_config(
    app: AppHandle,
    expected: OwnerScopeToken,
    operation_id: String,
) -> Result<serde_json::Value, String> {
    crate::channel_crew_config::retry(app, expected, operation_id).await
}

#[tauri::command]
pub(crate) async fn list_channel_crew_operations(
    app: AppHandle,
    expected: OwnerScopeToken,
    channel_id: String,
) -> Result<serde_json::Value, String> {
    crate::channel_crew_config::list(app, expected, channel_id).await
}

#[tauri::command]
pub(crate) async fn get_channel_crew_operation(
    app: AppHandle,
    expected: OwnerScopeToken,
    operation_id: String,
) -> Result<serde_json::Value, String> {
    crate::channel_crew_config::status(app, expected, operation_id).await
}
