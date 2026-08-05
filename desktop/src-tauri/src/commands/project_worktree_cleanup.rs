use std::path::{Path, PathBuf};

use buzz_worktree::advance_eviction_generation;

use super::project_worktree_auth::{
    authorize_verified_channel_mutation, IGNORED_LOCAL_EVICTION_REFUSAL,
};
use super::project_worktree_details::has_ignored_local_state;
use super::project_worktree_registry_parse::{
    classify_worktree, is_managed_branch, managed_root_for, parse_buzz_thread_roots,
    parse_worktree_porcelain, PorcelainWorktree, ProjectWorktreeKind,
};
use super::thread_workspace::{ThreadWorkspaceActionResult, ThreadWorkspaceActionStatus};
use super::thread_workspace_git::{git_output_at, git_output_dir, path_text};

/// Path-authorized checkout eviction for managed Buzz worktrees.
/// Compatibility shim name — frees local space while retaining the branch.
/// Does not loosen `validate_target` — this is a separate authorization path.
#[tauri::command]
pub async fn remove_project_worktree(
    repository_path: String,
    worktree_path: String,
    expected_routing_channel_id: String,
) -> Result<ThreadWorkspaceActionResult, String> {
    evict_project_worktree(repository_path, worktree_path, expected_routing_channel_id).await
}

/// Evict a managed checkout while retaining the deterministic branch and
/// durable lifecycle record. Never uses `--force` and never deletes branches.
///
/// Rust is the authorization boundary: the caller must prove the expected
/// routing channel, and a verified lifecycle record must match under an
/// exclusive lease. Legacy / no-root / conflict / other-channel refuse.
#[tauri::command]
pub async fn evict_project_worktree(
    repository_path: String,
    worktree_path: String,
    expected_routing_channel_id: String,
) -> Result<ThreadWorkspaceActionResult, String> {
    let authorized = match authorize_verified_channel_mutation(
        &repository_path,
        &worktree_path,
        &expected_routing_channel_id,
    )
    .await?
    {
        Ok(authorized) => authorized,
        Err(refusal) => return Ok(refusal),
    };

    if !git_output_at(&authorized.prepared.worktree, ["status", "--porcelain"])
        .await?
        .trim()
        .is_empty()
    {
        return Ok(ThreadWorkspaceActionResult {
            status: ThreadWorkspaceActionStatus::Refused,
            message: "Free local space is unavailable while files have uncommitted changes."
                .to_string(),
        });
    }
    // Detection errors fail closed via `?` — never treat unknown as clean.
    if has_ignored_local_state(&authorized.prepared.worktree).await? {
        return Ok(ThreadWorkspaceActionResult {
            status: ThreadWorkspaceActionStatus::Refused,
            message: IGNORED_LOCAL_EVICTION_REFUSAL.to_string(),
        });
    }

    git_output_dir(
        &authorized.prepared.common_git,
        [
            "worktree",
            "remove",
            "--",
            path_text(&authorized.prepared.worktree)?,
        ],
    )
    .await?;
    git_output_dir(&authorized.prepared.common_git, ["worktree", "prune"]).await?;

    // Never swallow generation advance — ACP self-heal depends on it, and when
    // persistence fails the next ensure still detects recreate (blocker 5).
    advance_eviction_generation(&authorized.prepared.common_git, &authorized.root_event_id)
        .map_err(|error| {
            format!("Worktree removed but eviction generation could not be persisted: {error}")
        })?;

    // Keep the lease alive until generation write completes.
    drop(authorized.lease);
    let _ = authorized.record;

    Ok(ThreadWorkspaceActionResult {
        status: ThreadWorkspaceActionStatus::Completed,
        message: "Freed local space. The branch is kept and will reattach on the next agent turn."
            .to_string(),
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
    pub(crate) branch: String,
    pub(crate) root_event_id: Option<String>,
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
        .ok_or_else(|| "Worktree has no branch checked out.".to_string())?
        .to_string();
    if !is_managed_branch(&branch) {
        return Err("Worktree branch is not a managed Buzz branch.".to_string());
    }

    let parent = worktree
        .parent()
        .ok_or_else(|| "Worktree path has no parent directory.".to_string())?;
    if parent != managed_root.as_path() {
        return Err("Worktree is outside the managed Buzz worktree root.".to_string());
    }

    let root_event_id = lookup_branch_root(&common_git, &branch).await;

    Ok(PreparedRemoval {
        common_git,
        worktree,
        branch,
        root_event_id,
    })
}

pub(crate) async fn resolve_repo(repository_path: &str) -> Result<(PathBuf, PathBuf), String> {
    let repository_path = std::fs::canonicalize(repository_path)
        .map_err(|error| format!("Project repository is not accessible: {error}"))?;
    let toplevel = git_output_at(&repository_path, ["rev-parse", "--show-toplevel"]).await?;
    let repo_root = std::fs::canonicalize(toplevel.trim())
        .map_err(|error| format!("Project repository is not accessible: {error}"))?;
    let common = git_output_at(&repo_root, ["rev-parse", "--git-common-dir"]).await?;
    let common_git = canonical_git_path(&repo_root, common.trim())?;
    Ok((repo_root, common_git))
}

async fn lookup_branch_root(common_git: &Path, branch: &str) -> Option<String> {
    let roots_text = git_output_dir(
        common_git,
        ["config", "--get-regexp", r"^branch\..*\.buzzthreadroot$"],
    )
    .await
    .unwrap_or_default();
    parse_buzz_thread_roots(&roots_text)
        .into_iter()
        .find(|(name, _)| name == branch)
        .map(|(_, root)| root)
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
