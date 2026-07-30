use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Product data-dir segment for managed Node/npm trees (`…/<product>/node-tools/…`).
/// Seeded once from Tauri config at startup; falls back to `"Buzz"` when unset.
static PRODUCT_DIR: OnceLock<String> = OnceLock::new();

const BUZZ_MANAGED_NODE_VERSION: &str = "v24.18.0";
const DEFAULT_PRODUCT_DIR: &str = "Buzz";

/// Seed the product directory name used under `dirs::data_dir()`.
///
/// Idempotent-safe: a second call with a different value logs and keeps the first.
pub(crate) fn set_managed_product_dir(name: String) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    let sanitized = sanitize_product_dir(trimmed);
    if let Err(existing) = PRODUCT_DIR.set(sanitized.clone()) {
        if existing.as_str() != sanitized.as_str() {
            eprintln!(
                "buzz-desktop: managed product dir already set to {existing:?}; ignoring {sanitized:?}"
            );
        }
    }
}

/// Derive a stable filesystem directory name from Tauri product/identifier config.
///
/// Prefer `product_name` when present (stable across releases of the same product);
/// otherwise use the last segment of the bundle identifier.
pub(crate) fn product_dir_from_tauri_config(
    product_name: Option<&str>,
    identifier: &str,
) -> String {
    if let Some(name) = product_name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return sanitize_product_dir(trimmed);
        }
    }
    identifier
        .rsplit('.')
        .next()
        .filter(|s| !s.is_empty())
        .map(sanitize_product_dir)
        .unwrap_or_else(|| DEFAULT_PRODUCT_DIR.to_string())
}

fn sanitize_product_dir(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' => '_',
            other => other,
        })
        .collect()
}

/// Resolve the product dir from an optional seed (pure helper for tests).
#[cfg(test)]
fn resolve_managed_product_dir(seeded: Option<&str>) -> &str {
    seeded.unwrap_or(DEFAULT_PRODUCT_DIR)
}

fn managed_product_dir() -> &'static str {
    PRODUCT_DIR
        .get()
        .map(String::as_str)
        .unwrap_or(DEFAULT_PRODUCT_DIR)
}

/// Map `(os, arch)` from `std::env::consts` to the Node distribution platform
/// segment (`darwin-arm64`, `win-x64`, …). Single source of truth for both the
/// managed Node runtime tree and the managed npm prefix.
pub(crate) fn managed_platform_segment_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("darwin-arm64"),
        ("macos", "x86_64") => Some("darwin-x64"),
        ("linux", "x86_64") => Some("linux-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("windows", "x86_64") => Some("win-x64"),
        ("windows", "aarch64") => Some("win-arm64"),
        _ => None,
    }
}

pub(crate) fn managed_platform_segment() -> Option<&'static str> {
    managed_platform_segment_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// Whether Windows Node archives omit the `bin/` subdirectory.
fn managed_node_bin_subdir(os: &str) -> Option<&'static str> {
    match os {
        "windows" => None,
        _ => Some("bin"),
    }
}

/// Arch-scoped managed npm prefix: `data_dir()/<product>/node-tools/<platform>/`.
pub(crate) fn buzz_managed_npm_prefix() -> Option<PathBuf> {
    let platform = managed_platform_segment()?;
    dirs::data_dir().map(|dir| {
        dir.join(managed_product_dir())
            .join("node-tools")
            .join(platform)
    })
}

/// Legacy unscoped prefix (`…/node-tools` with no platform segment).
/// Used only to reclaim disk after a successful scoped install.
pub(crate) fn buzz_legacy_unscoped_npm_prefix() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(managed_product_dir()).join("node-tools"))
}

pub(crate) fn buzz_managed_node_root() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| {
        dir.join(managed_product_dir())
            .join("runtimes")
            .join("node")
    })
}

