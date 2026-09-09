//! Read-only folder boundary and bounded Git probe for workspace binding.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::project_git::first_output_line;
use super::project_git_exec::clean_branch;

/// Relationship between the selected folder and the discovered Git root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectFolderSelection {
    /// The selected folder is the Git working tree root.
    Root,
    /// The selected folder is inside a Git tree; its boundary stays selected.
    Subdirectory,
    /// The accessible folder has no containing Git working tree.
    NotGit,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGitWorkspaceProbe {
    pub is_git: bool,
    /// Exact selected path, never silently replaced by a Git ancestor.
    pub selected_path: String,
    /// Canonical Git root, if the selected folder is inside a repository.
    pub git_root: Option<String>,
    /// Verified relation of the selected folder to its Git root.
    pub selection: ProjectFolderSelection,
    pub default_branch: Option<String>,
    pub current_branch: Option<String>,
    pub dirty: bool,
    pub uncommitted_count: u64,
    pub local_branches: Vec<String>,
    pub remote_branches: Vec<String>,
}

/// Probe an accessible local folder without mutating it or its Git index.
#[tauri::command]
pub fn probe_project_git_workspace(path: String) -> Result<ProjectGitWorkspaceProbe, String> {
    probe_project_git_workspace_at(Path::new(&path))
}

pub(crate) fn probe_project_git_workspace_at(
    path: &Path,
) -> Result<ProjectGitWorkspaceProbe, String> {
    if !path.is_absolute() {
        return Err("Choose an absolute local folder path.".into());
    }
    // Failure to access a folder is not evidence that it is a plain folder.
    std::fs::read_dir(path)
        .map_err(|error| format!("Cannot access the selected folder: {error}"))?;
    let selected_path = path
        .to_str()
        .ok_or("Selected folder path is not valid Unicode.")?
        .to_owned();
    let selected_canonical = path
        .canonicalize()
        .map_err(|error| format!("Cannot resolve the selected folder: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let empty = ProjectGitWorkspaceProbe {
        is_git: false,
        selected_path,
        git_root: None,
        selection: ProjectFolderSelection::NotGit,
        default_branch: None,
        current_branch: None,
        dirty: false,
        uncommitted_count: 0,
        local_branches: Vec::new(),
        remote_branches: Vec::new(),
    };
    let root_output = git_output(path, &["rev-parse", "--show-toplevel"], deadline)?;
    if !root_output.status.success() {
        let message = String::from_utf8_lossy(&root_output.stderr);
        if message.contains("not a git repository") {
            return Ok(empty);
        }
        return Err(format!(
            "Could not inspect the selected folder: {}",
            message.trim()
        ));
    }
    let root_output =
        String::from_utf8(root_output.stdout).map_err(|_| "Git root is not valid Unicode.")?;
    // Remove only Git's record terminator; spaces are valid path characters.
    let root = root_output.strip_suffix('\n').unwrap_or(&root_output);
    if root.is_empty() {
        return Err("Git did not return a repository root.".into());
    }
    let repo = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| format!("Cannot resolve Git root: {error}"))?;
    let selection = if repo == selected_canonical {
        ProjectFolderSelection::Root
    } else if selected_canonical.starts_with(&repo) {
        ProjectFolderSelection::Subdirectory
    } else {
        return Err("Git root does not contain the selected folder.".into());
    };
    let current_branch = git_optional_line(
        &repo,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        deadline,
    )?
    .and_then(|branch| clean_branch(Some(branch)));
    let default_branch = git_optional_line(
        &repo,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
        deadline,
    )?
    .and_then(|value| value.strip_prefix("origin/").map(str::to_owned))
    .and_then(|value| clean_branch(Some(value)))
    .or_else(|| current_branch.clone());
    let status = git_text(path, &["status", "--porcelain", "--", "."], deadline)?;
    let uncommitted_count = status
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count() as u64;
    Ok(ProjectGitWorkspaceProbe {
        is_git: true,
        git_root: Some(
            repo.to_str()
                .ok_or("Git root is not valid Unicode.")?
                .to_owned(),
        ),
        selection,
        default_branch,
        current_branch,
        dirty: uncommitted_count > 0,
        uncommitted_count,
        local_branches: list_refs(&repo, "refs/heads/", deadline)?,
        remote_branches: list_refs(&repo, "refs/remotes/origin/", deadline)?
            .into_iter()
            .filter(|branch| branch != "HEAD")
            .collect(),
        ..empty
    })
}

