use buzz_core_pkg::transport_status::{TransportRecord, MAX_RECORD_BYTES};
use std::path::Path;

pub(super) fn read_owned_record(path: &Path) -> Result<TransportRecord, String> {
    let bytes = read_owned_bytes(path)?;
    serde_json::from_slice(&bytes).map_err(|_| "local transport status malformed".into())
}

#[cfg(unix)]
fn read_owned_bytes(path: &Path) -> Result<Vec<u8>, String> {
    if !path.is_absolute() || path.as_os_str().len() > 4096 {
        return Err("local transport status path is invalid".into());
    }
    let parent = path
        .parent()
        .ok_or("local transport status parent is missing")?;
    let name = path
        .file_name()
        .ok_or("local transport status filename is missing")?;
    let directory = open_owned_directory(parent, false)?;
    read_owned_entry(&directory, name)
}

#[cfg(unix)]
pub(super) fn open_owned_directory(
    path: &Path,
    create_leaf: bool,
) -> Result<nix::dir::Dir, String> {
    use nix::dir::Dir;
    use nix::fcntl::OFlag;
    use nix::sys::stat::{fstat, mkdirat, Mode, SFlag};
    use nix::unistd::geteuid;
    use std::path::Component;
    if !path.is_absolute() || path.as_os_str().len() > 4096 {
        return Err("local transport status path is invalid".into());
    }
    let components: Vec<_> = path.components().collect();
    if components.len() < 2
        || components.len() > 256
        || components
            .iter()
            .skip(1)
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("local transport status path is invalid".into());
    }
    let flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC;
    let mut directory = Dir::open(Path::new("/"), flags, Mode::empty())
        .map_err(|_| "cannot anchor local transport status directory")?;
    let owner = geteuid().as_raw();
    for (index, component) in components.iter().enumerate().skip(1) {
        let next = Dir::openat(&directory, component.as_os_str(), flags, Mode::empty());
        directory = match next {
            Ok(next) => next,
            Err(nix::errno::Errno::ENOENT) if create_leaf && index + 1 == components.len() => {
                match mkdirat(&directory, component.as_os_str(), Mode::S_IRWXU) {
                    Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
                    Err(_) => {
                        return Err("cannot create private local transport status directory".into())
                    }
                }
                Dir::openat(&directory, component.as_os_str(), flags, Mode::empty())
                    .map_err(|_| "cannot open private local transport status directory")?
            }
            Err(nix::errno::Errno::ENOENT) => {
                return Err("local transport status ancestor is missing".into())
            }
            Err(nix::errno::Errno::ELOOP) => {
                return Err("local transport status ancestor is symlinked".into())
            }
            Err(_) => return Err("local transport status ancestor is inaccessible".into()),
        };
        let metadata =
            fstat(&directory).map_err(|_| "cannot inspect local transport status directory")?;
        let mode = Mode::from_bits_truncate(metadata.st_mode);
        if SFlag::from_bits_truncate(metadata.st_mode) != SFlag::S_IFDIR
            || (metadata.st_uid != 0 && metadata.st_uid != owner)
            || !((mode & (Mode::S_IWGRP | Mode::S_IWOTH)).is_empty()
                || metadata.st_uid == 0 && mode.contains(Mode::S_ISVTX))
        {
            return Err("local transport status ancestor is not trusted".into());
        }
    }
    let parent = fstat(&directory).map_err(|_| "cannot inspect local transport status parent")?;
    if parent.st_uid != owner || parent.st_mode & 0o777 != 0o700 {
        return Err("local transport status directory must be owner-owned and private".into());
    }
    Ok(directory)
}

/// Errors that mean the status sidechannel cannot currently be established.
///
/// These are environmental failures (for example a read-only or not-yet
/// created app-data parent). Security-policy failures such as a symlink,
/// group-writable ancestor, or non-private leaf remain hard refusals.
pub(super) fn is_storage_unavailable(error: &str) -> bool {
    matches!(
        error,
        "cannot anchor local transport status directory"
            | "local transport status ancestor is missing"
            | "local transport status ancestor is inaccessible"
            | "cannot create private local transport status directory"
            | "cannot open private local transport status directory"
            | "cannot inspect local transport status directory"
            | "cannot inspect local transport status parent"
    )
}

