//! Best-effort crew-role projection publish (issue #116 Slice 1).
//! Split from `agent_models.rs` to stay under the desktop file-size ratchet.

use crate::app_state::AppState;
use crate::managed_agents::ManagedAgentRecord;
use nostr::Keys;
use tauri::AppHandle;

/// Snapshot for post-lock projection publish when role changed.
pub type RoleProjectionJob = (String, String, Option<String>, String, String);

/// Apply patch before save; after save call [`finish_role_save`].
pub fn apply_crew_role_patch(
    record: &mut ManagedAgentRecord,
    patch: &Option<Option<String>>,
) -> Result<bool, String> {
    let Some(role_patch) = patch else {
        return Ok(false);
    };
    let parsed = crate::managed_agents::crew_role::parse_crew_role(role_patch.as_deref())?;
    if record.crew_role == parsed {
        return Ok(false);
    }
    record.crew_role = parsed;
    Ok(true)
}

/// Persist role file + optional projection job after the record is saved.
pub fn finish_role_save(
    app: &AppHandle,
    state: &AppState,
    role_changed: bool,
    record: &ManagedAgentRecord,
) -> Option<RoleProjectionJob> {
    let _ = crate::managed_agents::crew_role::write_crew_role_file(
        app,
        &record.pubkey,
        record.crew_role.as_deref(),
    );
    role_changed.then(|| {
        (
            record.pubkey.clone(),
            record.name.clone(),
            record.crew_role.clone(),
            record.private_key_nsec.clone(),
            crate::relay::effective_agent_relay_url(
                &record.relay_url,
                &crate::relay::relay_ws_url_with_override(state),
            ),
        )
    })
}

/// Publish kind 10100 with optional `crew-role` tag (spike 0015 projection).
pub async fn publish_crew_role_projection(
    state: &AppState,
    relay_url: &str,
    agent_keys: &Keys,
    display_name: &str,
    role: Option<&str>,
) -> Result<(), String> {
    let builder = crate::managed_agents::crew_role::build_agent_profile_event(
        display_name,
        Some("owner_only"),
        role,
        &[],
    )?;
    crate::relay::submit_event_at_with_keys(
        builder,
        state,
        &crate::relay::relay_http_base_url(relay_url),
        agent_keys,
    )
    .await
    .map(|_| ())
    .map_err(|e| format!("crew-role projection publish failed: {e}"))
}

/// Room-visible announcement stub (FOUNDER-PRODUCT).
pub async fn publish_role_announcement_stub(
    _state: &AppState,
    agent_name: &str,
    role: Option<&str>,
) -> Result<(), String> {
    let text = crate::managed_agents::crew_role::role_announcement_text(agent_name, role);
    tracing::info!(target: "crew_role", announcement = %text, "role assignment announcement");
    Ok(())
}

/// Parse agent key and best-effort publish projection + announcement.
pub async fn publish_role_side_effects(
    state: &AppState,
    name: &str,
    role: Option<&str>,
    nsec: &str,
    relay_url: &str,
) {
    let Ok(agent_keys) = Keys::parse(nsec) else {
        return;
    };
    let _ = publish_crew_role_projection(state, relay_url, &agent_keys, name, role).await;
    let _ = publish_role_announcement_stub(state, name, role).await;
}
