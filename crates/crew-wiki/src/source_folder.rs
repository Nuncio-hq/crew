//! Folder capture uses descriptor-relative, no-follow reads below the trusted bound root.

use crate::git_snapshot::RepoSnapshot;
use crate::WikiError;
use std::path::Path;

/// Capture an already authorized local folder without creating Git metadata.
/// The native caller must supply its bound root and repository coordinate.
pub fn capture_folder(root: &Path, coordinate: &str) -> Result<RepoSnapshot, WikiError> {
    #[cfg(unix)]
    {
        capture_inner(root, coordinate, &mut |_, _| {})
    }
    #[cfg(not(unix))]
    {
        let _ = (root, coordinate);
        Err(WikiError::Git(
            "safe folder capture unavailable on this host".into(),
        ))
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapturePoint {
    BeforeOpenDirectory,
    AfterOpenDirectory,
    BeforeOpenFile,
    AfterOpenFile,
    AfterReadAll,
}
#[cfg(unix)]
pub(crate) type Hook<'a> = dyn FnMut(CapturePoint, &str) + 'a;

#[cfg(unix)]
fn capture_inner(
    root: &Path,
    coordinate: &str,
    hook: &mut Hook<'_>,
) -> Result<RepoSnapshot, WikiError> {
    use crate::source_folder_walk::{error, stamp, Listing, Root};
    use crate::source_snapshot::{
        source_hash, SourceOmission, SourceOmissionReason, MAX_SOURCE_BYTES, MAX_SOURCE_FILES,
        MAX_SOURCE_FILE_BYTES,
    };
    use std::collections::{BTreeMap, BTreeSet};
    use std::time::{Duration, Instant};
    let (owner, repo) = buzz_core::wiki_page::parse_wiki_repo_a_tag(coordinate)
        .map_err(|_| error("invalid source repository coordinate"))?;
    if coordinate != format!("30617:{owner}:{repo}") {
        return Err(error("noncanonical source repository coordinate"));
    }
    let deadline = Instant::now() + Duration::from_secs(180);
    let root = Root::open(root)?;
    let before = Listing::capture(&root, deadline, hook)?;
    let mut retained = Vec::new();
    let mut preloaded = BTreeMap::new();
    let mut steered = BTreeSet::new();
    if let Some(entry) = before.files.get(".crew/wiki.json") {
        if entry.stamp.size < 0 || entry.stamp.size as usize > MAX_SOURCE_FILE_BYTES {
            return Err(error("Wiki steering exceeds source limit"));
        }
        let Some((file, bytes)) = read_file(entry, ".crew/wiki.json", hook)? else {
            return Err(error("Wiki steering unavailable"));
        };
        let text = String::from_utf8(bytes).map_err(|_| error("Wiki steering is not UTF-8"))?;
        let steering = crate::steering::parse_steering(&text)?;
        for page in steering.pages.into_iter().flatten() {
            for path in page.source_files.into_iter().flatten() {
                if !crate::source_snapshot::valid_source_path(&path) {
                    return Err(error("invalid steered source path"));
                }
                steered.insert(path);
                if steered.len() > MAX_SOURCE_FILES {
                    return Err(error("steered source limit exceeded"));
                }
            }
        }
        retained.push((file, entry.stamp.clone()));
        preloaded.insert(".crew/wiki.json".to_owned(), text);
    }
    let selected = |path: &str| {
        steered.contains(path)
            || !crate::cluster::filter_source_files(&[path.to_owned()]).is_empty()
    };
    let mut snapshot = RepoSnapshot {
        omissions: before.omissions.clone(),
        ..RepoSnapshot::default()
    };
    let mut total = 0_usize;
    for (path, entry) in &before.files {
        if Instant::now() >= deadline {
            return Err(error("source capture deadline reached"));
        }
        let mut omitted = None;
        let content = if !selected(path) {
            omitted = Some(SourceOmissionReason::Excluded);
            None
        } else if entry.stamp.size < 0 {
            return Err(error("invalid source size"));
        } else if entry.stamp.size as usize > MAX_SOURCE_FILE_BYTES {
            omitted = Some(SourceOmissionReason::Oversized);
            None
        } else {
            if snapshot.files.len() >= MAX_SOURCE_FILES
                || total.saturating_add(entry.stamp.size as usize) > MAX_SOURCE_BYTES
            {
                return Err(error("source capture quota exceeded"));
            }
            if let Some(text) = preloaded.remove(path) {
                Some(text)
            } else {
                match read_file(entry, path, hook)? {
                    None => {
                        omitted = Some(SourceOmissionReason::Unreadable);
                        None
                    }
                    Some((file, bytes)) => {
                        retained.push((file, entry.stamp.clone()));
                        match String::from_utf8(bytes) {
                            Ok(text) if !text.contains('\0') => Some(text),
                            _ => {
                                omitted = Some(SourceOmissionReason::Binary);
                                None
                            }
                        }
                    }
                }
            }
        };
        if let Some(reason) = omitted {
            snapshot.omissions.push(SourceOmission {
                path: path.clone(),
                reason,
            });
        }
        if let Some(content) = content {
            total += content.len();
            snapshot.files.push(path.clone());
            snapshot.contents.insert(path.clone(), content);
        }
    }
    hook(CapturePoint::AfterReadAll, "");
    let after = Listing::capture(&root, deadline, hook)?;
    if before.directories != after.directories
        || before.omissions != after.omissions
        || before.files.keys().ne(after.files.keys())
    {
        return Err(error("source folder changed during capture"));
    }
    for (path, entry) in &before.files {
        if selected(path)
            && after
                .files
                .get(path)
                .is_none_or(|now| now.stamp != entry.stamp)
        {
            return Err(error("source file changed during capture"));
        }
    }
    for (file, expected) in &retained {
        if stamp(file)? != *expected {
            return Err(error("source file changed during capture"));
        }
    }
    root.verify_binding()?;
    snapshot.omissions.sort_by(|a, b| a.path.cmp(&b.path));
    let files: Vec<_> = snapshot
        .contents
        .iter()
        .map(|(path, content)| (path, source_hash(content.as_bytes()), content.len() as u64))
        .collect();
    let omissions: Vec<_> = snapshot
        .omissions
        .iter()
        .map(|o| (&o.path, o.reason))
        .collect();
    let manifest = serde_json::to_vec(&(1, coordinate, files, omissions))
        .map_err(|_| error("source manifest unavailable"))?;
    snapshot.source_revision = format!("folder:{}", source_hash(&manifest));
    snapshot.commit = snapshot.source_revision.clone();
    Ok(snapshot)
}

#[cfg(unix)]
fn read_file(
    entry: &crate::source_folder_walk::Entry,
    path: &str,
    hook: &mut Hook<'_>,
) -> Result<Option<(std::fs::File, Vec<u8>)>, WikiError> {
    use crate::source_folder_walk::{error, stamp};
    use crate::source_snapshot::MAX_SOURCE_FILE_BYTES;
    use rustix::fs::{openat, Mode, OFlags};
    use std::io::Read;
    hook(CapturePoint::BeforeOpenFile, path);
    let fd = match openat(
        &*entry.parent,
        entry.name.as_str(),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    ) {
        Ok(fd) => fd,
        Err(rustix::io::Errno::ACCESS) => return Ok(None),
        Err(_) => return Err(error("source file changed or became a symlink")),
    };
    hook(CapturePoint::AfterOpenFile, path);
    if stamp(&fd)? != entry.stamp {
        return Err(error("source file identity changed"));
    }
    let mut file = std::fs::File::from(fd);
    let mut bytes = Vec::new();
    if (&mut file)
        .take((MAX_SOURCE_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Ok(None);
    }
    if bytes.len() > MAX_SOURCE_FILE_BYTES
        || bytes.len() as i64 != entry.stamp.size
        || stamp(&file)? != entry.stamp
    {
        return Err(error("source file changed during capture"));
    }
    Ok(Some((file, bytes)))
}

#[cfg(all(test, unix))]
#[path = "source_folder_tests.rs"]
mod tests;
