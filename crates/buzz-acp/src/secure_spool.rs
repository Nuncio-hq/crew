use std::ffi::{OsStr, OsString};
#[cfg(unix)]
use std::io::{Read, Write};
use std::path::{Component, Path};

pub(crate) const SECURE_SPOOL_LOCK_CONTENDED: &str = "durable spool lock is already held";

#[derive(Debug)]
pub(crate) struct SecureSpoolEntry {
    pub(crate) name: OsString,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct ClaimedSecureSpoolEntry {
    pub(crate) name: OsString,
    pub(crate) bytes: Vec<u8>,
    _claim: std::fs::File,
}

#[derive(Debug)]
pub(crate) struct ClaimedSecureSpoolEntries {
    pub(crate) entries: Vec<ClaimedSecureSpoolEntry>,
    pub(crate) failures: Vec<(OsString, String)>,
    pub(crate) skipped_contended: usize,
    pub(crate) attempted: usize,
    pub(crate) total_matching: usize,
    pub(crate) scan_has_more: bool,
}

#[derive(Debug)]
pub(crate) struct SecureSpoolDirectoryLock {
    _claim: std::fs::File,
}

#[derive(Debug)]
pub(crate) struct SecureSpoolEntryLease {
    _claim: std::fs::File,
}

async fn run_blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| format!("durable spool operation panicked: {error}"))?
}

pub(crate) async fn ensure_secure_directory(path: &Path) -> Result<(), String> {
    let path = path.to_owned();
    run_blocking(move || platform::ensure_secure_directory(&path)).await
}

pub(crate) async fn measure_secure_directory(
    path: &Path,
    max_entry_bytes: u64,
    max_entries: usize,
) -> Result<(usize, u64), String> {
    let path = path.to_owned();
    run_blocking(move || platform::measure_secure_directory(&path, max_entry_bytes, max_entries))
        .await
}

pub(crate) async fn lock_secure_directory(path: &Path) -> Result<SecureSpoolDirectoryLock, String> {
    let path = path.to_owned();
    run_blocking(move || {
        platform::lock_secure_directory(&path)
            .map(|claim| SecureSpoolDirectoryLock { _claim: claim })
    })
    .await
}

pub(crate) async fn lock_secure_entry_lease(
    path: &Path,
    name: &OsStr,
) -> Result<SecureSpoolEntryLease, String> {
    let path = path.to_owned();
    let name = name.to_owned();
    run_blocking(move || {
        platform::lock_secure_named_file(&path, &name)
            .map(|claim| SecureSpoolEntryLease { _claim: claim })
    })
    .await
}

pub(crate) async fn read_secure_entries(
    path: &Path,
    extension: &str,
    max_entry_bytes: u64,
) -> Result<Vec<SecureSpoolEntry>, String> {
    let path = path.to_owned();
    let extension = extension.to_owned();
    run_blocking(move || platform::read_secure_entries(&path, &extension, max_entry_bytes)).await
}

pub(crate) async fn read_secure_entry(
    path: &Path,
    name: &OsStr,
    max_entry_bytes: u64,
) -> Result<Option<Vec<u8>>, String> {
    let path = path.to_owned();
    let name = name.to_owned();
    run_blocking(move || platform::read_secure_entry(&path, &name, max_entry_bytes)).await
}

#[cfg(test)]
pub(crate) async fn claim_secure_entries(
    path: &Path,
    extension: &str,
    max_entry_bytes: u64,
) -> Result<ClaimedSecureSpoolEntries, String> {
    let path = path.to_owned();
    let extension = extension.to_owned();
    run_blocking(move || platform::claim_secure_entries(&path, &extension, max_entry_bytes)).await
}

pub(crate) async fn claim_secure_entries_bounded(
    path: &Path,
    extension: &str,
    max_entry_bytes: u64,
    cursor: usize,
    max_entries: usize,
) -> Result<ClaimedSecureSpoolEntries, String> {
    let path = path.to_owned();
    let extension = extension.to_owned();
    run_blocking(move || {
        platform::claim_secure_entries_bounded(
            &path,
            &extension,
            max_entry_bytes,
            cursor,
            max_entries,
        )
    })
    .await
}

pub(crate) async fn claim_secure_named_entries(
    path: &Path,
    extension: &str,
    max_entry_bytes: u64,
    names: &[OsString],
    max_entries: usize,
) -> Result<ClaimedSecureSpoolEntries, String> {
    let path = path.to_owned();
    let extension = extension.to_owned();
    let names = names.to_vec();
    run_blocking(move || {
        platform::claim_secure_named_entries(
            &path,
            &extension,
            max_entry_bytes,
            &names,
            max_entries,
        )
    })
    .await
}

/// Remove bounded crash-residue temporary files while the caller owns the
/// containing spool's root lock. Returns `false` when contended or excess
/// candidates remain for a later pass.
pub(crate) async fn cleanup_secure_temporary_entries(
    path: &Path,
    max_entry_bytes: u64,
    max_entries: usize,
) -> Result<bool, String> {
    let claimed =
        claim_secure_entries_bounded(path, "tmp", max_entry_bytes, 0, max_entries).await?;
    let attempted = claimed.entries.len().saturating_add(claimed.failures.len());
    let complete = claimed.skipped_contended == 0 && attempted == claimed.total_matching;
    for entry in &claimed.entries {
        remove_secure_entry(path, &entry.name).await?;
    }
    for (name, _) in &claimed.failures {
        remove_secure_entry(path, name).await?;
    }
    Ok(complete)
}

