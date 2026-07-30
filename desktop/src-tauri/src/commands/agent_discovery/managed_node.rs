use sha2::{Digest, Sha256};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use std::{io::Read, io::Write};

use crate::managed_agents::{is_npm_global_install, InstallStepResult};

const MANAGED_NODE_VERSION: &str = "v24.18.0";
const MANAGED_NODE_MAX_BYTES: u64 = 90 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
struct ManagedNodeArtifact {
    platform: &'static str,
    filename: &'static str,
    sha256: &'static str,
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const MANAGED_NODE_ARTIFACT: Option<ManagedNodeArtifact> = Some(ManagedNodeArtifact {
    platform: "darwin-arm64",
    filename: "node-v24.18.0-darwin-arm64.tar.gz",
    sha256: "e1a97e14c99c803e96c7339403282ea05a499c32f8d83defe9ef5ec66f979ed1",
});

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const MANAGED_NODE_ARTIFACT: Option<ManagedNodeArtifact> = Some(ManagedNodeArtifact {
    platform: "darwin-x64",
    filename: "node-v24.18.0-darwin-x64.tar.gz",
    sha256: "dfd0dbd3e721503434df7b7205e719f61b3a3a31b2bcf9729b8b91fea240f080",
});

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MANAGED_NODE_ARTIFACT: Option<ManagedNodeArtifact> = Some(ManagedNodeArtifact {
    platform: "linux-x64",
    filename: "node-v24.18.0-linux-x64.tar.gz",
    sha256: "783130984963db7ba9cbd01089eaf2c2efb055c7c1693c943174b967b3050cb8",
});

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const MANAGED_NODE_ARTIFACT: Option<ManagedNodeArtifact> = Some(ManagedNodeArtifact {
    platform: "linux-arm64",
    filename: "node-v24.18.0-linux-arm64.tar.gz",
    sha256: "6b4484c2190274175df9aa8f28e2d758a819cb1c1fe6ab481e2f95b463ab8508",
});

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const MANAGED_NODE_ARTIFACT: Option<ManagedNodeArtifact> = Some(ManagedNodeArtifact {
    platform: "win-x64",
    filename: "node-v24.18.0-win-x64.zip",
    sha256: "0ae68406b42d7725661da979b1403ec9926da205c6770827f33aac9d8f26e821",
});

#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
const MANAGED_NODE_ARTIFACT: Option<ManagedNodeArtifact> = Some(ManagedNodeArtifact {
    platform: "win-arm64",
    filename: "node-v24.18.0-win-arm64.zip",
    sha256: "f274669adb93b1fd0fbf8f21fd078609e9dcc84333d4f2718d2dde3f9a161a01",
});

#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "aarch64")
)))]
const MANAGED_NODE_ARTIFACT: Option<ManagedNodeArtifact> = None;

fn managed_node_unsupported_step() -> InstallStepResult {
    InstallStepResult {
        step: "node".to_string(),
        command: "managed Node.js runtime".to_string(),
        success: false,
        stdout: String::new(),
        stderr: format!(
            "Buzz does not provide a managed Node.js runtime for {}-{} yet",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        exit_code: None,
        hint: Some(
            "Install Node.js from https://nodejs.org, restart Buzz, then click Install again."
                .to_string(),
        ),
    }
}

fn managed_node_install_hint() -> String {
    "Buzz could not install its private Node.js runtime. Check your network and app-data directory permissions, then click Install again.".to_string()
}

fn managed_node_failed_step(stderr: String) -> InstallStepResult {
    InstallStepResult {
        step: "node".to_string(),
        command: "managed Node.js runtime".to_string(),
        success: false,
        stdout: String::new(),
        stderr,
        exit_code: None,
        hint: Some(managed_node_install_hint()),
    }
}

fn managed_node_runtime_ready() -> bool {
    let Some(node) = crate::managed_agents::buzz_managed_node_bin_path() else {
        return false;
    };
    if !node.is_file() {
        return false;
    }
    let mut cmd = std::process::Command::new(&node);
    cmd.arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    crate::util::configure_no_window(&mut cmd);
    let output = cmd.output();
    output
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim() == MANAGED_NODE_VERSION)
        .unwrap_or(false)
}

fn managed_node_install_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(super) fn managed_node_runtime_supported() -> bool {
    MANAGED_NODE_ARTIFACT.is_some() && crate::managed_agents::buzz_managed_node_bin_dir().is_some()
}

