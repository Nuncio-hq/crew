//! Provider launch serialization and captured mention-wake scope.
use super::{build_deploy_payload, AgentStartScope};
use crate::{app_state::AppState, managed_agents::*, util::now_iso};
use std::sync::Arc;
use tauri::AppHandle;

pub(super) async fn deploy_with_scope(
    app: &AppHandle,
    state: &AppState,
    pubkey: &str,
    scope: &AgentStartScope,
) -> Result<(), String> {
    scope.validate()?;
    let lock = {
        let mut locks = state
            .provider_deploy_locks
            .lock()
            .map_err(|e| e.to_string())?;
        Arc::clone(
            locks
                .entry(pubkey.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    };
    let _guard = lock.lock().await;
    // Waiting for another deploy may change the saved policy, relay or identity.
    // Build once after the await and validate the exact payload passed to the provider.
    let (provider_id, config, cached_binary_path, mut agent_json) = {
        let _store_guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let records = load_managed_agents(app)?;
        let record = records
            .iter()
            .find(|r| r.pubkey == pubkey)
            .ok_or_else(|| format!("agent {pubkey} not found"))?;
        let BackendKind::Provider { id, config } = &record.backend else {
            return Err(format!("agent {pubkey} is not provider-backed"));
        };
        (
            id.clone(),
            config.clone(),
            record.provider_binary_path.clone(),
            build_deploy_payload(app, state, record)?,
        )
    };
    prepare_scoped_payload(&mut agent_json, scope)?;
    // Resolve via discovered candidates only. Cached path must match BOTH
    // "is a discovered candidate" AND "belongs to this provider_id". A tampered
    // record cannot redirect deploys to a different provider's binary.
    let bin_path = cached_binary_path
        .as_deref()
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists())
        .map(|p| p.canonicalize().unwrap_or(p))
        .filter(|canonical| {
            discover_provider_candidates().iter().any(|(id, cp)| {
                id == &provider_id && cp.canonicalize().ok().as_ref() == Some(canonical)
            })
        })
        .map_or_else(|| resolve_provider_binary(&provider_id), Ok)?;

    let config_clone = config.clone();
    let deploy_result =
        tokio::task::spawn_blocking(move || provider_deploy(&bin_path, &agent_json, &config_clone))
            .await
            .map_err(|e| format!("spawn_blocking failed: {e}"))?;

    // Persist result under lock.
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut records = load_managed_agents(app)?;
    let rec = records
        .iter_mut()
        .find(|r| r.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))?;

    match deploy_result {
        Ok(backend_agent_id) => {
            rec.backend_agent_id = Some(backend_agent_id);
            rec.last_started_at = Some(now_iso());
            rec.updated_at = now_iso();
            rec.last_error = None;
        }
        Err(ref e) => {
            rec.last_error = Some(e.clone());
            rec.updated_at = now_iso();
            save_managed_agents(app, &records)?;
            return Err(e.clone());
        }
    }
    save_managed_agents(app, &records)?;
    Ok(())
}

/// Validate scope before adding invocation-only replay metadata; neither is persisted.
fn prepare_scoped_payload(
    payload: &mut serde_json::Value,
    scope: &AgentStartScope,
) -> Result<(), String> {
    scope.validate()?;
    if let Some(expected) = scope.expected_relay_url.as_deref() {
        let actual = payload
            .get("relay_url")
            .and_then(|v| v.as_str())
            .ok_or("deploy payload carries no relay; not deployed")?;
        crate::relay::assert_expected_relay_scope(
            Some(expected),
            &crate::relay::relay_http_base_url(actual),
        )?;
    }
    if let Some(expected) = scope.expected_signer_pubkey.as_deref() {
        let actual = payload
            .pointer("/launch/owner_pubkey")
            .and_then(|v| v.as_str())
            .ok_or("deploy payload carries no owner identity; not deployed")?;
        crate::relay::assert_expected_signer(Some(expected), actual)?;
    }
    if let Some(floor) = scope.replay_floor_unix {
        let launch = payload
            .get_mut("launch")
            .and_then(|v| v.as_object_mut())
            .ok_or("deploy payload carries no launch block; not deployed")?;
        if let Some(env) = launch.get_mut("env").and_then(|v| v.as_object_mut()) {
            env.retain(|key, _| !key.eq_ignore_ascii_case(REPLAY_FLOOR_ENV_VAR));
        }
        let policy = launch
            .entry("policy_env")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or("deploy payload carries invalid launch policy; not deployed")?;
        policy.retain(|key, _| !key.eq_ignore_ascii_case(REPLAY_FLOOR_ENV_VAR));
        policy.insert(REPLAY_FLOOR_ENV_VAR.into(), floor.to_string().into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "provider-deployment-tests.rs"]
mod tests;
