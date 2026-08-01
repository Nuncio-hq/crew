use serde::Serialize;
use tokio::process::Command;

use super::thread_workspace_git::{
    branch_is_checked_out, command_output, git_output_at, git_output_dir, git_success_at,
    git_success_dir, path_text, validate_checked_out_worktree, validate_target,
};

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ThreadWorkspaceActionStatus {
    Completed,
    Refused,
    NotFound,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ThreadWorkspaceActionResult {
    pub status: ThreadWorkspaceActionStatus,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadWorkspaceLifecycle {
    pub branch_checked_out: bool,
    pub branch_exists: bool,
    pub dirty: Option<bool>,
    pub worktree_exists: bool,
}

#[tauri::command]
pub async fn get_thread_workspace_lifecycle(
    repository_path: String,
    worktree_path: String,
    branch: String,
    root_event_id: String,
) -> Result<ThreadWorkspaceLifecycle, String> {
    let target = validate_target(&repository_path, &branch, &root_event_id).await?;
    let branch_exists = git_success_dir(
        &target.common_git,
        [
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .await?;
    let worktree = std::fs::canonicalize(worktree_path).ok();
    let dirty = match worktree.as_deref() {
        Some(path) => {
            validate_checked_out_worktree(&target, path).await?;
            Some(
                !git_output_at(path, ["status", "--porcelain"])
                    .await?
                    .trim()
                    .is_empty(),
            )
        }
        None => None,
    };
    Ok(ThreadWorkspaceLifecycle {
        branch_checked_out: branch_is_checked_out(&target).await?,
        branch_exists,
        dirty,
        worktree_exists: worktree.is_some(),
    })
}

#[tauri::command]
pub async fn remove_thread_worktree(
    repository_path: String,
    worktree_path: String,
    branch: String,
    root_event_id: String,
) -> Result<ThreadWorkspaceActionResult, String> {
    let target = validate_target(&repository_path, &branch, &root_event_id).await?;
    let worktree = std::fs::canonicalize(worktree_path)
        .map_err(|error| format!("Thread worktree is not accessible: {error}"))?;
    validate_checked_out_worktree(&target, &worktree).await?;
    if !git_output_at(&worktree, ["status", "--porcelain"])
        .await?
        .trim()
        .is_empty()
    {
        return Ok(refused(
            "Remove worktree is unavailable while files have uncommitted changes.",
        ));
    }
    git_output_dir(
        &target.common_git,
        ["worktree", "remove", "--", path_text(&worktree)?],
    )
    .await?;
    git_output_dir(&target.common_git, ["worktree", "prune"]).await?;
    Ok(completed("Removed the thread worktree."))
}

#[tauri::command]
pub async fn delete_thread_branch(
    repository_path: String,
    branch: String,
    root_event_id: String,
) -> Result<ThreadWorkspaceActionResult, String> {
    let target = validate_target(&repository_path, &branch, &root_event_id).await?;
    if branch_is_checked_out(&target).await? {
        return Ok(refused(
            "Delete branch is unavailable while the branch is checked out in a worktree.",
        ));
    }
    if !git_success_dir(
        &target.common_git,
        [
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .await?
    {
        return Ok(not_found("The thread branch no longer exists."));
    }
    if git_success_at(
        &target.repository_path,
        [
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/remotes/origin/{branch}"),
        ],
    )
    .await?
    {
        git_output_at(
            &target.repository_path,
            ["push", "origin", "--delete", branch.as_str()],
        )
        .await?;
    }
    git_output_dir(&target.common_git, ["branch", "-D", "--", branch.as_str()]).await?;
    Ok(completed("Deleted the thread branch."))
}

#[tauri::command]
pub async fn close_thread_pull_request(
    repository_path: String,
    branch: String,
    root_event_id: String,
) -> Result<ThreadWorkspaceActionResult, String> {
    let target = validate_target(&repository_path, &branch, &root_event_id).await?;
    let listed = command_output(
        Command::new("gh")
            .arg("pr")
            .arg("list")
            .args(["--state", "open", "--head", branch.as_str()])
            .args(["--json", "number", "--limit", "1"])
            .current_dir(&target.repository_path),
    )
    .await?;
    let pull_requests: Vec<serde_json::Value> = serde_json::from_slice(&listed.stdout)
        .map_err(|_| "gh returned invalid JSON".to_string())?;
    let Some(number) = pull_requests
        .first()
        .and_then(|pull_request| pull_request["number"].as_u64())
    else {
        return Ok(not_found("No open pull request exists for this branch."));
    };
    command_output(
        Command::new("gh")
            .arg("pr")
            .arg("close")
            .arg(number.to_string())
            .current_dir(&target.repository_path),
    )
    .await?;
    Ok(completed("Closed the thread pull request."))
}

fn completed(message: &str) -> ThreadWorkspaceActionResult {
    action(ThreadWorkspaceActionStatus::Completed, message)
}

fn refused(message: &str) -> ThreadWorkspaceActionResult {
    action(ThreadWorkspaceActionStatus::Refused, message)
}

fn not_found(message: &str) -> ThreadWorkspaceActionResult {
    action(ThreadWorkspaceActionStatus::NotFound, message)
}

fn action(status: ThreadWorkspaceActionStatus, message: &str) -> ThreadWorkspaceActionResult {
    ThreadWorkspaceActionResult {
        status,
        message: message.to_string(),
    }
}