pub(crate) fn buzz_managed_node_bin_dir() -> Option<PathBuf> {
    let platform = managed_platform_segment()?;
    let bin_subdir = managed_node_bin_subdir(std::env::consts::OS);
    buzz_managed_node_root().map(|root| {
        let dir = root.join(BUZZ_MANAGED_NODE_VERSION).join(platform);
        match bin_subdir {
            Some(sub) => dir.join(sub),
            None => dir,
        }
    })
}

pub(crate) fn buzz_managed_node_bin_path() -> Option<PathBuf> {
    buzz_managed_node_bin_dir().map(|bin| {
        #[cfg(windows)]
        {
            bin.join("node.exe")
        }
        #[cfg(not(windows))]
        {
            bin.join("node")
        }
    })
}

pub(crate) fn buzz_managed_npm_bin_dir() -> Option<PathBuf> {
    buzz_managed_npm_prefix().map(|prefix| {
        #[cfg(windows)]
        {
            prefix
        }
        #[cfg(not(windows))]
        {
            prefix.join("bin")
        }
    })
}

/// True when `path` resolves under the managed npm prefix for this product/arch.
pub(crate) fn path_is_under_managed_npm_prefix(path: &Path) -> bool {
    let Some(prefix) = buzz_managed_npm_prefix() else {
        return false;
    };
    path_is_within(path, &prefix)
}

/// Positive-only memo for managed-Node readiness.
///
/// `Some(())` means a prior probe returned true. `None` means "unknown / not
/// ready" — never store `false`, or a pre-install miss sticks through the
/// install path and breaks `ensure_managed_node_runtime_blocking`.
fn managed_node_probe_ready_cache() -> &'static std::sync::Mutex<Option<()>> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<Option<()>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Drop the positive managed-Node probe memo so the next call re-spawns `--version`.
/// Invoked from `clear_resolve_cache` and immediately after a successful Node install.
pub(crate) fn clear_managed_node_probe_cache() {
    if let Ok(mut guard) = managed_node_probe_ready_cache().lock() {
        *guard = None;
    }
}

fn probe_managed_node_runtime_uncached() -> bool {
    let Some(node) = buzz_managed_node_bin_path() else {
        return false;
    };
    if !is_executable_file(&node) {
        return false;
    }
    let mut cmd = std::process::Command::new(&node);
    cmd.arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    crate::util::configure_no_window(&mut cmd);
    cmd.output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == BUZZ_MANAGED_NODE_VERSION)
        .unwrap_or(false)
}

/// Probe managed Node with `--version` and require the pinned version string.
/// Shared by install readiness, PATH contribution, and managed-adapter spawn.
///
/// Only `true` is memoized. A not-ready result is always re-probed so install
/// can observe the tree flipping from missing → ready in the same call stack.
/// Hot PATH builds after Node is ready pay one spawn for the process lifetime
/// (until `clear_managed_node_probe_cache`).
pub(crate) fn managed_node_runtime_probe_ok() -> bool {
    if let Ok(guard) = managed_node_probe_ready_cache().lock() {
        if guard.is_some() {
            return true;
        }
    }
    let ok = probe_managed_node_runtime_uncached();
    if ok {
        if let Ok(mut guard) = managed_node_probe_ready_cache().lock() {
            *guard = Some(());
        }
    }
    ok
}

/// Fail loudly when a managed adapter would run without a ready managed Node.
/// Unmanaged (system PATH) adapters are unchanged.
pub(crate) fn require_managed_node_for_adapter(resolved_agent: &Path) -> Result<(), String> {
    if !path_is_under_managed_npm_prefix(resolved_agent) {
        return Ok(());
    }
    if managed_node_runtime_probe_ok() {
        Ok(())
    } else {
        Err(managed_node_missing_hint())
    }
}

/// Resolve an agent command, enforcing managed-Node readiness for managed shims.
pub(crate) fn resolve_managed_aware_agent_command(command: &str) -> Result<String, String> {
    match crate::managed_agents::resolve_command(command) {
        Some(path) => {
            require_managed_node_for_adapter(&path)?;
            Ok(path.display().to_string())
        }
        None => Ok(command.to_string()),
    }
}