pub(super) fn ensure_managed_node_runtime_blocking() -> Result<(), Box<InstallStepResult>> {
    if managed_node_runtime_ready() {
        return Ok(());
    }

    let Some(artifact) = MANAGED_NODE_ARTIFACT else {
        return Err(Box::new(managed_node_unsupported_step()));
    };
    let Some(root) = crate::managed_agents::buzz_managed_node_root() else {
        return Err(Box::new(managed_node_failed_step(
            "failed to resolve Buzz app-data directory for private Node.js runtime".to_string(),
        )));
    };

    let _guard = managed_node_install_lock().lock().map_err(|_| {
        Box::new(managed_node_failed_step(
            "managed Node.js install lock poisoned".to_string(),
        ))
    })?;

    if managed_node_runtime_ready() {
        return Ok(());
    }

    install_managed_node_runtime(&root, artifact)
        .map_err(|err| Box::new(managed_node_failed_step(err)))?;
    if managed_node_runtime_ready() {
        Ok(())
    } else {
        Err(Box::new(managed_node_failed_step(
            "managed Node.js runtime did not pass readiness after install".to_string(),
        )))
    }
}

fn install_managed_node_runtime(
    root: &std::path::Path,
    artifact: ManagedNodeArtifact,
) -> Result<(), String> {
    let final_dir = root.join(MANAGED_NODE_VERSION).join(artifact.platform);
    let temp_dir = root.join(format!(
        "{}.{}.tmp",
        MANAGED_NODE_VERSION, artifact.platform
    ));
    let archive_path = root.join(format!("{}.download", artifact.filename));

    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir).map_err(|e| format!("remove stale temp dir: {e}"))?;
    }
    std::fs::create_dir_all(root).map_err(|e| format!("create runtime root: {e}"))?;
    if let Some(parent) = final_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create runtime version dir: {e}"))?;
    }

    let url = format!(
        "https://nodejs.org/dist/{MANAGED_NODE_VERSION}/{}",
        artifact.filename
    );
    download_managed_node_archive(&url, &archive_path, artifact.sha256)?;

    std::fs::create_dir_all(&temp_dir).map_err(|e| format!("create temp dir: {e}"))?;
    extract_managed_node_archive(&archive_path, &temp_dir, artifact.filename)?;
    let _ = std::fs::remove_file(&archive_path);

    let extracted_dir = temp_dir.join(
        artifact
            .filename
            .trim_end_matches(".tar.gz")
            .trim_end_matches(".zip"),
    );
    let source_dir = if extracted_dir.is_dir() {
        extracted_dir
    } else {
        temp_dir.clone()
    };
    verify_node_tree(&source_dir)?;

    let old_dir = final_dir.with_extension("old");
    if old_dir.exists() {
        std::fs::remove_dir_all(&old_dir).map_err(|e| format!("remove stale old dir: {e}"))?;
    }
    if final_dir.exists() {
        std::fs::rename(&final_dir, &old_dir)
            .map_err(|e| format!("stage previous runtime: {e}"))?;
    }
    if let Err(error) = std::fs::rename(&source_dir, &final_dir) {
        if old_dir.exists() {
            let _ = std::fs::rename(&old_dir, &final_dir);
        }
        return Err(format!("install runtime: {error}"));
    }
    let _ = std::fs::remove_dir_all(&old_dir);
    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}

fn download_managed_node_archive(
    url: &str,
    dest: &std::path::Path,
    expected_sha256: &str,
) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5 * 60))
        .build()
        .map_err(|e| format!("build Node.js download client: {e}"))?;
    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("download Node.js request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "download Node.js HTTP {}: {}",
            response.status().as_u16(),
            response.status().canonical_reason().unwrap_or("unknown")
        ));
    }
    if let Some(total) = response.content_length() {
        if total > MANAGED_NODE_MAX_BYTES {
            return Err(format!(
                "download Node.js too large: {total} bytes (max {MANAGED_NODE_MAX_BYTES})"
            ));
        }
    }

    let mut response = response;
    let mut file =
        std::fs::File::create(dest).map_err(|e| format!("create Node.js archive: {e}"))?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|e| format!("download Node.js stream error: {e}"))?;
        if read == 0 {
            break;
        }
        downloaded += read as u64;
        if downloaded > MANAGED_NODE_MAX_BYTES {
            let _ = std::fs::remove_file(dest);
            return Err(format!(
                "download Node.js exceeded max size: {downloaded} bytes (max {MANAGED_NODE_MAX_BYTES})"
            ));
        }
        file.write_all(&buffer[..read])
            .map_err(|e| format!("write Node.js archive: {e}"))?;
        hasher.update(&buffer[..read]);
    }
    file.flush()
        .map_err(|e| format!("flush Node.js archive: {e}"))?;

    let actual = hex::encode(hasher.finalize());
    if actual != expected_sha256 {
        let _ = std::fs::remove_file(dest);
        return Err(format!(
            "download Node.js hash mismatch: expected {expected_sha256}, got {actual}"
        ));
    }
    Ok(())
}

