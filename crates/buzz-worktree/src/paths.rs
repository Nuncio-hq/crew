//! Canonical paths under the repository common Git directory.

use std::path::{Path, PathBuf};

use crate::identity::normalize_root_event_id;
use sha2::{Digest, Sha256};

/// Directory for per-root advisory lease lockfiles.
pub const LEASE_DIRECTORY: &str = "buzz-thread-workspace-leases";

/// Directory for path-keyed exclusive turn leases (shared checkouts).
pub const PATH_LEASE_DIRECTORY: &str = "buzz-thread-workspace-path-leases";

/// Directory for versioned lifecycle records (one file per full root).
pub const LIFECYCLE_RECORD_DIRECTORY: &str = "buzz-thread-workspace-lifecycle";

/// Current lifecycle record schema version.
pub const RECORD_SCHEMA_VERSION: u32 = 1;

/// Current lease lockfile marker version (embedded as file content).
pub const LEASE_SCHEMA_VERSION: u32 = 1;

/// Directory holding lease lockfiles.
pub fn lease_dir(common_git: &Path) -> PathBuf {
    common_git.join(LEASE_DIRECTORY)
}

/// Lockfile path for a validated full root.
pub fn lease_lock_path(common_git: &Path, root_event_id: &str) -> Result<PathBuf, String> {
    let root = normalize_root_event_id(root_event_id)?;
    Ok(lease_dir(common_git).join(format!("{root}.lease")))
}

/// Directory holding path-keyed exclusive turn lockfiles.
pub fn path_lease_dir(common_git: &Path) -> PathBuf {
    common_git.join(PATH_LEASE_DIRECTORY)
}

/// Lockfile path for a canonical checkout path (sha256 stem).
pub fn path_lease_lock_path(common_git: &Path, worktree_path: &Path) -> Result<PathBuf, String> {
    if worktree_path.as_os_str().is_empty() {
        return Err("worktree path is empty".into());
    }
    Ok(path_lease_dir(common_git).join(format!("{}.lease", path_lease_key(worktree_path))))
}

/// Stable lockfile stem for a checkout path (sha256 hex of the UTF-8 path).
pub fn path_lease_key(worktree_path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(worktree_path.to_string_lossy().as_bytes());
    hex::encode(hasher.finalize())
}

/// Directory holding lifecycle JSON records.
pub fn lifecycle_records_dir(common_git: &Path) -> PathBuf {
    common_git.join(LIFECYCLE_RECORD_DIRECTORY)
}

/// Record path keyed by the full 64-hex root (never the 12-hex prefix alone).
pub fn lifecycle_record_path(common_git: &Path, root_event_id: &str) -> Result<PathBuf, String> {
    let root = normalize_root_event_id(root_event_id)?;
    Ok(lifecycle_records_dir(common_git).join(format!("{root}.json")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn full_root_paths_do_not_alias_on_prefix() {
        let common = Path::new("/tmp/repo.git");
        let a = format!("{}{}", "a".repeat(12), "b".repeat(52));
        let b = format!("{}{}", "a".repeat(12), "c".repeat(52));
        let path_a = lifecycle_record_path(common, &a).unwrap();
        let path_b = lifecycle_record_path(common, &b).unwrap();
        assert_ne!(path_a, path_b);
        assert!(path_a
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(&"a".repeat(12)));
    }
}