#[cfg(unix)]
pub(super) fn read_owned_entry(
    directory: &nix::dir::Dir,
    name: &std::ffi::OsStr,
) -> Result<Vec<u8>, String> {
    use nix::fcntl::OFlag;
    use nix::sys::stat::{fstat, Mode};
    use std::io::Read;
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("local transport status filename is invalid".into());
    }
    let descriptor = nix::fcntl::openat(
        directory,
        name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| "local transport status file is missing or unsafe")?;
    let metadata = fstat(&descriptor).map_err(|_| "cannot inspect local transport status file")?;
    validate_file_metadata(&metadata, nix::unistd::geteuid().as_raw())?;
    let mut bytes = Vec::new();
    std::fs::File::from(descriptor)
        .take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read local transport status file")?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("local transport status file exceeds the size limit".into());
    }
    Ok(bytes)
}

#[cfg(unix)]
pub(super) fn validate_file_metadata(
    metadata: &nix::sys::stat::FileStat,
    owner: u32,
) -> Result<(), String> {
    use nix::sys::stat::SFlag;
    if SFlag::from_bits_truncate(metadata.st_mode) != SFlag::S_IFREG
        || metadata.st_uid != owner
        || metadata.st_mode & 0o777 != 0o600
        || metadata.st_size < 0
        || metadata.st_size as u64 > MAX_RECORD_BYTES
        || metadata.st_nlink != 1
    {
        return Err(
            "local transport status must be an owner-owned bounded 0600 regular file".into(),
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn read_owned_bytes(_path: &Path) -> Result<Vec<u8>, String> {
    Err("secure local transport status is unavailable on this platform".into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::PathBuf;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .canonicalize()
                .unwrap()
                .join(format!("crew-338-reader-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn record(&self) -> PathBuf {
            let path = self.0.join("status.json");
            std::fs::write(&path, br#"{"version":1,"runtimeId":"fixture","startNonce":"nonce","sequence":1,"timestampMs":100000,"terminal":false,"transport":{"state":"connected","code":"none","attempts":0,"elapsedMs":0,"nextRetryAtMs":null,"lastError":null}}"#).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn accepts_owned_private_regular_record() {
        let fixture = Fixture::new();
        assert_eq!(read_owned_record(&fixture.record()).unwrap().sequence, 1);
    }

    #[test]
    fn rejects_symlink_file_and_ancestor() {
        let fixture = Fixture::new();
        let path = fixture.record();
        let link = fixture.0.join("link.json");
        symlink(&path, &link).unwrap();
        assert!(
            read_owned_record(&link).is_err(),
            "file symlink must not be followed"
        );
        let parent_link = fixture.0.join("linked-parent");
        symlink(&fixture.0, &parent_link).unwrap();
        assert!(read_owned_record(&parent_link.join("status.json")).is_err());
    }

    #[test]
    fn rejects_world_readable_record_and_nonprivate_parent() {
        let fixture = Fixture::new();
        let path = fixture.record();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_owned_record(&path).is_err());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(&fixture.0, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(read_owned_record(&path).is_err());
    }

    #[test]
    fn rejects_oversized_even_when_json_is_valid() {
        let fixture = Fixture::new();
        let path = fixture.record();
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["transport"]["lastError"] = "x".repeat(9000).into();
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(read_owned_record(&path).is_err());
    }

    #[test]
    fn rejects_missing_malformed_and_truncated_files() {
        let fixture = Fixture::new();
        let path = fixture.record();
        std::fs::write(&path, b"{\"version\":1").unwrap();
        assert!(read_owned_record(&path).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(read_owned_record(&path).is_err());
    }

    #[test]
    fn rejects_hardlinked_record_and_fifo_without_blocking() {
        let fixture = Fixture::new();
        let path = fixture.record();
        std::fs::hard_link(&path, fixture.0.join("alias.json")).unwrap();
        assert!(read_owned_record(&path).is_err());
        let fifo = fixture.0.join("fifo");
        nix::unistd::mkfifo(
            &fifo,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )
        .unwrap();
        assert!(read_owned_record(&fifo).is_err());
    }

    #[test]
    fn file_owner_validation_rejects_another_uid() {
        let fixture = Fixture::new();
        let file = std::fs::File::open(fixture.record()).unwrap();
        let metadata = nix::sys::stat::fstat(&file).unwrap();
        assert!(validate_file_metadata(&metadata, metadata.st_uid).is_ok());
        assert!(validate_file_metadata(&metadata, metadata.st_uid.wrapping_add(1)).is_err());
    }
}