fn managed_node_missing_hint() -> String {
    "Buzz's managed Node.js runtime is missing for this architecture, so managed ACP adapters cannot start. Open Settings → Agent runtimes and click Install again.".to_string()
}

/// True when `path` contains a `..` component. Lexical parent walks treat `..`
/// as a normal component, so a path like `node-tools/../runtimes` would otherwise
/// appear to be "within" `node-tools`. Same pattern as archive zip/tar guards.
fn path_has_parent_dir_component(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Return true when `path` is equal to or nested under `ancestor`.
///
/// Rejects any path (or ancestor) that contains `..` — lexical parent walks
/// must not be allowed to climb out of the intended tree.
pub(crate) fn path_is_within(path: &Path, ancestor: &Path) -> bool {
    if path_has_parent_dir_component(path) || path_has_parent_dir_component(ancestor) {
        return false;
    }
    let mut current = path;
    loop {
        if current == ancestor {
            return true;
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return false,
        }
    }
}

/// Root under which purge is allowed: `data_dir()/<product>/node-tools`.
pub(crate) fn managed_node_tools_root() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(managed_product_dir()).join("node-tools"))
}

/// Guard: only **strict children** of `root` may be purged (not `root` itself).
///
/// Pure over `(path, root)` so tests can use a tempdir fixture. Rejects `..`
/// components on either side.
pub(crate) fn is_safe_to_purge_node_tools_path_with_root(path: &Path, root: &Path) -> bool {
    if path.as_os_str().is_empty() || root.as_os_str().is_empty() {
        return false;
    }
    if path_has_parent_dir_component(path) || path_has_parent_dir_component(root) {
        return false;
    }
    // Never delete the node-tools root wholesale — only a platform/legacy child.
    if path == root {
        return false;
    }
    path_is_within(path, root)
}

/// Guard: only paths that are strict children of `…/<product>/node-tools` may be purged.
pub(crate) fn is_safe_to_purge_node_tools_path(path: &Path) -> bool {
    let Some(root) = managed_node_tools_root() else {
        return false;
    };
    is_safe_to_purge_node_tools_path_with_root(path, &root)
}

