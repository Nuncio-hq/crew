//! Managed npm adapter install helpers: arch validation, purge, rewrite, hints.

use crate::managed_agents::{is_npm_global_install, InstallStepResult};

/// Optional platform packages that must match `managed_platform_segment()`.
struct AdapterPlatformFamily {
    /// Catalog runtime id (`codex`, `claude`) owning this family.
    runtime_id: &'static str,
    /// Packages under `@agentclientprotocol/` that belong to this family.
    adapter_names: &'static [&'static str],
    /// Optional native package prefix, e.g. `@openai/codex-`.
    optional_prefix: &'static str,
}

const ADAPTER_PLATFORM_FAMILIES: &[AdapterPlatformFamily] = &[
    AdapterPlatformFamily {
        runtime_id: "codex",
        adapter_names: &["codex-acp"],
        optional_prefix: "@openai/codex-",
    },
    AdapterPlatformFamily {
        runtime_id: "claude",
        adapter_names: &["claude-agent-acp", "claude-code-acp"],
        optional_prefix: "@anthropic-ai/claude-agent-sdk-",
    },
];

/// Result of inspecting a managed npm prefix for adapter arch correctness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdapterArchCheck {
    Matches,
    Mismatch,
    NoAdaptersInstalled,
}

fn npm_global_node_modules(prefix: &std::path::Path) -> std::path::PathBuf {
    #[cfg(windows)]
    {
        prefix.join("node_modules")
    }
    #[cfg(not(windows))]
    {
        prefix.join("lib").join("node_modules")
    }
}

fn list_optionals_for_prefix(modules: &std::path::Path, optional_prefix: &str) -> Vec<String> {
    let Some((scope, name_prefix)) = optional_prefix.split_once('/') else {
        return Vec::new();
    };
    let scope_dir = modules.join(scope);
    let Ok(entries) = std::fs::read_dir(&scope_dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(name_prefix) {
            found.push(format!("{scope}/{name}"));
        }
    }
    found
}

fn family_adapter_installed(modules: &std::path::Path, family: &AdapterPlatformFamily) -> bool {
    let scope = modules.join("@agentclientprotocol");
    family
        .adapter_names
        .iter()
        .any(|name| scope.join(name).is_dir())
}

/// Runtime ids whose adapter (or stale optional) is present under `prefix`.
///
/// Used so a prefix purge can reinstall every adapter it removed — not only the
/// runtime the user clicked Install for.
pub(crate) fn managed_npm_runtime_ids_present_in_prefix(
    prefix: &std::path::Path,
) -> Vec<&'static str> {
    let modules = npm_global_node_modules(prefix);
    if !modules.is_dir() {
        return Vec::new();
    }
    ADAPTER_PLATFORM_FAMILIES
        .iter()
        .filter(|family| {
            family_adapter_installed(&modules, family)
                || !list_optionals_for_prefix(&modules, family.optional_prefix).is_empty()
        })
        .map(|family| family.runtime_id)
        .collect()
}

/// Pure directory-fixture check: does `prefix` hold adapters for `expected_platform`?
///
/// Validates each adapter family independently. Any installed family missing its
/// expected optional package, or carrying a wrong-platform optional, is a
/// `Mismatch` — a correct Codex tree cannot mask a wrong Claude tree.
pub(crate) fn managed_adapter_arch_matches(
    prefix: &std::path::Path,
    expected_platform: &str,
) -> AdapterArchCheck {
    let modules = npm_global_node_modules(prefix);
    if !modules.is_dir() {
        return AdapterArchCheck::NoAdaptersInstalled;
    }

    let mut saw_family = false;
    let mut mismatch = false;

    for family in ADAPTER_PLATFORM_FAMILIES {
        let adapter_installed = family_adapter_installed(&modules, family);
        let optionals = list_optionals_for_prefix(&modules, family.optional_prefix);
        if !adapter_installed && optionals.is_empty() {
            continue;
        }
        saw_family = true;

        let expected = format!("{}{expected_platform}", family.optional_prefix);
        let has_expected = optionals.iter().any(|p| p == &expected);
        let has_wrong = optionals.iter().any(|p| p != &expected);

        if adapter_installed {
            if !has_expected || has_wrong {
                mismatch = true;
            }
        } else if has_wrong || !has_expected {
            // Orphan/stale optionals without the adapter package — repair.
            mismatch = true;
        }
    }

    if !saw_family {
        AdapterArchCheck::NoAdaptersInstalled
    } else if mismatch {
        AdapterArchCheck::Mismatch
    } else {
        AdapterArchCheck::Matches
    }
}

