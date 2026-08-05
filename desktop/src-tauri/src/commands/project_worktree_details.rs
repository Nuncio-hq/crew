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
    })
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

async fn disk_bytes_of(worktree: &Path) -> Result<u64, String> {
    let mut command = Command::new("du");
    command
        .arg("-sk")
        .arg(worktree)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(DU_TIMEOUT, command.output())
        .await
        .map_err(|_| "Command timed out.".to_string())?
        .map_err(|error| format!("Could not start command: {error}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if message.is_empty() {
            "Could not measure worktree disk usage.".to_string()
        } else {
            message
        });
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

    #[test]
    fn parses_rev_list_left_right() {
        assert_eq!(parse_left_right("3\t5"), Some((5, 3)));
        assert_eq!(parse_left_right("0 0"), Some((0, 0)));
        assert_eq!(parse_left_right("bad"), None);
    }
}
