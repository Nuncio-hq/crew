use std::path::{Path, PathBuf};

use super::project_worktree_registry_parse::{
    classify_worktree, is_managed_branch, managed_root_for, parse_worktree_porcelain,
    PorcelainWorktree, ProjectWorktreeKind,
};
use super::thread_workspace::{ThreadWorkspaceActionResult, ThreadWorkspaceActionStatus};
use super::thread_workspace_git::{git_output_at, git_output_dir, path_text};

/// Path-authorized removal for managed Buzz worktrees (including orphans).
/// Does not loosen `validate_target` — this is a separate authorization path.
#[tauri::command]
pub async fn remove_project_worktree(
    repository_path: String,
    worktree_path: String,
) -> Result<ThreadWorkspaceActionResult, String> {
    let prepared = prepare_managed_removal(&repository_path, &worktree_path).await?;
    if !git_output_at(&prepared.worktree, ["status", "--porcelain"])
        .await?
        .trim()
        .is_empty()
    {
        return Ok(ThreadWorkspaceActionResult {
            status: ThreadWorkspaceActionStatus::Refused,
            message: "Remove worktree is unavailable while files have uncommitted changes."
                .to_string(),
        });
    }
    git_output_dir(
        &prepared.common_git,
        ["worktree", "remove", "--", path_text(&prepared.worktree)?],
    )
    .await?;
    git_output_dir(&prepared.common_git, ["worktree", "prune"]).await?;
    Ok(ThreadWorkspaceActionResult {
        status: ThreadWorkspaceActionStatus::Completed,
        message: "Removed the project worktree.".to_string(),
    })
}

#[tauri::command]
pub async fn prune_project_worktrees(
    repository_path: String,
) -> Result<ThreadWorkspaceActionResult, String> {
    let (_, common_git) = resolve_repo(&repository_path).await?;
    git_output_dir(&common_git, ["worktree", "prune"]).await?;
    Ok(ThreadWorkspaceActionResult {
        status: ThreadWorkspaceActionStatus::Completed,
        message: "Pruned broken worktree registrations.".to_string(),
    })
}

#[derive(Debug)]
pub(crate) struct PreparedRemoval {
    pub(crate) common_git: PathBuf,
    pub(crate) worktree: PathBuf,
}

/// Shared guard chain for tests and the command. Never reaches `worktree remove`
/// unless every check passes.
pub(crate) async fn prepare_managed_removal(
    repository_path: &str,
    worktree_path: &str,
) -> Result<PreparedRemoval, String> {
    let (repo_root, common_git) = resolve_repo(repository_path).await?;
    // Deletion refuses the string-fallback path used by registry classification
    // for missing directories — prunable entries go through prune instead.
    let worktree = std::fs::canonicalize(worktree_path)
        .map_err(|_| "Worktree path is not accessible.".to_string())?;

    let porcelain = git_output_dir(&common_git, ["worktree", "list", "--porcelain"]).await?;
    let entries = parse_worktree_porcelain(&porcelain);
    let entry = find_entry(&entries, &worktree)
        .ok_or_else(|| "Worktree is not registered in this repository.".to_string())?;

    let primary = entries.first().map(|e| e.worktree_path.as_path());
    let is_primary = primary.is_some_and(|path| paths_equal(path, &worktree));
    if is_primary {
        return Err("The main worktree cannot be removed.".to_string());
    }

    let managed_root = managed_root_for(&repo_root)
        .ok_or_else(|| "Git repository has no parent directory.".to_string())?;
    let managed_root = std::fs::canonicalize(&managed_root).unwrap_or(managed_root);

    let kind = classify_worktree(&worktree, entry.branch.as_deref(), &managed_root, false);
    if kind != ProjectWorktreeKind::Managed {
        return Err("Only managed Buzz worktrees can be removed.".to_string());
    }

    let branch = entry
        .branch
        .as_deref()
        .ok_or_else(|| "Worktree has no branch checked out.".to_string())?;
    if !is_managed_branch(branch) {
        return Err("Worktree branch is not a managed Buzz branch.".to_string());
    }

    let parent = worktree
        .parent()
        .ok_or_else(|| "Worktree path has no parent directory.".to_string())?;
    if parent != managed_root.as_path() {
        return Err("Worktree is outside the managed Buzz worktree root.".to_string());
    }

    Ok(PreparedRemoval {
        common_git,
        worktree,
    })
}

async fn resolve_repo(repository_path: &str) -> Result<(PathBuf, PathBuf), String> {
    let repository_path = std::fs::canonicalize(repository_path)
        .map_err(|error| format!("Project repository is not accessible: {error}"))?;
    let toplevel = git_output_at(&repository_path, ["rev-parse", "--show-toplevel"]).await?;
    let repo_root = std::fs::canonicalize(toplevel.trim())
        .map_err(|error| format!("Project repository is not accessible: {error}"))?;
    let common = git_output_at(&repo_root, ["rev-parse", "--git-common-dir"]).await?;
    let common_git = canonical_git_path(&repo_root, common.trim())?;
    Ok((repo_root, common_git))
}

fn find_entry<'a>(
    entries: &'a [PorcelainWorktree],
    worktree: &Path,
) -> Option<&'a PorcelainWorktree> {
    entries.iter().find(|entry| {
        std::fs::canonicalize(&entry.worktree_path).ok().as_deref() == Some(worktree)
            || paths_equal(&entry.worktree_path, worktree)
    })
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    left == right
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