pub(crate) async fn write_secure_entry_if_absent(
    path: &Path,
    name: &OsStr,
    temporary_name: &OsStr,
    bytes: &[u8],
) -> Result<bool, String> {
    let path = path.to_owned();
    let name = name.to_owned();
    let temporary_name = temporary_name.to_owned();
    let bytes = bytes.to_vec();
    run_blocking(move || {
        platform::write_secure_entry_if_absent(&path, &name, &temporary_name, &bytes)
    })
    .await
}

pub(crate) async fn remove_secure_entry(path: &Path, name: &OsStr) -> Result<bool, String> {
    let path = path.to_owned();
    let name = name.to_owned();
    run_blocking(move || platform::remove_secure_entry(&path, &name)).await
}

pub(crate) async fn rename_secure_entry(
    path: &Path,
    source: &OsStr,
    destination: &OsStr,
) -> Result<bool, String> {
    let path = path.to_owned();
    let source = source.to_owned();
    let destination = destination.to_owned();
    run_blocking(move || platform::rename_secure_entry(&path, &source, &destination)).await
}

#[cfg(unix)]
mod platform {
    use super::*;
    use nix::dir::Dir;
    use nix::errno::Errno;
    use nix::fcntl::{renameat, AtFlags, OFlag};
    use nix::sys::stat::{fchmod, fstat, mkdirat, Mode, SFlag};
    use nix::unistd::{fsync, geteuid, linkat, unlinkat, UnlinkatFlags};
    use std::fs::File;
    use std::os::unix::ffi::OsStringExt;

    const MAX_SECURE_DIRECTORY_ENUMERATION: usize = 4_099;

    fn directory_flags() -> OFlag {
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC
    }

    fn anchor_directory(path: &Path) -> Result<Dir, String> {
        if !path.is_absolute() {
            return Err(format!(
                "durable spool path must be absolute: {}",
                path.display()
            ));
        }
        let directory = Dir::open(Path::new("/"), directory_flags(), Mode::empty())
            .map_err(|error| format!("failed to anchor durable spool path: {error}"))?;
        validate_directory_component(&directory, path, OsStr::new("/"), false)?;
        Ok(directory)
    }

