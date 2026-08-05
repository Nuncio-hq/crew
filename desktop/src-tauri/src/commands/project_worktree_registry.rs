use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::project_worktree_registry_github::{
    fetch_issues_by_number, fetch_pull_requests_by_branch, resolve_linked_issues,
    FetchPullRequestsError, RegistryIssue, RegistryPullRequest,
};
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
    CliMissing,
    CliFailed,
}

/// How the registry treats local lifecycle identity for a managed worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LifecycleIdentity {
    Verified,
    Legacy,
    Conflict,
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
    pub linked_issues: Vec<RegistryIssue>,
    pub routing_channel_id: Option<String>,
    pub created_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub lifecycle_identity: LifecycleIdentity,
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

    let (pr_result, issues_result) = tokio::join!(
        fetch_pull_requests_by_branch(&repo_root),
        fetch_issues_by_number(&repo_root),
    );

    let mut github = GithubAvailability::Available;
    let (prs_by_branch, linked_nums_by_branch) = match pr_result {
        Ok(fetched) => (fetched.by_branch, fetched.linked_issue_numbers_by_branch),
        Err(FetchPullRequestsError::CliMissing) => {
            github = GithubAvailability::CliMissing;
            (HashMap::new(), HashMap::new())
        }
        Err(FetchPullRequestsError::CliFailed) => {
            github = GithubAvailability::CliFailed;
            (HashMap::new(), HashMap::new())
        }
    };
    let issues_by_number = match issues_result {
        Ok(map) => map,
        Err(FetchPullRequestsError::CliMissing) => {
            if matches!(github, GithubAvailability::Available) {
                github = GithubAvailability::CliMissing;
            }
            HashMap::new()
        }
        Err(FetchPullRequestsError::CliFailed) => {
            if matches!(github, GithubAvailability::Available) {
                github = GithubAvailability::CliFailed;
            }
            HashMap::new()
        }
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
        let linked_issues = entry
            .branch
            .as_ref()
            .map(|branch| {
                let numbers = linked_nums_by_branch
                    .get(branch)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                resolve_linked_issues(numbers, &issues_by_number)
            })
            .unwrap_or_default();
        let worktree_path = path_text(&entry.worktree_path)?.to_string();
        let lifecycle = project_lifecycle(
            &common_git,
            kind,
            root_event_id.as_deref(),
            entry.branch.as_deref(),
            &worktree_path,
        );
        entries.push(ProjectWorktreeEntry {
            worktree_path,
            worktree_name: worktree_name(&entry.worktree_path),
            branch: entry.branch,
            head: entry.head,
            kind,
            root_event_id,
            prunable: entry.prunable,
            pull_requests,
            linked_issues,
            routing_channel_id: lifecycle.routing_channel_id,
            created_at: lifecycle.created_at,
            last_used_at: lifecycle.last_used_at,
            lifecycle_identity: lifecycle.identity,
        });
    }

    Ok(ProjectWorktreeRegistry {
        repository_path: path_text(&repo_root)?.to_string(),
        managed_root: path_text(&managed_root)?.to_string(),
        github,
        entries,
    })
}

struct LifecycleProjection {
    identity: LifecycleIdentity,
    routing_channel_id: Option<String>,
    created_at: Option<i64>,
    last_used_at: Option<i64>,
}

fn project_lifecycle(
    common_git: &Path,
    kind: ProjectWorktreeKind,
    root_event_id: Option<&str>,
    branch: Option<&str>,
    worktree_path: &str,
) -> LifecycleProjection {
    if kind != ProjectWorktreeKind::Managed {
        return LifecycleProjection {
            identity: LifecycleIdentity::Legacy,
            routing_channel_id: None,
            created_at: None,
            last_used_at: None,
        };
    }
    let Some(root) = root_event_id else {
        return LifecycleProjection {
            identity: LifecycleIdentity::Legacy,
            routing_channel_id: None,
            created_at: None,
            last_used_at: None,
        };
    };
    match buzz_worktree::read_lifecycle_record(common_git, root) {
        Ok(Some(record)) => {
            let path_mismatch = Path::new(&record.worktree_path) != Path::new(worktree_path);
            let branch_mismatch = branch.is_some_and(|b| b != record.branch);
            if path_mismatch || branch_mismatch {
                LifecycleProjection {
                    identity: LifecycleIdentity::Conflict,
                    routing_channel_id: Some(record.routing_channel_id),
                    created_at: Some(record.created_at),
                    last_used_at: Some(record.last_used_at),
                }
            } else {
                LifecycleProjection {
                    identity: LifecycleIdentity::Verified,
                    routing_channel_id: Some(record.routing_channel_id),
                    created_at: Some(record.created_at),
                    last_used_at: Some(record.last_used_at),
                }
            }
        }
        Ok(None) => LifecycleProjection {
            identity: LifecycleIdentity::Legacy,
            routing_channel_id: None,
            created_at: None,
            last_used_at: None,
        },
        Err(_) => LifecycleProjection {
            identity: LifecycleIdentity::Conflict,
            routing_channel_id: None,
            created_at: None,
            last_used_at: None,
        },
    }
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
