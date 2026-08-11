//! Hermes-specific profile readiness facts.
//!
//! Per D-025, the filesystem/config checks and Hermes binary probe stay in a
//! Crew-owned module; the result is projected through generic Crew/Buzz
//! readiness and status contracts.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use super::hermes_profile_lifecycle::hermes_profile_dir;

const BINARY_PROBE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HermesProfileReadiness {
    /// Reserved for a future truthful Hermes auth probe.
    Ready,
    Missing {
        profile: String,
    },
    BrokenConfig {
        profile: String,
        diagnostic: String,
    },
    BinaryMissing {
        command: String,
    },
    /// Authentication cannot be verified until Hermes exposes a headless probe.
    AuthUnknown {
        profile: String,
    },
}

#[derive(Clone, Copy)]
struct ProbeResult {
    runnable: bool,
    checked_at: Instant,
}

fn binary_probe_cache() -> &'static Mutex<HashMap<PathBuf, ProbeResult>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, ProbeResult>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Clear the short-lived Hermes `--version` probe cache after installation.
pub fn invalidate_hermes_binary_probe_cache() {
    if let Ok(mut cache) = binary_probe_cache().lock() {
        cache.clear();
    }
}

fn probe_binary(path: &Path) -> bool {
    if let Ok(cache) = binary_probe_cache().lock() {
        if let Some(result) = cache.get(path) {
            if result.checked_at.elapsed() < BINARY_PROBE_TTL {
                return result.runnable;
            }
        }
    }
    let runnable = Command::new(path)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if let Ok(mut cache) = binary_probe_cache().lock() {
        cache.insert(
            path.to_path_buf(),
            ProbeResult {
                runnable,
                checked_at: Instant::now(),
            },
        );
    }
    runnable
}

fn is_hermes_command(command: &str) -> bool {
    crate::managed_agents::known_acp_runtime(command).is_some_and(|runtime| runtime.id == "hermes")
}

pub(crate) fn read_profile_yaml(profile_dir: &Path) -> Result<Option<serde_yaml::Value>, String> {
    let config = profile_dir.join("config.yaml");
    if !config.is_file() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&config).map_err(|error| error.to_string())?;
    serde_yaml::from_str::<serde_yaml::Value>(&contents)
        .map(Some)
        .map_err(|error| error.to_string())
}

/// Evaluate one Hermes command and its optional bound profile.
pub fn hermes_profile_readiness(
    command: &str,
    profile: Option<&str>,
) -> Option<HermesProfileReadiness> {
    if !is_hermes_command(command) {
        return None;
    }
    let resolved = crate::managed_agents::resolve_command(command);
    hermes_profile_readiness_with(command, profile, resolved)
}

/// Injection seam for readiness tests and callers with a resolved binary path.
pub fn hermes_profile_readiness_with(
    command: &str,
    profile: Option<&str>,
    resolved: Option<PathBuf>,
) -> Option<HermesProfileReadiness> {
    if !is_hermes_command(command) {
        return None;
    }
    let Some(binary) = resolved else {
        return Some(HermesProfileReadiness::BinaryMissing {
            command: command.to_string(),
        });
    };
    if !probe_binary(&binary) {
        return Some(HermesProfileReadiness::BinaryMissing {
            command: command.to_string(),
        });
    }
    let profile = profile.map(str::trim).filter(|name| !name.is_empty())?;
    let Some(profile_dir) = hermes_profile_dir(profile) else {
        return Some(HermesProfileReadiness::Missing {
            profile: profile.to_string(),
        });
    };
    if !profile_dir.is_dir() {
        return Some(HermesProfileReadiness::Missing {
            profile: profile.to_string(),
        });
    }
    if let Err(error) = read_profile_yaml(&profile_dir) {
        return Some(HermesProfileReadiness::BrokenConfig {
            profile: profile.to_string(),
            diagnostic: error,
        });
    }
    Some(HermesProfileReadiness::AuthUnknown {
        profile: profile.to_string(),
    })
}
