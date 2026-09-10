use super::{unavailable, VerifiedSourceFile};
use crate::source_folder_walk::{stamp, Root, MAX_DEPTH, MAX_PATH_BYTES};
#[cfg(target_os = "linux")]
use crate::source_git_tree::{source_blob, GitReader};
use crate::source_snapshot::{
    source_hash, valid_source_path, SourceReference, MAX_SOURCE_FILE_BYTES,
};
use crate::WikiError;
use rustix::fs::{openat, statat, AtFlags, FileType, Mode, OFlags};
use std::io::Read;
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

pub(super) fn read(
    root: &Root,
    revision: &str,
    reference: &SourceReference,
    deadline: Instant,
) -> Result<VerifiedSourceFile, WikiError> {
    let (path, digest, size, start, end) = reference;
    if Instant::now() >= deadline
        || !valid_source_path(path)
        || path.len() > MAX_PATH_BYTES
        || path.split('/').count() > MAX_DEPTH
        || *size > MAX_SOURCE_FILE_BYTES as u64
    {
        return Err(unavailable());
    }
    root.verify_binding()?;
    let bytes = if let Some(commit) = revision.strip_prefix("git:") {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = commit;
            return Err(unavailable());
        }
        #[cfg(target_os = "linux")]
        {
            let cwd_fd = rustix::io::dup(&*root.directory).map_err(|_| unavailable())?;
            let cwd = PathBuf::from(format!("/dev/fd/{}", cwd_fd.as_raw_fd()));
            let mut reader = GitReader::with_directory_fd(&cwd, cwd_fd, deadline)?;
            if reader.text(&["rev-parse", "--is-inside-work-tree"], 128)? != "true"
                || !reader
                    .text(&["rev-parse", "--show-prefix"], MAX_PATH_BYTES)?
                    .is_empty()
            {
                return Err(unavailable());
            }
            source_blob(&mut reader, commit, path, *size as usize)?
        }
    } else if revision.strip_prefix("folder:").is_some_and(|id| {
        id.len() == 64
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }) {
        folder(root, path, deadline)?
    } else {
        return Err(unavailable());
    };
    root.verify_binding()?;
    if Instant::now() >= deadline || bytes.len() as u64 != *size || source_hash(&bytes) != *digest {
        return Err(unavailable());
    }
    let content = String::from_utf8(bytes).map_err(|_| unavailable())?;
    let lines = content.split_terminator('\n').count() as u64;
    if *start == 0 || end < start || *end > lines {
        return Err(unavailable());
    }
    Ok(VerifiedSourceFile {
        content,
        start_line: *start,
        end_line: *end,
    })
}

fn folder(root: &Root, path: &str, deadline: Instant) -> Result<Vec<u8>, WikiError> {
    let mut parent = Arc::clone(&root.directory);
    let parts: Vec<_> = path.split('/').collect();
    let mut links = Vec::new();
    for (index, name) in parts.iter().enumerate() {
        if Instant::now() >= deadline {
            return Err(unavailable());
        }
        let leaf = index + 1 == parts.len();
        let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
        let fd = openat(
            &*parent,
            *name,
            if leaf {
                flags
            } else {
                flags | OFlags::DIRECTORY
            },
            Mode::empty(),
        )
        .map_err(|_| unavailable())?;
        let before = stamp(&fd)?;
        links.push((Arc::clone(&parent), *name, before.clone()));
        if leaf {
            if FileType::from_raw_mode(before.mode as _) != FileType::RegularFile
                || before.size < 0
                || before.size as usize > MAX_SOURCE_FILE_BYTES
            {
                return Err(unavailable());
            }
            let mut file = std::fs::File::from(fd);
            let mut bytes = Vec::new();
            (&mut file)
                .take((MAX_SOURCE_FILE_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|_| unavailable())?;
            if stamp(&file)? != before || bytes.len() as i64 != before.size {
                return Err(unavailable());
            }
            for (directory, name, expected) in links {
                let actual: crate::source_folder_walk::Stamp =
                    statat(&*directory, name, AtFlags::SYMLINK_NOFOLLOW)
                        .map_err(|_| unavailable())?
                        .into();
                if actual != expected {
                    return Err(unavailable());
                }
            }
            return Ok(bytes);
        }
        parent = Arc::new(fd);
    }
    Err(unavailable())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn production_git_runner_chdirs_through_retained_fd_after_path_replacement() {
        let temp = std::env::temp_dir().join(format!("crew-source-fd-{}", std::process::id()));
        std::fs::create_dir(&temp).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let _cleanup = Cleanup(temp.clone());
        let selected = temp.join("selected");
        std::fs::create_dir(&selected).unwrap();
        let output = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&selected)
            .output()
            .unwrap();
        assert!(output.status.success());
        let root = Root::open(&selected).unwrap();
        std::fs::rename(&selected, temp.join("original")).unwrap();
        std::fs::create_dir(&selected).unwrap();
        // The replacement is deliberately not a repository. A path reopen fails this control.
        let cwd_fd = rustix::io::dup(&*root.directory).unwrap();
        let cwd = PathBuf::from(format!("/dev/fd/{}", cwd_fd.as_raw_fd()));
        let mut reader =
            GitReader::with_directory_fd(&cwd, cwd_fd, Instant::now() + Duration::from_secs(30))
                .unwrap();
        assert_eq!(
            reader
                .text(&["rev-parse", "--is-inside-work-tree"], 128)
                .unwrap(),
            "true"
        );
        assert!(
            root.verify_binding().is_err(),
            "public reads still reject the moved grant"
        );
    }
}
