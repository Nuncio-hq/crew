//! Tauri IPC for Hermes profile lifecycle (create / list / delete).
//!
//! Explicit manager actions only (D-019 item 6). Never touches `default`.

use crate::managed_agents::hermes_profile_lifecycle::{
    create_profile, delete_profile, list_profiles, HermesProfileLifecycleResult,
};

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
