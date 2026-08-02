use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::project_worktree_registry_github::{fetch_pull_requests_by_branch, RegistryPullRequest};
use super::project_worktree_registry_parse::{
    classify_worktree, managed_root_for, parse_buzz_thread_roots, parse_worktree_porcelain,
    worktree_name, ProjectWorktreeKind,
};
use super::thread_workspace_git::{git_output_at, git_output_dir, path_text};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorktreeRegistry {
    pub repository_path: String,
    pub managed_root: String,
    pub github: GithubAvailability,
    pub entries: Vec<ProjectWorktreeEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GithubAvailability {
    Available,
    Unavailable,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorktreeEntry {
    pub worktree_path: String,
    pub worktree_name: String,
    pub branch: Option<String>,
    pub head: String,
    pub kind: ProjectWorktreeKind,
    pub root_event_id: Option<String>,
    pub prunable: bool,
    pub pull_requests: Vec<RegistryPullRequest>,
}

#[tauri::command]
pub async fn get_project_worktree_registry(
    repository_path: String,
) -> Result<ProjectWorktreeRegistry, String> {
    let repository_path = std::fs::canonicalize(&repository_path)
        .map_err(|error| format!("Project repository is not accessible: {error}"))?;
    let toplevel = git_output_at(&repository_path, ["rev-parse", "--show-toplevel"]).await?;
    let repo_root = std::fs::canonicalize(toplevel.trim())
        .map_err(|error| format!("Project repository is not accessible: {error}"))?;
    let common = git_output_at(&repo_root, ["rev-parse", "--git-common-dir"]).await?;
    let common_git = canonical_git_path(&repo_root, common.trim())?;
    let managed_root = managed_root_for(&repo_root)
        .ok_or_else(|| "Git repository has no parent directory.".to_string())?;
    // Prefer a canonical managed root when the directory already exists.
    let managed_root = std::fs::canonicalize(&managed_root).unwrap_or(managed_root);

    let porcelain = git_output_dir(&common_git, ["worktree", "list", "--porcelain"]).await?;
    let worktrees = parse_worktree_porcelain(&porcelain);

    let roots_text = git_output_dir(
        &common_git,
        ["config", "--get-regexp", r"^branch\..*\.buzzthreadroot$"],
    )
    .await
    .unwrap_or_default();
    let root_by_branch: HashMap<String, String> =
        parse_buzz_thread_roots(&roots_text).into_iter().collect();

    let (github, prs_by_branch) = match fetch_pull_requests_by_branch(&repo_root).await {
        Some(map) => (GithubAvailability::Available, map),
        None => (GithubAvailability::Unavailable, HashMap::new()),
    };

    let primary_path = worktrees.first().map(|entry| entry.worktree_path.clone());
    let mut entries = Vec::with_capacity(worktrees.len());
    for entry in worktrees {
        if entry.bare {
            continue;
        }
        let is_primary = primary_path.as_ref() == Some(&entry.worktree_path);
        let kind = classify_worktree(
            &entry.worktree_path,
            entry.branch.as_deref(),
            &managed_root,
            is_primary,
        );
        let root_event_id = entry
            .branch
            .as_ref()
            .and_then(|branch| root_by_branch.get(branch).cloned());
        let pull_requests = entry
            .branch
            .as_ref()
            .and_then(|branch| prs_by_branch.get(branch).cloned())
            .unwrap_or_default();
        entries.push(ProjectWorktreeEntry {
            worktree_path: path_text(&entry.worktree_path)?.to_string(),
            worktree_name: worktree_name(&entry.worktree_path),
            branch: entry.branch,
            head: entry.head,
            kind,
            root_event_id,
            prunable: entry.prunable,
            pull_requests,
        });
    }

    Ok(ProjectWorktreeRegistry {
        repository_path: path_text(&repo_root)?.to_string(),
        managed_root: path_text(&managed_root)?.to_string(),
        github,
        entries,
    })
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
