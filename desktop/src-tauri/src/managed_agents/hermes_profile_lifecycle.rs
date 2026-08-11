//! Headless Hermes profile create/list/delete (Crew feature 0001 / Phase 03).
//!
//! Invokes only `hermes profile create|delete` as explicit manager actions
//! (D-019 item 6). Never touches the manager's `default` profile / `~/.hermes`
//! root. Delete always passes `-y` and verifies by directory absence
//! (spike 0011: bare `delete` on a non-TTY auto-cancels with exit 0).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::managed_agents::hermes_profile::{
    validate_hermes_profile_name, HERMES_FORBIDDEN_PROFILE_NAME,
};
use crate::managed_agents::resolve_command;
use crate::util::configure_no_window;

/// CLI create flags: `--no-alias` (Crew binds by name; wrappers violate P-5).
/// Bundled skills are kept by default (no `--no-skills`) — see D-023.
const CREATE_FLAGS: &[&str] = &["--no-alias"];

/// Outcome of a Hermes profile lifecycle operation that the UI can render.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HermesProfileLifecycleResult {
    Ok {
        name: String,
    },
    /// Profile was already absent — treat as success-ish for delete (idempotent).
    AlreadyGone {
        name: String,
    },
    InvalidName {
        name: String,
        message: String,
    },
    AlreadyExists {
        name: String,
        message: String,
    },
    DoesNotExist {
        name: String,
        message: String,
    },
    BinaryMissing {
        message: String,
    },
    Failed {
        name: String,
        message: String,
    },
}

impl HermesProfileLifecycleResult {
    /// True when the operation completed successfully (including already-gone delete).
    #[must_use]
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            HermesProfileLifecycleResult::Ok { .. }
                | HermesProfileLifecycleResult::AlreadyGone { .. }
        )
    }

    /// Human-readable message for UI surfaces.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            HermesProfileLifecycleResult::Ok { name } => {
                format!("Hermes profile '{name}' ready.")
            }
            HermesProfileLifecycleResult::AlreadyGone { name } => {
                format!("Hermes profile '{name}' was already gone.")
            }
            HermesProfileLifecycleResult::InvalidName { message, .. }
            | HermesProfileLifecycleResult::AlreadyExists { message, .. }
            | HermesProfileLifecycleResult::DoesNotExist { message, .. }
            | HermesProfileLifecycleResult::BinaryMissing { message }
            | HermesProfileLifecycleResult::Failed { message, .. } => message.clone(),
        }
    }
}

/// Resolve Hermes home: `HERMES_HOME` if set, else `~/.hermes`.
pub fn hermes_home() -> Option<PathBuf> {
    if let Ok(override_home) = std::env::var("HERMES_HOME") {
        let trimmed = override_home.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }
    dirs::home_dir().map(|home| home.join(".hermes"))
}

/// `~/.hermes/profiles` (or `$HERMES_HOME/profiles`).
pub fn hermes_profiles_dir() -> Option<PathBuf> {
    hermes_home().map(|home| home.join("profiles"))
}

/// Directory for a named profile. Returns `None` when home cannot be resolved.
pub fn hermes_profile_dir(name: &str) -> Option<PathBuf> {
    hermes_profiles_dir().map(|dir| dir.join(name.trim()))
}

