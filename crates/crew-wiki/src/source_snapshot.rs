//! Exact local source revisions extend the existing Wiki repository snapshot.

use crate::git_snapshot::RepoSnapshot;
use crate::source_git_tree::{self, GitReader};
use crate::WikiError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Maximum selected source files per generation.
pub const MAX_SOURCE_FILES: usize = 5_000;
/// Maximum complete source bytes in one file.
pub const MAX_SOURCE_FILE_BYTES: usize = 1024 * 1024;
/// Maximum complete selected source bytes in one generation.
pub const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;

/// Explicit Git selection; neither option fetches or changes the checkout.
#[derive(Clone, Debug)]
pub enum GitSourceChoice {
    /// Capture the branch currently selected by HEAD, or detached HEAD.
    CurrentHead,
    /// Use an already configured and locally resolvable remote HEAD.
    RemoteDefault(String),
}

/// Closed source-coverage omission taxonomy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceOmissionReason {
    /// Excluded by the existing source selection rules.
    Excluded,
    /// Content is not supported UTF-8 text.
    Binary,
    /// Complete file exceeds the source byte limit.
    Oversized,
    /// Symlink entries are never followed.
    Symlink,
    /// Gitlinks are not regular source blobs.
    Submodule,
    /// Path is not a supported relative locator.
    InvalidPath,
    /// Source cannot be read.
    Unreadable,
}

/// One explicit omitted path. Detected capture changes are errors, not omissions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceOmission {
    /// Exact valid UTF-8 repository-relative path.
    pub path: String,
    /// Why the path is not included in the captured source.
    pub reason: SourceOmissionReason,
}

/// Canonical source locator: path, SHA256 bytes, byte count, inclusive line range.
pub type SourceReference = (String, String, u64, u64, u64);

/// SHA256 of complete captured bytes; never of a displayed/truncated excerpt.
pub fn source_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn source_path_from_bytes(bytes: &[u8]) -> Result<&str, WikiError> {
    std::str::from_utf8(bytes).map_err(|_| unavailable("non-UTF-8 source paths are unsupported"))
}