fn extract_managed_node_archive(
    archive_path: &std::path::Path,
    dest_dir: &std::path::Path,
    filename: &str,
) -> Result<(), String> {
    if filename.ends_with(".tar.gz") {
        let file =
            std::fs::File::open(archive_path).map_err(|e| format!("open Node.js archive: {e}"))?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        validate_managed_node_archive_entries(&mut archive)?;

        let file = std::fs::File::open(archive_path)
            .map_err(|e| format!("open Node.js archive for extraction: {e}"))?;
        let decoder = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);
        archive
            .unpack(dest_dir)
            .map_err(|e| format!("extract Node.js archive: {e}"))
    } else if filename.ends_with(".zip") {
        let file =
            std::fs::File::open(archive_path).map_err(|e| format!("open Node.js archive: {e}"))?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("read Node.js zip archive: {e}"))?;
        validate_managed_node_zip_entries(&archive)?;
        extract_managed_node_zip(&mut archive, dest_dir)
    } else {
        Err(format!("unsupported managed Node.js archive: {filename}"))
    }
}

/// Validate ZIP entry names using platform-neutral string logic.
///
/// `std::path::Path` is intentionally avoided: its `is_absolute()` and
/// `Component` parsing use BUILD-HOST grammar, so `/etc/passwd` is not
/// `is_absolute()` on Windows (no drive prefix), causing the check to lie on
/// the platform this guard exists to protect.  Instead we apply pure string
/// rules that produce identical results on every host:
///
/// - Unix-rooted: starts with `/`
/// - Windows-rooted: starts with `\`, has a drive prefix (`X:`), or is UNC
///   (`\\` / `//`)
/// - Traversal: any component that is `..` when split on EITHER `/` or `\`
fn validate_managed_node_zip_entries(
    archive: &zip::ZipArchive<std::fs::File>,
) -> Result<(), String> {
    for i in 0..archive.len() {
        let name = archive
            .name_for_index(i)
            .ok_or_else(|| format!("Node.js zip entry {i}: missing name"))?;

        // Absolute-path checks (platform-neutral).
        if name.starts_with('/') || name.starts_with('\\') {
            return Err(format!("Node.js zip contains absolute path: {name}"));
        }
        // Drive prefix: one ASCII letter followed by ':'
        if name.len() >= 2 && name.as_bytes()[1] == b':' && name.as_bytes()[0].is_ascii_alphabetic()
        {
            return Err(format!("Node.js zip contains absolute path: {name}"));
        }
        // UNC prefix: // or \\ (covered by starts_with checks above for \\,
        // and // is caught by starts_with('/') then a second '/' — belt + suspenders).
        // (Already caught by the starts_with checks above; explicit for clarity.)

        // Traversal: split on both separators and check each component.
        let has_traversal = name.split(['/', '\\']).any(|component| component == "..");
        if has_traversal {
            return Err(format!("Node.js zip contains path traversal: {name}"));
        }
    }
    Ok(())
}

fn extract_managed_node_zip(
    archive: &mut zip::ZipArchive<std::fs::File>,
    dest_dir: &std::path::Path,
) -> Result<(), String> {
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Node.js zip entry {i}: {e}"))?;
        let outpath = match entry.enclosed_name() {
            Some(p) => dest_dir.join(p),
            None => {
                return Err(format!(
                    "Node.js zip contains unsafe path: {}",
                    entry.name()
                ))
            }
        };
        if entry.is_dir() {
            std::fs::create_dir_all(&outpath)
                .map_err(|e| format!("create dir in Node.js zip: {e}"))?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("create parent dir in Node.js zip: {e}"))?;
            }
            let mut out = std::fs::File::create(&outpath)
                .map_err(|e| format!("create file in Node.js zip: {e}"))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("extract file in Node.js zip: {e}"))?;
        }
    }
    Ok(())
}

