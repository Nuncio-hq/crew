use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use tokio::process::Command;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) struct ThreadWorkspaceTarget {
    pub(crate) repository_path: PathBuf,
    pub(crate) common_git: PathBuf,
    pub(crate) branch: String,
}

pub(crate) async fn validate_target(
    repository_path: &str,
    branch: &str,
    root_event_id: &str,
) -> Result<ThreadWorkspaceTarget, String> {
    if root_event_id.len() != 64 || !root_event_id.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err("Invalid thread root event ID.".to_string());
    }
    let expected_branch = format!("buzz/{}", &root_event_id[..12].to_ascii_lowercase());
    if branch != expected_branch {
        return Err("Thread branch does not match the thread root.".to_string());
    }
    let repository_path = std::fs::canonicalize(repository_path)
        .map_err(|error| format!("Project repository is not accessible: {error}"))?;
    let common = git_output_at(&repository_path, ["rev-parse", "--git-common-dir"]).await?;
    let common_git = canonical_git_path(&repository_path, common.trim())?;
    let root_key = format!("branch.{branch}.buzzThreadRoot");
    let roots =
        match git_optional_output_dir(&common_git, ["config", "--get-all", root_key.as_str()])
            .await?
        {
            Some(roots) => roots,
            None => std::fs::read_to_string(common_git.join("buzz-thread-workspace-roots").join(
                format!("{}.root", &root_event_id[..12].to_ascii_lowercase()),
            ))
            .map_err(|_| "Thread branch identity could not be verified.".to_string())?,
        };
    if roots.trim().is_empty()
        || roots
            .lines()
            .any(|root| !root.trim().eq_ignore_ascii_case(root_event_id))
    {
        return Err("Thread branch identity could not be verified.".to_string());
    }
    Ok(ThreadWorkspaceTarget {
        repository_path,
        common_git,
        branch: branch.to_string(),
    })
}

pub(crate) async fn validate_checked_out_worktree(
    target: &ThreadWorkspaceTarget,
    worktree: &Path,
) -> Result<(), String> {
    let root = git_output_at(worktree, ["rev-parse", "--show-toplevel"]).await?;
    if std::fs::canonicalize(root.trim()).ok().as_deref() != Some(worktree) {
        return Err("Thread worktree root could not be verified.".to_string());
    }
    let common = git_output_at(worktree, ["rev-parse", "--git-common-dir"]).await?;
    if canonical_git_path(worktree, common.trim())? != target.common_git {
        return Err("Thread worktree belongs to a different repository.".to_string());
    }
    let branch = git_output_at(worktree, ["symbolic-ref", "--short", "HEAD"]).await?;
    if branch.trim() != target.branch {
        return Err("Thread worktree has a different branch checked out.".to_string());
    }
    Ok(())
}

pub(crate) async fn branch_is_checked_out(target: &ThreadWorkspaceTarget) -> Result<bool, String> {
    let list = git_output_dir(&target.common_git, ["worktree", "list", "--porcelain"]).await?;
    Ok(list
        .lines()
        .any(|line| line.trim() == format!("branch refs/heads/{}", target.branch)))
}

fn canonical_git_path(repo_root: &Path, git_path: &str) -> Result<PathBuf, String> {
    let path = Path::new(git_path);
    std::fs::canonicalize(if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    })
    .map_err(|error| format!("Git repository metadata is not accessible: {error}"))
}

pub(crate) async fn git_output_at<I, S>(cwd: &Path, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new("git");
    command.arg("-C").arg(cwd).args(args);
    output_text(command_output(&mut command).await?)
}

pub(crate) async fn git_output_dir<I, S>(git_dir: &Path, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new("git");
    command.arg("--git-dir").arg(git_dir).args(args);
    output_text(command_output(&mut command).await?)
}

pub(crate) async fn git_success_at<I, S>(cwd: &Path, args: I) -> Result<bool, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new("git");
    command.arg("-C").arg(cwd).args(args);
    command_success(&mut command).await
}

pub(crate) async fn git_success_dir<I, S>(git_dir: &Path, args: I) -> Result<bool, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new("git");
    command.arg("--git-dir").arg(git_dir).args(args);
    command_success(&mut command).await
}

async fn git_optional_output_dir<I, S>(git_dir: &Path, args: I) -> Result<Option<String>, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new("git");
    command.arg("--git-dir").arg(git_dir).args(args);
    command.kill_on_drop(true);
    let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| "Command timed out.".to_string())?
        .map_err(|error| format!("Could not start command: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    output_text(output).map(Some)
}

async fn command_success(command: &mut Command) -> Result<bool, String> {
    command.kill_on_drop(true);
    tokio::time::timeout(COMMAND_TIMEOUT, command.status())
        .await
        .map_err(|_| "Command timed out.".to_string())?
        .map(|status| status.success())
        .map_err(|error| format!("Could not start command: {error}"))
}

pub(crate) async fn command_output(command: &mut Command) -> Result<std::process::Output, String> {
    command.kill_on_drop(true);
    let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| "Command timed out.".to_string())?
        .map_err(|error| format!("Could not start command: {error}"))?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if message.is_empty() {
            "Command failed.".to_string()
        } else {
            message
        });
    }
    Ok(output)
}

fn output_text(output: std::process::Output) -> Result<String, String> {
    String::from_utf8(output.stdout).map_err(|_| "Command returned non-UTF-8 output.".to_string())
}

pub(crate) fn path_text(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| "Thread worktree path is not valid UTF-8.".to_string())
}
