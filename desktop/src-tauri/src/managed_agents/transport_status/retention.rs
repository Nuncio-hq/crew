//! Bounded cache hygiene, never authority for a process or status projection.
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PreflightError {
    /// The status sidechannel cannot be established in the current app-data
    /// environment. The managed process may still run without diagnostics.
    Unavailable,
    /// Existing status data violates the bounded storage contract and must be
    /// surfaced before starting another generation.
    Refused,
}

#[cfg(unix)]
pub(super) fn preflight(
    directory: &Path,
    runtime_id: &str,
    protected: &HashSet<String>,
    now_secs: u64,
) -> Result<(), PreflightError> {
    platform::preflight(directory, runtime_id, protected, now_secs)
}

#[cfg(not(unix))]
pub(super) fn preflight(
    _directory: &Path,
    _runtime_id: &str,
    _protected: &HashSet<String>,
    _now_secs: u64,
) -> Result<(), PreflightError> {
    Err(PreflightError::Refused)
}

#[cfg(unix)]
mod platform {
    use super::super::reader;
    use super::*;
    use buzz_core_pkg::transport_status::{
        decode_transport_record, TransportRecordEnvelope, TransportState,
        GENERATION_ENTRY_RESERVATION, MAX_DIRECTORY_ENTRIES,
    };
    use fs4::fs_std::FileExt;
    use nix::dir::Dir;
    use nix::fcntl::{AtFlags, OFlag};
    use nix::sys::stat::{fchmod, fstat, fstatat, FileStat, Mode};
    use nix::unistd::{fsync, geteuid, unlinkat, UnlinkatFlags};
    use std::ffi::{OsStr, OsString};
    use std::os::unix::ffi::OsStringExt;

    const HISTORY_MAX_AGE: u64 = 24 * 60 * 60;

    struct Entry {
        name: OsString,
        metadata: FileStat,
    }

    /// Owns the advisory lock for the duration of one retention transaction.
    ///
    /// On Linux, `flock` is attached to the open-file description. A child
    /// forked while this guard is live can therefore keep the lock after the
    /// guard's file is closed. Explicitly unlocking before the guard is
    /// dropped releases that description-wide lock even if a duplicate was
    /// inherited by a child during the fork/exec window.
    pub(super) struct DirectoryLock {
        file: std::fs::File,
        released: bool,
    }

    impl DirectoryLock {
        fn release(&mut self) -> Result<(), String> {
            if self.released {
                return Ok(());
            }
            fs4::fs_std::FileExt::unlock(&self.file)
                .map_err(|_| "cannot release diagnostics lock".to_string())?;
            self.released = true;
            Ok(())
        }

        #[cfg(test)]
        pub(super) fn try_clone_file(&self) -> std::io::Result<std::fs::File> {
            self.file.try_clone()
        }
    }

    impl Drop for DirectoryLock {
        fn drop(&mut self) {
            // The transaction already reports its own error. Keep this
            // best-effort fallback for early returns and inherited fds.
            let _ = self.release();
        }
    }

    pub(super) fn lock(directory: &Dir) -> Result<DirectoryLock, String> {
        let name = OsStr::new(".spool.lock");
        let flags = OFlag::O_RDWR | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK;
        let (descriptor, created) = match nix::fcntl::openat(
            directory,
            name,
            flags | OFlag::O_CREAT | OFlag::O_EXCL,
            Mode::S_IRUSR | Mode::S_IWUSR,
        ) {
            Ok(descriptor) => (descriptor, true),
            Err(nix::errno::Errno::EEXIST) => (
                nix::fcntl::openat(directory, name, flags, Mode::empty())
                    .map_err(|_| "cannot open diagnostics lock")?,
                false,
            ),
            Err(_) => return Err("cannot create diagnostics lock".into()),
        };
        if created {
            fchmod(&descriptor, Mode::S_IRUSR | Mode::S_IWUSR)
                .map_err(|_| "cannot secure diagnostics lock")?;
        }
        let metadata = fstat(&descriptor).map_err(|_| "cannot inspect diagnostics lock")?;
        reader::validate_file_metadata(&metadata, geteuid().as_raw())?;
        if metadata.st_size != 0 {
            return Err("diagnostics lock must be empty".into());
        }
        if created {
            fsync(&descriptor)
                .and_then(|_| fsync(directory))
                .map_err(|_| "cannot commit diagnostics lock")?;
        }
        let file = std::fs::File::from(descriptor);
        file.try_lock_exclusive()
            .map_err(|_| "diagnostics lock is busy")?;
        Ok(DirectoryLock {
            file,
            released: false,
        })
    }

    fn inspect(directory: &mut Dir) -> Result<Vec<Entry>, String> {
        let mut names = Vec::new();
        for entry in directory.iter() {
            let entry = entry.map_err(|_| "cannot enumerate diagnostics")?;
            let name = entry.file_name().to_bytes();
            if name == b"." || name == b".." || name == b".spool.lock" {
                continue;
            }
            if names.len() >= MAX_DIRECTORY_ENTRIES {
                return Err("diagnostics exceed entry capacity".into());
            }
            names.push(OsString::from_vec(name.to_vec()));
        }
        let mut entries = Vec::with_capacity(names.len());
        for name in names {
            let descriptor = nix::fcntl::openat(
                &*directory,
                name.as_os_str(),
                OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
                Mode::empty(),
            )
            .map_err(|_| "diagnostics entry is unsafe")?;
            let metadata = fstat(&descriptor).map_err(|_| "cannot inspect diagnostics entry")?;
            reader::validate_file_metadata(&metadata, geteuid().as_raw())?;
            entries.push(Entry { name, metadata });
        }
        Ok(entries)
    }

