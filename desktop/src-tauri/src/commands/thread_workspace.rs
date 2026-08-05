use serde::Serialize;

use super::gh_cli::gh_command;
use super::thread_github_target::origin_repo_target;
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
    /// True when ignored/local entries exist. Presence only — never paths.
    pub has_ignored_local_state: Option<bool>,
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
    let (dirty, has_ignored_local_state) = match worktree.as_deref() {
        Some(path) => {
            validate_checked_out_worktree(&target, path).await?;
            let dirty = !git_output_at(path, ["status", "--porcelain"])
                .await?
                .trim()
                .is_empty();
            // Detection errors fail closed via `?` — never treat unknown as clean.
            let has_ignored =
                super::project_worktree_details::has_ignored_local_state(path).await?;
            (Some(dirty), Some(has_ignored))
        }
        None => (None, None),
    };
    Ok(ThreadWorkspaceLifecycle {
        branch_checked_out: branch_is_checked_out(&target).await?,
        branch_exists,
        dirty,
        has_ignored_local_state,
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
    use buzz_worktree::{advance_eviction_generation, try_acquire_exclusive, LeaseError};

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
            "Free local space is unavailable while files have uncommitted changes.",
        ));
    }
    if super::project_worktree_details::has_ignored_local_state(&worktree).await? {
        return Ok(refused(
            super::project_worktree_auth::IGNORED_LOCAL_EVICTION_REFUSAL,
        ));
    }

    let lease = match try_acquire_exclusive(&target.common_git, &root_event_id) {
        Ok(lease) => lease,
        Err(LeaseError::Busy) => {
            return Ok(refused(
                "Free local space is unavailable while an agent is using this worktree.",
            ));
        }
        Err(error) => {
            return Err(format!(
                "Could not acquire worktree eviction lease: {error}"
            ));
        }
    };

    // Revalidate under the exclusive lease before mutation.
    let revalidated = validate_target(&repository_path, &branch, &root_event_id).await?;
    if revalidated.common_git != target.common_git
        || revalidated.repository_path != target.repository_path
    {
        return Ok(refused(
            "Worktree changed during eviction authorization; try again.",
        ));
    }
    let worktree = std::fs::canonicalize(&worktree)
        .map_err(|error| format!("Thread worktree is not accessible: {error}"))?;
    validate_checked_out_worktree(&revalidated, &worktree).await?;
    if !git_output_at(&worktree, ["status", "--porcelain"])
        .await?
        .trim()
        .is_empty()
    {
        return Ok(refused(
            "Free local space is unavailable while files have uncommitted changes.",
        ));
    }
    // Detection errors fail closed via `?` — never treat unknown as clean.
    if super::project_worktree_details::has_ignored_local_state(&worktree).await? {
        return Ok(refused(
            super::project_worktree_auth::IGNORED_LOCAL_EVICTION_REFUSAL,
        ));
    }

    git_output_dir(
        &revalidated.common_git,
        ["worktree", "remove", "--", path_text(&worktree)?],
    )
    .await?;
    git_output_dir(&revalidated.common_git, ["worktree", "prune"]).await?;

    // Advance generation only when a durable record exists. Thread panel can
    // run before the first ACP adopt; missing records must not block
    // branch-retaining eviction. Other persistence failures fail closed.
    if buzz_worktree::read_lifecycle_record(&revalidated.common_git, &root_event_id)
        .map_err(|error| format!("Could not read lifecycle record: {error}"))?
        .is_some()
    {
        advance_eviction_generation(&revalidated.common_git, &root_event_id).map_err(|error| {
            format!("Worktree removed but eviction generation could not be persisted: {error}")
        })?;
    }
    drop(lease);

    Ok(completed(
        "Freed local space. The branch is kept and will reattach on the next agent turn.",
    ))
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
    // Both calls pin the same repository so the close cannot land on a
    // same-named branch in the checkout's upstream remote.
    let repo = origin_repo_target(&target.repository_path).await;
    let mut list = gh_command()
        .await
        .map_err(|_| "GitHub CLI (gh) was not found.".to_string())?;
    list.arg("pr")
        .arg("list")
        .args(["--state", "open", "--head", branch.as_str()])
        .args(["--json", "number", "--limit", "1"])
        .current_dir(&target.repository_path);
    if let Some(repo) = repo.as_deref() {
        list.args(["--repo", repo]);
    }
    let listed = command_output(&mut list).await?;
    let pull_requests: Vec<serde_json::Value> = serde_json::from_slice(&listed.stdout)
        .map_err(|_| "gh returned invalid JSON".to_string())?;
    let Some(number) = pull_requests
        .first()
        .and_then(|pull_request| pull_request["number"].as_u64())
    else {
        return Ok(not_found("No open pull request exists for this branch."));
    };
    let mut close = gh_command()
        .await
        .map_err(|_| "GitHub CLI (gh) was not found.".to_string())?;
    close
        .arg("pr")
        .arg("close")
        .arg(number.to_string())
        .current_dir(&target.repository_path);
    if let Some(repo) = repo.as_deref() {
        close.args(["--repo", repo]);
    }
    command_output(&mut close).await?;
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