fn list_refs(repo: &Path, prefix: &str, deadline: Instant) -> Result<Vec<String>, String> {
    let output = git_text(
        repo,
        &[
            "for-each-ref",
            "--count=200",
            "--format=%(refname:short)",
            prefix,
        ],
        deadline,
    )?;
    let mut branches = output
        .lines()
        .filter_map(|line| {
            let name = line.trim().strip_prefix("origin/").unwrap_or(line.trim());
            clean_branch(Some(name.to_owned()))
        })
        .collect::<Vec<_>>();
    branches.sort();
    branches.dedup();
    Ok(branches)
}

fn git_optional_line(
    cwd: &Path,
    args: &[&str],
    deadline: Instant,
) -> Result<Option<String>, String> {
    let output = git_output(cwd, args, deadline)?;
    if output.status.code() == Some(1) {
        return Ok(None);
    }
    if !output.status.success() {
        return Err(format!(
            "Git probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(first_output_line(&String::from_utf8_lossy(&output.stdout)))
}

fn git_text(cwd: &Path, args: &[&str], deadline: Instant) -> Result<String, String> {
    let output = git_output(cwd, args, deadline)?;
    if !output.status.success() {
        return Err(format!(
            "Git probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| "Git returned invalid Unicode.".to_owned())
}

fn git_output(
    cwd: &Path,
    args: &[&str],
    deadline: Instant,
) -> Result<std::process::Output, String> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|time| !time.is_zero())
        .ok_or("Folder probe exceeded its 10 second budget.")?;
    let mut command = Command::new("git");
    command
        .args(["-c", "core.fsmonitor=false"])
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        command.env_remove(key);
    }
    crate::managed_agents::bounded_local_command(command, remaining).ok_or_else(|| {
        "Folder probe failed, timed out, or exceeded the 1 MiB output limit.".to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::{probe_project_git_workspace_at, ProjectFolderSelection};
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
        assert_eq!(probe.selection, ProjectFolderSelection::Root);
        assert_eq!(probe.current_branch.as_deref(), Some("main"));
        assert!(probe.local_branches.iter().any(|branch| branch == "main"));
        assert!(probe
            .local_branches
            .iter()
            .any(|branch| branch == "release"));
        assert!(probe.dirty);
        assert!(probe.uncommitted_count >= 1);
    }
    #[test]
    fn missing_and_relative_folders_are_errors_not_plain_workspaces() {
        let temp = TempDir::new().unwrap();
        assert!(probe_project_git_workspace_at(&temp.path().join("missing")).is_err());
        assert!(probe_project_git_workspace_at(std::path::Path::new("relative")).is_err());
    }

    #[test]
    fn git_subdirectory_preserves_exact_selected_boundary() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("Kho dữ liệu ");
        let selected = repo.join("Tài liệu with spaces ");
        fs::create_dir_all(&selected).unwrap();
        git(&repo, &["init", "-b", "main"]);
        fs::write(repo.join("outside-selected.txt"), "outside").unwrap();
        let probe = probe_project_git_workspace_at(&selected).unwrap();
        assert_eq!(
            probe.uncommitted_count, 0,
            "probe must not scan changes outside selected folder"
        );
        assert_eq!(probe.selected_path, selected.to_str().unwrap());
        assert_eq!(
            probe.git_root.as_deref(),
            repo.canonicalize().unwrap().to_str()
        );
        assert_eq!(probe.selection, ProjectFolderSelection::Subdirectory);
        assert!(!selected.join(".git").exists());
    }

    #[test]
    fn plain_folder_probe_does_not_initialize_git() {
        let temp = TempDir::new().unwrap();
        let probe = probe_project_git_workspace_at(temp.path()).unwrap();
        assert_eq!(probe.selection, ProjectFolderSelection::NotGit);
        assert_eq!(probe.git_root, None);
        assert_eq!(probe.selected_path, temp.path().to_str().unwrap());
        assert!(!temp.path().join(".git").exists());
    }
}
