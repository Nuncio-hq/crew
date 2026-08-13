//! Shared-checkout helpers: worktree list, dirty count, attach without `-b`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use tokio::process::Command;

pub(crate) struct ListedWorktree {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub is_primary: bool,
}

pub(crate) async fn list_worktrees(repo_root: &Path) -> Result<Vec<ListedWorktree>> {
    let output = git_output(repo_root, ["worktree", "list", "--porcelain"]).await?;
    Ok(parse_worktree_porcelain(&output, repo_root))
}

pub(crate) fn find_worktree_for_branch<'a>(
    list: &'a [ListedWorktree],
    branch: &str,
) -> Option<&'a ListedWorktree> {
    list.iter()
        .find(|entry| entry.branch.as_deref() == Some(branch))
}

pub(crate) fn existing_branch_worktree_path(repo_root: &Path, branch: &str) -> Result<PathBuf> {
    let repo_name = repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let parent = repo_root
        .parent()
        .context("git repository has no parent directory")?
        .join(".buzz-worktrees");
    Ok(parent.join(format!(
        "{repo_name}-ws-{}",
        sanitize_branch_for_path(branch)
    )))
}

pub(crate) fn sanitize_branch_for_path(branch: &str) -> String {
    let sanitized: String = branch
        .chars()
        .map(|character| match character {
            '/' | '\\' => '-',
            character
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') =>
            {
                character
            }
            _ => '-',
        })
        .collect();
    let sanitized = sanitized.trim_matches('-');
    let sanitized = if sanitized.is_empty() {
        "branch"
    } else {
        sanitized
    };
    sanitized.chars().take(80).collect()
}

pub(crate) async fn uncommitted_count(path: &Path) -> u64 {
    let Ok(status) = git_output(path, ["status", "--porcelain"]).await else {
        return 0;
    };
    status
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .count() as u64
}

pub(crate) async fn current_branch(path: &Path) -> String {
    git_output(path, ["symbolic-ref", "--short", "HEAD"])
        .await
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "HEAD".to_string())
}

pub(crate) async fn attach_existing_branch(
    repo_root: &Path,
    worktree_path: &Path,
    branch: &str,
) -> Result<()> {
    if let Some(parent) = worktree_path.parent() {
        fs::create_dir_all(parent).context("could not create Buzz worktree directory")?;
    }
    let attach = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["worktree", "add"])
        .arg(worktree_path)
        .arg(branch)
        .kill_on_drop(true)
        .output()
        .await
        .context("could not start git worktree add")?;
    if !attach.status.success() {
        let stderr = String::from_utf8_lossy(&attach.stderr);
        bail!("git worktree add failed: {}", stderr.trim());
    }
    Ok(())
}

fn parse_worktree_porcelain(text: &str, repo_root: &Path) -> Vec<ListedWorktree> {
    let mut entries = Vec::new();
    let mut path = None;
    let mut branch = None;
    let mut is_first = true;
    let finish = |path: &mut Option<PathBuf>,
                  branch: &mut Option<String>,
                  is_first: &mut bool,
                  entries: &mut Vec<ListedWorktree>| {
        if let Some(path) = path.take() {
            entries.push(ListedWorktree {
                path,
                branch: branch.take(),
                is_primary: *is_first,
            });
            *is_first = false;
        }
    };
    for line in text.lines() {
        if line.is_empty() {
            finish(&mut path, &mut branch, &mut is_first, &mut entries);
            continue;
        }
        if let Some(value) = line.strip_prefix("worktree ") {
            finish(&mut path, &mut branch, &mut is_first, &mut entries);
            path = Some(PathBuf::from(value));
        } else if let Some(value) = line.strip_prefix("branch refs/heads/") {
            branch = Some(value.to_string());
        }
    }
    finish(&mut path, &mut branch, &mut is_first, &mut entries);
    if entries.is_empty() {
        entries.push(ListedWorktree {
            path: repo_root.to_path_buf(),
            branch: None,
            is_primary: true,
        });
    }
    entries
}

async fn git_output<I, S>(cwd: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .kill_on_drop(true)
        .output()
        .await
        .context("could not start git")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    String::from_utf8(output.stdout).context("git returned non-UTF-8 output")
}

#[cfg(test)]
mod tests {
    use super::{parse_worktree_porcelain, sanitize_branch_for_path};
    use std::path::Path;

    #[test]
    fn porcelain_parser_marks_first_entry_primary() {
        let text = "\
worktree /repo/crew
HEAD abc
branch refs/heads/main

worktree /repo/.buzz-worktrees/crew-ws-feature-x
HEAD def
branch refs/heads/feature/x
";
        let entries = parse_worktree_porcelain(text, Path::new("/repo/crew"));
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_primary);
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert!(!entries[1].is_primary);
        assert_eq!(entries[1].branch.as_deref(), Some("feature/x"));
    }

    #[test]
    fn branch_path_sanitizes_slashes() {
        assert_eq!(sanitize_branch_for_path("feature/demo"), "feature-demo");
        assert_eq!(
            sanitize_branch_for_path("buzz/aaaaaaaaaaaa"),
            "buzz-aaaaaaaaaaaa"
        );
    }
}