fn validate_managed_node_archive_entries<R: std::io::Read>(
    archive: &mut tar::Archive<R>,
) -> Result<(), String> {
    let entries = archive
        .entries()
        .map_err(|e| format!("read Node.js archive entries: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Node.js archive entry: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("Node.js archive entry path: {e}"))?;
        let path_str = path.to_string_lossy();
        if path.is_absolute() {
            return Err(format!(
                "Node.js archive contains absolute path: {path_str}"
            ));
        }
        if path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(format!(
                "Node.js archive contains path traversal: {path_str}"
            ));
        }
    }
    Ok(())
}

fn verify_node_tree(dir: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        // Windows zip layout: node.exe + npm.cmd + npm (POSIX sh shim) at archive root
        let node = dir.join("node.exe");
        let npm_cmd = dir.join("npm.cmd");
        let npm = dir.join("npm");
        if !node.is_file() {
            return Err("Node.js archive missing node.exe".to_string());
        }
        if !npm_cmd.is_file() {
            return Err("Node.js archive missing npm.cmd".to_string());
        }
        if !npm.is_file() {
            return Err("Node.js archive missing npm".to_string());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        // Unix tarball layout: bin/node + bin/npm
        let node = dir.join("bin").join("node");
        let npm = dir.join("bin").join("npm");
        if !node.is_file() {
            return Err("Node.js archive missing bin/node".to_string());
        }
        if !npm.is_file() {
            return Err("Node.js archive missing bin/npm".to_string());
        }
        Ok(())
    }
}

// ── managed npm adapter installs ──────────────────────────────────────────────

/// Guidance text shown when the Buzz-private npm prefix is not available.
fn managed_npm_prefix_hint() -> String {
    "Buzz could not create its private Node tools directory. Check app-data directory permissions, restart Buzz, then click Install again.".to_string()
}

