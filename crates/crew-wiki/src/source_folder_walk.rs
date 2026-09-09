//! Capability walk used by folder capture. No path-based leaf read is permitted.

use crate::source_folder::{CapturePoint, Hook};
use crate::source_snapshot::{valid_source_path, SourceOmission, SourceOmissionReason};
use crate::WikiError;
use rustix::fd::OwnedFd;
use rustix::fs::{fstat, open, openat, statat, AtFlags, Dir, FileType, Mode, OFlags, Stat};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::Instant;

pub(crate) const MAX_ENTRIES: usize = 20_000;
pub(crate) const MAX_DEPTH: usize = 64;
pub(crate) const MAX_PATH_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub(crate) device: u64,
    pub(crate) inode: u64,
    pub(crate) size: i64,
    pub(crate) mode: u32,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl From<Stat> for Stamp {
    // POSIX ABI field widths differ across supported Unix targets.
    #[allow(clippy::unnecessary_cast)]
    fn from(stat: Stat) -> Self {
        Self {
            device: stat.st_dev as u64,
            inode: stat.st_ino as u64,
            size: stat.st_size as i64,
            mode: stat.st_mode as u32,
            modified: (stat.st_mtime as i64, stat.st_mtime_nsec as i64),
            changed: (stat.st_ctime as i64, stat.st_ctime_nsec as i64),
        }
    }
}

struct RootLink {
    parent: Arc<OwnedFd>,
    name: OsString,
    stamp: Stamp,
}
pub(crate) struct Root {
    pub(crate) directory: Arc<OwnedFd>,
    links: Vec<RootLink>,
}
impl Root {
    pub(crate) fn open(bound_root: &Path) -> Result<Self, WikiError> {
        let canonical =
            std::fs::canonicalize(bound_root).map_err(|_| error("bound folder unavailable"))?;
        let expected =
            std::fs::metadata(&canonical).map_err(|_| error("bound folder unavailable"))?;
        if !expected.is_dir() {
            return Err(error("bound folder is not a directory"));
        }
        let mut directory = Arc::new(
            open("/", directory_flags(), Mode::empty())
                .map_err(|_| error("root capability unavailable"))?,
        );
        let mut links = Vec::new();
        for component in canonical.components() {
            let Component::Normal(name) = component else {
                if component == Component::RootDir {
                    continue;
                }
                return Err(error("invalid canonical root"));
            };
            if links.len() >= MAX_DEPTH {
                return Err(error("bound root depth exceeded"));
            }
            let next = Arc::new(
                openat(&*directory, name, directory_flags(), Mode::empty())
                    .map_err(|_| error("bound root changed or contains a symlink"))?,
            );
            let stamp = stamp(&*next)?;
            links.push(RootLink {
                parent: Arc::clone(&directory),
                name: name.to_owned(),
                stamp,
            });
            directory = next;
        }
        let actual = stamp(&*directory)?;
        if actual.device != expected.dev() || actual.inode != expected.ino() {
            return Err(error("bound root identity changed"));
        }
        Ok(Self { directory, links })
    }
    pub(crate) fn verify_binding(&self) -> Result<(), WikiError> {
        for link in &self.links {
            let now: Stamp = statat(&*link.parent, &link.name, AtFlags::SYMLINK_NOFOLLOW)
                .map_err(|_| error("bound root moved or disappeared"))?
                .into();
            if now.device != link.stamp.device
                || now.inode != link.stamp.inode
                || FileType::from_raw_mode(now.mode as _) != FileType::Directory
            {
                return Err(error("bound root identity changed"));
            }
        }
        Ok(())
    }
}

