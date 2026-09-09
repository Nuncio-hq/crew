//! Bounded Git discovery from digest-verified raw commit and tree objects.
use crate::source_git_command::{self, GitOutput};
use crate::source_snapshot::{
    source_hash, source_path_from_bytes, valid_source_path, MAX_SOURCE_BYTES, MAX_SOURCE_FILES,
};
use crate::WikiError;
#[cfg(unix)]
use rustix::fd::OwnedFd;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 20_000;
const MAX_DEPTH: usize = 64;
const MAX_PATH_BYTES: usize = 4096;
const MAX_COMMANDS: usize = 25_008;

pub(crate) struct GitReader<'a> {
    root: &'a Path,
    deadline: Instant,
    commands: usize,
    blob_reads: usize,
    #[cfg(unix)]
    cwd_fd: Option<OwnedFd>,
}
impl<'a> GitReader<'a> {
    pub(crate) fn new(root: &'a Path) -> Self {
        Self::with_deadline(root, Instant::now() + Duration::from_secs(180))
    }
    pub(crate) fn with_deadline(root: &'a Path, deadline: Instant) -> Self {
        Self {
            root,
            deadline,
            commands: 0,
            blob_reads: 0,
            #[cfg(unix)]
            cwd_fd: None,
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn with_directory_fd(
        root: &'a Path,
        cwd_fd: OwnedFd,
        deadline: Instant,
    ) -> Result<Self, WikiError> {
        use rustix::io::{fcntl_setfd, FdFlags};
        fcntl_setfd(&cwd_fd, FdFlags::empty()).map_err(|_| invalid())?;
        Ok(Self {
            root,
            deadline,
            commands: 0,
            blob_reads: 0,
            cwd_fd: Some(cwd_fd),
        })
    }
    pub(crate) fn run(&mut self, args: &[&str], limit: usize) -> Result<GitOutput, WikiError> {
        self.commands = self.commands.checked_add(1).ok_or_else(invalid)?;
        if self.commands > MAX_COMMANDS {
            return Err(invalid());
        }
        #[cfg(unix)]
        if let Some(directory) = self.cwd_fd.as_ref() {
            return source_git_command::run_with_directory_fd(
                directory,
                args,
                limit,
                self.deadline,
            );
        }
        source_git_command::run(self.root, args, limit, self.deadline)
    }
    pub(crate) fn bytes(&mut self, args: &[&str], limit: usize) -> Result<Vec<u8>, WikiError> {
        let output = self.run(args, limit)?;
        if !output.success {
            return Err(invalid());
        }
        Ok(output.bytes)
    }
    pub(crate) fn text(&mut self, args: &[&str], limit: usize) -> Result<String, WikiError> {
        String::from_utf8(self.bytes(args, limit)?)
            .map(|s| s.trim_end_matches('\n').to_owned())
            .map_err(|_| invalid())
    }
    pub(crate) fn blob(&mut self, oid: &str, size: usize) -> Result<Vec<u8>, WikiError> {
        self.blob_reads = self.blob_reads.checked_add(1).ok_or_else(invalid)?;
        if self.blob_reads > MAX_SOURCE_FILES + 1 {
            return Err(invalid());
        }
        let bytes = self.bytes(&["cat-file", "blob", oid], size)?;
        if bytes.len() != size {
            return Err(invalid());
        }
        verify_object(&bytes, "blob", oid)?;
        Ok(bytes)
    }
}

pub(crate) struct TreeEntry {
    pub path: String,
    pub mode: String,
    pub oid: String,
}

/// Resolve only the authenticated path through verified raw objects.
#[cfg(target_os = "linux")]
pub(crate) fn source_blob(
    reader: &mut GitReader<'_>,
    commit: &str,
    path: &str,
    size: usize,
) -> Result<Vec<u8>, WikiError> {
    if !valid_oid(commit, commit.len()) || !valid_source_path(path) {
        return Err(invalid());
    }
    let raw = reader.bytes(&["cat-file", "commit", commit], 1024 * 1024)?;
    verify_object(&raw, "commit", commit)?;
    let first = raw.split(|b| *b == b'\n').next().ok_or_else(invalid)?;
    let mut oid = std::str::from_utf8(first)
        .map_err(|_| invalid())?
        .strip_prefix("tree ")
        .filter(|id| valid_oid(id, commit.len()))
        .ok_or_else(invalid)?
        .to_owned();
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() > MAX_DEPTH || path.len() > MAX_PATH_BYTES {
        return Err(invalid());
    }
    let mut remaining = MAX_SOURCE_BYTES
        .checked_sub(raw.len())
        .ok_or_else(invalid)?;
    for (index, wanted) in parts.iter().enumerate() {
        let raw = reader.bytes(&["cat-file", "tree", &oid], remaining)?;
        verify_object(&raw, "tree", &oid)?;
        remaining = remaining.checked_sub(raw.len()).ok_or_else(invalid)?;
        let mut cursor = 0;
        let mut found = None;
        let mut names = BTreeSet::new();
        while cursor < raw.len() {
            let rest = &raw[cursor..];
            let space = rest.iter().position(|b| *b == b' ').ok_or_else(invalid)?;
            let mode = std::str::from_utf8(&rest[..space]).map_err(|_| invalid())?;
            if !matches!(
                mode,
                "40000" | "040000" | "100644" | "100755" | "120000" | "160000"
            ) {
                return Err(invalid());
            }
            let start = cursor + space + 1;
            let end = start
                + raw[start..]
                    .iter()
                    .position(|b| *b == 0)
                    .ok_or_else(invalid)?;
            let name = source_path_from_bytes(&raw[start..end])?;
            if name.contains('/')
                || !valid_source_path(name)
                || !names.insert(name)
                || names.len() > MAX_ENTRIES
            {
                return Err(invalid());
            }
            let next = end + 1 + commit.len() / 2;
            let id = raw.get(end + 1..next).ok_or_else(invalid)?;
            if name == *wanted {
                found = Some((
                    mode,
                    id.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                ));
            }
            cursor = next;
        }
        let (mode, child) = found.ok_or_else(invalid)?;
        if index + 1 == parts.len() {
            if !matches!(mode, "100644" | "100755") {
                return Err(invalid());
            }
            return reader.blob(&child, size);
        }
        if !matches!(mode, "40000" | "040000") {
            return Err(invalid());
        }
        oid = child;
    }
    Err(invalid())
}
struct Budget {
    metadata: usize,
    entries: usize,
    paths: usize,
    seen: BTreeSet<String>,
}

pub(crate) fn discover(
    reader: &mut GitReader<'_>,
    commit: &str,
) -> Result<Vec<TreeEntry>, WikiError> {
    let raw = reader.bytes(&["cat-file", "commit", commit], 1024 * 1024)?;
    verify_object(&raw, "commit", commit)?;
    let first = raw.split(|b| *b == b'\n').next().ok_or_else(invalid)?;
    let root = std::str::from_utf8(first)
        .map_err(|_| invalid())?
        .strip_prefix("tree ")
        .ok_or_else(invalid)?;
    if !valid_oid(root, commit.len()) {
        return Err(invalid());
    }
    let mut budget = Budget {
        metadata: raw.len(),
        entries: 0,
        paths: 0,
        seen: BTreeSet::new(),
    };
    let mut entries = Vec::new();
    walk(reader, root, "", 0, &mut budget, &mut entries)?;
    Ok(entries)
}

fn walk(
    reader: &mut GitReader<'_>,
    oid: &str,
    prefix: &str,
    depth: usize,
    budget: &mut Budget,
    entries: &mut Vec<TreeEntry>,
) -> Result<(), WikiError> {
    if depth > MAX_DEPTH {
        return Err(invalid());
    }
    let remaining = MAX_SOURCE_BYTES
        .checked_sub(budget.metadata)
        .ok_or_else(invalid)?;
    let raw = reader.bytes(&["cat-file", "tree", oid], remaining)?;
    // These exact verified bytes, never a subsequent ls-tree, supply every path and child ID.
    verify_object(&raw, "tree", oid)?;
    budget.metadata = budget.metadata.checked_add(raw.len()).ok_or_else(invalid)?;
    let mut cursor = 0_usize;
    while cursor < raw.len() {
        budget.entries = budget.entries.checked_add(1).ok_or_else(invalid)?;
        if budget.entries > MAX_ENTRIES {
            return Err(invalid());
        }
        let rest = &raw[cursor..];
        let space = rest.iter().position(|b| *b == b' ').ok_or_else(invalid)?;
        let mode = std::str::from_utf8(&rest[..space]).map_err(|_| invalid())?;
        if !matches!(
            mode,
            "40000" | "040000" | "100644" | "100755" | "120000" | "160000"
        ) {
            return Err(invalid());
        }
        let name_start = cursor.checked_add(space + 1).ok_or_else(invalid)?;
        let name_len = raw[name_start..]
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(invalid)?;
        let name_end = name_start.checked_add(name_len).ok_or_else(invalid)?;
        let name = source_path_from_bytes(&raw[name_start..name_end])?;
        if name.contains('/') || !valid_source_path(name) {
            return Err(invalid());
        }
        let path = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        budget.paths = budget.paths.checked_add(path.len()).ok_or_else(invalid)?;
        if path.len() > MAX_PATH_BYTES
            || budget.paths > MAX_SOURCE_BYTES
            || !budget.seen.insert(path.clone())
        {
            return Err(invalid());
        }
        let id_start = name_end.checked_add(1).ok_or_else(invalid)?;
        let id_end = id_start.checked_add(oid.len() / 2).ok_or_else(invalid)?;
        let id_bytes = raw.get(id_start..id_end).ok_or_else(invalid)?;
        let child: String = id_bytes.iter().map(|b| format!("{b:02x}")).collect();
        cursor = id_end;
        if matches!(mode, "40000" | "040000") {
            walk(reader, &child, &path, depth + 1, budget, entries)?;
        } else {
            entries.push(TreeEntry {
                path,
                mode: mode.into(),
                oid: child,
            });
        }
    }
    Ok(())
}

fn valid_oid(oid: &str, width: usize) -> bool {
    matches!(width, 40 | 64)
        && oid.len() == width
        && oid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn verify_object(bytes: &[u8], kind: &str, oid: &str) -> Result<(), WikiError> {
    use nostr::hashes::{sha1, Hash};
    let mut object = format!("{kind} {}\0", bytes.len()).into_bytes();
    object.extend_from_slice(bytes);
    let actual = if oid.len() == 40 {
        sha1::Hash::hash(&object).to_string()
    } else {
        source_hash(&object)
    };
    if actual != oid {
        return Err(invalid());
    }
    Ok(())
}
fn invalid() -> WikiError {
    WikiError::Git("Git source object identity, structure or capture limit unavailable".into())
}
