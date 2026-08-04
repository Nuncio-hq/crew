//! Resolve and spawn the GitHub CLI (`gh`) with an augmented PATH.
//!
//! Packaged macOS apps inherit launchd's PATH (`/usr/bin:/bin:/usr/sbin:/sbin`),
//! which does not include Homebrew. Bare `Command::new("gh")` therefore fails
//! in Finder/Dock launches even when `gh` is installed. This helper reuses the
//! app's existing binary discovery (`find_command`) and sets the same augmented
//! PATH used by other CLI probes so `gh`'s own `git` subprocesses resolve too.

use std::path::PathBuf;
use std::sync::Mutex;

use tokio::process::Command;

/// Why a `gh` invocation could not be attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GhUnavailable {
    /// No `gh` on any known path.
    CliMissing,
}

static GH_BINARY: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Resolved absolute path to `gh`, cached across calls.
///
/// Only successful resolutions are cached: a user who installs `gh` while the
/// app is running should not have to restart. Repeated misses stay cheap
/// because the login-shell PATH probe behind `find_command` has its own cache.
async fn gh_binary() -> Option<PathBuf> {
    if let Ok(guard) = GH_BINARY.lock() {
        if let Some(path) = guard.as_ref() {
            return Some(path.clone());
        }
    }
    let resolved = tokio::task::spawn_blocking(|| crate::managed_agents::find_command("gh"))
        .await
        .ok()
        .flatten()?;
    if let Ok(mut guard) = GH_BINARY.lock() {
        *guard = Some(resolved.clone());
    }
    Some(resolved)
}

/// A `gh` command with the resolved binary and an augmented `PATH`, so the
/// `git` subprocesses `gh` spawns resolve too.
pub(crate) async fn gh_command() -> Result<Command, GhUnavailable> {
    let binary = gh_binary().await.ok_or(GhUnavailable::CliMissing)?;
    let mut command = Command::new(&binary);
    if let Some(path) = crate::managed_agents::readiness::cli_probe::augmented_path() {
        command.env("PATH", path);
    }
    Ok(command)
}

#[cfg(test)]
pub(crate) fn clear_gh_binary_cache() {
    if let Ok(mut guard) = GH_BINARY.lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn write_executable(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write binary");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod binary");
        path
    }

    fn block_on_gh_command() -> Result<Command, GhUnavailable> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(gh_command())
    }

    #[cfg(unix)]
    #[test]
    fn resolves_gh_from_path_and_sets_child_path() {
        let _guard = crate::managed_agents::lock_path_mutex();
        let dir = tempfile::tempdir().expect("temp dir");
        let expected = write_executable(dir.path(), "gh", "#!/bin/sh\nexit 0\n");

        let original_path = std::env::var("PATH").unwrap_or_default();
        // Prefer the fixture over any real gh by putting it first.
        std::env::set_var(
            "PATH",
            format!("{}:{}", dir.path().display(), original_path),
        );
        crate::managed_agents::clear_resolve_cache();
        clear_gh_binary_cache();

        let command = block_on_gh_command().expect("gh should resolve");
        let program = command.as_std().get_program().to_owned();
        assert_eq!(PathBuf::from(program), expected);

        let path_env = command.as_std().get_envs().find_map(|(key, value)| {
            if key == "PATH" {
                value.map(|v| v.to_string_lossy().into_owned())
            } else {
                None
            }
        });
        assert!(
            path_env.as_ref().is_some_and(|p| !p.is_empty()),
            "augmented PATH must be set on the child so gh→git resolves"
        );

        std::env::set_var("PATH", original_path);
        crate::managed_agents::clear_resolve_cache();
        clear_gh_binary_cache();
    }

    #[cfg(unix)]
    #[test]
    fn missing_gh_returns_cli_missing() {
        let _guard = crate::managed_agents::lock_path_mutex();
        let dir = tempfile::tempdir().expect("temp dir");
        let original_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", dir.path());
        crate::managed_agents::clear_resolve_cache();
        clear_gh_binary_cache();

        // Login-shell / Homebrew probes may still find a real gh on developer
        // machines — skip when that happens (still a valid resolution path).
        if crate::managed_agents::find_command("gh").is_some() {
            std::env::set_var("PATH", original_path);
            crate::managed_agents::clear_resolve_cache();
            clear_gh_binary_cache();
            return;
        }

        let err = block_on_gh_command().expect_err("gh must be missing");
        assert_eq!(err, GhUnavailable::CliMissing);

        std::env::set_var("PATH", original_path);
        crate::managed_agents::clear_resolve_cache();
        clear_gh_binary_cache();
    }
}