/// Remove a managed npm prefix only when the purge guard accepts the path.
pub(crate) fn purge_managed_npm_prefix(path: &std::path::Path) -> Result<(), String> {
    let Some(root) = crate::managed_agents::managed_node_tools_root() else {
        return Err("failed to resolve managed node-tools root for purge guard".to_string());
    };
    purge_managed_npm_prefix_with_root(path, &root)
}

/// Pure purge helper: guard against `root`, then `remove_dir_all(path)`.
pub(crate) fn purge_managed_npm_prefix_with_root(
    path: &std::path::Path,
    root: &std::path::Path,
) -> Result<(), String> {
    if !crate::managed_agents::is_safe_to_purge_node_tools_path_with_root(path, root) {
        return Err(format!(
            "refusing to purge path outside managed node-tools root: {}",
            path.display()
        ));
    }
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(|e| {
            format!(
                "failed to remove managed npm prefix '{}': {e}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn purge_failed_step(path: &std::path::Path, error: String) -> InstallStepResult {
    InstallStepResult {
        step: "adapter".to_string(),
        command: "purge mismatched managed npm prefix".to_string(),
        success: false,
        stdout: String::new(),
        stderr: error,
        exit_code: None,
        hint: Some(format!(
            "Buzz could not remove the mismatched Node tools directory at '{}'. Delete it manually, then open Settings → Agent runtimes and click Install again.",
            path.display()
        )),
    }
}

/// On arch mismatch, purge the scoped prefix so the install path rebuilds it.
///
/// Returns `Ok(ids)` where `ids` are the catalog runtime ids that were present
/// under the prefix before purge (empty when no purge happened). Callers must
/// reinstall every returned id — or surface a visible failure for each — so no
/// adapter disappears silently.
pub(super) fn ensure_managed_adapter_arch_ready_blocking(
) -> Result<Vec<&'static str>, Box<InstallStepResult>> {
    let Some(expected) = crate::managed_agents::managed_platform_segment() else {
        return Ok(Vec::new());
    };
    let Some(prefix) = crate::managed_agents::buzz_managed_npm_prefix() else {
        return Ok(Vec::new());
    };

    match managed_adapter_arch_matches(&prefix, expected) {
        AdapterArchCheck::Matches | AdapterArchCheck::NoAdaptersInstalled => Ok(Vec::new()),
        AdapterArchCheck::Mismatch => {
            let present = managed_npm_runtime_ids_present_in_prefix(&prefix);
            if let Err(error) = purge_managed_npm_prefix(&prefix) {
                return Err(Box::new(purge_failed_step(&prefix, error)));
            }
            Ok(present)
        }
    }
}

/// Sibling runtime ids that must be reinstalled after a prefix purge, excluding
/// the runtime the user explicitly asked to install.
pub(crate) fn sibling_runtimes_to_reinstall_after_purge(
    purged_runtime_ids: &[&str],
    requested_runtime_id: &str,
) -> Vec<&'static str> {
    ADAPTER_PLATFORM_FAMILIES
        .iter()
        .map(|family| family.runtime_id)
        .filter(|id| {
            *id != requested_runtime_id && purged_runtime_ids.iter().any(|present| present == id)
        })
        .collect()
}

pub(super) fn sibling_adapter_reinstall_failure_step(
    sibling_runtime_id: &str,
    command: &str,
    stderr: String,
) -> InstallStepResult {
    let label = crate::managed_agents::known_acp_runtime_exact(sibling_runtime_id)
        .map(|runtime| runtime.label)
        .unwrap_or(sibling_runtime_id);
    InstallStepResult {
        step: "adapter-repair".to_string(),
        command: command.to_string(),
        success: false,
        stdout: String::new(),
        stderr,
        exit_code: None,
        hint: Some(format!(
            "{label} adapter was removed during architecture repair and could not be reinstalled; open Settings → Agent runtimes and click Install for {label}."
        )),
    }
}

/// Best-effort reclaim of the legacy unscoped `node-tools/` tree after a
/// successful scoped install. Failure is logged, not fatal.
pub(super) fn reclaim_legacy_unscoped_npm_prefix() {
    let Some(legacy) = crate::managed_agents::buzz_legacy_unscoped_npm_prefix() else {
        return;
    };
    let modules = npm_global_node_modules(&legacy);
    if !modules.is_dir() {
        return;
    }
    reclaim_legacy_unscoped_contents(&legacy);
}

fn reclaim_legacy_unscoped_contents(legacy_root: &std::path::Path) {
    for name in ["bin", "lib", "cache", "corepack", "node_modules"] {
        let path = legacy_root.join(name);
        if !path.exists() {
            continue;
        }
        if !crate::managed_agents::is_safe_to_purge_node_tools_path(&path) {
            eprintln!(
                "buzz-desktop: refusing legacy reclaim outside node-tools: {}",
                path.display()
            );
            continue;
        }
        if let Err(error) = std::fs::remove_dir_all(&path) {
            eprintln!(
                "buzz-desktop: legacy node-tools reclaim failed for {}: {error}",
                path.display()
            );
        }
    }
}

fn managed_npm_prefix_hint() -> String {
    "Buzz could not create its private Node tools directory. Check app-data directory permissions, restart Buzz, then click Install again.".to_string()
}

/// Map Rust `std::env::consts::{OS,ARCH}` to npm `--os` / `--cpu` values
/// (`process.platform` / `process.arch`).
pub(crate) fn npm_install_platform_flags_for(
    os: &str,
    arch: &str,
) -> Option<(&'static str, &'static str)> {
    let cpu = match arch {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => return None,
    };
    let os_flag = match os {
        "macos" => "darwin",
        "linux" => "linux",
        "windows" => "win32",
        _ => return None,
    };
    Some((cpu, os_flag))
}

fn npm_install_platform_flags() -> Option<(&'static str, &'static str)> {
    npm_install_platform_flags_for(std::env::consts::OS, std::env::consts::ARCH)
}

pub(super) fn managed_npm_command(command: &str) -> Result<Option<String>, Box<InstallStepResult>> {
    if !is_npm_global_install(command) {
        return Ok(None);
    }

    let Some(prefix) = crate::managed_agents::buzz_managed_npm_prefix() else {
        return Err(Box::new(InstallStepResult {
            step: "adapter".to_string(),
            command: command.to_string(),
            success: false,
            stdout: String::new(),
            stderr: "failed to resolve Buzz app-data directory for private npm prefix".to_string(),
            exit_code: None,
            hint: Some(managed_npm_prefix_hint()),
        }));
    };
    if let Err(error) = std::fs::create_dir_all(&prefix) {
        return Err(Box::new(InstallStepResult {
            step: "adapter".to_string(),
            command: command.to_string(),
            success: false,
            stdout: String::new(),
            stderr: format!(
                "failed to create Buzz private npm prefix '{}': {error}",
                prefix.display()
            ),
            exit_code: None,
            hint: Some(managed_npm_prefix_hint()),
        }));
    }

    let prefix_arg = shell_quote(&prefix);
    Ok(Some(rewrite_npm_global_install(command, &prefix_arg)))
}

fn rewrite_npm_global_install(command: &str, quoted_prefix: &str) -> String {
    rewrite_npm_global_install_with_platform(command, quoted_prefix, npm_install_platform_flags())
}

fn rewrite_npm_global_install_with_platform(
    command: &str,
    quoted_prefix: &str,
    platform: Option<(&str, &str)>,
) -> String {
    let trimmed = command.trim_start();
    let platform_flags = platform
        .map(|(cpu, os)| format!(" --cpu={cpu} --os={os}"))
        .unwrap_or_default();
    if let Some(rest) = trimmed.strip_prefix("npm install -g ") {
        format!("npm install --global --prefix {quoted_prefix}{platform_flags} {rest}")
    } else if let Some(rest) = trimmed.strip_prefix("npm i -g ") {
        format!("npm i --global --prefix {quoted_prefix}{platform_flags} {rest}")
    } else if let Some(rest) = trimmed.strip_prefix("npm uninstall -g ") {
        format!("npm uninstall --global --prefix {quoted_prefix} {rest}")
    } else {
        trimmed.to_string()
    }
}

fn shell_quote(path: &std::path::Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Inspect `stderr` for known npm EACCES patterns and return actionable
/// guidance if matched, or `None` when the error is unrelated.
pub(super) fn npm_eacces_hint(stderr: &str, _command: &str) -> Option<String> {
    if stderr.contains("EACCES: permission denied") || stderr.contains("npm error EACCES") {
        Some(
            "npm could not write to Buzz's private Node tools directory. Check app-data directory permissions, restart Buzz, then click Install again."
                .to_string(),
        )
    } else {
        None
    }
}

/// Detect upstream Codex/Claude "Missing optional dependency …" dead-end advice.
pub(super) fn missing_optional_dependency_hint(stderr: &str) -> Option<String> {
    let lower = stderr.to_ascii_lowercase();
    let matches = lower.contains("missing optional dependency")
        && (lower.contains("@openai/codex-") || lower.contains("@anthropic-ai/claude-agent-sdk-"));
    if !matches {
        return None;
    }
    Some(
        "A managed ACP adapter is missing its native package for this architecture. Open Settings → Agent runtimes and click Install again so Buzz can repair its private Node tools directory.".to_string(),
    )
}

/// Prefer a specific adapter-stderr hint when present.
pub(super) fn managed_adapter_stderr_hint(stderr: &str, command: &str) -> Option<String> {
    npm_eacces_hint(stderr, command).or_else(|| missing_optional_dependency_hint(stderr))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plant_adapter(modules: &std::path::Path, name: &str) {
        std::fs::create_dir_all(modules.join("@agentclientprotocol").join(name)).unwrap();
    }

    fn plant_optional(modules: &std::path::Path, full_name: &str) {
        let (scope, pkg) = full_name.split_once('/').unwrap();
        std::fs::create_dir_all(modules.join(scope).join(pkg)).unwrap();
    }

    #[test]
    fn test_npm_eacces_hint_guidance_mentions_buzz_private_dir() {
        let hint = npm_eacces_hint("EACCES: permission denied", "npm install -g foo").unwrap();
        assert!(
            hint.contains("Buzz's private Node tools directory"),
            "hint: {hint}"
        );
    }

    #[test]
    fn test_rewrite_npm_install_uses_private_prefix_and_platform_flags() {
        assert_eq!(
            rewrite_npm_global_install_with_platform(
                "npm install -g @agentclientprotocol/codex-acp",
                "'/tmp/Buzz Node'",
                Some(("arm64", "darwin")),
            ),
            "npm install --global --prefix '/tmp/Buzz Node' --cpu=arm64 --os=darwin @agentclientprotocol/codex-acp"
        );
    }

    #[test]
    fn test_rewrite_npm_i_uses_private_prefix_and_platform_flags() {
        assert_eq!(
            rewrite_npm_global_install_with_platform(
                "npm i -g some-package",
                "'/tmp/buzz'",
                Some(("x64", "linux")),
            ),
            "npm i --global --prefix '/tmp/buzz' --cpu=x64 --os=linux some-package"
        );
    }

    #[test]
    fn test_rewrite_npm_uninstall_omits_cpu_os() {
        assert_eq!(
            rewrite_npm_global_install_with_platform(
                "npm uninstall -g @zed-industries/codex-acp",
                "'/tmp/buzz'",
                Some(("arm64", "darwin")),
            ),
            "npm uninstall --global --prefix '/tmp/buzz' @zed-industries/codex-acp"
        );
    }

    #[test]
    fn test_rewrite_ignores_non_global_command() {
        assert_eq!(
            rewrite_npm_global_install_with_platform(
                "npm install foo",
                "'/tmp/buzz'",
                Some(("arm64", "darwin")),
            ),
            "npm install foo"
        );
    }

    #[test]
    fn test_npm_install_platform_flags_mapping() {
        assert_eq!(
            npm_install_platform_flags_for("macos", "aarch64"),
            Some(("arm64", "darwin"))
        );
        assert_eq!(
            npm_install_platform_flags_for("macos", "x86_64"),
            Some(("x64", "darwin"))
        );
        assert_eq!(
            npm_install_platform_flags_for("linux", "x86_64"),
            Some(("x64", "linux"))
        );
        assert_eq!(
            npm_install_platform_flags_for("linux", "aarch64"),
            Some(("arm64", "linux"))
        );
        assert_eq!(
            npm_install_platform_flags_for("windows", "x86_64"),
            Some(("x64", "win32"))
        );
        assert_eq!(
            npm_install_platform_flags_for("windows", "aarch64"),
            Some(("arm64", "win32"))
        );
        assert_eq!(npm_install_platform_flags_for("freebsd", "x86_64"), None);
    }

    #[test]
    fn test_shell_quote_escapes_single_quotes() {
        assert_eq!(
            shell_quote(std::path::Path::new("/tmp/Buzz's Node")),
            "'/tmp/Buzz'\\''s Node'"
        );
    }

    #[test]
    fn adapter_arch_mismatch_on_wrong_platform_package() {
        let temp = tempfile::tempdir().unwrap();
        let modules = npm_global_node_modules(temp.path());
        plant_adapter(&modules, "codex-acp");
        plant_optional(&modules, "@openai/codex-darwin-x64");
        assert_eq!(
            managed_adapter_arch_matches(temp.path(), "darwin-arm64"),
            AdapterArchCheck::Mismatch
        );
    }

    #[test]
    fn adapter_arch_matches_expected_platform_package() {
        let temp = tempfile::tempdir().unwrap();
        let modules = npm_global_node_modules(temp.path());
        plant_adapter(&modules, "codex-acp");
        plant_optional(&modules, "@openai/codex-darwin-arm64");
        assert_eq!(
            managed_adapter_arch_matches(temp.path(), "darwin-arm64"),
            AdapterArchCheck::Matches
        );
    }

    #[test]
    fn adapter_arch_no_adapters_when_empty() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            managed_adapter_arch_matches(temp.path(), "darwin-arm64"),
            AdapterArchCheck::NoAdaptersInstalled
        );
        std::fs::create_dir_all(npm_global_node_modules(temp.path())).unwrap();
        assert_eq!(
            managed_adapter_arch_matches(temp.path(), "darwin-arm64"),
            AdapterArchCheck::NoAdaptersInstalled
        );
    }

    #[test]
    fn adapter_arch_mixed_families_mismatch_when_one_wrong() {
        let temp = tempfile::tempdir().unwrap();
        let modules = npm_global_node_modules(temp.path());
        plant_adapter(&modules, "codex-acp");
        plant_optional(&modules, "@openai/codex-darwin-arm64");
        plant_adapter(&modules, "claude-agent-acp");
        plant_optional(&modules, "@anthropic-ai/claude-agent-sdk-darwin-x64");
        assert_eq!(
            managed_adapter_arch_matches(temp.path(), "darwin-arm64"),
            AdapterArchCheck::Mismatch
        );
        assert_eq!(
            managed_npm_runtime_ids_present_in_prefix(temp.path()),
            vec!["codex", "claude"]
        );
        assert_eq!(
            sibling_runtimes_to_reinstall_after_purge(&["codex", "claude"], "codex"),
            vec!["claude"]
        );
    }

    #[test]
    fn adapter_arch_present_adapter_missing_optional_is_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let modules = npm_global_node_modules(temp.path());
        plant_adapter(&modules, "codex-acp");
        assert_eq!(
            managed_adapter_arch_matches(temp.path(), "darwin-arm64"),
            AdapterArchCheck::Mismatch
        );
    }

    #[test]
    fn adapter_arch_stale_wrong_optional_without_adapter_is_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let modules = npm_global_node_modules(temp.path());
        plant_optional(&modules, "@openai/codex-darwin-x64");
        assert_eq!(
            managed_adapter_arch_matches(temp.path(), "darwin-arm64"),
            AdapterArchCheck::Mismatch
        );
    }

    #[test]
    fn purge_removes_scoped_prefix_and_leaves_runtimes() {
        let temp = tempfile::tempdir().unwrap();
        let product = temp.path().join("Buzz");
        let root = product.join("node-tools");
        let platform_dir = root.join("darwin-arm64");
        let runtimes_sibling = product.join("runtimes").join("node").join("keep-me");
        std::fs::create_dir_all(&platform_dir).unwrap();
        std::fs::create_dir_all(&runtimes_sibling).unwrap();
        std::fs::write(platform_dir.join("marker"), "x").unwrap();

        purge_managed_npm_prefix_with_root(&platform_dir, &root).expect("purge");
        assert!(!platform_dir.exists());
        assert!(runtimes_sibling.exists());
    }

    #[test]
    fn purge_refuses_path_outside_node_tools() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("node-tools");
        std::fs::create_dir_all(&root).unwrap();
        let outside = temp.path().join("evil");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep"), "x").unwrap();
        let err = purge_managed_npm_prefix_with_root(&outside, &root).unwrap_err();
        assert!(err.contains("refusing to purge"), "{err}");
        assert!(outside.join("keep").exists());
    }

    #[test]
    fn purge_refuses_node_tools_root_itself() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("node-tools");
        std::fs::create_dir_all(&root).unwrap();
        let err = purge_managed_npm_prefix_with_root(&root, &root).unwrap_err();
        assert!(err.contains("refusing to purge"), "{err}");
        assert!(root.exists());
    }

    #[test]
    fn purge_refuses_parent_dir_escape() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("node-tools");
        std::fs::create_dir_all(temp.path().join("runtimes")).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        let escape = root.join("..").join("runtimes");
        let err = purge_managed_npm_prefix_with_root(&escape, &root).unwrap_err();
        assert!(err.contains("refusing to purge"), "{err}");
        assert!(temp.path().join("runtimes").exists());
    }

    #[test]
    fn missing_optional_dependency_hint_matches_codex_and_claude() {
        let codex = "Error: Missing optional dependency @openai/codex-darwin-arm64.\nReinstall Codex: npm install -g @openai/codex@latest";
        let hint = missing_optional_dependency_hint(codex).expect("codex hint");
        assert!(hint.contains("Settings → Agent runtimes"), "{hint}");
        assert!(!hint.contains("npm install -g"), "{hint}");

        let claude =
            "Error: Missing optional dependency @anthropic-ai/claude-agent-sdk-darwin-arm64.";
        assert!(missing_optional_dependency_hint(claude).is_some());
        assert!(missing_optional_dependency_hint("ENOENT: no such file").is_none());
    }
}