    fn path_components(path: &Path) -> Result<Vec<&OsStr>, String> {
        let components = path
            .components()
            .filter_map(|component| match component {
                Component::RootDir | Component::CurDir => None,
                Component::Normal(name) => Some(Ok(name)),
                Component::ParentDir | Component::Prefix(_) => Some(Err(format!(
                    "unsafe durable spool path component: {}",
                    path.display()
                ))),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if components.is_empty() {
            return Err("durable spool cannot use the filesystem root".to_string());
        }
        Ok(components)
    }

    fn validate_directory_component(
        directory: &Dir,
        path: &Path,
        name: &OsStr,
        is_leaf: bool,
    ) -> Result<(), String> {
        let metadata = fstat(directory)
            .map_err(|error| format!("failed to inspect durable spool component: {error}"))?;
        let file_type = SFlag::from_bits_truncate(metadata.st_mode);
        let mode = Mode::from_bits_truncate(metadata.st_mode);
        let permissions = mode & (Mode::S_IRWXU | Mode::S_IRWXG | Mode::S_IRWXO);
        let owner = metadata.st_uid;
        let current_owner = geteuid().as_raw();
        if !file_type.contains(SFlag::S_IFDIR) || (owner != 0 && owner != current_owner) {
            return Err(format!(
                "unsafe durable spool component: {}",
                name.to_string_lossy()
            ));
        }
        if is_leaf {
            if owner != current_owner || permissions != Mode::S_IRWXU {
                return Err(format!(
                    "durable spool directory must be an owner-owned 0700 directory: {}",
                    path.display()
                ));
            }
        } else if (permissions & (Mode::S_IWGRP | Mode::S_IWOTH)) != Mode::empty()
            && !(owner == 0 && mode.contains(Mode::S_ISVTX))
        {
            return Err(format!(
                "writable durable spool ancestor is not trusted: {}",
                name.to_string_lossy()
            ));
        }
        Ok(())
    }

    fn open_directory_chain(path: &Path) -> Result<Dir, String> {
        let mut directory = anchor_directory(path)?;
        let components = path_components(path)?;
        for (index, name) in components.iter().enumerate() {
            directory = Dir::openat(&directory, *name, directory_flags(), Mode::empty())
                .map_err(|error| {
                    format!(
                        "failed to open durable spool component {} without following links: {error}",
                        name.to_string_lossy()
                    )
                })?;
            validate_directory_component(&directory, path, name, index + 1 == components.len())?;
        }
        Ok(directory)
    }

    fn ensure_directory_chain(path: &Path) -> Result<Dir, String> {
        let mut directory = anchor_directory(path)?;
        let components = path_components(path)?;
        for (index, name) in components.iter().enumerate() {
            let (next, created) = match Dir::openat(
                &directory,
                *name,
                directory_flags(),
                Mode::empty(),
            ) {
                Ok(next) => (next, false),
                Err(Errno::ENOENT) => {
                    let created = match mkdirat(&directory, *name, Mode::S_IRWXU) {
                        Ok(()) => true,
                        Err(Errno::EEXIST) => false,
                        Err(error) => {
                            return Err(format!(
                                "failed to create durable spool component {}: {error}",
                                name.to_string_lossy()
                            ));
                        }
                    };
                    if created {
                        fsync(&directory).map_err(|error| {
                            format!("failed to commit durable spool directory creation: {error}")
                        })?;
                    }
                    let next = Dir::openat(&directory, *name, directory_flags(), Mode::empty())
                        .map_err(|error| {
                            format!(
                                "failed to open newly created durable spool component {}: {error}",
                                name.to_string_lossy()
                            )
                        })?;
                    if created {
                        fchmod(&next, Mode::S_IRWXU).map_err(|error| {
                            format!(
                                "failed to secure newly created durable spool component {}: {error}",
                                name.to_string_lossy()
                            )
                        })?;
                    }
                    (next, created)
                }
                Err(error) => {
                    return Err(format!(
                        "failed to open durable spool component {} without following links: {error}",
                        name.to_string_lossy()
                    ));
                }
            };
            directory = next;
            validate_directory_component(&directory, path, name, index + 1 == components.len())?;
            if created {
                fsync(&directory).map_err(|error| {
                    format!("failed to commit durable spool permissions: {error}")
                })?;
            }
        }
        Ok(directory)
    }

    fn validate_leaf_directory(directory: &Dir, path: &Path) -> Result<(), String> {
        let metadata = fstat(directory)
            .map_err(|error| format!("failed to inspect durable spool directory: {error}"))?;
        if !SFlag::from_bits_truncate(metadata.st_mode).contains(SFlag::S_IFDIR)
            || metadata.st_uid != geteuid().as_raw()
            || Mode::from_bits_truncate(metadata.st_mode)
                & (Mode::S_IRWXU | Mode::S_IRWXG | Mode::S_IRWXO)
                != Mode::S_IRWXU
        {
            return Err(format!(
                "durable spool directory must be an owner-owned 0700 directory: {}",
                path.display()
            ));
        }
        Ok(())
    }

    pub(super) fn ensure_secure_directory(path: &Path) -> Result<(), String> {
        let directory = ensure_directory_chain(path)?;
        validate_leaf_directory(&directory, path)
    }

    fn open_regular_file(
        directory: &Dir,
        name: &OsStr,
        max_entry_bytes: u64,
    ) -> Result<File, String> {
        let descriptor = nix::fcntl::openat(
            directory,
            name,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
            Mode::empty(),
        )
        .map_err(|error| {
            format!(
                "failed to open durable spool entry {} without following links: {error}",
                name.to_string_lossy()
            )
        })?;
        let metadata = fstat(&descriptor).map_err(|error| {
            format!(
                "failed to inspect durable spool entry {}: {error}",
                name.to_string_lossy()
            )
        })?;
        if !SFlag::from_bits_truncate(metadata.st_mode).contains(SFlag::S_IFREG)
            || metadata.st_uid != geteuid().as_raw()
            || Mode::from_bits_truncate(metadata.st_mode)
                & (Mode::S_IRWXU | Mode::S_IRWXG | Mode::S_IRWXO)
                != (Mode::S_IRUSR | Mode::S_IWUSR)
            || metadata.st_size < 0
            || metadata.st_size as u64 > max_entry_bytes
        {
            return Err(format!(
                "durable spool entry must be an owner-owned bounded 0600 regular file: {}",
                name.to_string_lossy()
            ));
        }
        Ok(File::from(descriptor))
    }

    fn read_bounded_file(
        file: &mut File,
        name: &OsStr,
        max_entry_bytes: u64,
    ) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        file.take(max_entry_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| {
                format!(
                    "failed to read durable spool entry {}: {error}",
                    name.to_string_lossy()
                )
            })?;
        if bytes.len() as u64 > max_entry_bytes {
            return Err(format!(
                "durable spool entry exceeds the byte limit: {}",
                name.to_string_lossy()
            ));
        }
        Ok(bytes)
    }

    fn open_validated_directory(path: &Path) -> Result<Dir, String> {
        let directory = open_directory_chain(path)?;
        validate_leaf_directory(&directory, path)?;
        Ok(directory)
    }

    fn validate_entry_name(name: &OsStr) -> Result<(), String> {
        let mut components = Path::new(name).components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err(format!(
                "unsafe durable spool entry name: {}",
                name.to_string_lossy()
            ));
        }
        Ok(())
    }