pub(crate) fn buzz_managed_command_path(command: &str, basename: &str) -> Option<PathBuf> {
    if command.contains(std::path::MAIN_SEPARATOR)
        || !matches!(
            command,
            "codex-acp" | "claude-agent-acp" | "claude-code-acp" | "node" | "npm"
        )
    {
        return None;
    }

    let mut dirs = Vec::new();
    if let Some(managed_bin) = buzz_managed_npm_bin_dir() {
        dirs.push(managed_bin);
    }
    if let Some(managed_node_bin) = buzz_managed_node_bin_dir() {
        dirs.push(managed_node_bin);
    }

    dirs.into_iter()
        .map(|dir| dir.join(basename))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

/// Only contribute a managed PATH entry when the directory exists.
pub(crate) fn existing_managed_npm_bin_dir() -> Option<PathBuf> {
    buzz_managed_npm_bin_dir().filter(|dir| dir.is_dir())
}

/// Only contribute a managed Node PATH entry when the pinned Node passes probe.
pub(crate) fn existing_managed_node_bin_dir() -> Option<PathBuf> {
    if !managed_node_runtime_probe_ok() {
        return None;
    }
    buzz_managed_node_bin_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_segment_covers_all_six_pairs() {
        assert_eq!(
            managed_platform_segment_for("macos", "aarch64"),
            Some("darwin-arm64")
        );
        assert_eq!(
            managed_platform_segment_for("macos", "x86_64"),
            Some("darwin-x64")
        );
        assert_eq!(
            managed_platform_segment_for("linux", "x86_64"),
            Some("linux-x64")
        );
        assert_eq!(
            managed_platform_segment_for("linux", "aarch64"),
            Some("linux-arm64")
        );
        assert_eq!(
            managed_platform_segment_for("windows", "x86_64"),
            Some("win-x64")
        );
        assert_eq!(
            managed_platform_segment_for("windows", "aarch64"),
            Some("win-arm64")
        );
        assert_eq!(managed_platform_segment_for("macos", "riscv64"), None);
        assert_eq!(managed_platform_segment_for("freebsd", "x86_64"), None);
    }

    #[test]
    fn product_dir_falls_back_when_unseeded() {
        assert_eq!(resolve_managed_product_dir(None), "Buzz");
        assert_eq!(
            resolve_managed_product_dir(Some("NuncioCrew")),
            "NuncioCrew"
        );
    }

    #[test]
    fn product_dir_from_tauri_prefers_product_name() {
        assert_eq!(
            product_dir_from_tauri_config(Some("NuncioCrew"), "com.nuncio.crew"),
            "NuncioCrew"
        );
        assert_eq!(
            product_dir_from_tauri_config(None, "com.nuncio.crew"),
            "crew"
        );
        assert_eq!(
            product_dir_from_tauri_config(Some("  "), "xyz.block.buzz.app"),
            "app"
        );
        assert_eq!(
            product_dir_from_tauri_config(Some("Buzz Dev"), "xyz.block.buzz.app.dev"),
            "Buzz Dev"
        );
    }

    #[test]
    fn sanitize_replaces_path_separators() {
        assert_eq!(sanitize_product_dir("a/b\\c:d"), "a_b_c_d");
    }

    #[test]
    fn npm_prefix_and_node_bin_dir_agree_on_platform_segment() {
        let Some(platform) = managed_platform_segment() else {
            // Unsupported host — both accessors must be None.
            assert!(buzz_managed_npm_prefix().is_none());
            assert!(buzz_managed_node_bin_dir().is_none());
            return;
        };

        let npm = buzz_managed_npm_prefix().expect("npm prefix");
        let node_bin = buzz_managed_node_bin_dir().expect("node bin dir");

        assert!(
            npm.ends_with(Path::new("node-tools").join(platform)),
            "npm prefix must end with node-tools/{platform}: {}",
            npm.display()
        );

        // Relationship: npm's platform segment equals the parent-of-bin (or the
        // dir itself on Windows) segment under the versioned node root.
        let npm_platform = npm.file_name().and_then(|s| s.to_str());
        assert_eq!(npm_platform, Some(platform));

        // Node bin dir contains `/<version>/<platform>/…` — find platform in components.
        let has_platform = node_bin.components().any(|c| c.as_os_str() == platform);
        assert!(
            has_platform,
            "node bin dir must contain platform segment {platform}: {}",
            node_bin.display()
        );
    }

    #[test]
    fn legacy_unscoped_prefix_has_no_platform_segment() {
        let Some(legacy) = buzz_legacy_unscoped_npm_prefix() else {
            return;
        };
        assert_eq!(
            legacy.file_name().and_then(|s| s.to_str()),
            Some("node-tools")
        );
        if let Some(platform) = managed_platform_segment() {
            assert!(
                !legacy.ends_with(platform),
                "legacy must not include platform: {}",
                legacy.display()
            );
        }
    }

    #[test]
    fn require_managed_node_leaves_unmanaged_cli_unchanged() {
        // System PATH adapters are outside the managed npm prefix — never gated.
        assert!(require_managed_node_for_adapter(Path::new("/usr/local/bin/codex-acp")).is_ok());
        assert!(
            require_managed_node_for_adapter(Path::new("/opt/homebrew/bin/claude-agent-acp"))
                .is_ok()
        );
    }

    #[test]
    fn managed_node_probe_caches_only_true_never_false() {
        clear_managed_node_probe_cache();
        let ready = managed_node_runtime_probe_ok();
        let cached = managed_node_probe_ready_cache()
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false);
        if ready {
            assert!(
                cached,
                "ready probe must memoize so PATH builds avoid re-spawn"
            );
            assert!(managed_node_runtime_probe_ok());
            clear_managed_node_probe_cache();
            assert!(managed_node_probe_ready_cache()
                .lock()
                .map(|g| g.is_none())
                .unwrap_or(false));
        } else {
            assert!(
                !cached,
                "not-ready must never be memoized — install path must re-probe after download"
            );
            // Second call still not cached after another miss.
            assert!(!managed_node_runtime_probe_ok());
            assert!(managed_node_probe_ready_cache()
                .lock()
                .map(|g| g.is_none())
                .unwrap_or(false));
        }
    }

    /// Policy gate for ensure_managed_node_runtime_blocking: miss → install →
    /// ready must stay observable. Cache type is `Option<()>` (true-only); there
    /// is no representation for a stuck false.
    #[test]
    fn managed_node_probe_false_then_true_transition_is_observable() {
        clear_managed_node_probe_cache();
        assert!(
            managed_node_probe_ready_cache()
                .lock()
                .map(|g| g.is_none())
                .unwrap_or(false),
            "pre-install miss leaves cache empty"
        );
        // Simulate post-install success memo (install path stores true only).
        if let Ok(mut guard) = managed_node_probe_ready_cache().lock() {
            *guard = Some(());
        }
        assert!(managed_node_runtime_probe_ok());
        clear_managed_node_probe_cache();
        assert!(
            managed_node_probe_ready_cache()
                .lock()
                .map(|g| g.is_none())
                .unwrap_or(false),
            "post-install invalidate must clear so the next probe is fresh"
        );
    }

    #[test]
    fn path_within_detects_nested_and_rejects_siblings() {
        let root = PathBuf::from("/data/Buzz/node-tools");
        assert!(path_is_within(
            Path::new("/data/Buzz/node-tools/darwin-arm64"),
            &root
        ));
        assert!(path_is_within(&root, &root));
        assert!(!path_is_within(Path::new("/data/Buzz/runtimes"), &root));
        assert!(!path_is_within(Path::new("/tmp/evil"), &root));
    }

    #[test]
    fn path_within_rejects_parent_dir_components() {
        let root = PathBuf::from("/data/Buzz/node-tools");
        assert!(!path_is_within(
            Path::new("/data/Buzz/node-tools/.."),
            &root
        ));
        assert!(!path_is_within(
            Path::new("/data/Buzz/node-tools/../runtimes"),
            &root
        ));
        assert!(!path_is_within(
            Path::new("/data/Buzz/node-tools/../../../../etc"),
            &root
        ));
        assert!(!path_is_within(
            Path::new("/data/Buzz/node-tools/darwin-arm64"),
            Path::new("/data/Buzz/node-tools/../node-tools"),
        ));
    }

    #[test]
    fn purge_guard_allows_strict_children_only() {
        let root = PathBuf::from("/data/Buzz/node-tools");
        assert!(!is_safe_to_purge_node_tools_path_with_root(&root, &root));
        assert!(is_safe_to_purge_node_tools_path_with_root(
            &root.join("darwin-arm64"),
            &root
        ));
        assert!(is_safe_to_purge_node_tools_path_with_root(
            &root.join("bin"),
            &root
        ));
        assert!(!is_safe_to_purge_node_tools_path_with_root(
            Path::new("/data/Buzz/runtimes"),
            &root
        ));
        assert!(!is_safe_to_purge_node_tools_path_with_root(
            Path::new("/tmp"),
            &root
        ));
        assert!(!is_safe_to_purge_node_tools_path_with_root(
            Path::new(""),
            &root
        ));
    }

    #[test]
    fn purge_guard_rejects_parent_dir_escapes() {
        let root = PathBuf::from("/data/NuncioCrew/node-tools");
        assert!(!is_safe_to_purge_node_tools_path_with_root(
            Path::new("/data/NuncioCrew/node-tools/.."),
            &root
        ));
        assert!(!is_safe_to_purge_node_tools_path_with_root(
            Path::new("/data/NuncioCrew/node-tools/../runtimes"),
            &root
        ));
        assert!(!is_safe_to_purge_node_tools_path_with_root(
            Path::new("/data/NuncioCrew/node-tools/../../../../etc"),
            &root
        ));
    }
}