/// Exact relative locator grammar, with no Unicode, case or whitespace normalization.
pub fn valid_source_path(path: &str) -> bool {
    if path.is_empty() || path.contains(['\0', '\\']) {
        return false;
    }
    if path.len() >= 2 && path.as_bytes()[0].is_ascii_alphabetic() && path.as_bytes()[1] == b':' {
        return false;
    }
    path.split('/')
        .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Bind an inclusive LF line range to the complete in-memory source file.
pub fn source_reference(
    snapshot: &RepoSnapshot,
    path: &str,
    start: u64,
    end: u64,
) -> Result<SourceReference, WikiError> {
    let content = snapshot
        .contents
        .get(path)
        .ok_or_else(|| unavailable("source is unavailable"))?;
    let lines = if content.is_empty() {
        0
    } else {
        content.split_terminator('\n').count() as u64
    };
    if !valid_source_path(path) || start == 0 || end < start || end > lines {
        return Err(unavailable("source line range is unavailable"));
    }
    Ok((
        path.into(),
        source_hash(content.as_bytes()),
        content.len() as u64,
        start,
        end,
    ))
}

/// Capture immutable regular Git blobs. Worktree bytes are never a fallback.
pub fn capture_git(root: &Path, choice: GitSourceChoice) -> Result<RepoSnapshot, WikiError> {
    let mut reader = GitReader::new(root);
    let (reference, branch) = resolve_ref(&mut reader, choice)?;
    let commit = reader.text(
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ],
        128,
    )?;
    if !valid_object_id(&commit) {
        return Err(unavailable("invalid Git source revision"));
    }
    let tree = source_git_tree::discover(&mut reader, &commit)?;
    let mut steered_sources = BTreeSet::new();
    // Steering comes from the same captured Git revision, never dirty disk.
    for entry in &tree {
        if entry.path != ".crew/wiki.json" {
            continue;
        }
        if !matches!(entry.mode.as_str(), "100644" | "100755") {
            return Err(unavailable("Wiki steering is not a regular source blob"));
        }
        let size: usize = reader
            .text(&["cat-file", "-s", &entry.oid], 128)?
            .parse()
            .map_err(|_| unavailable("invalid Wiki steering size"))?;
        if size > MAX_SOURCE_FILE_BYTES {
            return Err(unavailable("Wiki steering exceeds source limit"));
        }
        let raw = reader.blob(&entry.oid, size)?;
        let text =
            std::str::from_utf8(&raw).map_err(|_| unavailable("Wiki steering is not UTF-8"))?;
        let steering = crate::steering::parse_steering(text)?;
        for page in steering.pages.into_iter().flatten() {
            for path in page.source_files.into_iter().flatten() {
                if !valid_source_path(&path) {
                    return Err(unavailable("invalid steered source path"));
                }
                steered_sources.insert(path);
                if steered_sources.len() > MAX_SOURCE_FILES {
                    return Err(unavailable("steered source limit exceeded"));
                }
            }
        }
    }
    let mut snapshot = RepoSnapshot {
        commit: commit.clone(),
        branch,
        source_revision: format!("git:{commit}"),
        files: Vec::new(),
        contents: BTreeMap::new(),
        omissions: Vec::new(),
    };
    let mut total = 0_usize;
    for entry in &tree {
        let path = entry.path.as_str();
        let reason = if entry.mode == "120000" {
            Some(SourceOmissionReason::Symlink)
        } else if entry.mode == "160000" {
            Some(SourceOmissionReason::Submodule)
        } else {
            None
        };
        if let Some(reason) = reason {
            omit(&mut snapshot, path, reason);
            continue;
        }
        if !steered_sources.contains(path)
            && crate::cluster::filter_source_files(&[path.to_owned()]).is_empty()
        {
            omit(&mut snapshot, path, SourceOmissionReason::Excluded);
            continue;
        }
        let size: usize = reader
            .text(&["cat-file", "-s", &entry.oid], 128)?
            .parse()
            .map_err(|_| unavailable("invalid Git blob size"))?;
        if size > MAX_SOURCE_FILE_BYTES {
            omit(&mut snapshot, path, SourceOmissionReason::Oversized);
            continue;
        }
        if snapshot.files.len() >= MAX_SOURCE_FILES || total.saturating_add(size) > MAX_SOURCE_BYTES
        {
            return Err(unavailable("source capture quota exceeded"));
        }
        let raw = reader.blob(&entry.oid, size)?;
        let content = match String::from_utf8(raw) {
            Ok(content) if !content.contains('\0') => content,
            _ => {
                omit(&mut snapshot, path, SourceOmissionReason::Binary);
                continue;
            }
        };
        total += size;
        snapshot.files.push(path.into());
        snapshot.contents.insert(path.into(), content);
    }
    snapshot.files.sort();
    snapshot.omissions.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(snapshot)
}

fn resolve_ref(
    reader: &mut GitReader<'_>,
    choice: GitSourceChoice,
) -> Result<(String, String), WikiError> {
    match choice {
        GitSourceChoice::CurrentHead => {
            let result = reader.run(&["symbolic-ref", "--quiet", "HEAD"], 4096)?;
            if result.success {
                let reference = String::from_utf8(result.bytes)
                    .map_err(|_| unavailable("invalid Git branch"))?
                    .trim_end_matches('\n')
                    .to_owned();
                let branch = reference
                    .strip_prefix("refs/heads/")
                    .ok_or_else(|| unavailable("unsupported HEAD reference"))?
                    .to_owned();
                Ok((reference, branch))
            } else {
                Ok(("HEAD".into(), String::new()))
            }
        }
        GitSourceChoice::RemoteDefault(remote) => {
            if remote.is_empty()
                || remote.starts_with('-')
                || !remote
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
            {
                return Err(unavailable("configured remote HEAD is unavailable"));
            }
            let name = format!("refs/remotes/{remote}/HEAD");
            let reference = reader.text(&["symbolic-ref", "--quiet", &name], 4096)?;
            let branch = reference
                .strip_prefix(&format!("refs/remotes/{remote}/"))
                .ok_or_else(|| unavailable("configured remote HEAD is unavailable"))?
                .to_owned();
            Ok((reference, branch))
        }
    }
}

fn omit(snapshot: &mut RepoSnapshot, path: &str, reason: SourceOmissionReason) {
    snapshot.omissions.push(SourceOmission {
        path: path.into(),
        reason,
    });
}
fn valid_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn unavailable(message: &str) -> WikiError {
    WikiError::Git(message.into())
}
