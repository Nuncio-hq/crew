//! Tauri IPC for Hermes profile lifecycle (create / list / delete).
//!
//! Explicit manager actions only (D-019 item 6). Never touches `default`.

use crate::app_state::AppState;
use crate::managed_agents::hermes_profile_archive::{
    archive_profile, estimate_profile, list_archives, permanently_delete_archive, restore_archive,
    running_agent_for_profile, HermesProfileArchiveAgent, HermesProfileArchiveEstimate,
    HermesProfileArchiveListing, HermesProfileArchiveResult,
};
use crate::managed_agents::hermes_profile_config::{
    read_profile_config, write_profile_config, HermesProfileConfigResult,
};
use crate::managed_agents::hermes_profile_lifecycle::{
    create_profile, delete_profile, list_profiles, HermesProfileLifecycleResult,
};
use crate::managed_agents::hermes_profile_soul::{
    read_profile_soul, write_profile_soul, HermesProfileSoulResult,
};
use crate::managed_agents::load_managed_agents;
use tauri::{AppHandle, Manager};

/// List named Hermes profiles under `~/.hermes/profiles/` (directory read).
#[tauri::command]
pub fn list_hermes_profiles() -> Result<Vec<String>, String> {
    list_profiles()
}

/// Create a Hermes profile: `hermes profile create <name> --no-alias`.
#[tauri::command]
pub fn create_hermes_profile(name: String) -> HermesProfileLifecycleResult {
    let result = create_profile(&name);
    tracing::debug!(
        success = result.is_success(),
        message = %result.message(),
        "create_hermes_profile"
    );
    result
}

/// Delete a Hermes profile: `hermes profile delete <name> -y` (directory-absence gate).
#[tauri::command]
pub fn delete_hermes_profile(name: String) -> HermesProfileLifecycleResult {
    let result = delete_profile(&name);
    tracing::debug!(
        success = result.is_success(),
        message = %result.message(),
        "delete_hermes_profile"
    );
    result
}

#[tauri::command]
pub fn read_hermes_profile_model(name: String) -> HermesProfileConfigResult {
    read_profile_config(&name)
}

#[tauri::command]
pub fn write_hermes_profile_model(
    name: String,
    provider: Option<String>,
    model: Option<String>,
) -> HermesProfileConfigResult {
    write_profile_config(&name, provider, model)
}

#[tauri::command]
pub fn read_hermes_profile_soul(name: String) -> HermesProfileSoulResult {
    read_profile_soul(&name)
}

#[tauri::command]
pub fn write_hermes_profile_soul(name: String, content: String) -> HermesProfileSoulResult {
    write_profile_soul(&name, &content)
}

#[tauri::command]
pub fn estimate_hermes_profile_archive(
    profile: String,
) -> Result<HermesProfileArchiveEstimate, String> {
    estimate_profile(&profile)
}

#[tauri::command]
pub fn archive_hermes_profile(
    profile: String,
    reason: Option<String>,
    app: AppHandle,
) -> HermesProfileArchiveResult {
    let state = app.state::<AppState>();
    let records = match load_managed_agents(&app) {
        Ok(records) => records,
        Err(message) => {
            return HermesProfileArchiveResult::Failed {
                profile: Some(profile),
                id: None,
                message,
            }
        }
    };
    let mut runtimes = match state.managed_agent_processes.lock() {
        Ok(runtimes) => runtimes,
        Err(error) => {
            return HermesProfileArchiveResult::Failed {
                profile: Some(profile),
                id: None,
                message: error.to_string(),
            }
        }
    };
    if let Some(agent) = running_agent_for_profile(&profile, &records, &mut runtimes) {
        return HermesProfileArchiveResult::AgentRunning {
            profile,
            agent_name: agent.name,
            agent_pubkey: agent.pubkey,
            message: "stop the running agent before archiving its Hermes profile".to_string(),
        };
    }
    let agent = records.iter().find_map(|record| {
        record
            .hermes_profile
            .as_deref()
            .is_some_and(|bound| bound.trim() == profile.trim())
            .then(|| HermesProfileArchiveAgent {
                name: record.name.clone(),
                pubkey: record.pubkey.clone(),
            })
    });
    archive_profile(&profile, agent.as_ref(), reason.as_deref())
}

#[tauri::command]
pub fn list_hermes_profile_archives() -> Result<Vec<HermesProfileArchiveListing>, String> {
    list_archives()
}

#[tauri::command]
pub fn restore_hermes_profile_archive(id: String, app: AppHandle) -> HermesProfileArchiveResult {
    let state = app.state::<AppState>();
    let records = match load_managed_agents(&app) {
        Ok(records) => records,
        Err(message) => {
            return HermesProfileArchiveResult::Failed {
                profile: None,
                id: Some(id),
                message,
            }
        }
    };
    let manifests = match list_archives() {
        Ok(manifests) => manifests,
        Err(message) => {
            return HermesProfileArchiveResult::Failed {
                profile: None,
                id: Some(id),
                message,
            }
        }
    };
    let Some(listing) = manifests.into_iter().find(|listing| listing.id == id) else {
        return HermesProfileArchiveResult::DoesNotExist {
            id,
            message: "archive id does not exist".to_string(),
        };
    };
    let mut runtimes = match state.managed_agent_processes.lock() {
        Ok(runtimes) => runtimes,
        Err(error) => {
            return HermesProfileArchiveResult::Failed {
                profile: Some(listing.manifest.profile),
                id: Some(id),
                message: error.to_string(),
            }
        }
    };
    if let Some(agent) =
        running_agent_for_profile(&listing.manifest.profile, &records, &mut runtimes)
    {
        return HermesProfileArchiveResult::AgentRunning {
            profile: listing.manifest.profile,
            agent_name: agent.name,
            agent_pubkey: agent.pubkey,
            message: "stop the running agent before restoring this Hermes profile".to_string(),
        };
    }
    restore_archive(&id)
}

#[tauri::command]
pub fn permanently_delete_hermes_profile_archive(
    id: String,
    confirmation_token: String,
    app: AppHandle,
) -> HermesProfileArchiveResult {
    let state = app.state::<AppState>();
    let records = match load_managed_agents(&app) {
        Ok(records) => records,
        Err(message) => {
            return HermesProfileArchiveResult::Failed {
                profile: None,
                id: Some(id),
                message,
            }
        }
    };
    let manifests = match list_archives() {
        Ok(manifests) => manifests,
        Err(message) => {
            return HermesProfileArchiveResult::Failed {
                profile: None,
                id: Some(id),
                message,
            }
        }
    };
    let Some(listing) = manifests.into_iter().find(|listing| listing.id == id) else {
        return HermesProfileArchiveResult::DoesNotExist {
            id,
            message: "archive id does not exist".to_string(),
        };
    };
    let profile = listing.manifest.profile;
    let mut runtimes = match state.managed_agent_processes.lock() {
        Ok(runtimes) => runtimes,
        Err(error) => {
            return HermesProfileArchiveResult::Failed {
                profile: Some(profile),
                id: Some(id),
                message: error.to_string(),
            }
        }
    };
    if let Some(agent) = running_agent_for_profile(&profile, &records, &mut runtimes) {
        return HermesProfileArchiveResult::AgentRunning {
            profile,
            agent_name: agent.name,
            agent_pubkey: agent.pubkey,
            message: "stop the running agent before permanently deleting its archive".to_string(),
        };
    }
    permanently_delete_archive(&id, &confirmation_token)
}
