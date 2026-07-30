//! Blocking ACP runtime install: CLI, managed Node, adapter, arch repair.

use crate::managed_agents::{is_npm_global_install, InstallRuntimeResult, InstallStepResult};

use super::install_exec::run_install_command_with_retry;
use super::managed_adapter_install::{
    ensure_managed_adapter_arch_ready_blocking, managed_adapter_stderr_hint, managed_npm_command,
    reclaim_legacy_unscoped_npm_prefix, sibling_adapter_reinstall_failure_step,
    sibling_runtimes_to_reinstall_after_purge,
};
use super::managed_node::{ensure_managed_node_runtime_blocking, managed_node_runtime_supported};
use super::post_install_verification;

fn active_installs() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static ACTIVE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Returns the adapter install commands that `install_acp_runtime_blocking` would
/// run for `runtime_id` given a resolved adapter binary at `adapter_path` (or
/// `None` if none was found).
///
/// Returns `None` when no install is needed (adapter is present and current).
/// Returns `Some(cmds)` when the adapter is missing or (for codex) below its
/// minimum supported version.
///
/// For the codex **outdated** case the returned sequence is a two-step
/// reinstall: first uninstall the old `@zed-industries/codex-acp` package
/// (idempotent — exit 0 when absent), then install the new
/// `@agentclientprotocol/codex-acp`.  This is required because both packages
/// install a global binary named `codex-acp`, and npm ≥7 refuses to overwrite
/// a bin file owned by a different package with `EEXIST`.
///
/// For the **missing** case the catalog's `adapter_install_commands` are used
/// as-is (no prior package to remove).
///
/// This is a pure planning function: it never spawns a process.  Tests use it to
/// assert the correct install command is selected without touching real npm.
pub(crate) fn plan_adapter_install<'c>(
    runtime_id: &str,
    adapter_path: Option<&std::path::Path>,
    adapter_install_commands: &'c [&'c str],
    adapter_probe_path: Option<&str>,
) -> Option<Vec<&'c str>> {
    match adapter_path {
        // Adapter present and current — no install needed.
        Some(_) if runtime_id != "codex" => None,
        Some(path)
            if !crate::managed_agents::codex_adapter_is_outdated_with_path(
                path,
                adapter_probe_path,
            ) =>
        {
            None
        }
        // Codex adapter is outdated: uninstall the old package first so npm
        // doesn't hit EEXIST on the shared `codex-acp` bin-link, then install.
        Some(_) => Some(vec![
            "npm uninstall -g @zed-industries/codex-acp",
            "npm install -g @agentclientprotocol/codex-acp",
        ]),
        // Adapter missing: use the catalog's install commands directly.
        None => Some(adapter_install_commands.to_vec()),
    }
}

fn failed_install(steps: Vec<InstallStepResult>) -> InstallRuntimeResult {
    InstallRuntimeResult {
        success: false,
        steps,
        restarted_count: 0,
        failed_restart_count: 0,
    }
}

fn run_adapter_commands(
    cmds: &[&str],
    use_managed_npm: bool,
    step_name: &str,
    steps: &mut Vec<InstallStepResult>,
) -> Result<(), ()> {
    for cmd in cmds {
        let planned = match if use_managed_npm {
            managed_npm_command(cmd)
        } else {
            Ok(None)
        } {
            Ok(Some(command)) => command,
            Ok(None) => cmd.to_string(),
            Err(step) => {
                steps.push(*step);
                return Err(());
            }
        };

        let mut result = run_install_command_with_retry(step_name, &planned);
        if !result.success && result.hint.is_none() && is_npm_global_install(cmd) {
            result.hint = managed_adapter_stderr_hint(&result.stderr, cmd);
        }
        let success = result.success;
        steps.push(result);
        if !success {
            return Err(());
        }
    }
    Ok(())
}

/// Reinstall adapters removed by a prefix purge, excluding `requested_runtime_id`.
///
/// Failures are appended as visible `adapter-repair` steps but do **not** fail the
/// requested runtime's install — the user asked to install one runtime, and a
/// sibling network blip must not report that install as failed. Silent loss is
/// still forbidden: every purged sibling gets a success or a surfaced error.
fn reinstall_purged_sibling_adapters(
    requested_runtime_id: &str,
    purged_runtime_ids: &[&str],
    steps: &mut Vec<InstallStepResult>,
) {
    let siblings =
        sibling_runtimes_to_reinstall_after_purge(purged_runtime_ids, requested_runtime_id);
    for sibling_id in siblings {
        let Some(sibling) = crate::managed_agents::known_acp_runtime_exact(sibling_id) else {
            continue;
        };
        if sibling.adapter_install_commands.is_empty() {
            continue;
        }
        let use_managed_npm = sibling
            .adapter_install_commands
            .iter()
            .any(|cmd| is_npm_global_install(cmd))
            && managed_node_runtime_supported();
        for cmd in sibling.adapter_install_commands {
            let planned = match if use_managed_npm {
                managed_npm_command(cmd)
            } else {
                Ok(None)
            } {
                Ok(Some(command)) => command,
                Ok(None) => cmd.to_string(),
                Err(step) => {
                    let mut failure = *step;
                    failure.step = "adapter-repair".to_string();
                    failure.hint = Some(format!(
                        "{} adapter was removed during architecture repair and could not be reinstalled; open Settings → Agent runtimes and click Install for {}.",
                        sibling.label, sibling.label
                    ));
                    steps.push(failure);
                    break;
                }
            };

            let result = run_install_command_with_retry("adapter-repair", &planned);
            if !result.success {
                steps.push(sibling_adapter_reinstall_failure_step(
                    sibling_id,
                    &planned,
                    result.stderr,
                ));
                break;
            }
            steps.push(result);
        }
    }
}

