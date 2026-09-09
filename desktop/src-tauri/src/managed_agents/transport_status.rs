//! Local transport diagnostics owned by existing managed-runtime generations.
mod eligibility;
mod monitor;
mod poll;
mod reader;
mod retention;
pub use eligibility::*;
pub(crate) use poll::start;

use crate::app_state::AppState;
use crate::managed_agents::ManagedAgentRuntimeKey;
pub(crate) use monitor::{Diagnostics, Monitor, ReadTicket};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

pub(crate) const PROCESS_INSPECTION_ERROR: &str =
    "Cannot inspect the managed process. Try stopping or restarting the agent.";

const STATUS_PATH_ENV: &str = "CREW_ACP_TRANSPORT_STATUS_PATH";
const START_NONCE_ENV: &str = "CREW_ACP_TRANSPORT_START_NONCE";

fn status_path(log_path: &Path, nonce: &str) -> PathBuf {
    log_path
        .with_extension("transport")
        .join(format!("{nonce}.json"))
}

fn can_monitor(nonce: &str, owner: Option<&str>, setup: bool) -> bool {
    can_monitor_with_storage(cfg!(unix), nonce, owner, setup)
}

fn can_monitor_with_storage(
    supported: bool,
    nonce: &str,
    owner: Option<&str>,
    setup: bool,
) -> bool {
    supported
        && !setup
        && owner.is_some_and(|owner| {
            owner.len() == 64 && owner.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        && uuid::Uuid::parse_str(nonce).is_ok_and(|value| {
            !value.is_nil() && (value.simple().to_string() == nonce || value.to_string() == nonce)
        })
}

pub(crate) fn configure_child(
    command: &mut std::process::Command,
    log_path: &Path,
    nonce: &str,
    owner: Option<&str>,
    setup: bool,
) {
    command
        .env_remove(STATUS_PATH_ENV)
        .env_remove(START_NONCE_ENV);
    if can_monitor(nonce, owner, setup) {
        command
            .env(STATUS_PATH_ENV, status_path(log_path, nonce))
            .env(START_NONCE_ENV, nonce);
    }
}

/// Called at the shared spawn boundary only after existing lifecycle guards
/// establish that this pair has no live registered child. Live starts return
/// before this seam; this is not a background process or directory sweep.
pub(crate) fn preflight_child(
    app: &AppHandle,
    key: &ManagedAgentRuntimeKey,
    log_path: &Path,
    nonce: &str,
    owner: Option<&str>,
    setup: bool,
) -> Result<(), String> {
    if !can_monitor(nonce, owner, setup) {
        return Ok(());
    }
    let state = app.state::<AppState>();
    let mut protected = std::collections::HashSet::from([nonce.to_owned()]);
    if let Some(previous) = state
        .managed_transport_diagnostics
        .lock()
        .map_err(|_| "local transport diagnostics unavailable")?
        .protected_nonce(key, std::time::Instant::now())
    {
        protected.insert(previous);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "local transport clock unavailable")?
        .as_secs();
    retention::preflight(
        &log_path.with_extension("transport"),
        &key.runtime_id(),
        &protected,
        now,
    )
}

fn registered_monitor(
    key: &ManagedAgentRuntimeKey,
    log_path: &Path,
    nonce: &str,
    owner: Option<&str>,
    setup: bool,
    diagnostics: &Arc<Mutex<Diagnostics>>,
) -> Option<Monitor> {
    // Called inside the existing processes mutex, only once the new generation
    // is registered. Failed spawn attempts do not discard the previous token.
    diagnostics
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear_key(key);
    if !can_monitor(nonce, owner, setup) {
        return None;
    }
    Some(Monitor::new(
        monitor::ReadTicket {
            key: key.clone(),
            nonce: nonce.into(),
            path: status_path(log_path, nonce),
            owner: owner?.to_ascii_lowercase(),
            epoch: 0,
        },
        diagnostics,
    ))
}

pub(crate) fn bind_registered(
    app: &AppHandle,
    key: &ManagedAgentRuntimeKey,
    runtime: &mut super::ManagedAgentPairRuntime,
    owner: Option<&str>,
) {
    let state = app.state::<AppState>();
    runtime.transport = registered_monitor(
        key,
        &runtime.log_path,
        &runtime.start_nonce,
        owner,
        runtime.setup_mode,
        &state.managed_transport_diagnostics,
    );
}

/// Caller holds the processes mutex before clearing derived diagnostics.
pub(crate) fn clear_key<R: tauri::Runtime>(app: &AppHandle<R>, key: &ManagedAgentRuntimeKey) {
    app.state::<AppState>()
        .managed_transport_diagnostics
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear_key(key);
}

/// Caller holds the processes mutex; successful delete/stop clears all pairs.
pub(crate) fn clear_pubkey<R: tauri::Runtime>(app: &AppHandle<R>, pubkey: &str) {
    app.state::<AppState>()
        .managed_transport_diagnostics
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear_pubkey(pubkey);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn transport_environment_keys_are_reserved_from_user_overrides() {
        assert!(super::super::env_vars::is_reserved_env_key(STATUS_PATH_ENV));
        assert!(super::super::env_vars::is_reserved_env_key(START_NONCE_ENV));
    }

    #[test]
    #[cfg(unix)]
    fn managed_spawn_overwrites_status_pair_and_setup_strips_both() {
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let owner = "a".repeat(64);
        let log = Path::new("/fixture/runtime.log");
        let mut command = std::process::Command::new("fixture");
        command
            .env(STATUS_PATH_ENV, "/untrusted/path")
            .env(START_NONCE_ENV, "untrusted");
        configure_child(&mut command, log, &nonce, Some(&owner), false);
        let env: HashMap<_, _> = command.get_envs().collect();
        assert_eq!(
            env[std::ffi::OsStr::new(STATUS_PATH_ENV)],
            Some(status_path(log, &nonce).as_os_str())
        );
        assert_eq!(
            env[std::ffi::OsStr::new(START_NONCE_ENV)],
            Some(std::ffi::OsStr::new(&nonce))
        );
        configure_child(&mut command, log, &nonce, Some(&owner), true);
        let env: HashMap<_, _> = command.get_envs().collect();
        assert_eq!(env[std::ffi::OsStr::new(STATUS_PATH_ENV)], None);
        assert_eq!(env[std::ffi::OsStr::new(START_NONCE_ENV)], None);
    }

    #[test]
    fn unsupported_secure_storage_never_opts_a_valid_generation_in() {
        assert!(!can_monitor_with_storage(
            false,
            &uuid::Uuid::new_v4().simple().to_string(),
            Some(&"a".repeat(64)),
            false
        ));
    }

    #[test]
    #[cfg(not(unix))]
    fn unsupported_platform_strips_the_pair_without_blocking_legacy_spawn() {
        let mut command = std::process::Command::new("fixture");
        command
            .env(STATUS_PATH_ENV, "untrusted")
            .env(START_NONCE_ENV, "untrusted");
        configure_child(
            &mut command,
            Path::new("runtime.log"),
            &uuid::Uuid::new_v4().simple().to_string(),
            Some(&"a".repeat(64)),
            false,
        );
        let env: HashMap<_, _> = command.get_envs().collect();
        assert_eq!(env[std::ffi::OsStr::new(STATUS_PATH_ENV)], None);
        assert_eq!(env[std::ffi::OsStr::new(START_NONCE_ENV)], None);
    }

    #[test]
    fn missing_owner_and_adopted_generation_remain_unmonitored() {
        let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
        let key = ManagedAgentRuntimeKey::new("a".repeat(64), "ws://fixture").unwrap();
        let log = Path::new("/fixture/runtime.log");
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        assert!(registered_monitor(&key, log, &nonce, None, false, &diagnostics).is_none());
        assert!(
            registered_monitor(&key, log, "", Some(&"b".repeat(64)), false, &diagnostics).is_none()
        );
        assert!(
            registered_monitor(&key, log, &nonce, Some(&"b".repeat(64)), true, &diagnostics)
                .is_none()
        );
    }

    #[test]
    #[cfg(unix)]
    fn registration_binds_native_owner_and_clears_retired_token() {
        let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
        let key = ManagedAgentRuntimeKey::new("a".repeat(64), "ws://fixture").unwrap();
        let owner = "b".repeat(64);
        let log = Path::new("/fixture/runtime.log");
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let old_ticket = monitor::ReadTicket {
            key: key.clone(),
            nonce: "old".into(),
            path: log.into(),
            owner: owner.clone(),
            epoch: 0,
        };
        Monitor::new(old_ticket, &diagnostics).retire(true, std::time::Instant::now());
        assert!(diagnostics
            .lock()
            .unwrap()
            .projection(&key, &owner, std::time::Instant::now())
            .is_some());
        let monitor = registered_monitor(&key, log, &nonce, Some(&owner), false, &diagnostics)
            .expect("registered generation must own a monitor");
        let ticket = monitor.snapshot().unwrap();
        assert_eq!(ticket.key, key);
        assert_eq!(ticket.nonce, nonce);
        assert_eq!(ticket.owner, owner);
        assert_eq!(ticket.path, status_path(log, &nonce));
        assert!(diagnostics
            .lock()
            .unwrap()
            .projection(&key, &owner, std::time::Instant::now())
            .is_none());
    }
}
