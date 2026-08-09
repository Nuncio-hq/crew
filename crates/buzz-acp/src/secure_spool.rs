use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Component, Path};

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
) -> Result<(usize, u64), String> {
    let path = path.to_owned();
    run_blocking(move || platform::measure_secure_directory(&path, max_entry_bytes)).await
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

pub(crate) async fn claim_secure_entries(
    path: &Path,
    extension: &str,
    max_entry_bytes: u64,
) -> Result<Vec<ClaimedSecureSpoolEntry>, String> {
    let path = path.to_owned();
    let extension = extension.to_owned();
    run_blocking(move || platform::claim_secure_entries(&path, &extension, max_entry_bytes)).await
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
    use nix::fcntl::{renameat, AtFlags, OFlag};
    use nix::sys::stat::{fchmod, fstat, Mode, SFlag};
    use nix::unistd::{fsync, geteuid, linkat, unlinkat, UnlinkatFlags};
    use std::fs::File;
    use std::os::unix::ffi::OsStringExt;

    fn directory_flags() -> OFlag {
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC
    }

    fn open_directory_chain(path: &Path) -> Result<Dir, String> {
        let mut directory = if path.is_absolute() {
            Dir::open(Path::new("/"), directory_flags(), Mode::empty())
        } else {
            Dir::open(Path::new("."), directory_flags(), Mode::empty())
        }
        .map_err(|error| format!("failed to anchor durable spool path: {error}"))?;

        for component in path.components() {
            let name = match component {
                Component::RootDir | Component::CurDir => continue,
                Component::Normal(name) => name,
                Component::ParentDir | Component::Prefix(_) => {
                    return Err(format!(
                        "unsafe durable spool path component: {}",
                        path.display()
                    ));
                }
            };
            directory = Dir::openat(&directory, name, directory_flags(), Mode::empty())
                .map_err(|error| {
                    format!(
                        "failed to open durable spool component {} without following links: {error}",
                        name.to_string_lossy()
                    )
                })?;
            let metadata = fstat(&directory)
                .map_err(|error| format!("failed to inspect durable spool component: {error}"))?;
            let owner = metadata.st_uid;
            if !SFlag::from_bits_truncate(metadata.st_mode).contains(SFlag::S_IFDIR)
                || (owner != 0 && owner != geteuid().as_raw())
            {
                return Err(format!(
                    "unsafe durable spool component: {}",
                    name.to_string_lossy()
                ));
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
        let existed = path
            .try_exists()
            .map_err(|error| format!("failed to inspect durable spool: {error}"))?;
        if !existed {
            std::fs::create_dir_all(path)
                .map_err(|error| format!("failed to create durable spool: {error}"))?;
        }
        let directory = open_directory_chain(path)?;
        if !existed {
            fchmod(&directory, Mode::S_IRWXU)
                .map_err(|error| format!("failed to secure durable spool: {error}"))?;
        }
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
    ) -> Result<(usize, u64), String> {
        let mut directory = open_validated_directory(path)?;
        let mut count = 0_usize;
        let mut total = 0_u64;
        let names = directory
            .iter()
            .map(|entry| {
                entry
                    .map(|entry| entry.file_name().to_bytes().to_vec())
                    .map_err(|error| format!("failed to enumerate durable spool: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for name in names {
            if name == b"." || name == b".." {
                continue;
            }
            let name = OsString::from_vec(name);
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
            count = count.saturating_add(1);
            total = total.saturating_add(metadata.st_size.max(0) as u64);
        }
        Ok((count, total))
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

    pub(super) fn claim_secure_entries(
        path: &Path,
        extension: &str,
        max_entry_bytes: u64,
    ) -> Result<Vec<ClaimedSecureSpoolEntry>, String> {
        use fs4::fs_std::FileExt;

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
            if let Err(error) = file.try_lock_exclusive() {
                if error.raw_os_error() == fs4::lock_contended_error().raw_os_error() {
                    continue;
                }
                return Err(format!(
                    "failed to claim durable spool entry {}: {error}",
                    name.to_string_lossy()
                ));
            }
            let bytes = read_bounded_file(&mut file, &name, max_entry_bytes)?;
            entries.push(ClaimedSecureSpoolEntry {
                name,
                bytes,
                _claim: file,
            });
        }
        Ok(entries)
    }
}

#[cfg(not(unix))]
mod platform {
    use super::*;

    fn validate_directory(path: &Path) -> Result<(), String> {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|error| format!("failed to inspect durable spool: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!("unsafe durable spool path: {}", path.display()));
        }
        Ok(())
    }

    pub(super) fn ensure_secure_directory(path: &Path) -> Result<(), String> {
        std::fs::create_dir_all(path)
            .map_err(|error| format!("failed to create durable spool: {error}"))?;
        validate_directory(path)
    }

    fn read_entry(path: &Path, max_entry_bytes: u64) -> Result<Vec<u8>, String> {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|error| format!("failed to inspect durable spool entry: {error}"))?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > max_entry_bytes
        {
            return Err(format!("unsafe durable spool entry: {}", path.display()));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .and_then(|file| {
                file.take(max_entry_bytes.saturating_add(1))
                    .read_to_end(&mut bytes)
            })
            .map_err(|error| format!("failed to read durable spool entry: {error}"))?;
        if bytes.len() as u64 > max_entry_bytes {
            return Err(format!(
                "durable spool entry exceeds byte limit: {}",
                path.display()
            ));
        }
        Ok(bytes)
    }

    pub(super) fn measure_secure_directory(
        path: &Path,
        max_entry_bytes: u64,
    ) -> Result<(usize, u64), String> {
        validate_directory(path)?;
        let mut count = 0_usize;
        let mut total = 0_u64;
        for entry in std::fs::read_dir(path)
            .map_err(|error| format!("failed to enumerate durable spool: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("failed to enumerate spool entry: {error}"))?;
            let bytes = read_entry(&entry.path(), max_entry_bytes)?;
            count = count.saturating_add(1);
            total = total.saturating_add(bytes.len() as u64);
        }
        Ok((count, total))
    }

    pub(super) fn read_secure_entries(
        path: &Path,
        extension: &str,
        max_entry_bytes: u64,
    ) -> Result<Vec<SecureSpoolEntry>, String> {
        validate_directory(path)?;
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path)
            .map_err(|error| format!("failed to enumerate durable spool: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("failed to enumerate spool entry: {error}"))?;
            if entry.path().extension().and_then(OsStr::to_str) != Some(extension) {
                continue;
            }
            entries.push(SecureSpoolEntry {
                name: entry.file_name(),
                bytes: read_entry(&entry.path(), max_entry_bytes)?,
            });
        }
        Ok(entries)
    }

    pub(super) fn read_secure_entry(
        path: &Path,
        name: &OsStr,
        max_entry_bytes: u64,
    ) -> Result<Option<Vec<u8>>, String> {
        let entry = path.join(name);
        if !entry
            .try_exists()
            .map_err(|error| format!("failed to inspect durable spool entry: {error}"))?
        {
            return Ok(None);
        }
        read_entry(&entry, max_entry_bytes).map(Some)
    }

    pub(super) fn claim_secure_entries(
        path: &Path,
        extension: &str,
        max_entry_bytes: u64,
    ) -> Result<Vec<ClaimedSecureSpoolEntry>, String> {
        use fs4::fs_std::FileExt;

        validate_directory(path)?;
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path)
            .map_err(|error| format!("failed to enumerate durable spool: {error}"))?
        {
            let entry =
                entry.map_err(|error| format!("failed to enumerate spool entry: {error}"))?;
            if entry.path().extension().and_then(OsStr::to_str) != Some(extension) {
                continue;
            }
            let file = std::fs::File::open(entry.path())
                .map_err(|error| format!("failed to open durable spool entry: {error}"))?;
            if let Err(error) = file.try_lock_exclusive() {
                if error.raw_os_error() == fs4::lock_contended_error().raw_os_error() {
                    continue;
                }
                return Err(format!("failed to claim durable spool entry: {error}"));
            }
            let bytes = read_entry(&entry.path(), max_entry_bytes)?;
            entries.push(ClaimedSecureSpoolEntry {
                name: entry.file_name(),
                bytes,
                _claim: file,
            });
        }
        Ok(entries)
    }

    pub(super) fn write_secure_entry_if_absent(
        path: &Path,
        name: &OsStr,
        temporary_name: &OsStr,
        bytes: &[u8],
    ) -> Result<bool, String> {
        validate_directory(path)?;
        let temporary = path.join(temporary_name);
        let destination = path.join(name);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = options
            .open(&temporary)
            .map_err(|error| format!("failed to securely create durable spool entry: {error}"))?;
        let result = file.write_all(bytes).and_then(|()| file.sync_all());
        drop(file);
        if let Err(error) = result {
            let _ = std::fs::remove_file(&temporary);
            return Err(format!("failed to persist durable spool entry: {error}"));
        }
        match std::fs::hard_link(&temporary, &destination) {
            Ok(()) => {
                std::fs::remove_file(&temporary)
                    .map_err(|error| format!("failed to remove temporary spool entry: {error}"))?;
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = std::fs::remove_file(&temporary);
                Ok(false)
            }
            Err(error) => {
                let _ = std::fs::remove_file(&temporary);
                Err(format!("failed to commit durable spool entry: {error}"))
            }
        }
    }

    pub(super) fn remove_secure_entry(path: &Path, name: &OsStr) -> Result<bool, String> {
        validate_directory(path)?;
        match std::fs::remove_file(path.join(name)) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("failed to remove durable spool entry: {error}")),
        }
    }

    pub(super) fn rename_secure_entry(
        path: &Path,
        source: &OsStr,
        destination: &OsStr,
    ) -> Result<bool, String> {
        validate_directory(path)?;
        match std::fs::rename(path.join(source), path.join(destination)) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("failed to rename durable spool entry: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

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
        assert_eq!(first.len(), 1);
        let contended = claim_secure_entries(&directory, "json", 1024)
            .await
            .expect("contended scan");
        assert!(contended.is_empty());

        drop(first);
        let reclaimed = claim_secure_entries(&directory, "json", 1024)
            .await
            .expect("reclaim after drop");
        assert_eq!(reclaimed.len(), 1);
        drop(reclaimed);
        std::fs::remove_dir_all(directory).expect("clean secure spool");
    }
}
