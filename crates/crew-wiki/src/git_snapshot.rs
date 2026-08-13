//! Local git snapshot + rename-aware diff.

use crate::WikiError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Files and HEAD metadata for planning / generation.
#[derive(Debug, Clone, Default)]
pub struct RepoSnapshot {
    /// HEAD commit (full or abbreviated).
    pub commit: String,
    /// Default / current branch name.
    pub branch: String,
    /// Source-ish paths (not yet filtered).
    pub files: Vec<String>,
    /// Optional file contents (tests / heuristic generator).
    pub contents: BTreeMap<String, String>,
}

impl RepoSnapshot {
    /// Paths only.
    pub fn paths(&self) -> Vec<String> {
        self.files.clone()
    }

    /// Build from a local git worktree via `git ls-tree`.
    pub fn from_git(root: &Path) -> Result<Self, WikiError> {
        let commit = git_stdout(root, &["rev-parse", "HEAD"])?;
        let branch = git_stdout(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let tree = git_stdout(root, &["ls-tree", "-r", "--name-only", "HEAD"])?;
        let files = tree
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(ToString::to_string)
            .collect();
        Ok(Self {
            commit,
            branch,
            files,
            contents: BTreeMap::new(),
        })
    }

    /// Read a file from the snapshot cache or disk.
    pub fn read(&self, path: &str, root: Option<&Path>) -> String {
        if let Some(cached) = self.contents.get(path) {
            return cached.clone();
        }
        if let Some(root) = root {
            if let Ok(bytes) = std::fs::read_to_string(root.join(path)) {
                return bytes.chars().take(8_000).collect();
            }
        }
        String::new()
    }
}

/// One path change between commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    /// Current path (empty on delete).
    pub path: String,
    /// Previous path (empty on add; set on rename).
    pub old_path: Option<String>,
    /// Change kind.
    pub kind: FileChangeKind,
}

/// Git-style change kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileChangeKind {
    /// Added.
    Add,
    /// Modified.
    Modify,
    /// Deleted.
    Delete,
    /// Renamed or moved.
    Rename,
}

/// `git diff --name-status` between two commits.
pub fn diff_commits(root: &Path, old: &str, new: &str) -> Result<Vec<FileChange>, WikiError> {
    let out = git_stdout(root, &["diff", "--name-status", "--find-renames", old, new])?;
    Ok(parse_name_status(&out))
}

/// Parse `git diff --name-status` output (also used by tests without git).
pub fn parse_name_status(raw: &str) -> Vec<FileChange> {
    let mut changes = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let status = parts.next().unwrap_or("");
        let code = status.chars().next().unwrap_or('M');
        match code {
            'A' => {
                if let Some(path) = parts.next() {
                    changes.push(FileChange {
                        path: path.to_string(),
                        old_path: None,
                        kind: FileChangeKind::Add,
                    });
                }
            }
            'D' => {
                if let Some(path) = parts.next() {
                    changes.push(FileChange {
                        path: path.to_string(),
                        old_path: Some(path.to_string()),
                        kind: FileChangeKind::Delete,
                    });
                }
            }
            'R' => {
                let old = parts.next().unwrap_or("");
                let new = parts.next().unwrap_or("");
                changes.push(FileChange {
                    path: new.to_string(),
                    old_path: Some(old.to_string()),
                    kind: FileChangeKind::Rename,
                });
            }
            _ => {
                if let Some(path) = parts.next() {
                    changes.push(FileChange {
                        path: path.to_string(),
                        old_path: None,
                        kind: FileChangeKind::Modify,
                    });
                }
            }
        }
    }
    changes
}

/// List source files (helper for MCP / CLI).
pub fn list_source_files(root: &Path) -> Result<Vec<String>, WikiError> {
    Ok(RepoSnapshot::from_git(root)?.files)
}

fn git_stdout(root: &Path, args: &[&str]) -> Result<String, WikiError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| WikiError::Git(e.to_string()))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(WikiError::Git(err.trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Resolve a repo path for tests.
pub fn fixture_root(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rename_and_modify() {
        let raw = "M\tsrc/lib.rs\nR100\told/a.rs\tnew/b.rs\nA\tadded.md\nD\tgone.rs\n";
        let changes = parse_name_status(raw);
        assert_eq!(changes.len(), 4);
        assert_eq!(changes[1].kind, FileChangeKind::Rename);
        assert_eq!(changes[1].old_path.as_deref(), Some("old/a.rs"));
        assert_eq!(changes[1].path, "new/b.rs");
        assert_eq!(changes[2].kind, FileChangeKind::Add);
        assert_eq!(changes[3].kind, FileChangeKind::Delete);
    }
}
