//! Cross-process shared/exclusive advisory leases for Project worktrees.
//!
//! Uses the `fs4` crate (MIT/Apache-2.0) for flock-style locks compatible with
//! the workspace MSRV (Rust 1.88). Shared holders (active ACP turns) coexist;
//! exclusive holders (eviction/cache reclaim) refuse while any shared holder
//! exists, and shared acquisition refuses while exclusive is held.
//!
//! No home-grown PID lockfile protocol and no `unsafe` in this crate.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use fs4::fs_std::FileExt;

use crate::error::LeaseError;
use crate::identity::validate_root_event_id;
use crate::paths::{lease_dir, lease_lock_path, LEASE_SCHEMA_VERSION};

/// Shared (active-turn) lease guard. Releases on drop.
#[derive(Debug)]
pub struct SharedLease {
    _file: File,
    path: PathBuf,
}

impl SharedLease {
    /// Path of the underlying lockfile.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SharedLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self._file);
    }
}

/// Exclusive (eviction) lease guard. Releases on drop.
#[derive(Debug)]
pub struct ExclusiveLease {
    _file: File,
    path: PathBuf,
}

impl ExclusiveLease {
    /// Path of the underlying lockfile.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ExclusiveLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self._file);
    }
}

/// Non-blocking shared lease for an active Project turn.
pub fn try_acquire_shared(
    common_git: &Path,
    root_event_id: &str,
) -> Result<SharedLease, LeaseError> {
    let root = validate_root_event_id(root_event_id)?;
    let path = lease_lock_path(common_git, &root).map_err(LeaseError::InvalidIdentity)?;
    let file = open_lease_file(common_git, &path)?;
    // UFCS: avoid std::fs::File inherent try_lock_* (post-1.88) shadowing fs4.
    match FileExt::try_lock_shared(&file) {
        Ok(()) => Ok(SharedLease { _file: file, path }),
        Err(error) if is_lock_conflict(&error) => Err(LeaseError::Busy),
        Err(error) => Err(LeaseError::Io {
            path,
            source: error,
        }),
    }
}

/// Non-blocking exclusive lease for cache reclaim or checkout eviction.
pub fn try_acquire_exclusive(
    common_git: &Path,
    root_event_id: &str,
) -> Result<ExclusiveLease, LeaseError> {
    let root = validate_root_event_id(root_event_id)?;
    let path = lease_lock_path(common_git, &root).map_err(LeaseError::InvalidIdentity)?;
    let file = open_lease_file(common_git, &path)?;
    match FileExt::try_lock_exclusive(&file) {
        Ok(()) => Ok(ExclusiveLease { _file: file, path }),
        Err(error) if is_lock_conflict(&error) => Err(LeaseError::Busy),
        Err(error) => Err(LeaseError::Io {
            path,
            source: error,
        }),
    }
}

fn open_lease_file(common_git: &Path, path: &Path) -> Result<File, LeaseError> {
    fs::create_dir_all(lease_dir(common_git)).map_err(|source| LeaseError::Io {
        path: lease_dir(common_git),
        source,
    })?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|source| LeaseError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    ensure_lease_version(&mut file, path)?;
    Ok(file)
}

fn ensure_lease_version(file: &mut File, path: &Path) -> Result<(), LeaseError> {
    let mut buf = String::new();
    file.read_to_string(&mut buf)
        .map_err(|source| LeaseError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let trimmed = buf.trim();
    if trimmed.is_empty() {
        file.set_len(0).map_err(|source| LeaseError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        writeln!(file, "v{LEASE_SCHEMA_VERSION}").map_err(|source| LeaseError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        file.sync_all().map_err(|source| LeaseError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        return Ok(());
    }
    let Some(version) = trimmed
        .strip_prefix('v')
        .and_then(|rest| rest.parse::<u32>().ok())
    else {
        return Err(LeaseError::UnsupportedVersion {
            found: 0,
            supported: LEASE_SCHEMA_VERSION,
        });
    };
    if version != LEASE_SCHEMA_VERSION {
        return Err(LeaseError::UnsupportedVersion {
            found: version,
            supported: LEASE_SCHEMA_VERSION,
        });
    }
    Ok(())
}

fn is_lock_conflict(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::AlreadyExists
    ) || error.raw_os_error() == Some(35) // EAGAIN on some platforms
        || error.raw_os_error() == Some(11) // EAGAIN/EWOULDBLOCK
        || error.raw_os_error() == Some(16) // EBUSY
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn root() -> String {
        "a".repeat(64)
    }

    #[test]
    fn multiple_shared_holders_coexist() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let a = try_acquire_shared(common, &root()).expect("shared a");
        let b = try_acquire_shared(common, &root()).expect("shared b");
        drop(a);
        drop(b);
    }

    #[test]
    fn exclusive_refuses_while_shared_held() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let shared = try_acquire_shared(common, &root()).unwrap();
        let err = try_acquire_exclusive(common, &root()).unwrap_err();
        assert!(matches!(err, LeaseError::Busy));
        drop(shared);
        let exclusive = try_acquire_exclusive(common, &root()).unwrap();
        drop(exclusive);
    }

    #[test]
    fn shared_refuses_while_exclusive_held() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let exclusive = try_acquire_exclusive(common, &root()).unwrap();
        let err = try_acquire_shared(common, &root()).unwrap_err();
        assert!(matches!(err, LeaseError::Busy));
        drop(exclusive);
    }

    #[test]
    fn locks_release_on_drop() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        {
            let _exclusive = try_acquire_exclusive(common, &root()).unwrap();
        }
        let shared = try_acquire_shared(common, &root()).unwrap();
        drop(shared);
    }

    #[test]
    fn invalid_root_fails_closed() {
        let temp = TempDir::new().unwrap();
        let err = try_acquire_shared(temp.path(), "short").unwrap_err();
        assert!(matches!(err, LeaseError::InvalidIdentity(_)));
    }

    #[test]
    fn unsupported_lease_version_fails_closed() {
        let temp = TempDir::new().unwrap();
        let common = temp.path();
        let path = lease_lock_path(common, &root()).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "v999\n").unwrap();
        let err = try_acquire_shared(common, &root()).unwrap_err();
        assert!(matches!(
            err,
            LeaseError::UnsupportedVersion {
                found: 999,
                supported: LEASE_SCHEMA_VERSION
            }
        ));
    }
}
