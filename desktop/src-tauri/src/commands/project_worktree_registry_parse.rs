use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectWorktreeKind {
    Main,
    Managed,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PorcelainWorktree {
    pub worktree_path: PathBuf,
    pub head: String,
    pub branch: Option<String>,
    pub prunable: bool,
    pub bare: bool,
}

/// Split `git worktree list --porcelain` into entries.
pub fn parse_worktree_porcelain(text: &str) -> Vec<PorcelainWorktree> {
    let mut entries = Vec::new();
    let mut current: Option<PorcelainWorktree> = None;
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(PorcelainWorktree {
                worktree_path: PathBuf::from(path),
                head: String::new(),
                branch: None,
                prunable: false,
                bare: false,
            });
            continue;
        }
        let Some(entry) = current.as_mut() else {
            continue;
        };
        if let Some(head) = line.strip_prefix("HEAD ") {
            entry.head = head.to_string();
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            entry.branch = Some(branch.to_string());
        } else if line == "detached" {
            entry.branch = None;
        } else if line.starts_with("prunable") {
            entry.prunable = true;
        } else if line == "bare" {
            entry.bare = true;
        }
    }
    if let Some(entry) = current.take() {
        entries.push(entry);
    }
    entries
}

/// Parse `git config --get-regexp '^branch\..*\.buzzthreadroot$'`.
/// Keys are case-insensitive; branch names keep their casing.
pub fn parse_buzz_thread_roots(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let value = value.trim();
        if !is_hex64(value) {
            continue;
        }
        let Some(branch) = branch_from_buzz_thread_root_key(key) else {
            continue;
        };
        out.push((branch, value.to_ascii_lowercase()));
    }
    out
}

fn branch_from_buzz_thread_root_key(key: &str) -> Option<String> {
    let key = key.trim();
    let rest = key.strip_prefix("branch.")?;
    // Case-insensitive suffix: git lowercases the key in --get-regexp output.
    let suffix = ".buzzthreadroot";
    if rest.len() <= suffix.len() || !rest.to_ascii_lowercase().ends_with(suffix) {
        return None;
    }
    Some(rest[..rest.len() - suffix.len()].to_string())
}

fn is_hex64(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn classify_worktree(
    worktree_path: &Path,
    _branch: Option<&str>,
    managed_root: &Path,
    is_primary: bool,
) -> ProjectWorktreeKind {
    if is_primary {
        return ProjectWorktreeKind::Main;
    }
    let Ok(canonical) = std::fs::canonicalize(worktree_path) else {
        // Path may be prunable / missing — still classify by parent string.
        return classify_by_parent_string(worktree_path, managed_root);
    };
    classify_by_parent_string(&canonical, managed_root)
}

fn classify_by_parent_string(worktree_path: &Path, managed_root: &Path) -> ProjectWorktreeKind {
    match worktree_path.parent() {
        Some(parent) if parent == managed_root => ProjectWorktreeKind::Managed,
        _ => ProjectWorktreeKind::External,
    }
}

pub fn is_managed_branch(branch: &str) -> bool {
    let Some(short) = branch.strip_prefix("buzz/") else {
        return false;
    };
    short.len() == 12 && short.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

pub fn worktree_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("worktree")
        .to_string()
}

pub fn managed_root_for(repo_root: &Path) -> Option<PathBuf> {
    Some(repo_root.parent()?.join(".buzz-worktrees"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_kinds() {
        let text = "\
worktree /repo/crew
HEAD abc
branch refs/heads/main

worktree /repo/.buzz-worktrees/crew-02cc85801c3d
HEAD def
branch refs/heads/buzz/02cc85801c3d

worktree /repo/.worktrees/crew-docs-fork-identity
HEAD ghi
branch refs/heads/docs/nunciocrew-fork-identity

worktree /repo/.buzz-worktrees/gone
HEAD jkl
detached
prunable gitdir file points to non-existent location

";
        let entries = parse_worktree_porcelain(text);
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert_eq!(entries[1].branch.as_deref(), Some("buzz/02cc85801c3d"));
        assert!(entries[2].branch.as_deref().unwrap().starts_with("docs/"));
        assert!(entries[3].branch.is_none());
        assert!(entries[3].prunable);
    }

    #[test]
    fn parses_lowercase_buzzthreadroot_keys_with_dots_in_branch() {
        let text = "\
branch.buzz/eb791333c0ee.buzzthreadroot eb791333c0ee702ed09d4c55403852a33c086a7cb68beda99a9e401ab2c1436a
branch.feature.with.dots.buzzThreadRoot aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
branch.short.buzzthreadroot deadbeef
branch.bad.buzzthreadroot not-hex
";
        let roots = parse_buzz_thread_roots(text);
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].0, "buzz/eb791333c0ee");
        assert_eq!(roots[1].0, "feature.with.dots");
    }

    #[test]
    fn classifies_managed_orphan_and_external() {
        let repo = Path::new("/repo/crew");
        let managed = Path::new("/repo/.buzz-worktrees");
        assert_eq!(
            classify_worktree(repo, Some("main"), managed, true),
            ProjectWorktreeKind::Main
        );
        assert_eq!(
            classify_worktree(
                Path::new("/repo/.buzz-worktrees/crew-02cc85801c3d"),
                Some("buzz/02cc85801c3d"),
                managed,
                false,
            ),
            ProjectWorktreeKind::Managed
        );
        assert_eq!(
            classify_worktree(
                Path::new("/repo/.buzz-worktrees/crew-ws-feature-x"),
                Some("feature/x"),
                managed,
                false,
            ),
            ProjectWorktreeKind::Managed
        );
        assert_eq!(
            classify_worktree(
                Path::new("/repo/.worktrees/crew-docs-fork-identity"),
                Some("docs/nunciocrew-fork-identity"),
                managed,
                false,
            ),
            ProjectWorktreeKind::External
        );
    }

    #[test]
    fn canonical_checkout_is_main_not_a_managed_gc_candidate() {
        let managed = Path::new("/repo/.buzz-worktrees");
        assert_eq!(
            classify_worktree(Path::new("/repo/crew"), Some("main"), managed, true),
            ProjectWorktreeKind::Main
        );
    }
}
