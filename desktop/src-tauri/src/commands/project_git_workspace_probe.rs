//! Local git-ness + branch probe for composer workspace binding and Add Project.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::project_git::first_output_line;
use super::project_git_exec::clean_branch;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGitWorkspaceProbe {
    pub is_git: bool,
    pub default_branch: Option<String>,
    pub current_branch: Option<String>,
    pub dirty: bool,
    pub uncommitted_count: u64,
    pub local_branches: Vec<String>,
    pub remote_branches: Vec<String>,
}

/// Probe a local folder without mutating it. Non-git folders return `is_git: false`.
#[tauri::command]
pub fn probe_project_git_workspace(path: String) -> Result<ProjectGitWorkspaceProbe, String> {
    probe_project_git_workspace_at(Path::new(path.trim()))
}

pub(crate) fn probe_project_git_workspace_at(
    path: &Path,
) -> Result<ProjectGitWorkspaceProbe, String> {
    if !path.is_absolute() {
        return Err("Choose an absolute local folder path.".into());
    }
    let empty = ProjectGitWorkspaceProbe {
        is_git: false,
        default_branch: None,
        current_branch: None,
        dirty: false,
        uncommitted_count: 0,
        local_branches: Vec::new(),
        remote_branches: Vec::new(),
    };
    let Some(toplevel) = git_line(path, &["rev-parse", "--show-toplevel"]) else {
        return Ok(empty);
    };
    let repo = PathBuf::from(toplevel);
    let current_branch = git_line(&repo, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .and_then(|branch| clean_branch(Some(branch)));
    let default_branch = remote_default_branch(&repo).or_else(|| current_branch.clone());
    let status = git_output(&repo, &["status", "--porcelain"]).unwrap_or_default();
    let uncommitted_count = status
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .count() as u64;
    Ok(ProjectGitWorkspaceProbe {
        is_git: true,
        default_branch,
        current_branch,
        dirty: uncommitted_count > 0,
        uncommitted_count,
        local_branches: list_refs(&repo, "refs/heads/"),
        remote_branches: list_refs(&repo, "refs/remotes/origin/")
            .into_iter()
            .filter(|branch| branch != "HEAD")
            .collect(),
    })
}

fn remote_default_branch(repo: &Path) -> Option<String> {
    git_line(
        repo,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )
    .and_then(|value| value.strip_prefix("origin/").map(str::to_string))
    .and_then(|branch| clean_branch(Some(branch)))
}

fn list_refs(repo: &Path, prefix: &str) -> Vec<String> {
    let output = git_output(
        repo,
        &[
            "for-each-ref",
            "--count=200",
            "--format=%(refname:short)",
            prefix,
        ],
    )
    .unwrap_or_default();
    let mut branches = output
        .lines()
        .filter_map(|line| {
            let name = line.trim().strip_prefix("origin/").unwrap_or(line.trim());
            clean_branch(Some(name.to_string()))
        })
        .collect::<Vec<_>>();
    branches.sort();
    branches.dedup();
    branches
}

fn git_line(cwd: &Path, args: &[&str]) -> Option<String> {
    git_output(cwd, args).and_then(|output| first_output_line(&output))
}

fn git_output(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::probe_project_git_workspace_at;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn git(cwd: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .status()
            .expect("git starts");
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn non_git_folder_is_not_a_repository() {
        let temp = TempDir::new().unwrap();
        let probe = probe_project_git_workspace_at(temp.path()).unwrap();
        assert!(!probe.is_git);
    }

    #[test]
    fn git_folder_lists_branches_and_dirty_count() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        fs::write(repo.join("README.md"), "ok").unwrap();
        git(&repo, &["add", "README.md"]);
        git(&repo, &["commit", "-m", "init"]);
        git(&repo, &["branch", "release"]);
        fs::write(repo.join("dirty.txt"), "x").unwrap();

        let probe = probe_project_git_workspace_at(&repo).unwrap();
        assert!(probe.is_git);
        assert_eq!(probe.current_branch.as_deref(), Some("main"));
        assert!(probe.local_branches.iter().any(|branch| branch == "main"));
        assert!(probe
            .local_branches
            .iter()
            .any(|branch| branch == "release"));
        assert!(probe.dirty);
        assert!(probe.uncommitted_count >= 1);
    }
}