/// Err(_) = infrastructure failure (panic, concurrency guard).
/// Ok({success: false}) = an install step failed (stderr captured in steps).
pub(super) fn install_acp_runtime_blocking(
    runtime_id: &str,
) -> Result<InstallRuntimeResult, String> {
    // Re-fetch the login-shell PATH so a Node.js installation that happened
    // after app launch (or after a previous failed install) is visible to this
    // run and to the subsequent discover_acp_providers call.
    crate::managed_agents::refresh_login_shell_path();
    // Clear the resolve cache so newly-installed binaries are found.
    crate::managed_agents::clear_resolve_cache();

    // Prevent concurrent installs for the same runtime.
    {
        let mut set = active_installs()
            .lock()
            .map_err(|_| "install lock poisoned".to_string())?;
        if !set.insert(runtime_id.to_string()) {
            return Err(format!(
                "an install is already in progress for {runtime_id}"
            ));
        }
    }

    struct Guard(String);
    impl Drop for Guard {
        fn drop(&mut self) {
            if let Ok(mut set) = active_installs().lock() {
                set.remove(&self.0);
            }
        }
    }
    let _guard = Guard(runtime_id.to_string());

    let runtime = crate::managed_agents::known_acp_runtime_exact(runtime_id)
        .ok_or_else(|| format!("unknown runtime: {runtime_id}"))?;

    let mut steps = Vec::new();

    // Phase 1: Install CLI if missing and commands are available.
    // Today every entry in `cli_install_commands` is a curl-pipe; npm-backed
    // adapter installs live in Phase 2 below where they are rewritten to a
    // Buzz-private prefix before execution.
    if let Some(cli) = runtime.underlying_cli {
        if crate::managed_agents::resolve_command(cli).is_none() {
            for cmd in runtime.cli_install_commands_for_os() {
                let result = run_install_command_with_retry("cli", cmd);
                let success = result.success;
                steps.push(result);
                if !success {
                    return Ok(failed_install(steps));
                }
            }
        }
    }

    // Phase 2: Install adapter if missing (or outdated) and commands are available.
    // For the codex runtime, "found" is not enough — the resolved binary must also
    // pass the 1.x version gate. An outdated 0.16.x adapter must be overwritten by
    // the new npm install so the CODEX_CONFIG spawn contract works correctly.
    let catalog_uses_managed_npm = runtime
        .adapter_install_commands
        .iter()
        .any(|cmd| is_npm_global_install(cmd))
        && managed_node_runtime_supported();

    // Arch validation must run even when plan_adapter_install returns None
    // (adapter present + current). A current shim with wrong-arch optional
    // packages is exactly the permanently-broken state from issue #4.
    // Scope: purge removes the whole platform prefix (global), so we capture
    // which adapters were present and reinstall every one — not only the
    // runtime the user clicked.
    let mut force_adapter_reinstall = false;
    let mut purged_runtime_ids: Vec<&'static str> = Vec::new();
    if catalog_uses_managed_npm {
        match ensure_managed_adapter_arch_ready_blocking() {
            Ok(present) if !present.is_empty() => {
                force_adapter_reinstall = true;
                purged_runtime_ids = present;
                crate::managed_agents::clear_resolve_cache();
            }
            Ok(_) => {}
            Err(step) => {
                steps.push(*step);
                return Ok(failed_install(steps));
            }
        }
    }

    let adapter_path = runtime
        .commands
        .iter()
        .find_map(|cmd| crate::managed_agents::resolve_command(cmd));
    let adapter_probe_path = crate::managed_agents::readiness::cli_probe::augmented_path();
    let planned_cmds = if force_adapter_reinstall {
        Some(runtime.adapter_install_commands.to_vec())
    } else {
        plan_adapter_install(
            runtime_id,
            adapter_path.as_deref(),
            runtime.adapter_install_commands,
            adapter_probe_path.as_deref(),
        )
    };
    if let Some(cmds) = planned_cmds {
        let use_managed_npm =
            cmds.iter().any(|cmd| is_npm_global_install(cmd)) && managed_node_runtime_supported();
        if use_managed_npm {
            if let Err(step) = ensure_managed_node_runtime_blocking() {
                steps.push(*step);
                return Ok(failed_install(steps));
            }
        }

        if run_adapter_commands(&cmds, use_managed_npm, "adapter", &mut steps).is_err() {
            return Ok(failed_install(steps));
        }

        if use_managed_npm {
            reclaim_legacy_unscoped_npm_prefix();
        }
    }

    // After a prefix purge, rebuild every other adapter that was removed.
    // Sibling failures are visible but must not flip the requested runtime to
    // success:false (post_install_verification still gates the primary).
    if force_adapter_reinstall {
        reinstall_purged_sibling_adapters(runtime_id, &purged_runtime_ids, &mut steps);
    }

    let primary_ok_before_verify = steps
        .iter()
        .filter(|step| step.step != "adapter-repair")
        .all(|step| step.success);

    post_install_verification::run(runtime_id, &mut steps);

    let primary_ok = primary_ok_before_verify
        && steps
            .iter()
            .filter(|step| step.step != "adapter-repair")
            .all(|step| step.success);

    Ok(InstallRuntimeResult {
        success: primary_ok,
        steps,
        restarted_count: 0,
        failed_restart_count: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// plan_adapter_install is the pure install-plan seam used by
    /// install_acp_runtime_blocking. These tests verify:
    ///   - A 0.x binary (AdapterOutdated) → uninstall-then-install sequence returned
    ///   - A current 1.x binary (Available) → None (no reinstall)
    ///   - A 1.x binary below the floor → install plan returned
    ///   - Missing binary (None path) → catalog install commands returned
    #[cfg(unix)]
    #[test]
    fn test_plan_adapter_install_selects_npm_command_for_outdated_0x_codex_binary() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("codex-acp");
        // Simulate old 0.16.x: --version exits non-zero (unrecognised flag)
        std::fs::write(&bin, "#!/bin/sh\nexit 1\n").expect("write script");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod script");

        let install_cmds = &["npm install -g @agentclientprotocol/codex-acp"];
        let plan = plan_adapter_install("codex", Some(&bin), install_cmds, Some("/usr/bin:/bin"));

        assert!(
            plan.is_some(),
            "0.x codex adapter must trigger install plan"
        );
        let cmds = plan.unwrap();
        // Outdated arm: must uninstall the old package first, then install new.
        assert_eq!(
            cmds,
            vec![
                "npm uninstall -g @zed-industries/codex-acp",
                "npm install -g @agentclientprotocol/codex-acp",
            ],
            "outdated codex adapter must produce uninstall-then-install sequence; got {cmds:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_plan_adapter_install_returns_none_for_current_1x_codex_binary() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("codex-acp");
        // Simulate the minimum supported adapter version.
        std::fs::write(
            &bin,
            "#!/bin/sh\necho '@agentclientprotocol/codex-acp 1.1.7'\nexit 0\n",
        )
        .expect("write script");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod script");

        let install_cmds = &["npm install -g @agentclientprotocol/codex-acp"];
        let plan = plan_adapter_install("codex", Some(&bin), install_cmds, Some("/usr/bin:/bin"));

        assert!(
            plan.is_none(),
            "current codex adapter must not trigger install plan (no reinstall needed)"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_plan_adapter_install_updates_older_1x_codex_binary() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("codex-acp");
        // A 1.x adapter below MIN_CODEX_ACP_VERSION must still be reinstalled.
        std::fs::write(
            &bin,
            "#!/bin/sh\necho '@agentclientprotocol/codex-acp 1.1.5'\nexit 0\n",
        )
        .expect("write script");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod script");

        let install_cmds = &["npm install -g @agentclientprotocol/codex-acp"];
        let plan = plan_adapter_install("codex", Some(&bin), install_cmds, Some("/usr/bin:/bin"));

        assert!(
            plan.is_some(),
            "older 1.x codex adapter must trigger update plan"
        );
    }

    #[test]
    fn test_plan_adapter_install_returns_catalog_cmds_when_no_adapter_path() {
        let install_cmds = &["npm install -g @agentclientprotocol/codex-acp"];
        let plan = plan_adapter_install("codex", None, install_cmds, None);
        assert!(plan.is_some(), "missing adapter must trigger install plan");
        // Missing arm: use the catalog's install commands directly (no prior
        // package to uninstall — fresh install, not a reinstall).
        assert_eq!(
            plan.unwrap(),
            vec!["npm install -g @agentclientprotocol/codex-acp"],
            "missing codex adapter must use catalog install commands only"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_plan_adapter_install_non_codex_runtime_never_reinstalls() {
        use std::os::unix::fs::PermissionsExt;

        // For non-codex runtimes, any resolved binary means no install needed.
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("goose-acp");
        std::fs::write(&bin, "#!/bin/sh\nexit 1\n").expect("write script");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod script");

        let install_cmds = &["npm install -g @block/goose-acp"];
        let plan = plan_adapter_install("goose", Some(&bin), install_cmds, None);
        assert!(
            plan.is_none(),
            "non-codex runtime with resolved binary must not trigger reinstall"
        );
    }

    #[test]
    fn sibling_reinstall_list_excludes_requested_and_keeps_purged() {
        assert_eq!(
            sibling_runtimes_to_reinstall_after_purge(&["codex", "claude"], "codex"),
            vec!["claude"]
        );
        assert_eq!(
            sibling_runtimes_to_reinstall_after_purge(&["codex"], "codex"),
            Vec::<&str>::new()
        );
    }
}
