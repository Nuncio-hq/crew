use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use tokio::process::Command;

const REMOTE_NAME: &str = "origin";
const REMOTE_TIMEOUT: Duration = Duration::from_secs(5);

static DEFAULT_BRANCH_BY_REPO: LazyLock<Mutex<HashMap<PathBuf, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BaseSource {
    Remote,
    LocalFallback,
}

impl BaseSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Remote => "remote",
            Self::LocalFallback => "local-fallback",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceBase {
    pub(crate) revision: String,
    pub(crate) source: BaseSource,
    pub(crate) remote_default_branch: Option<String>,
    pub(crate) requested_base: Option<String>,
}

pub(crate) async fn resolve_workspace_base(repo_root: &Path) -> Result<WorkspaceBase> {
    resolve_workspace_base_ref(repo_root, None).await
}

pub(crate) async fn resolve_workspace_base_ref(
    repo_root: &Path,
    requested_base: Option<&str>,
) -> Result<WorkspaceBase> {
    let local_revision = git_output(repo_root, ["rev-parse", "HEAD"]).await?;
    let local_revision = local_revision.trim().to_string();
    let fetched = git_success(repo_root, ["fetch", REMOTE_NAME, "--quiet"]).await;
    if let Some(branch) = requested_base
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return resolve_named_base(repo_root, branch, fetched, local_revision).await;
    }
    let default_branch = resolve_default_branch(repo_root, fetched).await;
    let remote_revision = match default_branch.as_deref() {
        Some(branch) => git_optional_output(
            repo_root,
            ["rev-parse", "--verify", &format!("{REMOTE_NAME}/{branch}")],
        )
        .await
        .map(|revision| revision.trim().to_string()),
        None => None,
    };
    if fetched {
        if let Some(revision) = remote_revision {
            return Ok(WorkspaceBase {
                revision,
                source: BaseSource::Remote,
                remote_default_branch: default_branch,
                requested_base: None,
            });
        }
    }

    Ok(WorkspaceBase {
        revision: local_revision,
        source: BaseSource::LocalFallback,
        remote_default_branch: default_branch,
        requested_base: None,
    })
}

async fn resolve_named_base(
    repo_root: &Path,
    branch: &str,
    fetched: bool,
    local_revision: String,
) -> Result<WorkspaceBase> {
    let default_branch = resolve_default_branch(repo_root, fetched).await;
    let remote_ref = format!("{REMOTE_NAME}/{branch}");
    if let Some(revision) =
        git_optional_output(repo_root, ["rev-parse", "--verify", remote_ref.as_str()])
            .await
            .map(|revision| revision.trim().to_string())
    {
        return Ok(WorkspaceBase {
            revision,
            source: if fetched {
                BaseSource::Remote
            } else {
                BaseSource::LocalFallback
            },
            remote_default_branch: default_branch,
            requested_base: Some(branch.to_string()),
        });
    }
    if let Some(revision) = git_optional_output(repo_root, ["rev-parse", "--verify", branch])
        .await
        .map(|revision| revision.trim().to_string())
    {
        return Ok(WorkspaceBase {
            revision,
            source: BaseSource::LocalFallback,
            remote_default_branch: default_branch,
            requested_base: Some(branch.to_string()),
        });
    }
    let _ = local_revision;
    bail!("base branch '{branch}' was not found");
}

async fn resolve_default_branch(repo_root: &Path, allow_remote_query: bool) -> Option<String> {
    if let Some(branch) = DEFAULT_BRANCH_BY_REPO
        .lock()
        .ok()
        .and_then(|cache| cache.get(repo_root).cloned())
    {
        return Some(branch);
    }

    let branch = git_optional_output(
        repo_root,
        [
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
    .await
    .and_then(|value| value.trim().strip_prefix("origin/").map(str::to_string));
    let branch = match branch {
        Some(branch) => Some(branch),
        None if allow_remote_query => {
            git_optional_output(repo_root, ["ls-remote", "--symref", REMOTE_NAME, "HEAD"])
                .await
                .and_then(|output| parse_remote_head_branch(&output))
        }
        None => None,
    };
    let branch = match branch {
        Some(branch) => Some(branch),
        None => git_optional_output(
            repo_root,
            [
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
        )
        .await
        .and_then(|value| value.trim().strip_prefix("origin/").map(str::to_string)),
    }?;
    if !git_success(repo_root, ["check-ref-format", "--branch", branch.as_str()]).await {
        return None;
    }
    if let Ok(mut cache) = DEFAULT_BRANCH_BY_REPO.lock() {
        cache.insert(repo_root.to_path_buf(), branch.clone());
    }
    Some(branch)
}

fn parse_remote_head_branch(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        line.strip_prefix("ref: refs/heads/")
            .and_then(|value| value.strip_suffix("\tHEAD"))
            .map(str::to_string)
    })
}

async fn git_success<I, S>(cwd: &Path, args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let command = Command::new("git");
    run_git(command, cwd, args)
        .await
        .is_some_and(|output| output.status.success())
}

async fn git_optional_output<I, S>(cwd: &Path, args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let command = Command::new("git");
    let output = run_git(command, cwd, args).await?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}

async fn git_output<I, S>(cwd: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let command = Command::new("git");
    let output = run_git(command, cwd, args)
        .await
        .context("git command timed out")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    String::from_utf8(output.stdout).context("git returned non-UTF-8 output")
}

async fn run_git<I, S>(mut command: Command, cwd: &Path, args: I) -> Option<std::process::Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    command.arg("-C").arg(cwd).args(args).kill_on_drop(true);
    tokio::time::timeout(REMOTE_TIMEOUT, command.output())
        .await
        .ok()
        .and_then(Result::ok)
}

#[cfg(test)]
mod tests {
    use super::parse_remote_head_branch;

    #[test]
    fn parses_only_symbolic_remote_head() {
        assert_eq!(
            parse_remote_head_branch(
                "ref: refs/heads/trunk\tHEAD\n0123456789012345678901234567890123456789\tHEAD\n"
            ),
            Some("trunk".to_string())
        );
        assert_eq!(
            parse_remote_head_branch("0123456789012345678901234567890123456789\tHEAD\n"),
            None
        );
    }
}
