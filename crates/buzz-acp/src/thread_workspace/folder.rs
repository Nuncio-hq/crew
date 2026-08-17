//! Cowork (`mode=folder`) skip-worktree planning.
//!
//! Agent cwd is the folder itself. Shadow-git lives in app data and is the
//! path-lease `common_git`. No branches, no LifecycleRecord.

use anyhow::{bail, Context, Result};

use super::base::{BaseSource, WorkspaceBase};
use super::{
    validate_root_event_id, CheckoutKind, EnsureKind, ProjectWorkspace, ThreadWorkspace,
    ThreadWorkspacePlan,
};

pub(crate) fn plan_folder_workspace(
    workspace: &ProjectWorkspace,
    root_event_id: &str,
) -> Result<ThreadWorkspacePlan> {
    validate_root_event_id(root_event_id)?;
    let folder = super::canonicalize_project_workspace(&workspace.local_path)?;
    if !folder.is_dir() {
        bail!("Project workspace is not a folder: {}", folder.display());
    }
    let history_root = buzz_cowork::history_dir_from_env()
        .context("version history location is not configured")?;
    let opened = buzz_cowork::ShadowRepo::open_or_init(
        &history_root,
        &workspace.repo_address,
        &folder,
        None,
    )
    .map_err(|error| anyhow::anyhow!("{error}"))?;
    if opened.rebuilt {
        tracing::warn!(
            repo = %workspace.repo_address,
            notice = opened.notice.as_deref().unwrap_or(""),
            "cowork version history was rebuilt after corruption"
        );
    }
    Ok(ThreadWorkspacePlan {
        root_event_id: root_event_id.to_string(),
        repository_path: folder.clone(),
        worktree_path: folder,
        branch: "folder".into(),
        common_git: opened.repo.git_dir().to_path_buf(),
        workspace_base: WorkspaceBase {
            revision: "0".repeat(40),
            source: BaseSource::LocalFallback,
            remote_default_branch: None,
            requested_base: None,
        },
        checkout_kind: CheckoutKind::Folder,
        claim_exclusive_root: false,
    })
}

pub(crate) fn ensure_folder_workspace(
    plan: &ThreadWorkspacePlan,
) -> Result<(ThreadWorkspace, EnsureKind)> {
    let worktree_name = plan
        .worktree_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("folder")
        .to_string();
    Ok((
        ThreadWorkspace {
            root_event_id: plan.root_event_id.clone(),
            repository_path: plan.repository_path.clone(),
            worktree_path: plan.worktree_path.clone(),
            worktree_name,
            branch: plan.branch.clone(),
            base_revision: plan.workspace_base.revision.clone(),
            base_source: plan.workspace_base.source,
            remote_default_branch: None,
            commits_behind_remote: None,
            common_git: plan.common_git.clone(),
            checkout_kind: CheckoutKind::Folder,
            requested_base: None,
            uncommitted_count: 0,
        },
        EnsureKind::AlreadyPresent,
    ))
}