/// List named profiles under `profiles/` that pass Crew's name regex.
/// Directory read only — no CLI (cheap, no TTY concerns).
pub fn list_profiles() -> Result<Vec<String>, String> {
    let Some(dir) = hermes_profiles_dir() else {
        return Ok(vec![]);
    };
    if !dir.exists() {
        return Ok(vec![]);
    }
    let entries =
        fs::read_dir(&dir).map_err(|e| format!("failed to read {}: {e}", dir.display()))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("failed to read profile entry: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("failed to stat profile entry: {e}"))?;
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if validate_hermes_profile_name(&name).is_ok() {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// Create a named Hermes profile via `hermes profile create <name> --no-alias`.
///
/// Success = exit 0 **and** directory exists afterward.
pub fn create_profile(name: &str) -> HermesProfileLifecycleResult {
    create_profile_with(name, resolve_command("hermes"))
}

/// Test/injection seam for [`create_profile`].
pub(crate) fn create_profile_with(
    name: &str,
    binary: Option<PathBuf>,
) -> HermesProfileLifecycleResult {
    let trimmed = name.trim();
    if let Err(message) = validate_hermes_profile_name(trimmed) {
        return HermesProfileLifecycleResult::InvalidName {
            name: trimmed.to_string(),
            message,
        };
    }
    if trimmed == HERMES_FORBIDDEN_PROFILE_NAME {
        return HermesProfileLifecycleResult::InvalidName {
            name: trimmed.to_string(),
            message: "hermes profile 'default' cannot be created or bound by Crew".to_string(),
        };
    }

    let Some(profile_path) = hermes_profile_dir(trimmed) else {
        return HermesProfileLifecycleResult::Failed {
            name: trimmed.to_string(),
            message: "could not resolve Hermes home directory".to_string(),
        };
    };
    if profile_path.is_dir() {
        return HermesProfileLifecycleResult::AlreadyExists {
            name: trimmed.to_string(),
            message: format!(
                "Profile '{trimmed}' already exists at {}",
                profile_path.display()
            ),
        };
    }

    let Some(binary) = binary else {
        return HermesProfileLifecycleResult::BinaryMissing {
            message: "hermes binary not found on PATH".to_string(),
        };
    };

    let mut args = vec![
        "profile".to_string(),
        "create".to_string(),
        trimmed.to_string(),
    ];
    args.extend(CREATE_FLAGS.iter().map(|s| (*s).to_string()));

    match run_hermes(&binary, &args) {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{stdout}{stderr}");
            let exit_ok = output.status.success();
            let dir_present = profile_path.is_dir();

            if exit_ok && dir_present {
                return HermesProfileLifecycleResult::Ok {
                    name: trimmed.to_string(),
                };
            }

            let lower = combined.to_lowercase();
            if lower.contains("invalid profile name") {
                return HermesProfileLifecycleResult::InvalidName {
                    name: trimmed.to_string(),
                    message: first_error_line(&combined)
                        .unwrap_or_else(|| format!("Invalid profile name '{trimmed}'")),
                };
            }
            if lower.contains("already exists") {
                return HermesProfileLifecycleResult::AlreadyExists {
                    name: trimmed.to_string(),
                    message: first_error_line(&combined).unwrap_or_else(|| {
                        format!(
                            "Profile '{trimmed}' already exists at {}",
                            profile_path.display()
                        )
                    }),
                };
            }

            HermesProfileLifecycleResult::Failed {
                name: trimmed.to_string(),
                message: if !exit_ok {
                    first_error_line(&combined).unwrap_or_else(|| {
                        format!(
                            "hermes profile create failed (exit {})",
                            output.status.code().unwrap_or(-1)
                        )
                    })
                } else {
                    format!(
                        "hermes profile create exited 0 but directory is missing at {}",
                        profile_path.display()
                    )
                },
            }
        }
        Err(message) => HermesProfileLifecycleResult::Failed {
            name: trimmed.to_string(),
            message,
        },
    }
}

/// Delete a named Hermes profile via `hermes profile delete <name> -y`.
///
/// Success = exit 0 **and** directory absent afterward. Exit 0 with the
/// directory still present is a hard failure (spike 0011 non-TTY trap).
/// Missing profile (exit 1 "does not exist") maps to [`AlreadyGone`].
pub fn delete_profile(name: &str) -> HermesProfileLifecycleResult {
    delete_profile_with(name, resolve_command("hermes"))
}

/// Test/injection seam for [`delete_profile`].
pub(crate) fn delete_profile_with(
    name: &str,
    binary: Option<PathBuf>,
) -> HermesProfileLifecycleResult {
    let trimmed = name.trim();
    if trimmed == HERMES_FORBIDDEN_PROFILE_NAME || trimmed.is_empty() {
        return HermesProfileLifecycleResult::InvalidName {
            name: trimmed.to_string(),
            message: "Crew never deletes the manager's 'default' Hermes profile".to_string(),
        };
    }
    if let Err(message) = validate_hermes_profile_name(trimmed) {
        return HermesProfileLifecycleResult::InvalidName {
            name: trimmed.to_string(),
            message,
        };
    }

    let Some(profile_path) = hermes_profile_dir(trimmed) else {
        return HermesProfileLifecycleResult::Failed {
            name: trimmed.to_string(),
            message: "could not resolve Hermes home directory".to_string(),
        };
    };

    if !profile_path.exists() {
        return HermesProfileLifecycleResult::AlreadyGone {
            name: trimmed.to_string(),
        };
    }

    let Some(binary) = binary else {
        return HermesProfileLifecycleResult::BinaryMissing {
            message: "hermes binary not found on PATH".to_string(),
        };
    };

    let args = [
        "profile".to_string(),
        "delete".to_string(),
        trimmed.to_string(),
        "-y".to_string(),
    ];

    match run_hermes(&binary, &args) {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{stdout}{stderr}");
            let dir_gone = !profile_path.exists();

            if dir_gone {
                return HermesProfileLifecycleResult::Ok {
                    name: trimmed.to_string(),
                };
            }

            let lower = combined.to_lowercase();
            if lower.contains("does not exist") {
                // CLI said missing but we saw the dir earlier — treat as already gone
                // only if the dir is actually gone; otherwise fall through to Failed.
                if dir_gone {
                    return HermesProfileLifecycleResult::AlreadyGone {
                        name: trimmed.to_string(),
                    };
                }
            }

            // Spike 0011 trap: exit 0 without -y leaves the directory intact.
            HermesProfileLifecycleResult::Failed {
                name: trimmed.to_string(),
                message: format!(
                    "hermes profile delete reported success but '{}' still exists — \
                     deletion requires -y and directory absence (spike 0011)",
                    profile_path.display()
                ),
            }
        }
        Err(message) => {
            // If the binary reported "does not exist" via a spawn failure path
            // we still verify by directory.
            if !profile_path.exists() {
                return HermesProfileLifecycleResult::AlreadyGone {
                    name: trimmed.to_string(),
                };
            }
            HermesProfileLifecycleResult::Failed {
                name: trimmed.to_string(),
                message,
            }
        }
    }
}

fn run_hermes(binary: &Path, args: &[String]) -> Result<std::process::Output, String> {
    let mut command = Command::new(binary);
    command.args(args);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    configure_no_window(&mut command);
    if let Some(home) = hermes_home() {
        // Ensure the CLI targets the same home we verify against.
        command.env("HERMES_HOME", &home);
    }
    command
        .output()
        .map_err(|e| format!("failed to spawn hermes: {e}"))
}

fn first_error_line(combined: &str) -> Option<String> {
    combined
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_agents::{clear_resolve_cache, lock_path_mutex};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::MutexGuard;

    struct TestEnv {
        _path_guard: MutexGuard<'static, ()>,
        _temp: tempfile::TempDir,
        hermes_home: PathBuf,
        bin_dir: PathBuf,
        original_path: Option<String>,
        original_hermes_home: Option<String>,
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            clear_resolve_cache();
            match &self.original_path {
                Some(p) => std::env::set_var("PATH", p),
                None => std::env::remove_var("PATH"),
            }
            match &self.original_hermes_home {
                Some(h) => std::env::set_var("HERMES_HOME", h),
                None => std::env::remove_var("HERMES_HOME"),
            }
        }
    }

    /// Fake `hermes` that implements create/delete against `$HERMES_HOME/profiles`.
    /// Modes:
    /// - normal: create makes dir; delete -y removes dir; missing → exit 1
    /// - trap: delete without checking -y exits 0 and leaves dir (spike 0011)
    /// - no-delete-flag-trap: delete always exits 0 without removing (even with -y)
    fn write_fake_hermes(bin_dir: &Path, mode: &str) {
        let script = bin_dir.join("hermes");
        let body = match mode {
            "trap-bare-delete" => {
                r#"#!/bin/sh
# Spike 0011: bare delete (no -y) prints Cancelled and exits 0 without deleting.
set -eu
HOME_ROOT="${HERMES_HOME:?}"
PROFILES="$HOME_ROOT/profiles"
cmd="${1:-}"; sub="${2:-}"; name="${3:-}"; flag="${4:-}"
if [ "$cmd" = "profile" ] && [ "$sub" = "create" ]; then
  case "$name" in
    *[!a-z0-9_-]*|"") echo "Error: Invalid profile name" >&2; exit 1 ;;
  esac
  if [ -d "$PROFILES/$name" ]; then
    echo "Error: Profile '$name' already exists at $PROFILES/$name" >&2
    exit 1
  fi
  mkdir -p "$PROFILES/$name"
  echo "created $name"
  exit 0
fi
if [ "$cmd" = "profile" ] && [ "$sub" = "delete" ]; then
  if [ "$flag" != "-y" ]; then
    echo "Type '$name' to confirm:"
    echo "Cancelled."
    exit 0
  fi
  if [ ! -d "$PROFILES/$name" ]; then
    echo "Error: Profile '$name' does not exist." >&2
    exit 1
  fi
  rm -rf "$PROFILES/$name"
  echo "Profile '$name' deleted."
  exit 0
fi
echo "unexpected: $*" >&2
exit 2
"#
            }
            "exit0-but-keep-dir" => {
                r#"#!/bin/sh
# Malicious/broken CLI: always exits 0 on delete without removing the dir.
set -eu
HOME_ROOT="${HERMES_HOME:?}"
PROFILES="$HOME_ROOT/profiles"
cmd="${1:-}"; sub="${2:-}"; name="${3:-}"
if [ "$cmd" = "profile" ] && [ "$sub" = "create" ]; then
  mkdir -p "$PROFILES/$name"
  exit 0
fi
if [ "$cmd" = "profile" ] && [ "$sub" = "delete" ]; then
  echo "Profile '$name' deleted."
  exit 0
fi
exit 2
"#
            }
            _ => {
                r#"#!/bin/sh
set -eu
HOME_ROOT="${HERMES_HOME:?}"
PROFILES="$HOME_ROOT/profiles"
cmd="${1:-}"; sub="${2:-}"; name="${3:-}"; flag="${4:-}"
if [ "$cmd" = "profile" ] && [ "$sub" = "create" ]; then
  case "$name" in
    *[!a-z0-9_-]*|"") echo "Error: Invalid profile name '$name'. Must match [a-z0-9][a-z0-9_-]{0,63}" >&2; exit 1 ;;
  esac
  if [ "$name" = "default" ]; then
    echo "Error: Invalid profile name 'default'" >&2
    exit 1
  fi
  if [ -d "$PROFILES/$name" ]; then
    echo "Error: Profile '$name' already exists at $PROFILES/$name" >&2
    exit 1
  fi
  mkdir -p "$PROFILES/$name"
  echo "created $name"
  exit 0
fi
if [ "$cmd" = "profile" ] && [ "$sub" = "delete" ]; then
  if [ "$flag" != "-y" ]; then
    echo "Type '$name' to confirm:"
    echo "Cancelled."
    exit 0
  fi
  if [ ! -d "$PROFILES/$name" ]; then
    echo "Error: Profile '$name' does not exist." >&2
    exit 1
  fi
  rm -rf "$PROFILES/$name"
  echo "Profile '$name' deleted."
  exit 0
fi
echo "unexpected: $*" >&2
exit 2
"#
            }
        };
        fs::write(&script, body).expect("write fake hermes");
        let mut perms = fs::metadata(&script).expect("meta").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod");
    }

    fn setup(mode: &str) -> TestEnv {
        let path_guard = lock_path_mutex();
        let temp = tempfile::tempdir().expect("tempdir");
        let hermes_home = temp.path().join("hermes-home");
        let bin_dir = temp.path().join("bin");
        fs::create_dir_all(hermes_home.join("profiles")).expect("profiles dir");
        fs::create_dir_all(&bin_dir).expect("bin dir");
        write_fake_hermes(&bin_dir, mode);

        let original_path = std::env::var("PATH").ok();
        let original_hermes_home = std::env::var("HERMES_HOME").ok();
        std::env::set_var("HERMES_HOME", &hermes_home);
        // Keep /bin:/usr/bin so the fake shell script can find mkdir/rm.
        let path_value = format!("{}:/bin:/usr/bin", bin_dir.display());
        std::env::set_var("PATH", &path_value);
        clear_resolve_cache();

        TestEnv {
            _path_guard: path_guard,
            _temp: temp,
            hermes_home,
            bin_dir,
            original_path,
            original_hermes_home,
        }
    }

    fn fake_binary(env: &TestEnv) -> PathBuf {
        env.bin_dir.join("hermes")
    }

    #[test]
    fn create_profile_success_makes_directory() {
        let env = setup("normal");
        let result = create_profile_with("scout", Some(fake_binary(&env)));
        assert!(
            matches!(result, HermesProfileLifecycleResult::Ok { ref name } if name == "scout"),
            "{result:?}"
        );
        assert!(env.hermes_home.join("profiles/scout").is_dir());
    }

    #[test]
    fn create_profile_rejects_invalid_name_before_spawn() {
        let _env = setup("normal");
        let result = create_profile_with("Bad!", Some(fake_binary(&_env)));
        assert!(
            matches!(result, HermesProfileLifecycleResult::InvalidName { .. }),
            "{result:?}"
        );
    }

    #[test]
    fn create_profile_rejects_default_at_service_layer() {
        let env = setup("normal");
        let result = create_profile_with("default", Some(fake_binary(&env)));
        assert!(
            matches!(result, HermesProfileLifecycleResult::InvalidName { ref name, .. } if name == "default"),
            "{result:?}"
        );
        assert!(!env.hermes_home.join("profiles/default").exists());
    }

    #[test]
    fn create_profile_duplicate_returns_already_exists() {
        let env = setup("normal");
        fs::create_dir_all(env.hermes_home.join("profiles/scout")).expect("seed");
        let result = create_profile_with("scout", Some(fake_binary(&env)));
        assert!(
            matches!(result, HermesProfileLifecycleResult::AlreadyExists { .. }),
            "{result:?}"
        );
    }

    #[test]
    fn create_profile_missing_binary() {
        let env = setup("normal");
        let result = create_profile_with("scout", None);
        assert!(
            matches!(result, HermesProfileLifecycleResult::BinaryMissing { .. }),
            "{result:?}"
        );
        assert!(!env.hermes_home.join("profiles/scout").exists());
    }

    #[test]
    fn delete_profile_success_removes_directory() {
        let env = setup("normal");
        fs::create_dir_all(env.hermes_home.join("profiles/scout")).expect("seed");
        let result = delete_profile_with("scout", Some(fake_binary(&env)));
        assert!(
            matches!(result, HermesProfileLifecycleResult::Ok { ref name } if name == "scout"),
            "{result:?}"
        );
        assert!(!env.hermes_home.join("profiles/scout").exists());
    }

    #[test]
    fn delete_profile_exit0_but_dir_present_fails() {
        let env = setup("exit0-but-keep-dir");
        fs::create_dir_all(env.hermes_home.join("profiles/scout")).expect("seed");
        let result = delete_profile_with("scout", Some(fake_binary(&env)));
        assert!(
            matches!(result, HermesProfileLifecycleResult::Failed { .. }),
            "spike 0011 trap must fail the operation; got {result:?}"
        );
        assert!(
            env.hermes_home.join("profiles/scout").is_dir(),
            "directory must still be present for this trap case"
        );
    }

    #[test]
    fn delete_profile_missing_is_already_gone() {
        let env = setup("normal");
        let result = delete_profile_with("ghost", Some(fake_binary(&env)));
        assert!(
            matches!(result, HermesProfileLifecycleResult::AlreadyGone { ref name } if name == "ghost"),
            "{result:?}"
        );
    }

    #[test]
    fn delete_profile_rejects_default() {
        let env = setup("normal");
        let result = delete_profile_with("default", Some(fake_binary(&env)));
        assert!(
            matches!(result, HermesProfileLifecycleResult::InvalidName { .. }),
            "{result:?}"
        );
    }

    #[test]
    fn list_profiles_filters_by_name_regex() {
        let env = setup("normal");
        fs::create_dir_all(env.hermes_home.join("profiles/scout")).expect("scout");
        fs::create_dir_all(env.hermes_home.join("profiles/builder-1")).expect("builder");
        fs::create_dir_all(env.hermes_home.join("profiles/Bad Name")).expect("bad");
        fs::write(env.hermes_home.join("profiles/not-a-dir"), b"x").expect("file");
        let names = list_profiles().expect("list");
        assert_eq!(names, vec!["builder-1".to_string(), "scout".to_string()]);
    }

    #[test]
    fn bare_delete_trap_script_exits_zero_without_removing() {
        // Documents the spike 0011 behavior our service must defend against.
        let env = setup("trap-bare-delete");
        fs::create_dir_all(env.hermes_home.join("profiles/scout")).expect("seed");
        let binary = fake_binary(&env);
        let output = Command::new(&binary)
            .args(["profile", "delete", "scout"])
            .env("HERMES_HOME", &env.hermes_home)
            .env("PATH", format!("{}:/bin:/usr/bin", env.bin_dir.display()))
            .stdin(Stdio::null())
            .output()
            .expect("spawn");
        assert!(output.status.success());
        assert!(env.hermes_home.join("profiles/scout").is_dir());
        // Our API always passes -y and verifies absence:
        let result = delete_profile_with("scout", Some(binary));
        assert!(result.is_success(), "{result:?}");
        assert!(!env.hermes_home.join("profiles/scout").exists());
    }
}
