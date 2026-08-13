//! Paths and identifiers for Cowork shadow history.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Env var Desktop injects so the harness and Tauri share one history root.
pub const HISTORY_DIR_ENV: &str = "BUZZ_ACP_COWORK_HISTORY_DIR";
/// Default per-file size threshold (bytes). Larger files are not versioned.
pub const DEFAULT_SIZE_THRESHOLD: u64 = 50 * 1024 * 1024;
/// Default retention window for owner-invoked compact: keep every checkpoint
/// inside this many days, then one per calendar day.
pub const DEFAULT_COMPACT_KEEP_DAYS: u64 = 30;
/// Loud notice stored when history is rebuilt empty after corruption.
pub const CORRUPTION_NOTICE: &str =
    "Version history was damaged and had to be rebuilt. Earlier versions are gone.";

/// Filesystem-safe id derived from the NIP-34 repo address.
pub fn project_history_id(repo_address: &str) -> String {
    let digest = Sha256::digest(repo_address.as_bytes());
    hex::encode(&digest[..16])
}

/// `<history-root>/<id>.git`
pub fn history_git_dir(history_root: &Path, repo_address: &str) -> PathBuf {
    history_root.join(format!("{}.git", project_history_id(repo_address)))
}

/// History root from the harness/desktop env, if set.
pub fn history_dir_from_env() -> Option<PathBuf> {
    std::env::var_os(HISTORY_DIR_ENV).map(PathBuf::from)
}

pub(crate) fn empty_hooks_dir(git_dir: &Path) -> PathBuf {
    git_dir.join("crew-no-hooks")
}

pub(crate) fn exclude_path(git_dir: &Path) -> PathBuf {
    git_dir.join("info").join("exclude")
}

pub(crate) fn meta_path(git_dir: &Path) -> PathBuf {
    git_dir.join("crew-cowork.json")
}

pub(crate) fn corrupt_backup_dir(git_dir: &Path, now_secs: i64) -> PathBuf {
    let name = git_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("history");
    git_dir
        .parent()
        .unwrap_or(git_dir)
        .join(format!("{name}.corrupt-{now_secs}"))
}
