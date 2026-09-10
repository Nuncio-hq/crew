//! Selected-run control publication through the shared captured-owner transport.
use nostr::PublicKey;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use super::owner_operation_transport::{OperationTransportError, OwnerOperationTransport};
use crate::app_state::owner_scope::{assert_current, capture, OwnerScopeToken};

/// Only exact-turn Stop is admitted by this initial scoped command.
#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScopedObserverControl {
    #[serde(rename_all = "camelCase")]
    CancelTurn {
        channel_id: uuid::Uuid,
        conversation_id: uuid::Uuid,
        turn_id: String,
        request_id: uuid::Uuid,
    },
}

/// Publication is distinct from the harness's correlated control result.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum ScopedControlPublication {
    Accepted { event_id: String },
    Unknown { message: String },
    NotAttempted { message: String },
}

fn transport_failure(error: OperationTransportError) -> ScopedControlPublication {
    match error {
        OperationTransportError::InvalidInput(message)
        | OperationTransportError::NotAttempted(message) => {
            ScopedControlPublication::NotAttempted { message }
        }
        other => ScopedControlPublication::Unknown {
            message: other.to_string(),
        },
    }
}

/// Captures owner/relay natively; the shared transport rechecks after admission.
#[tauri::command]
pub(crate) async fn send_scoped_observer_control(
    app: AppHandle,
    agent_pubkey: String,
    payload: ScopedObserverControl,
    expected_scope: OwnerScopeToken,
) -> ScopedControlPublication {
    send_at_scope(app, agent_pubkey, payload, expected_scope).await
}

async fn send_at_scope<R: Runtime>(
    app: AppHandle<R>,
    agent_pubkey: String,
    payload: ScopedObserverControl,
    expected_scope: OwnerScopeToken,
) -> ScopedControlPublication {
    match prepare_control(app.clone(), agent_pubkey, payload, expected_scope).await {
        Ok(prepared) => publish_prepared(app, prepared).await,
        // This Result is used only for preparation; no post-send error enters it.
        Err(message) => ScopedControlPublication::NotAttempted { message },
    }
}

struct PreparedControl {
    event: nostr::Event,
    transport: OwnerOperationTransport,
    token: OwnerScopeToken,
}

async fn prepare_control<R: Runtime>(
    app: AppHandle<R>,
    agent_pubkey: String,
    payload: ScopedObserverControl,
    expected_scope: OwnerScopeToken,
) -> Result<PreparedControl, String> {
    let ScopedObserverControl::CancelTurn { ref turn_id, .. } = payload;
    if turn_id.trim().is_empty() || turn_id.len() > 128 {
        return Err("selected turn identity is missing or invalid".into());
    }
    let agent = PublicKey::from_hex(agent_pubkey.trim())
        .map_err(|error| format!("invalid agent pubkey: {error}"))?;
    let captured = capture(app.clone()).await?;
    if captured.token != expected_scope {
        return Err("active owner or workspace changed before sending".into());
    }
    let value = serde_json::to_value(payload).map_err(|error| error.to_string())?;
    let encrypted =
        buzz_core_pkg::observer::encrypt_observer_payload(&captured.keys, &agent, &value)
            .map_err(|error| format!("encrypt observer control failed: {error}"))?;
    let event = buzz_sdk_pkg::build_agent_observer_frame(
        &agent.to_hex(),
        &agent.to_hex(),
        buzz_core_pkg::observer::OBSERVER_FRAME_CONTROL,
        &encrypted,
    )
    .map_err(|error| format!("build observer control failed: {error}"))?
    .sign_with_keys(&captured.keys)
    .map_err(|error| format!("sign observer control failed: {error}"))?;
    let transport = match OwnerOperationTransport::captured(
        &app.state::<crate::AppState>(),
        captured.token.scope.community.clone(),
        captured.keys,
        None,
    ) {
        Ok(transport) => transport,
        Err(error) => return Err(error.to_string()),
    };
    Ok(PreparedControl {
        event,
        transport,
        token: captured.token,
    })
}

async fn publish_prepared<R: Runtime>(
    app: AppHandle<R>,
    prepared: PreparedControl,
) -> ScopedControlPublication {
    // Pass a lazy future: the shared transport polls it AFTER admission.
    let guard = async { assert_current(app.clone(), &prepared.token).await };
    match prepared.transport.publish(&prepared.event, guard).await {
        Ok(result) if result.accepted => ScopedControlPublication::Accepted {
            event_id: result.event_id,
        },
        Ok(result) => ScopedControlPublication::Unknown {
            message: result.message,
        },
        Err(error) => transport_failure(error),
    }
}

#[cfg(test)]
#[path = "scoped_observer_control_tests.rs"]
mod tests;