    fn canonical_nonce(value: &str) -> bool {
        uuid::Uuid::parse_str(value).is_ok_and(|nonce| {
            !nonce.is_nil() && (nonce.simple().to_string() == value || nonce.to_string() == value)
        })
    }

    fn staging_nonce(name: &str) -> Option<&str> {
        let value = name.strip_prefix('.')?;
        let nonce = value
            .strip_suffix(".pending")
            .or_else(|| value.strip_suffix(".tmp"))?;
        canonical_nonce(nonce).then_some(nonce)
    }

    fn record_nonce(name: &str) -> Option<&str> {
        let nonce = name.strip_suffix(".json")?;
        canonical_nonce(nonce).then_some(nonce)
    }

    fn valid_record(record: &TransportRecordEnvelope, runtime_id: &str, nonce: &str) -> bool {
        (matches!(record, TransportRecordEnvelope::V1(record) if record.version == 1)
            || matches!(record, TransportRecordEnvelope::V2(record) if record.version == 2))
            && record.sequence() > 0
            && record.runtime_id() == runtime_id
            && record.start_nonce() == nonce
            && record.transport().has_safe_error()
            && record
                .v2()
                .is_none_or(|record| record.validate_shape().is_ok())
            && (!record.terminal()
                || matches!(
                    record.transport().state,
                    TransportState::Exhausted
                        | TransportState::AuthRejected
                        | TransportState::Unknown
                ))
    }

    pub(super) fn preflight(
        path: &Path,
        runtime_id: &str,
        protected: &HashSet<String>,
        now_secs: u64,
    ) -> Result<(), super::PreflightError> {
        let mut directory = reader::open_owned_directory(path, true).map_err(|error| {
            if reader::is_storage_unavailable(&error) {
                super::PreflightError::Unavailable
            } else {
                super::PreflightError::Refused
            }
        })?;
        let result = (|| -> Result<(), String> {
            let mut lock = lock(&directory)?;
            let entries = inspect(&mut directory)?;
            let mut remove = HashSet::new();
            let mut records = Vec::new();
            for (index, entry) in entries.iter().enumerate() {
                let Some(name) = entry.name.to_str() else {
                    continue;
                };
                if let Some(nonce) = staging_nonce(name) {
                    if !protected.contains(nonce) {
                        remove.insert(index);
                    }
                    continue;
                }
                let Some(nonce) = record_nonce(name) else {
                    continue;
                };
                if protected.contains(nonce) {
                    continue;
                }
                let bytes = reader::read_owned_entry(&directory, &entry.name)?;
                let Ok(record) = decode_transport_record(&bytes) else {
                    continue;
                };
                if valid_record(&record, runtime_id, nonce) {
                    records.push(index);
                }
            }
            // A status record retained for historical inspection does not acquire
            // a native ticket. Keep at most one recent unprotected record, in
            // addition to the registered/last-required nonce exclusions.
            records.sort_by(|left, right| {
                let left = &entries[*left];
                let right = &entries[*right];
                (
                    right.metadata.st_mtime,
                    right.metadata.st_mtime_nsec,
                    &right.name,
                )
                    .cmp(&(
                        left.metadata.st_mtime,
                        left.metadata.st_mtime_nsec,
                        &left.name,
                    ))
            });
            let recent = records.first().copied().filter(|index| {
                let modified = u64::try_from(entries[*index].metadata.st_mtime).unwrap_or(0);
                modified <= now_secs.saturating_add(5)
                    && now_secs.saturating_sub(modified) < HISTORY_MAX_AGE
            });
            remove.extend(records.into_iter().filter(|index| Some(*index) != recent));
            // Required state and unknown files are never sacrificed to make room.
            if entries.len().saturating_sub(remove.len()) + GENERATION_ENTRY_RESERVATION
                > MAX_DIRECTORY_ENTRIES
            {
                return Err("diagnostics require manual capacity recovery".into());
            }
            for index in remove {
                let entry = &entries[index];
                let current = fstatat(
                    &directory,
                    entry.name.as_os_str(),
                    AtFlags::AT_SYMLINK_NOFOLLOW,
                )
                .map_err(|_| "diagnostics changed before cleanup")?;
                if current.st_ino != entry.metadata.st_ino
                    || current.st_dev != entry.metadata.st_dev
                {
                    return Err("diagnostics changed before cleanup".into());
                }
                reader::validate_file_metadata(&current, geteuid().as_raw())?;
                unlinkat(
                    &directory,
                    entry.name.as_os_str(),
                    UnlinkatFlags::NoRemoveDir,
                )
                .map_err(|_| "cannot reclaim diagnostics")?;
            }
            fsync(&directory).map_err(|_| "cannot commit diagnostics cleanup")?;
            lock.release()?;
            Ok(())
        })();
        result.map_err(|_| super::PreflightError::Refused)
    }
}

#[cfg(all(test, unix))]
#[path = "retention/tests.rs"]
mod tests;