pub(crate) struct Entry {
    pub(crate) parent: Arc<OwnedFd>,
    pub(crate) name: String,
    pub(crate) stamp: Stamp,
}
pub(crate) struct Listing {
    pub(crate) files: BTreeMap<String, Entry>,
    pub(crate) directories: BTreeMap<String, Stamp>,
    pub(crate) omissions: Vec<SourceOmission>,
    visited: usize,
}
impl Listing {
    pub(crate) fn capture(
        root: &Root,
        deadline: Instant,
        hook: &mut Hook<'_>,
    ) -> Result<Self, WikiError> {
        let mut listing = Self {
            files: BTreeMap::new(),
            directories: BTreeMap::new(),
            omissions: Vec::new(),
            visited: 0,
        };
        listing.walk(Arc::clone(&root.directory), "", 0, deadline, hook)?;
        listing.omissions.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(listing)
    }
    fn walk(
        &mut self,
        directory: Arc<OwnedFd>,
        prefix: &str,
        depth: usize,
        deadline: Instant,
        hook: &mut Hook<'_>,
    ) -> Result<(), WikiError> {
        if depth > MAX_DEPTH {
            return Err(error("source depth limit exceeded"));
        }
        self.directories.insert(prefix.into(), stamp(&*directory)?);
        let entries =
            Dir::read_from(&*directory).map_err(|_| error("source directory unavailable"))?;
        for item in entries {
            if Instant::now() >= deadline {
                return Err(error("source capture deadline reached"));
            }
            let item = item.map_err(|_| error("source directory changed"))?;
            let bytes = item.file_name().to_bytes();
            if matches!(bytes, b"." | b"..") {
                continue;
            }
            self.visited += 1;
            if self.visited > MAX_ENTRIES {
                return Err(error("source enumeration limit exceeded"));
            }
            let name = crate::source_snapshot::source_path_from_bytes(bytes)?;
            let path = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}/{name}")
            };
            if path.len() > MAX_PATH_BYTES {
                return Err(error("source path limit exceeded"));
            }
            if !valid_source_path(&path) {
                self.omit(&path, SourceOmissionReason::InvalidPath);
                continue;
            }
            let stat = statat(&*directory, name, AtFlags::SYMLINK_NOFOLLOW)
                .map_err(|_| error("source entry changed"))?;
            let kind = FileType::from_raw_mode(stat.st_mode as _);
            let expected: Stamp = stat.into();
            match kind {
                FileType::Symlink => self.omit(&path, SourceOmissionReason::Symlink),
                FileType::Directory => {
                    if crate::cluster::skip_source_directory(name)
                        || (path.starts_with('.') && !path.starts_with(".crew"))
                    {
                        self.omit(&path, SourceOmissionReason::Excluded);
                        continue;
                    }
                    hook(CapturePoint::BeforeOpenDirectory, &path);
                    let child = match openat(&*directory, name, directory_flags(), Mode::empty()) {
                        Ok(child) => Arc::new(child),
                        Err(rustix::io::Errno::ACCESS) => {
                            self.omit(&path, SourceOmissionReason::Unreadable);
                            continue;
                        }
                        Err(_) => {
                            return Err(error("source directory changed or became a symlink"))
                        }
                    };
                    hook(CapturePoint::AfterOpenDirectory, &path);
                    if stamp(&*child)? != expected {
                        return Err(error("source directory identity changed"));
                    }
                    self.walk(child, &path, depth + 1, deadline, hook)?;
                }
                FileType::RegularFile => {
                    self.files.insert(
                        path,
                        Entry {
                            parent: Arc::clone(&directory),
                            name: name.into(),
                            stamp: expected,
                        },
                    );
                }
                _ => self.omit(&path, SourceOmissionReason::Unreadable),
            }
        }
        Ok(())
    }
    fn omit(&mut self, path: &str, reason: SourceOmissionReason) {
        self.omissions.push(SourceOmission {
            path: path.into(),
            reason,
        });
    }
}

pub(crate) fn stamp(fd: &impl std::os::fd::AsFd) -> Result<Stamp, WikiError> {
    fstat(fd)
        .map(Stamp::from)
        .map_err(|_| error("source metadata unavailable"))
}
fn directory_flags() -> OFlags {
    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}
pub(crate) fn error(message: &str) -> WikiError {
    WikiError::Git(message.into())
}
