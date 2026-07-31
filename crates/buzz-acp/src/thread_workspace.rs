use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use tokio::process::Command;

mod base;

pub(crate) use base::BaseSource;
use base::{resolve_workspace_base, WorkspaceBase};

const CONTEXT_URL_PREFIX: &str = "buzz://project-workspace?";
const ROOT_CLAIM_DIRECTORY: &str = "buzz-thread-workspace-roots";
const ROOT_CLAIM_READ_ATTEMPTS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectWorkspace {
    pub repo_address: String,
    pub local_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ThreadWorkspace {
    pub(crate) root_event_id: String,
    pub(crate) repository_path: PathBuf,
    pub(crate) worktree_path: PathBuf,
    pub(crate) worktree_name: String,
    pub(crate) branch: String,
    pub(crate) base_revision: String,
    pub(crate) base_source: BaseSource,
    pub(crate) remote_default_branch: Option<String>,
    pub(crate) commits_behind_remote: Option<u64>,
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

/// Ensure the deterministic worktree for a thread exists and return verified metadata.
pub async fn ensure_thread_worktree(
    workspace: &ProjectWorkspace,
    root_event_id: &str,
) -> Result<ThreadWorkspace> {
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
    let workspace_base = resolve_workspace_base(&repo_root).await?;

    if let Some(metadata) = verified_metadata(
        &repo_root,
        &worktree_path,
        &common_git,
        &branch,
        root_event_id,
        &workspace_base,
    )
    .await?
    {
        return Ok(metadata);
    }
    fs::create_dir_all(&parent).context("could not create Buzz worktree directory")?;

    let create = Command::new("git")
        .arg("-C")
        .arg(&repo_root)
        .args(["worktree", "add", "-b", &branch])
        .arg(&worktree_path)
        .arg(&workspace_base.revision)
        .kill_on_drop(true)
        .output()
        .await
        .context("could not start git worktree add")?;

    if !create.status.success() {
        // Another harness may have won the same idempotent create race.
        for _ in 0..10 {
            if let Some(metadata) = verified_metadata(
                &repo_root,
                &worktree_path,
                &common_git,
                &branch,
                root_event_id,
                &workspace_base,
            )
            .await?
            {
                return Ok(metadata);
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        // The deterministic branch can outlive a manually removed worktree.
        // Reattach it instead of treating that recoverable state as a task
        // failure. Git still rejects a branch checked out somewhere else.
        if branch_root_matches(&repo_root, &common_git, &branch, root_event_id).await {
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
            if !attach.status.success() {
                let stderr = String::from_utf8_lossy(&create.stderr);
                bail!("git worktree add failed: {}", stderr.trim());
            }
            if let Some(metadata) = verified_metadata(
                &repo_root,
                &worktree_path,
                &common_git,
                &branch,
                root_event_id,
                &workspace_base,
            )
            .await?
            {
                return Ok(metadata);
            }
        }
        let stderr = String::from_utf8_lossy(&create.stderr);
        bail!("git worktree add failed: {}", stderr.trim());
    }

    verified_metadata(
        &repo_root,
        &worktree_path,
        &common_git,
        &branch,
        root_event_id,
        &workspace_base,
    )
    .await?
    .context("created worktree failed repository verification")
}

async fn verified_metadata(
    repo_root: &Path,
    path: &Path,
    expected_common_git: &Path,
    expected_branch: &str,
    root_event_id: &str,
    workspace_base: &WorkspaceBase,
) -> Result<Option<ThreadWorkspace>> {
    if !verify_worktree(path, expected_common_git, expected_branch, root_event_id).await {
        return Ok(None);
    }
    let worktree_path =
        fs::canonicalize(path).context("could not canonicalize verified worktree path")?;
    let worktree_name = worktree_path
        .file_name()
        .and_then(|name| name.to_str())
        .context("verified worktree name is not valid UTF-8")?
        .to_string();
    // On creation this is exactly the source checkout HEAD supplied to
    // `git worktree add`. On normal idempotent reuse, merge-base preserves
    // that branch point after either the source or worktree branch advances.
    let base_revision = git_output(
        &worktree_path,
        ["merge-base", "HEAD", workspace_base.revision.as_str()],
    )
    .await?
    .trim()
    .to_string();
    if base_revision.len() != 40
        || !base_revision
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        bail!("git returned an invalid worktree base revision");
    }
    Ok(Some(ThreadWorkspace {
        root_event_id: root_event_id.to_string(),
        repository_path: repo_root.to_path_buf(),
        worktree_path,
        worktree_name,
        branch: expected_branch.to_string(),
        base_revision,
        base_source: workspace_base.source,
        remote_default_branch: workspace_base.remote_default_branch.clone(),
        commits_behind_remote: workspace_base.commits_behind_remote,
    }))
}

async fn verify_worktree(
    path: &Path,
    expected_common_git: &Path,
    expected_branch: &str,
    expected_root_event_id: &str,
) -> bool {
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
    let Ok(branch) = git_output(path, ["symbolic-ref", "--short", "HEAD"]).await else {
        return false;
    };
    root == path
        && common_path == expected_common_git
        && branch.trim() == expected_branch
        && verify_or_claim_branch_root(
            path,
            expected_common_git,
            expected_branch,
            expected_root_event_id,
        )
        .await
}

async fn record_branch_root(repo_root: &Path, branch: &str, root_event_id: &str) -> Result<()> {
    let key = branch_root_config_key(branch);
    git_output(
        repo_root,
        ["config", "--local", "--add", key.as_str(), root_event_id],
    )
    .await?;
    Ok(())
}

async fn branch_root_matches(
    repo_root: &Path,
    common_git: &Path,
    branch: &str,
    root_event_id: &str,
) -> bool {
    let Ok(recorded_roots) = read_branch_roots(repo_root, branch).await else {
        return false;
    };
    !recorded_roots.is_empty()
        && recorded_roots
            .iter()
            .all(|recorded| recorded.eq_ignore_ascii_case(root_event_id))
        && root_claim_matches(common_git, root_event_id).await
}

async fn verify_or_claim_branch_root(
    repo_root: &Path,
    common_git: &Path,
    branch: &str,
    root_event_id: &str,
) -> bool {
    let Ok(recorded_roots) = read_branch_roots(repo_root, branch).await else {
        return false;
    };
    if recorded_roots
        .iter()
        .any(|recorded| !recorded.eq_ignore_ascii_case(root_event_id))
    {
        return false;
    }
    let Ok(claimed) = claim_root(common_git, root_event_id).await else {
        return false;
    };
    if !claimed {
        return false;
    }
    if recorded_roots.is_empty()
        && record_branch_root(repo_root, branch, root_event_id)
            .await
            .is_err()
    {
        return false;
    }
    branch_root_matches(repo_root, common_git, branch, root_event_id).await
}

async fn claim_root(common_git: &Path, root_event_id: &str) -> Result<bool> {
    let claim_directory = common_git.join(ROOT_CLAIM_DIRECTORY);
    fs::create_dir_all(&claim_directory).context("could not create thread root claim directory")?;
    let claim_path = root_claim_path(common_git, root_event_id);
    let normalized_root = root_event_id.to_ascii_lowercase();

    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&claim_path)
    {
        Ok(mut claim_file) => {
            claim_file
                .write_all(normalized_root.as_bytes())
                .and_then(|()| claim_file.write_all(b"\n"))
                .context("could not write thread root claim")?;
            claim_file
                .sync_all()
                .context("could not persist thread root claim")?;
            fs::File::open(&claim_directory)
                .and_then(|directory| directory.sync_all())
                .context("could not persist thread root claim directory")?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            root_claim_matches_result(&claim_path, root_event_id).await
        }
        Err(error) => Err(error).context("could not create thread root claim"),
    }
}

async fn root_claim_matches(common_git: &Path, root_event_id: &str) -> bool {
    root_claim_matches_result(&root_claim_path(common_git, root_event_id), root_event_id)
        .await
        .unwrap_or(false)
}

async fn root_claim_matches_result(claim_path: &Path, root_event_id: &str) -> Result<bool> {
    for attempt in 0..ROOT_CLAIM_READ_ATTEMPTS {
        match fs::read_to_string(claim_path) {
            Ok(recorded_root) if !recorded_root.trim().is_empty() => {
                return Ok(recorded_root.trim().eq_ignore_ascii_case(root_event_id));
            }
            Ok(_) if attempt + 1 < ROOT_CLAIM_READ_ATTEMPTS => {}
            Ok(_) => return Ok(false),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && attempt + 1 < ROOT_CLAIM_READ_ATTEMPTS => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error).context("could not read thread root claim"),
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    Ok(false)
}

fn root_claim_path(common_git: &Path, root_event_id: &str) -> PathBuf {
    common_git
        .join(ROOT_CLAIM_DIRECTORY)
        .join(format!("{}.root", root_event_id[..12].to_ascii_lowercase()))
}

async fn read_branch_roots(repo_root: &Path, branch: &str) -> Result<Vec<String>> {
    let key = branch_root_config_key(branch);
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["config", "--local", "--get-all", key.as_str()])
        .kill_on_drop(true)
        .output()
        .await
        .context("could not start git")?;
    if !output.status.success() {
        if output.stderr.is_empty() {
            return Ok(Vec::new());
        }
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8(output.stdout)
        .context("git returned non-UTF-8 output")?
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect())
}

fn branch_root_config_key(branch: &str) -> String {
    format!("branch.{branch}.buzzThreadRoot")
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
