use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use tokio::process::Command;

const CONTEXT_URL_PREFIX: &str = "buzz://project-workspace?";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWorkspace {
    pub repo_address: String,
    pub local_path: PathBuf,
}

pub fn parse_project_workspace(content: &str) -> Result<Option<ProjectWorkspace>> {
    let Some(start) = content.find(CONTEXT_URL_PREFIX) else {
        return Ok(None);
    };
    let suffix = &content[start..];
    let end = suffix
        .find(['>', ' ', '\n', '\r', '\t'])
        .unwrap_or(suffix.len());
    let url = url::Url::parse(&suffix[..end]).context("invalid Project workspace URL")?;
    let mut repo_address = None;
    let mut local_path = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "repo" => repo_address = Some(value.into_owned()),
            "path" => local_path = Some(PathBuf::from(value.into_owned())),
            _ => {}
        }
    }
    let repo_address = repo_address.context("Project workspace URL is missing repo")?;
    let local_path = local_path.context("Project workspace URL is missing path")?;
    if repo_address.trim().is_empty() || !local_path.is_absolute() {
        bail!("Project workspace metadata is invalid");
    }
    Ok(Some(ProjectWorkspace {
        repo_address,
        local_path,
    }))
}

/// Ensure the deterministic worktree for a thread exists and return its cwd.
pub async fn ensure_thread_worktree(
    workspace: &ProjectWorkspace,
    root_event_id: &str,
) -> Result<PathBuf> {
    validate_root_event_id(root_event_id)?;
    let selected_path = fs::canonicalize(&workspace.local_path).with_context(|| {
        format!(
            "Project workspace does not exist: {}",
            workspace.local_path.display()
        )
    })?;
    let repo_root = git_output(&selected_path, ["rev-parse", "--show-toplevel"]).await?;
    let repo_root =
        fs::canonicalize(repo_root.trim()).context("could not canonicalize git repository root")?;
    let common_git = git_output(&repo_root, ["rev-parse", "--git-common-dir"]).await?;
    let common_git = canonical_git_path(&repo_root, common_git.trim())
        .context("could not canonicalize git common directory")?;

    let short_root = &root_event_id[..12];
    let repo_name = repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let parent = repo_root
        .parent()
        .context("git repository has no parent directory")?
        .join(".buzz-worktrees");
    let worktree_path = parent.join(format!("{repo_name}-{short_root}"));
    let branch = format!("buzz/{short_root}");

    if verify_worktree(&worktree_path, &common_git).await {
        return Ok(worktree_path);
    }
    fs::create_dir_all(&parent).context("could not create Buzz worktree directory")?;

    let create = Command::new("git")
        .arg("-C")
        .arg(&repo_root)
        .args(["worktree", "add", "-b", &branch])
        .arg(&worktree_path)
        .arg("HEAD")
        .kill_on_drop(true)
        .output()
        .await
        .context("could not start git worktree add")?;

    if !create.status.success() {
        // Another harness may have won the same idempotent create race.
        for _ in 0..10 {
            if verify_worktree(&worktree_path, &common_git).await {
                return Ok(worktree_path);
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        // The deterministic branch can outlive a manually removed worktree.
        // Reattach it instead of treating that recoverable state as a task
        // failure. Git still rejects a branch checked out somewhere else.
        let attach = Command::new("git")
            .arg("-C")
            .arg(&repo_root)
            .args(["worktree", "add"])
            .arg(&worktree_path)
            .arg(&branch)
            .kill_on_drop(true)
            .output()
            .await
            .context("could not start git worktree reattach")?;
        if attach.status.success() && verify_worktree(&worktree_path, &common_git).await {
            return Ok(worktree_path);
        }
        let stderr = String::from_utf8_lossy(&create.stderr);
        bail!("git worktree add failed: {}", stderr.trim());
    }

    if !verify_worktree(&worktree_path, &common_git).await {
        bail!("created worktree failed repository verification");
    }
    Ok(worktree_path)
}

async fn verify_worktree(path: &Path, expected_common_git: &Path) -> bool {
    let Ok(root) = git_output(path, ["rev-parse", "--show-toplevel"]).await else {
        return false;
    };
    let Ok(common) = git_output(path, ["rev-parse", "--git-common-dir"]).await else {
        return false;
    };
    let Ok(root) = fs::canonicalize(root.trim()) else {
        return false;
    };
    let Ok(common_path) = canonical_git_path(&root, common.trim()) else {
        return false;
    };
    common_path == expected_common_git
}

fn canonical_git_path(repo_root: &Path, path: &str) -> std::io::Result<PathBuf> {
    let path = Path::new(path);
    fs::canonicalize(if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    })
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

fn validate_root_event_id(root_event_id: &str) -> Result<()> {
    if root_event_id.len() != 64 || !root_event_id.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("thread root event ID must be 64 hex characters");
    }
    Ok(())
}
