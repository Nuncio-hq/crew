//! Local transport diagnostics owned by existing managed-runtime generations.
mod eligibility;
mod export;
mod monitor;
mod poll;
#[cfg(all(test, unix))]
mod producer_fixture;
mod reader;
mod retention;
pub use eligibility::*;
pub(crate) use poll::start;

use crate::app_state::AppState;
use crate::managed_agents::ManagedAgentRuntimeKey;
pub(crate) use export::{
    live_auth_evidence, retired_auth_evidence, NATIVE_AUTH_EVIDENCE_EXPORT_ENV,
};
#[cfg(test)]
pub(crate) use monitor::ReadTicket;
pub(crate) use monitor::{Diagnostics, Monitor};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

pub(crate) const PROCESS_INSPECTION_ERROR: &str =
    "Cannot inspect the managed process. Try stopping or restarting the agent.";

const STATUS_PATH_ENV: &str = "CREW_ACP_TRANSPORT_STATUS_PATH";
const START_NONCE_ENV: &str = "CREW_ACP_TRANSPORT_START_NONCE";
pub(crate) const STATUS_VERSION_ENV: &str = "CREW_ACP_TRANSPORT_STATUS_VERSION";
pub(crate) const SPAWN_STARTED_AT_ENV: &str = "CREW_ACP_TRANSPORT_SPAWN_STARTED_AT_MS";

fn status_path(log_path: &Path, nonce: &str) -> PathBuf {
    log_path
        .with_extension("transport")
        .join(format!("{nonce}.json"))
}

fn can_monitor(nonce: &str, owner: Option<&str>, setup: bool) -> bool {
    can_monitor_with_storage(cfg!(unix), nonce, owner, setup)
}

pub(crate) fn can_monitor_for_spawn(nonce: &str, owner: Option<&str>, setup: bool) -> bool {
    can_monitor(nonce, owner, setup)
}

pub(crate) fn pre_spawn_started_at_ms() -> Result<u64, String> {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "local transport clock unavailable")?
        .as_millis();
    let millis = u64::try_from(millis).map_err(|_| "local transport clock unavailable")?;
    (millis > 0)
        .then_some(millis)
        .ok_or_else(|| "local transport clock unavailable".into())
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

#[cfg(test)]
pub(crate) fn configure_child(
    command: &mut std::process::Command,
    log_path: &Path,
    nonce: &str,
    owner: Option<&str>,
    setup: bool,
) {
    configure_child_with_storage(command, log_path, nonce, owner, setup, true);
}

/// Configure the optional status pair after native preflight. If the
/// sidechannel is unavailable, the child keeps the legacy ACP environment and
/// Desktop projects its transport as unknown instead of preventing startup.
pub(crate) fn configure_child_with_storage(
    command: &mut std::process::Command,
    log_path: &Path,
    nonce: &str,
    owner: Option<&str>,
    setup: bool,
    storage_available: bool,
) {
    command
        .env_remove(STATUS_PATH_ENV)
        .env_remove(START_NONCE_ENV)
        .env_remove(STATUS_VERSION_ENV)
        .env_remove(SPAWN_STARTED_AT_ENV)
        .env_remove(NATIVE_AUTH_EVIDENCE_EXPORT_ENV);
    if storage_available && can_monitor(nonce, owner, setup) {
        command
            .env(STATUS_PATH_ENV, status_path(log_path, nonce))
            .env(START_NONCE_ENV, nonce);
    }
}

/// Add the negotiated v2 capability and native spawn lower bound only after
/// all caller-provided environment entries have been applied. Setup and
/// unavailable-storage children keep both keys absent.
pub(crate) fn configure_child_transport_version(
    command: &mut std::process::Command,
    enabled: bool,
    spawn_started_at_ms: u64,
) {
    command
        .env_remove(STATUS_VERSION_ENV)
        .env_remove(SPAWN_STARTED_AT_ENV);
    if enabled {
        command
            .env(STATUS_VERSION_ENV, "2")
            .env(SPAWN_STARTED_AT_ENV, spawn_started_at_ms.to_string());
    }
}

