//! Path-keyed exclusive turn leases for shared checkouts.
//!
//! Root-keyed shared/exclusive leases still coordinate eviction of one
//! thread's managed worktree. Path-keyed exclusive leases serialize turns
//! across threads bound to the same canonical path (`ws=main` or a shared
//! `ws=branch:` worktree).

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::LeaseError;
use crate::identity::validate_root_event_id;
use crate::paths::{path_lease_dir, path_lease_lock_path, LEASE_SCHEMA_VERSION};

/// Exclusive turn lease keyed by canonical checkout path. Releases on drop.
#[derive(Debug)]
pub struct PathExclusiveLease {
    _file: File,
    path: PathBuf,
}

impl PathExclusiveLease {
    /// Path of the underlying lockfile.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PathExclusiveLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self._file);
    }
}

/// Identity of the thread currently holding a path lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathLeaseHolder {
    /// Full 64-hex thread root event id.
    pub root_event_id: String,
    /// Short owner-facing label (first line of the thread, truncated).
    pub label: String,
}

/// Non-blocking exclusive lease for a checkout path.
pub fn try_acquire_path_exclusive(
    common_git: &Path,
    worktree_path: &Path,
    holder: &PathLeaseHolder,
) -> Result<PathExclusiveLease, LeaseError> {
    let root = validate_root_event_id(&holder.root_event_id)?;
    let holder = PathLeaseHolder {
        root_event_id: root,
        label: holder.label.trim().chars().take(64).collect(),
    };
    let path =
        path_lease_lock_path(common_git, worktree_path).map_err(LeaseError::InvalidIdentity)?;
    let mut file = open_path_lease_file(common_git, &path)?;
    match FileExt::try_lock_exclusive(&file) {
        Ok(()) => {
            write_holder(&mut file, &path, &holder)?;
            Ok(PathExclusiveLease { _file: file, path })
        }
        Err(error) if is_lock_conflict(&error) => Err(LeaseError::Busy),
        Err(error) => Err(LeaseError::Io {
            path,
            source: error,
        }),
    }
}

/// Best-effort read of the current path-lease holder (for named Busy copy).
pub fn read_path_lease_holder(common_git: &Path, worktree_path: &Path) -> Option<PathLeaseHolder> {
    let path = path_lease_lock_path(common_git, worktree_path).ok()?;
    let bytes = fs::read(&path).ok()?;
    parse_holder_bytes(&bytes)
}

fn open_path_lease_file(common_git: &Path, path: &Path) -> Result<File, LeaseError> {
    fs::create_dir_all(path_lease_dir(common_git)).map_err(|source| LeaseError::Io {
        path: path_lease_dir(common_git),
        source,
    })?;
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|source| LeaseError::Io {
            path: path.to_path_buf(),
            source,
        })
}

fn write_holder(file: &mut File, path: &Path, holder: &PathLeaseHolder) -> Result<(), LeaseError> {
    let payload = serde_json::to_string(holder).map_err(|source| LeaseError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(ErrorKind::InvalidData, source),
    })?;
    file.set_len(0).map_err(|source| LeaseError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    write!(file, "v{LEASE_SCHEMA_VERSION}\n{payload}\n").map_err(|source| LeaseError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    file.sync_all().map_err(|source| LeaseError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

fn parse_holder_bytes(bytes: &[u8]) -> Option<PathLeaseHolder> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut lines = text.lines();
    let version = lines.next()?.trim();
    if version != format!("v{LEASE_SCHEMA_VERSION}") {
        return None;
    }
    let json = lines.next()?.trim();
    serde_json::from_str(json).ok()
}

fn is_lock_conflict(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::AlreadyExists
    ) || error.raw_os_error() == Some(35)
        || error.raw_os_error() == Some(11)
        || error.raw_os_error() == Some(16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn holder(label: &str) -> PathLeaseHolder {
        PathLeaseHolder {
            root_event_id: "a".repeat(64),
            label: label.into(),
        }
    }

    fn other_holder() -> PathLeaseHolder {
        PathLeaseHolder {
            root_event_id: "b".repeat(64),
            label: "Fix reconnect".into(),
        }
    }

    #[test]
    fn exclusive_path_lease_serializes_two_threads() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let checkout = temp.path().join("repo");
        let first = try_acquire_path_exclusive(common, &checkout, &holder("one")).unwrap();
        let err = try_acquire_path_exclusive(common, &checkout, &other_holder()).unwrap_err();
        assert!(matches!(err, LeaseError::Busy));
        let stored = read_path_lease_holder(common, &checkout).expect("holder written");
        assert_eq!(stored.label, "one");
        drop(first);
        let second = try_acquire_path_exclusive(common, &checkout, &other_holder()).unwrap();
        drop(second);
    }

    #[test]
    fn distinct_paths_do_not_conflict() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let a = try_acquire_path_exclusive(common, &temp.path().join("a"), &holder("a")).unwrap();
        let b =
            try_acquire_path_exclusive(common, &temp.path().join("b"), &other_holder()).unwrap();
        drop(a);
        drop(b);
    }

    #[test]
    fn invalid_root_fails_closed() {
        let temp = TempDir::new().unwrap();
        let holder = PathLeaseHolder {
            root_event_id: "short".into(),
            label: "x".into(),
        };
        let err = try_acquire_path_exclusive(temp.path(), temp.path(), &holder).unwrap_err();
        assert!(matches!(err, LeaseError::InvalidIdentity(_)));
    }
}
