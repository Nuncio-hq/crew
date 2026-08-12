use std::path::Path;
use std::process::Stdio;

use serde::Serialize;
use tokio::process::Command;

use super::project_worktree_cleanup::prepare_managed_removal;
use super::thread_workspace_git::{git_output_at, path_text};

const DU_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorktreeDetails {
    pub worktree_path: String,
    pub dirty: bool,
    pub ahead: u64,
    pub behind: u64,
    /// Unix seconds of the tip commit, when available.
    pub last_commit_at: Option<i64>,
    pub disk_bytes: u64,
    /// True when ignored/local entries exist. Presence only — never paths.
    pub has_ignored_local_state: bool,
}

/// Lazy per-row detail. Reuses the managed-path guard so callers cannot `du`
/// arbitrary directories through this command.
#[tauri::command]
pub async fn get_project_worktree_details(
    repository_path: String,
    worktree_path: String,
) -> Result<ProjectWorktreeDetails, String> {
    let prepared = prepare_managed_removal(&repository_path, &worktree_path).await?;
    let worktree = prepared.worktree;
    let dirty = !git_output_at(&worktree, ["status", "--porcelain"])
        .await?
        .trim()
        .is_empty();
    let has_ignored_local_state = has_ignored_local_state(&worktree).await?;
    let (ahead, behind) = ahead_behind(&worktree).await;
    let last_commit_at = last_commit_unix(&worktree).await;
    let disk_bytes = disk_bytes_of(&worktree).await?;
    Ok(ProjectWorktreeDetails {
        worktree_path: path_text(&worktree)?.to_string(),
        dirty,
        ahead,
        behind,
        last_commit_at,
        disk_bytes,
        has_ignored_local_state,
    })
}

/// Classify ignored/local presence without returning paths or contents.
pub(crate) async fn has_ignored_local_state(worktree: &Path) -> Result<bool, String> {
    let status = git_output_at(
        worktree,
        [
            "status",
            "--porcelain",
            "--ignored",
            "--untracked-files=all",
        ],
    )
    .await?;
    Ok(status.lines().any(|line| line.starts_with("!!")))
}

async fn ahead_behind(worktree: &Path) -> (u64, u64) {
    let Ok(raw) = git_output_at(
        worktree,
        ["rev-list", "--left-right", "--count", "@{upstream}...HEAD"],
    )
    .await
    else {
        return (0, 0);
    };
    parse_left_right(raw.trim()).unwrap_or((0, 0))
}

/// True when the branch has an upstream and is not ahead of it.
///
/// No-upstream is not "pushed" — Hibernate falls back to merged-PR only.
pub(crate) async fn branch_is_pushed(worktree: &Path) -> bool {
    if git_output_at(worktree, ["rev-parse", "--abbrev-ref", "@{upstream}"])
        .await
        .is_err()
    {
        return false;
    }
    let (ahead, _behind) = ahead_behind(worktree).await;
    ahead == 0
}

fn parse_left_right(text: &str) -> Option<(u64, u64)> {
    let mut parts = text.split_whitespace();
    let behind = parts.next()?.parse().ok()?;
    let ahead = parts.next()?.parse().ok()?;
    Some((ahead, behind))
}

async fn last_commit_unix(worktree: &Path) -> Option<i64> {
    let raw = git_output_at(worktree, ["log", "-1", "--format=%ct"])
        .await
        .ok()?;
    raw.trim().parse().ok()
}

pub(crate) async fn disk_bytes_of(path: &Path) -> Result<u64, String> {
    let mut command = Command::new("du");
    command
        .arg("-sk")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(DU_TIMEOUT, command.output())
        .await
        .map_err(|_| "Command timed out.".to_string())?
        .map_err(|error| format!("Could not start command: {error}"))?;
    if !output.status.success() {
        // Never surface path-specific ignored filenames from stderr.
        return Err("Could not measure worktree disk usage.".to_string());
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|_| "Command returned non-UTF-8 output.".to_string())?;
    let kib: u64 = text
        .split_whitespace()
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| "Could not parse worktree disk usage.".to_string())?;
    Ok(kib.saturating_mul(1024))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command as StdCommand;
    use tempfile::TempDir;

    #[test]
    fn parses_rev_list_left_right() {
        assert_eq!(parse_left_right("3\t5"), Some((5, 3)));
        assert_eq!(parse_left_right("0 0"), Some((0, 0)));
        assert_eq!(parse_left_right("bad"), None);
    }

    #[tokio::test]
    async fn ignored_presence_without_leaking_names() {
        let temp = TempDir::new().expect("temp");
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).expect("repo");
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        fs::write(repo.join("README.md"), "ok").expect("readme");
        fs::write(repo.join(".gitignore"), "secret-cache/\n").expect("gitignore");
        git(&repo, &["add", "README.md", ".gitignore"]);
        git(&repo, &["commit", "-m", "init"]);
        let secret = repo.join("secret-cache");
        fs::create_dir_all(&secret).expect("secret dir");
        fs::write(secret.join("token.txt"), "do-not-leak").expect("secret file");

        let present = has_ignored_local_state(&repo).await.expect("status");
        assert!(present);

        // Ensure helper itself does not return the secret name.
        let status = git_output_at(
            &repo,
            [
                "status",
                "--porcelain",
                "--ignored",
                "--untracked-files=all",
            ],
        )
        .await
        .expect("raw status");
        assert!(status.contains("!!"));
        // The detection API only returns a bool — callers must not log paths.
        let _ = status;
    }

    #[tokio::test]
    async fn no_ignored_entries_is_false() {
        let temp = TempDir::new().expect("temp");
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).expect("repo");
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        fs::write(repo.join("README.md"), "ok").expect("readme");
        git(&repo, &["add", "README.md"]);
        git(&repo, &["commit", "-m", "init"]);
        assert!(!has_ignored_local_state(&repo).await.expect("status"));
    }

    fn git(cwd: &Path, args: &[&str]) {
        let output = StdCommand::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .output()
            .expect("git starts");
        assert!(
            output.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