/// Compute and stamp the v2 capability once, immediately before the child is
/// spawned. A legacy or unavailable generation receives no stamp and remains
/// on the v1 health-only contract.
pub(crate) fn configure_child_transport_stamp(
    command: &mut std::process::Command,
    storage_available: bool,
    nonce: &str,
    owner: Option<&str>,
    setup: bool,
) -> Result<u64, String> {
    let enabled = storage_available && can_monitor_for_spawn(nonce, owner, setup);
    let spawn_started_at_ms = if enabled {
        pre_spawn_started_at_ms()?
    } else {
        0
    };
    configure_child_transport_version(command, enabled, spawn_started_at_ms);
    Ok(spawn_started_at_ms)
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
) -> Result<bool, String> {
    if !can_monitor(nonce, owner, setup) {
        return Ok(false);
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
    match retention::preflight(
        &log_path.with_extension("transport"),
        &key.runtime_id(),
        &protected,
        now,
    ) {
        Ok(()) => Ok(true),
        Err(retention::PreflightError::Unavailable) => {
            eprintln!(
                "local transport diagnostics unavailable for {}; continuing without status sidechannel",
                key.runtime_id()
            );
            Ok(false)
        }
        Err(retention::PreflightError::Refused) => {
            Err(buzz_core_pkg::transport_status::STORAGE_REVIEW_ERROR.into())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn registered_monitor(
    key: &ManagedAgentRuntimeKey,
    log_path: &Path,
    nonce: &str,
    process_id: u32,
    spawn_started_at_ms: u64,
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
    let path = status_path(log_path, nonce);
    if reader::open_owned_directory(path.parent()?, false).is_err() {
        // Preflight may have degraded the optional sidechannel because the
        // app-data tree is temporarily unavailable. Keep the process tracked,
        // but do not mint a monitor whose writer cannot be read safely.
        return None;
    }
    Some(Monitor::new(
        monitor::ReadTicket {
            key: key.clone(),
            nonce: nonce.into(),
            path,
            owner: owner?.to_ascii_lowercase(),
            epoch: 0,
            process_id,
            spawn_started_at_ms,
            wire_version: if spawn_started_at_ms > 0 { 2 } else { 1 },
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
        runtime.child.id(),
        runtime.spawn_started_at_ms,
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
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn transport_environment_keys_are_reserved_from_user_overrides() {
        assert!(super::super::env_vars::is_reserved_env_key(STATUS_PATH_ENV));
        assert!(super::super::env_vars::is_reserved_env_key(START_NONCE_ENV));
        assert!(super::super::env_vars::is_reserved_env_key(
            STATUS_VERSION_ENV
        ));
        assert!(super::super::env_vars::is_reserved_env_key(
            SPAWN_STARTED_AT_ENV
        ));
        assert!(super::super::env_vars::is_reserved_env_key(
            NATIVE_AUTH_EVIDENCE_EXPORT_ENV
        ));
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
            .env(START_NONCE_ENV, "untrusted")
            .env(NATIVE_AUTH_EVIDENCE_EXPORT_ENV, "1");
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
        assert_eq!(
            env[std::ffi::OsStr::new(NATIVE_AUTH_EVIDENCE_EXPORT_ENV)],
            None
        );
        configure_child(&mut command, log, &nonce, Some(&owner), true);
        let env: HashMap<_, _> = command.get_envs().collect();
        assert_eq!(env[std::ffi::OsStr::new(STATUS_PATH_ENV)], None);
        assert_eq!(env[std::ffi::OsStr::new(START_NONCE_ENV)], None);
        assert_eq!(
            env[std::ffi::OsStr::new(NATIVE_AUTH_EVIDENCE_EXPORT_ENV)],
            None
        );
    }

    #[test]
    #[cfg(unix)]
    fn unavailable_status_storage_keeps_legacy_spawn_environment() {
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let owner = "a".repeat(64);
        let mut command = std::process::Command::new("fixture");
        command
            .env(STATUS_PATH_ENV, "/untrusted/path")
            .env(START_NONCE_ENV, "untrusted");
        configure_child_with_storage(
            &mut command,
            Path::new("/fixture/runtime.log"),
            &nonce,
            Some(&owner),
            false,
            false,
        );
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
        assert!(registered_monitor(&key, log, &nonce, 1, 1, None, false, &diagnostics).is_none());
        assert!(registered_monitor(
            &key,
            log,
            "",
            1,
            1,
            Some(&"b".repeat(64)),
            false,
            &diagnostics,
        )
        .is_none());
        assert!(registered_monitor(
            &key,
            log,
            &nonce,
            1,
            1,
            Some(&"b".repeat(64)),
            true,
            &diagnostics,
        )
        .is_none());
    }

    #[test]
    #[cfg(unix)]
    fn registration_binds_native_owner_and_clears_retired_token() {
        let diagnostics = Arc::new(Mutex::new(Diagnostics::default()));
        let key = ManagedAgentRuntimeKey::new("a".repeat(64), "ws://fixture").unwrap();
        let owner = "b".repeat(64);
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("crew-338-monitor-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let log = root.join("runtime.log");
        let transport_dir = log.with_extension("transport");
        std::fs::create_dir(&transport_dir).unwrap();
        std::fs::set_permissions(&transport_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        assert!(reader::open_owned_directory(&transport_dir, false).is_ok());
        let old_ticket = monitor::ReadTicket {
            key: key.clone(),
            nonce: "old".into(),
            path: log.clone(),
            owner: owner.clone(),
            epoch: 0,
            process_id: 1,
            spawn_started_at_ms: 1,
            wire_version: 1,
        };
        Monitor::new(old_ticket, &diagnostics).retire(true, std::time::Instant::now());
        assert!(diagnostics
            .lock()
            .unwrap()
            .projection(&key, &owner, std::time::Instant::now())
            .is_some());
        let monitor =
            registered_monitor(&key, &log, &nonce, 1, 1, Some(&owner), false, &diagnostics)
                .expect("registered generation must own a monitor");
        let ticket = monitor.snapshot().unwrap();
        assert_eq!(ticket.key, key);
        assert_eq!(ticket.nonce, nonce);
        assert_eq!(ticket.owner, owner);
        assert_eq!(ticket.path, status_path(&log, &nonce));
        assert!(diagnostics
            .lock()
            .unwrap()
            .projection(&key, &owner, std::time::Instant::now())
            .is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