    pub(super) fn write_secure_entry_if_absent(
        path: &Path,
        name: &OsStr,
        temporary_name: &OsStr,
        bytes: &[u8],
    ) -> Result<bool, String> {
        validate_entry_name(name)?;
        validate_entry_name(temporary_name)?;
        let directory = open_validated_directory(path)?;
        let descriptor = nix::fcntl::openat(
            &directory,
            temporary_name,
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::S_IRUSR | Mode::S_IWUSR,
        )
        .map_err(|error| {
            format!(
                "failed to securely create durable spool entry {}: {error}",
                temporary_name.to_string_lossy()
            )
        })?;
        fchmod(&descriptor, Mode::S_IRUSR | Mode::S_IWUSR).map_err(|error| {
            format!(
                "failed to secure durable spool entry {}: {error}",
                temporary_name.to_string_lossy()
            )
        })?;
        let mut file = File::from(descriptor);
        let write_result = file
            .write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| {
                format!(
                    "failed to persist durable spool entry {}: {error}",
                    temporary_name.to_string_lossy()
                )
            });
        drop(file);
        if let Err(error) = write_result {
            let _ = unlinkat(&directory, temporary_name, UnlinkatFlags::NoRemoveDir);
            return Err(error);
        }
        let linked = match linkat(
            &directory,
            temporary_name,
            &directory,
            name,
            AtFlags::empty(),
        ) {
            Ok(()) => true,
            Err(nix::errno::Errno::EEXIST) => false,
            Err(error) => {
                let _ = unlinkat(&directory, temporary_name, UnlinkatFlags::NoRemoveDir);
                return Err(format!(
                    "failed to commit durable spool entry {}: {error}",
                    name.to_string_lossy()
                ));
            }
        };
        if linked {
            fsync(&directory).map_err(|error| {
                format!("failed to commit durable spool entry directory metadata: {error}")
            })?;
        }
        unlinkat(&directory, temporary_name, UnlinkatFlags::NoRemoveDir).map_err(|error| {
            format!(
                "failed to remove temporary durable spool entry {}: {error}",
                temporary_name.to_string_lossy()
            )
        })?;
        fsync(&directory)
            .map_err(|error| format!("failed to sync durable spool directory: {error}"))?;
        Ok(linked)
    }

    pub(super) fn remove_secure_entry(path: &Path, name: &OsStr) -> Result<bool, String> {
        validate_entry_name(name)?;
        match std::fs::symlink_metadata(path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                return Err(format!(
                    "failed to inspect durable spool directory {}: {error}",
                    path.display()
                ));
            }
        }
        let directory = open_validated_directory(path)?;
        match unlinkat(&directory, name, UnlinkatFlags::NoRemoveDir) {
            Ok(()) => {
                fsync(&directory)
                    .map_err(|error| format!("failed to sync durable spool directory: {error}"))?;
                Ok(true)
            }
            Err(nix::errno::Errno::ENOENT) => Ok(false),
            Err(error) => Err(format!(
                "failed to remove durable spool entry {}: {error}",
                name.to_string_lossy()
            )),
        }
    }

    pub(super) fn rename_secure_entry(
        path: &Path,
        source: &OsStr,
        destination: &OsStr,
    ) -> Result<bool, String> {
        validate_entry_name(source)?;
        validate_entry_name(destination)?;
        let directory = open_validated_directory(path)?;
        match renameat(&directory, source, &directory, destination) {
            Ok(()) => {
                fsync(&directory)
                    .map_err(|error| format!("failed to sync durable spool directory: {error}"))?;
                Ok(true)
            }
            Err(nix::errno::Errno::ENOENT) => Ok(false),
            Err(error) => Err(format!(
                "failed to rename durable spool entry {} to {}: {error}",
                source.to_string_lossy(),
                destination.to_string_lossy()
            )),
        }
    }

    pub(super) fn measure_secure_directory(
        path: &Path,
        max_entry_bytes: u64,
        max_entries: usize,
    ) -> Result<(usize, u64), String> {
        fn measure_recursive(
            mut directory: Dir,
            max_entry_bytes: u64,
            max_entries: usize,
            depth: usize,
            count: &mut usize,
            total: &mut u64,
        ) -> Result<(usize, u64), String> {
            if depth > 4 {
                return Err("durable spool directory nesting exceeds the safety limit".to_owned());
            }
            let remaining = max_entries.saturating_sub(*count).saturating_add(1);
            let scan_limit = remaining.saturating_add(3);
            let mut names = Vec::with_capacity(scan_limit.min(4_096));
            for entry in directory.iter() {
                if names.len() >= scan_limit {
                    *count = max_entries.saturating_add(1);
                    return Ok((*count, *total));
                }
                names.push(
                    entry
                        .map(|entry| entry.file_name().to_bytes().to_vec())
                        .map_err(|error| format!("failed to enumerate durable spool: {error}"))?,
                );
            }
            for name in names {
                if name == b"." || name == b".." {
                    continue;
                }
                let name = OsString::from_vec(name);
                if name == OsStr::new(".spool.lock") {
                    continue;
                }
                let fd = nix::fcntl::openat(
                    &directory,
                    name.as_os_str(),
                    OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
                    Mode::empty(),
                )
                .map_err(|error| {
                    format!(
                        "failed to inspect durable spool entry {} without following links: {error}",
                        name.to_string_lossy()
                    )
                })?;
                let metadata = fstat(&fd).map_err(|error| {
                    format!(
                        "failed to inspect durable spool entry {}: {error}",
                        name.to_string_lossy()
                    )
                })?;
                let file_type = SFlag::from_bits_truncate(metadata.st_mode);
                if file_type.contains(SFlag::S_IFDIR) {
                    let permissions = Mode::from_bits_truncate(metadata.st_mode)
                        & (Mode::S_IRWXU | Mode::S_IRWXG | Mode::S_IRWXO);
                    if metadata.st_uid != geteuid().as_raw() || permissions != Mode::S_IRWXU {
                        return Err(format!(
                            "durable spool subdirectory is not owner-only: {}",
                            name.to_string_lossy()
                        ));
                    }
                    *count = count.saturating_add(1);
                    if *count > max_entries {
                        return Ok((*count, *total));
                    }
                    drop(fd);
                    let nested = Dir::openat(
                        &directory,
                        name.as_os_str(),
                        directory_flags(),
                        Mode::empty(),
                    )
                    .map_err(|error| {
                        format!(
                            "failed to open durable spool subdirectory {}: {error}",
                            name.to_string_lossy()
                        )
                    })?;
                    measure_recursive(
                        nested,
                        max_entry_bytes,
                        max_entries,
                        depth + 1,
                        count,
                        total,
                    )?;
                    continue;
                }
                if !file_type.contains(SFlag::S_IFREG)
                    || metadata.st_uid != geteuid().as_raw()
                    || Mode::from_bits_truncate(metadata.st_mode)
                        & (Mode::S_IRWXU | Mode::S_IRWXG | Mode::S_IRWXO)
                        != (Mode::S_IRUSR | Mode::S_IWUSR)
                    || metadata.st_size < 0
                    || metadata.st_size as u64 > max_entry_bytes
                {
                    return Err(format!(
                        "durable spool entry must be an owner-owned bounded 0600 regular file: {}",
                        name.to_string_lossy()
                    ));
                }
                *count = count.saturating_add(1);
                *total = total.saturating_add(metadata.st_size.max(0) as u64);
                if *count > max_entries || *total > max_entry_bytes {
                    return Ok((*count, *total));
                }
            }
            Ok((*count, *total))
        }

        let directory = open_validated_directory(path)?;
        let mut count = 0;
        let mut total = 0;
        measure_recursive(
            directory,
            max_entry_bytes,
            max_entries,
            0,
            &mut count,
            &mut total,
        )
    }

    pub(super) fn lock_secure_directory(path: &Path) -> Result<File, String> {
        lock_secure_named_file(path, OsStr::new(".spool.lock"))
    }

    pub(super) fn lock_secure_named_file(path: &Path, name: &OsStr) -> Result<File, String> {
        use fs4::fs_std::FileExt;

        let directory = open_validated_directory(path)?;
        validate_entry_name(name)?;
        let (descriptor, created) = match nix::fcntl::openat(
            &directory,
            name,
            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::S_IRUSR | Mode::S_IWUSR,
        ) {
            Ok(descriptor) => (descriptor, true),
            Err(Errno::EEXIST) => (
                nix::fcntl::openat(
                    &directory,
                    name,
                    OFlag::O_RDWR | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|error| format!("failed to open durable spool lock: {error}"))?,
                false,
            ),
            Err(error) => return Err(format!("failed to create durable spool lock: {error}")),
        };
        if created {
            fchmod(&descriptor, Mode::S_IRUSR | Mode::S_IWUSR)
                .map_err(|error| format!("failed to secure durable spool lock: {error}"))?;
            fsync(&descriptor).map_err(|error| {
                format!("failed to commit durable spool lock metadata: {error}")
            })?;
        }
        let metadata = fstat(&descriptor)
            .map_err(|error| format!("failed to inspect durable spool lock: {error}"))?;
        let permissions = Mode::from_bits_truncate(metadata.st_mode)
            & (Mode::S_IRWXU | Mode::S_IRWXG | Mode::S_IRWXO);
        if !SFlag::from_bits_truncate(metadata.st_mode).contains(SFlag::S_IFREG)
            || metadata.st_uid != geteuid().as_raw()
            || permissions != (Mode::S_IRUSR | Mode::S_IWUSR)
            || metadata.st_size != 0
        {
            return Err("durable spool lock must be an owner-owned empty 0600 file".to_owned());
        }
        if created {
            fsync(&directory)
                .map_err(|error| format!("failed to commit durable spool lock: {error}"))?;
        }
        let file = File::from(descriptor);
        file.try_lock_exclusive().map_err(|error| {
            if error.raw_os_error() == fs4::lock_contended_error().raw_os_error() {
                SECURE_SPOOL_LOCK_CONTENDED.to_owned()
            } else {
                format!("failed to claim durable spool capacity lock: {error}")
            }
        })?;
        Ok(file)
    }

    pub(super) fn read_secure_entries(
        path: &Path,
        extension: &str,
        max_entry_bytes: u64,
    ) -> Result<Vec<SecureSpoolEntry>, String> {
        let mut directory = open_validated_directory(path)?;
        let names = directory
            .iter()
            .map(|entry| {
                entry
                    .map(|entry| entry.file_name().to_bytes().to_vec())
                    .map_err(|error| format!("failed to enumerate durable spool: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut entries = Vec::new();
        for name in names {
            if name == b"." || name == b".." {
                continue;
            }
            let name = OsString::from_vec(name);
            if Path::new(&name).extension().and_then(OsStr::to_str) != Some(extension) {
                continue;
            }
            let mut file = open_regular_file(&directory, &name, max_entry_bytes)?;
            let bytes = read_bounded_file(&mut file, &name, max_entry_bytes)?;
            entries.push(SecureSpoolEntry { name, bytes });
        }
        Ok(entries)
    }

    pub(super) fn read_secure_entry(
        path: &Path,
        name: &OsStr,
        max_entry_bytes: u64,
    ) -> Result<Option<Vec<u8>>, String> {
        validate_entry_name(name)?;
        let directory = open_validated_directory(path)?;
        let descriptor = match nix::fcntl::openat(
            &directory,
            name,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
            Mode::empty(),
        ) {
            Ok(descriptor) => descriptor,
            Err(nix::errno::Errno::ENOENT) => return Ok(None),
            Err(error) => {
                return Err(format!(
                    "failed to open durable spool entry {} without following links: {error}",
                    name.to_string_lossy()
                ));
            }
        };
        let metadata = fstat(&descriptor).map_err(|error| {
            format!(
                "failed to inspect durable spool entry {}: {error}",
                name.to_string_lossy()
            )
        })?;
        if !SFlag::from_bits_truncate(metadata.st_mode).contains(SFlag::S_IFREG)
            || metadata.st_uid != geteuid().as_raw()
            || Mode::from_bits_truncate(metadata.st_mode)
                & (Mode::S_IRWXU | Mode::S_IRWXG | Mode::S_IRWXO)
                != (Mode::S_IRUSR | Mode::S_IWUSR)
            || metadata.st_size < 0
            || metadata.st_size as u64 > max_entry_bytes
        {
            return Err(format!(
                "durable spool entry must be an owner-owned bounded 0600 regular file: {}",
                name.to_string_lossy()
            ));
        }
        let mut file = File::from(descriptor);
        read_bounded_file(&mut file, name, max_entry_bytes).map(Some)
    }

    #[cfg(test)]
    pub(super) fn claim_secure_entries(
        path: &Path,
        extension: &str,
        max_entry_bytes: u64,
    ) -> Result<ClaimedSecureSpoolEntries, String> {
        claim_secure_entries_bounded(path, extension, max_entry_bytes, 0, usize::MAX)
    }

    pub(super) fn claim_secure_entries_bounded(
        path: &Path,
        extension: &str,
        max_entry_bytes: u64,
        cursor: usize,
        max_entries: usize,
    ) -> Result<ClaimedSecureSpoolEntries, String> {
        use fs4::fs_std::FileExt;

        let mut directory = open_validated_directory(path)?;
        let mut enumerated = Vec::new();
        for entry in directory.iter() {
            if enumerated.len() >= MAX_SECURE_DIRECTORY_ENUMERATION {
                return Err(format!(
                    "durable spool exceeds the bounded directory scan of {MAX_SECURE_DIRECTORY_ENUMERATION} entries"
                ));
            }
            enumerated.push(
                entry
                    .map_err(|error| format!("failed to enumerate durable spool: {error}"))?
                    .file_name()
                    .to_bytes()
                    .to_vec(),
            );
        }
        let mut names = enumerated
            .into_iter()
            .filter(|name| {
                name != b"."
                    && name != b".."
                    && Path::new(&OsString::from_vec(name.clone()))
                        .extension()
                        .and_then(OsStr::to_str)
                        == Some(extension)
            })
            .collect::<Vec<_>>();
        names.sort();
        let total_matching = names.len();
        if !names.is_empty() {
            let offset = cursor % names.len();
            names.rotate_left(offset);
        }
        let mut entries = Vec::new();
        let mut failures = Vec::new();
        let mut skipped_contended = 0_usize;
        let attempted = names.len().min(max_entries);
        for name in names.into_iter().take(max_entries) {
            let name = OsString::from_vec(name);
            let mut file = match open_regular_file(&directory, &name, max_entry_bytes) {
                Ok(file) => file,
                Err(error) => {
                    failures.push((name, error));
                    continue;
                }
            };
            if let Err(error) = file.try_lock_exclusive() {
                if error.raw_os_error() == fs4::lock_contended_error().raw_os_error() {
                    skipped_contended = skipped_contended.saturating_add(1);
                    continue;
                }
                failures.push((
                    name,
                    format!("failed to claim durable spool entry: {error}"),
                ));
                continue;
            }
            let bytes = match read_bounded_file(&mut file, &name, max_entry_bytes) {
                Ok(bytes) => bytes,
                Err(error) => {
                    failures.push((name, error));
                    continue;
                }
            };
            entries.push(ClaimedSecureSpoolEntry {
                name,
                bytes,
                _claim: file,
            });
        }
        Ok(ClaimedSecureSpoolEntries {
            scan_has_more: skipped_contended > 0
                || !failures.is_empty()
                || attempted < total_matching,
            entries,
            failures,
            skipped_contended,
            attempted,
            total_matching,
        })
    }

    pub(super) fn claim_secure_named_entries(
        path: &Path,
        extension: &str,
        max_entry_bytes: u64,
        requested_names: &[OsString],
        max_entries: usize,
    ) -> Result<ClaimedSecureSpoolEntries, String> {
        use fs4::fs_std::FileExt;

        for name in requested_names {
            validate_entry_name(name)?;
        }
        let mut directory = open_validated_directory(path)?;
        let mut matching_names = directory
            .iter()
            .map(|entry| {
                entry
                    .map(|entry| entry.file_name().to_bytes().to_vec())
                    .map_err(|error| format!("failed to enumerate durable spool: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|name| {
                name != b"."
                    && name != b".."
                    && Path::new(&OsString::from_vec(name.clone()))
                        .extension()
                        .and_then(OsStr::to_str)
                        == Some(extension)
            })
            .collect::<Vec<_>>();
        matching_names.sort();
        let total_matching = matching_names.len();
        let requested_matching_names = matching_names
            .into_iter()
            .filter(|name| {
                let name = OsString::from_vec(name.clone());
                requested_names.iter().any(|requested| requested == &name)
            })
            .collect::<Vec<_>>();
        let mut entries = Vec::new();
        let mut failures = Vec::new();
        let mut skipped_contended = 0_usize;
        let attempted = requested_matching_names.len().min(max_entries);
        for name in requested_matching_names.into_iter().take(max_entries) {
            let name = OsString::from_vec(name);
            let mut file = match open_regular_file(&directory, &name, max_entry_bytes) {
                Ok(file) => file,
                Err(error) => {
                    failures.push((name, error));
                    continue;
                }
            };
            if let Err(error) = file.try_lock_exclusive() {
                if error.raw_os_error() == fs4::lock_contended_error().raw_os_error() {
                    skipped_contended = skipped_contended.saturating_add(1);
                    continue;
                }
                failures.push((
                    name,
                    format!("failed to claim durable spool entry: {error}"),
                ));
                continue;
            }
            let bytes = match read_bounded_file(&mut file, &name, max_entry_bytes) {
                Ok(bytes) => bytes,
                Err(error) => {
                    failures.push((name, error));
                    continue;
                }
            };
            entries.push(ClaimedSecureSpoolEntry {
                name,
                bytes,
                _claim: file,
            });
        }
        Ok(ClaimedSecureSpoolEntries {
            scan_has_more: skipped_contended > 0
                || !failures.is_empty()
                || attempted < total_matching,
            entries,
            failures,
            skipped_contended,
            attempted,
            total_matching,
        })
    }
}

#[cfg(not(unix))]
mod platform {
    use super::*;

    fn unsupported<T>() -> Result<T, String> {
        Err("durable ACP spooling is unavailable on platforms without secure descriptor-relative filesystem support".to_owned())
    }

    pub(super) fn ensure_secure_directory(_path: &Path) -> Result<(), String> {
        unsupported()
    }

    pub(super) fn measure_secure_directory(
        _path: &Path,
        _max_entry_bytes: u64,
        _max_entries: usize,
    ) -> Result<(usize, u64), String> {
        unsupported()
    }

    pub(super) fn lock_secure_directory(_path: &Path) -> Result<std::fs::File, String> {
        unsupported()
    }

    pub(super) fn lock_secure_named_file(
        _path: &Path,
        _name: &OsStr,
    ) -> Result<std::fs::File, String> {
        unsupported()
    }

    pub(super) fn read_secure_entries(
        _path: &Path,
        _extension: &str,
        _max_entry_bytes: u64,
    ) -> Result<Vec<SecureSpoolEntry>, String> {
        unsupported()
    }

    pub(super) fn read_secure_entry(
        _path: &Path,
        _name: &OsStr,
        _max_entry_bytes: u64,
    ) -> Result<Option<Vec<u8>>, String> {
        unsupported()
    }

    #[cfg(test)]
    pub(super) fn claim_secure_entries(
        _path: &Path,
        _extension: &str,
        _max_entry_bytes: u64,
    ) -> Result<ClaimedSecureSpoolEntries, String> {
        unsupported()
    }

    pub(super) fn claim_secure_entries_bounded(
        _path: &Path,
        _extension: &str,
        _max_entry_bytes: u64,
        _cursor: usize,
        _max_entries: usize,
    ) -> Result<ClaimedSecureSpoolEntries, String> {
        unsupported()
    }

    pub(super) fn claim_secure_named_entries(
        _path: &Path,
        _extension: &str,
        _max_entry_bytes: u64,
        _requested_names: &[OsString],
        _max_entries: usize,
    ) -> Result<ClaimedSecureSpoolEntries, String> {
        unsupported()
    }

    pub(super) fn write_secure_entry_if_absent(
        _path: &Path,
        _name: &OsStr,
        _temporary_name: &OsStr,
        _bytes: &[u8],
    ) -> Result<bool, String> {
        unsupported()
    }

    pub(super) fn remove_secure_entry(_path: &Path, _name: &OsStr) -> Result<bool, String> {
        unsupported()
    }

    pub(super) fn rename_secure_entry(
        _path: &Path,
        _source: &OsStr,
        _destination: &OsStr,
    ) -> Result<bool, String> {
        unsupported()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn secure_test_dir(label: &str) -> std::path::PathBuf {
        std::fs::canonicalize(std::env::temp_dir())
            .unwrap_or_else(|_| std::env::temp_dir())
            .join(format!("buzz-acp-secure-spool-{label}-{}", Uuid::new_v4()))
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn relative_spool_roots_fail_before_the_first_side_effect() {
        let first = format!("buzz-acp-relative-spool-{}", Uuid::new_v4());
        let path = std::path::PathBuf::from(&first).join("child");

        let error = ensure_secure_directory(&path)
            .await
            .expect_err("relative spool roots must fail closed");

        assert!(error.contains("absolute"));
        assert!(
            !Path::new(&first).exists(),
            "anchor validation must precede mkdirat"
        );
        assert!(ensure_secure_directory(Path::new("/"))
            .await
            .expect_err("filesystem root must not be accepted as a spool")
            .contains("filesystem root"));
    }

    #[tokio::test]
    async fn entry_claims_are_exclusive_and_release_on_drop() {
        let directory = std::fs::canonicalize(std::env::temp_dir())
            .unwrap_or_else(|_| std::env::temp_dir())
            .join(format!("buzz-acp-secure-spool-{}", Uuid::new_v4()));
        ensure_secure_directory(&directory)
            .await
            .expect("create secure spool");
        assert!(write_secure_entry_if_absent(
            &directory,
            OsStr::new("event.json"),
            OsStr::new("event.tmp"),
            b"{}",
        )
        .await
        .expect("persist entry"));

        let first = claim_secure_entries(&directory, "json", 1024)
            .await
            .expect("first claim");
        assert_eq!(first.entries.len(), 1);
        assert_eq!(first.skipped_contended, 0);
        let contended = claim_secure_entries(&directory, "json", 1024)
            .await
            .expect("contended scan");
        assert!(contended.entries.is_empty());
        assert_eq!(contended.skipped_contended, 1);
        assert_eq!(contended.attempted, 1);

        drop(first);
        let reclaimed = claim_secure_entries(&directory, "json", 1024)
            .await
            .expect("reclaim after drop");
        assert_eq!(reclaimed.entries.len(), 1);
        assert_eq!(reclaimed.skipped_contended, 0);
        drop(reclaimed);
        std::fs::remove_dir_all(directory).expect("clean secure spool");
    }

    #[tokio::test]
    async fn bounded_claims_select_before_opening_the_spool() {
        let directory = secure_test_dir("bounded-claims");
        ensure_secure_directory(&directory)
            .await
            .expect("create secure spool");
        for name in ["a.json", "b.json", "c.json"] {
            assert!(write_secure_entry_if_absent(
                &directory,
                OsStr::new(name),
                OsStr::new(&format!("{name}.tmp")),
                b"{}",
            )
            .await
            .expect("persist entry"));
        }

        let claimed = claim_secure_entries_bounded(&directory, "json", 1024, 1, 1)
            .await
            .expect("bounded claim");

        assert_eq!(claimed.total_matching, 3);
        assert_eq!(claimed.entries.len(), 1);
        assert_eq!(claimed.entries[0].name, OsStr::new("b.json"));
        assert!(claimed.scan_has_more);
        drop(claimed);

        let named =
            claim_secure_named_entries(&directory, "json", 1024, &[OsString::from("c.json")], 1)
                .await
                .expect("named claim");
        assert_eq!(named.total_matching, 3);
        assert_eq!(named.entries.len(), 1);
        assert_eq!(named.entries[0].name, OsStr::new("c.json"));
        assert!(named.scan_has_more, "unclaimed siblings remain visible");
        drop(named);
        std::fs::remove_dir_all(directory).expect("clean secure spool");
    }

    #[tokio::test]
    async fn bounded_claims_isolate_an_invalid_sibling() {
        let directory = secure_test_dir("invalid-sibling");
        ensure_secure_directory(&directory)
            .await
            .expect("create secure spool");
        for (name, bytes) in [
            ("a-invalid.json", vec![0_u8; 2_048]),
            ("b-healthy.json", b"{}".to_vec()),
        ] {
            assert!(write_secure_entry_if_absent(
                &directory,
                OsStr::new(name),
                OsStr::new(&format!("{name}.tmp")),
                &bytes,
            )
            .await
            .expect("persist entry"));
        }

        let claimed = claim_secure_entries_bounded(&directory, "json", 1_024, 0, 1)
            .await
            .expect("invalid sibling is isolated");

        assert!(claimed.entries.is_empty());
        assert_eq!(claimed.failures.len(), 1);
        assert_eq!(claimed.failures[0].0, OsStr::new("a-invalid.json"));
        assert!(claimed.scan_has_more);
        drop(claimed);

        let next = claim_secure_entries_bounded(&directory, "json", 1_024, 1, 1)
            .await
            .expect("fair cursor reaches healthy sibling");
        assert_eq!(next.entries.len(), 1);
        assert_eq!(next.entries[0].name, OsStr::new("b-healthy.json"));
        assert!(next.failures.is_empty());
        drop(next);
        std::fs::remove_dir_all(directory).expect("clean secure spool");
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn temporary_cleanup_removes_crash_residue_without_reading_payloads() {
        let directory = secure_test_dir("tmp-cleanup");
        ensure_secure_directory(&directory)
            .await
            .expect("create secure spool");
        let temporary = directory.join("entry.json.crash.tmp");
        std::fs::write(&temporary, vec![7_u8; 1024]).expect("seed crash residue");

        assert!(cleanup_secure_temporary_entries(&directory, 0, 1)
            .await
            .expect("clean temporary residue"));
        assert!(!temporary.exists());

        std::fs::remove_dir_all(directory).expect("clean secure spool");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn secure_directory_creation_never_follows_an_ancestor_symlink() {
        use std::os::unix::fs::symlink;

        let root = std::fs::canonicalize(std::env::temp_dir())
            .unwrap_or_else(|_| std::env::temp_dir())
            .join(format!("buzz-acp-secure-spool-ancestor-{}", Uuid::new_v4()));
        let target = root.join("target");
        let linked = root.join("linked");
        std::fs::create_dir_all(&target).expect("create symlink target");
        symlink(&target, &linked).expect("create ancestor symlink");

        let result = ensure_secure_directory(&linked.join("must-not-exist")).await;

        assert!(result.is_err(), "symlinked ancestors must fail closed");
        assert!(
            !target.join("must-not-exist").exists(),
            "validation failure must happen before any redirected side effect"
        );
        std::fs::remove_dir_all(root).expect("clean symlink probe");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn root_capacity_measurement_includes_secure_nested_ledgers() {
        let root = secure_test_dir("nested-capacity");
        let nested = root.join("pending-requests");
        ensure_secure_directory(&root).await.expect("secure root");
        ensure_secure_directory(&nested)
            .await
            .expect("secure nested ledger");
        assert!(write_secure_entry_if_absent(
            &root,
            OsStr::new("resolution.json"),
            OsStr::new("resolution.tmp"),
            b"resolution",
        )
        .await
        .expect("write root entry"));
        assert!(write_secure_entry_if_absent(
            &nested,
            OsStr::new("request.json"),
            OsStr::new("request.tmp"),
            b"request",
        )
        .await
        .expect("write nested entry"));

        assert_eq!(measure_secure_directory(&root, 1024, 16).await, Ok((3, 17)));
        std::fs::remove_dir_all(root).expect("clean nested capacity probe");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn directory_capacity_claims_are_exclusive() {
        let directory = secure_test_dir("directory-claim");
        ensure_secure_directory(&directory)
            .await
            .expect("secure spool");
        let first = lock_secure_directory(&directory)
            .await
            .expect("first capacity claim");
        assert!(
            lock_secure_directory(&directory)
                .await
                .expect_err("concurrent capacity claim must fail closed")
                .contains("already held"),
            "capacity contention must stay bounded"
        );
        drop(first);
        lock_secure_directory(&directory)
            .await
            .expect("capacity claim transfers after release");
        std::fs::remove_dir_all(directory).expect("clean capacity claim probe");
    }
}