/// Optional platform packages that must match `managed_platform_segment()`.
const MANAGED_ADAPTER_PLATFORM_PACKAGE_PREFIXES: &[&str] = &[
    "@openai/codex-",
    "@anthropic-ai/claude-agent-sdk-",
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

fn list_platform_optional_packages(modules: &std::path::Path) -> Vec<String> {
    let mut found = Vec::new();
    for scope in ["@openai", "@anthropic-ai"] {
        let scope_dir = modules.join(scope);
        let Ok(entries) = std::fs::read_dir(&scope_dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let full = format!("{scope}/{name}");
            if MANAGED_ADAPTER_PLATFORM_PACKAGE_PREFIXES
                .iter()
                .any(|prefix| full.starts_with(prefix))
            {
                found.push(full);
            }
        }
    }
    found
}

fn has_managed_adapter_package(modules: &std::path::Path) -> bool {
    let scope = modules.join("@agentclientprotocol");
    for name in ["codex-acp", "claude-agent-acp", "claude-code-acp"] {
        if scope.join(name).is_dir() {
            return true;
        }
    }
    false
}

/// Pure directory-fixture check: does `prefix` hold adapters for `expected_platform`?
pub(crate) fn managed_adapter_arch_matches(
    prefix: &std::path::Path,
    expected_platform: &str,
) -> AdapterArchCheck {
    let modules = npm_global_node_modules(prefix);
    if !modules.is_dir() {
        return AdapterArchCheck::NoAdaptersInstalled;
    }

    let packages = list_platform_optional_packages(&modules);
    if packages.is_empty() {
        if has_managed_adapter_package(&modules) {
            // Adapter present but no platform optional package dirs — treat as
            // mismatch so self-repair can reinstall.
            return AdapterArchCheck::Mismatch;
        }
        return AdapterArchCheck::NoAdaptersInstalled;
    }

    let mut saw_expected = false;
    let mut saw_wrong = false;
    for package in &packages {
        let is_expected = MANAGED_ADAPTER_PLATFORM_PACKAGE_PREFIXES
            .iter()
            .any(|prefix| package == &format!("{prefix}{expected_platform}"));
        if is_expected {
            saw_expected = true;
        } else {
            saw_wrong = true;
        }
    }

    if saw_wrong && !saw_expected {
        AdapterArchCheck::Mismatch
    } else if saw_expected {
        AdapterArchCheck::Matches
    } else {
        AdapterArchCheck::NoAdaptersInstalled
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
        std::fs::remove_dir_all(path)
            .map_err(|e| format!("failed to remove managed npm prefix '{}': {e}", path.display()))?;
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
/// Returns `Ok(true)` when a mismatched tree was purged (caller should force
/// reinstall even if an adapter binary still resolves elsewhere).
pub(super) fn ensure_managed_adapter_arch_ready_blocking() -> Result<bool, Box<InstallStepResult>> {
    let Some(expected) = crate::managed_agents::managed_platform_segment() else {
        return Ok(false);
    };
    let Some(prefix) = crate::managed_agents::buzz_managed_npm_prefix() else {
        return Ok(false);
    };

    match managed_adapter_arch_matches(&prefix, expected) {
        AdapterArchCheck::Matches | AdapterArchCheck::NoAdaptersInstalled => Ok(false),
        AdapterArchCheck::Mismatch => {
            if let Err(error) = purge_managed_npm_prefix(&prefix) {
                return Err(Box::new(purge_failed_step(&prefix, error)));
            }
            Ok(true)
        }
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
    // Only remove the legacy root when it still looks like an unscoped install
    // (has lib/node_modules or node_modules directly). Never touch platform
    // children — those are the new layout living under the same parent name.
    // The legacy path IS `…/node-tools`; platform dirs are `…/node-tools/<platform>`.
    // Removing `legacy` would delete platform children too — so only remove
    // contents that belong to the old layout, not the directory wholesale if
    // scoped children exist.
    reclaim_legacy_unscoped_contents(&legacy);
}

fn reclaim_legacy_unscoped_contents(legacy_root: &std::path::Path) {
    // Old layout stored `bin/`, `lib/`, `cache/`, `corepack/` directly under
    // node-tools/. New layout stores only platform directories there. Remove
    // known legacy entries; leave platform dirs alone.
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
    // npm uses Node's process.platform: darwin | linux | win32
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
        // uninstall must not carry --cpu/--os
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

/// Detect upstream Codex/Claude "Missing optional dependency …-darwin-arm64"
/// dead-end advice and replace it with a Buzz-specific repair hint.
pub(super) fn missing_optional_dependency_hint(stderr: &str) -> Option<String> {
    let lower = stderr.to_ascii_lowercase();
    let matches = lower.contains("missing optional dependency")
        && (lower.contains("@openai/codex-")
            || lower.contains("@anthropic-ai/claude-agent-sdk-"));
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

// ── end managed npm adapter installs ──────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
        std::fs::create_dir_all(modules.join("@openai").join("codex-darwin-x64")).unwrap();
        assert_eq!(
            managed_adapter_arch_matches(temp.path(), "darwin-arm64"),
            AdapterArchCheck::Mismatch
        );
    }

    #[test]
    fn adapter_arch_matches_expected_platform_package() {
        let temp = tempfile::tempdir().unwrap();
        let modules = npm_global_node_modules(temp.path());
        std::fs::create_dir_all(modules.join("@openai").join("codex-darwin-arm64")).unwrap();
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

        let claude = "Error: Missing optional dependency @anthropic-ai/claude-agent-sdk-darwin-arm64.";
        assert!(missing_optional_dependency_hint(claude).is_some());
        assert!(missing_optional_dependency_hint("ENOENT: no such file").is_none());
    }


    // ── zip validation tests ──────────────────────────────────────────────────

    /// Build an in-memory zip archive with the supplied entry names and return
    /// a temporary file containing it (zip::ZipArchive requires Seek).
    fn make_zip_with_entries(entry_names: &[&str]) -> tempfile::NamedTempFile {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = zip::write::SimpleFileOptions::default();
            for name in entry_names {
                writer.start_file(*name, opts).unwrap();
            }
            writer.finish().unwrap();
        }
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut tmp, &buf).unwrap();
        tmp
    }

    #[test]
    fn test_validate_zip_accepts_normal_entries() {
        let tmp = make_zip_with_entries(&[
            "node-v24.18.0-win-x64/node.exe",
            "node-v24.18.0-win-x64/npm.cmd",
            "node-v24.18.0-win-x64/npm",
        ]);
        let file = std::fs::File::open(tmp.path()).unwrap();
        let archive = zip::ZipArchive::new(file).unwrap();
        assert!(validate_managed_node_zip_entries(&archive).is_ok());
    }

    #[test]
    fn test_validate_zip_rejects_absolute_path() {
        let tmp = make_zip_with_entries(&["/etc/passwd"]);
        let file = std::fs::File::open(tmp.path()).unwrap();
        let archive = zip::ZipArchive::new(file).unwrap();
        let err = validate_managed_node_zip_entries(&archive).unwrap_err();
        assert!(
            err.contains("absolute path"),
            "expected 'absolute path' in: {err}"
        );
    }

    #[test]
    fn test_validate_zip_rejects_path_traversal() {
        let tmp = make_zip_with_entries(&["../../../etc/passwd"]);
        let file = std::fs::File::open(tmp.path()).unwrap();
        let archive = zip::ZipArchive::new(file).unwrap();
        let err = validate_managed_node_zip_entries(&archive).unwrap_err();
        assert!(
            err.contains("path traversal"),
            "expected 'path traversal' in: {err}"
        );
    }

    #[test]
    fn test_validate_zip_rejects_backslash_rooted() {
        // Windows-style absolute path using backslash — must reject on every host.
        let tmp = make_zip_with_entries(&["\\Windows\\system32\\evil.dll"]);
        let file = std::fs::File::open(tmp.path()).unwrap();
        let archive = zip::ZipArchive::new(file).unwrap();
        let err = validate_managed_node_zip_entries(&archive).unwrap_err();
        assert!(
            err.contains("absolute path"),
            "expected 'absolute path' in: {err}"
        );
    }

    #[test]
    fn test_validate_zip_rejects_drive_prefix() {
        // Windows drive-letter absolute path — must reject on every host.
        let tmp = make_zip_with_entries(&["C:\\evil\\payload.exe"]);
        let file = std::fs::File::open(tmp.path()).unwrap();
        let archive = zip::ZipArchive::new(file).unwrap();
        let err = validate_managed_node_zip_entries(&archive).unwrap_err();
        assert!(
            err.contains("absolute path"),
            "expected 'absolute path' in: {err}"
        );
    }

    #[test]
    fn test_validate_zip_rejects_backslash_traversal() {
        // Path traversal using Windows separator — must reject on every host.
        let tmp = make_zip_with_entries(&["node-v24.18.0-win-x64\\..\\..\\evil"]);
        let file = std::fs::File::open(tmp.path()).unwrap();
        let archive = zip::ZipArchive::new(file).unwrap();
        let err = validate_managed_node_zip_entries(&archive).unwrap_err();
        assert!(
            err.contains("path traversal"),
            "expected 'path traversal' in: {err}"
        );
    }

    // ── verify_node_tree layout tests ─────────────────────────────────────────

    #[test]
    fn test_verify_node_tree_unix_layout_passes() {
        let tmp = tempfile::TempDir::new().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("node"), b"").unwrap();
        std::fs::write(bin.join("npm"), b"").unwrap();
        // On non-Windows the unix branch is active — this must pass.
        #[cfg(not(windows))]
        assert!(verify_node_tree(tmp.path()).is_ok());
        // On Windows the windows branch is active — unix layout must fail.
        #[cfg(windows)]
        assert!(verify_node_tree(tmp.path()).is_err());
    }

    #[test]
    fn test_verify_node_tree_unix_layout_missing_npm_fails() {
        let tmp = tempfile::TempDir::new().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("node"), b"").unwrap();
        // npm intentionally absent
        #[cfg(not(windows))]
        {
            let err = verify_node_tree(tmp.path()).unwrap_err();
            assert!(err.contains("bin/npm"), "err: {err}");
        }
    }

    #[test]
    fn test_verify_node_tree_windows_layout_passes() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("node.exe"), b"").unwrap();
        std::fs::write(tmp.path().join("npm.cmd"), b"").unwrap();
        std::fs::write(tmp.path().join("npm"), b"").unwrap();
        // On Windows the windows branch is active — this must pass.
        #[cfg(windows)]
        assert!(verify_node_tree(tmp.path()).is_ok());
        // On non-Windows the unix branch is active — windows-layout root files
        // don't satisfy bin/node + bin/npm, so this must fail.
        #[cfg(not(windows))]
        assert!(verify_node_tree(tmp.path()).is_err());
    }

    #[test]
    fn test_verify_node_tree_windows_layout_missing_npm_shim_fails() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("node.exe"), b"").unwrap();
        std::fs::write(tmp.path().join("npm.cmd"), b"").unwrap();
        // npm POSIX shim intentionally absent
        #[cfg(windows)]
        {
            let err = verify_node_tree(tmp.path()).unwrap_err();
            assert!(err.contains("npm"), "err: {err}");
        }
    }
}
