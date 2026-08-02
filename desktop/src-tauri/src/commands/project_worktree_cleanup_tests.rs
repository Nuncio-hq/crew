use std::{fs, path::PathBuf, process::Command};

use tempfile::TempDir;

use super::project_worktree_cleanup::{
    prepare_managed_removal, prune_project_worktrees, remove_project_worktree,
};
use super::thread_workspace::ThreadWorkspaceActionStatus;

struct Fixture {
    _temp: TempDir,
    repository: PathBuf,
    managed_root: PathBuf,
    managed_worktree: PathBuf,
    external_worktree: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temp dir");
        let root = temp.path();
        let repository = root.join("crew");
        let managed_root = root.join(".buzz-worktrees");
        let managed_worktree = managed_root.join("crew-aaaaaaaaaaaa");
        let external_worktree = root.join(".worktrees").join("crew-docs-fork");
        fs::create_dir_all(&repository).expect("repository dir");
        fs::create_dir_all(&managed_root).expect("managed root");
        fs::create_dir_all(external_worktree.parent().expect("parent")).expect("external parent");
        git(&repository, &["init", "-b", "main"]);
        git(&repository, &["config", "user.email", "test@example.com"]);
        git(&repository, &["config", "user.name", "Test"]);
        fs::write(repository.join("README.md"), "fixture").expect("fixture file");
        git(&repository, &["add", "README.md"]);
        git(&repository, &["commit", "-m", "fixture"]);
        let branch = "buzz/aaaaaaaaaaaa".to_string();
        git(
            &repository,
            &[
                "worktree",
                "add",
                "-b",
                branch.as_str(),
                managed_worktree.to_str().expect("UTF-8"),
                "HEAD",
            ],
        );
        git(
            &repository,
            &[
                "worktree",
                "add",
                "-b",
                "docs/human",
                external_worktree.to_str().expect("UTF-8"),
                "HEAD",
            ],
        );
        let _ = branch;
        Self {
            _temp: temp,
            repository,
            managed_root,
            managed_worktree,
            external_worktree,
        }
    }

    fn repo(&self) -> String {
        self.repository.to_string_lossy().into_owned()
    }

    fn managed_path(&self) -> String {
        self.managed_worktree.to_string_lossy().into_owned()
    }
}

#[tokio::test]
async fn removes_clean_managed_worktree() {
    let fixture = Fixture::new();
    let result = remove_project_worktree(fixture.repo(), fixture.managed_path())
        .await
        .expect("remove succeeds");
    assert_eq!(result.status, ThreadWorkspaceActionStatus::Completed);
    assert!(!fixture.managed_worktree.exists());
}

#[tokio::test]
async fn dirty_managed_worktree_refuses_without_remove() {
    let fixture = Fixture::new();
    fs::write(fixture.managed_worktree.join("dirty.txt"), "dirty").expect("dirty");
    let result = remove_project_worktree(fixture.repo(), fixture.managed_path())
        .await
        .expect("command returns");
    assert_eq!(result.status, ThreadWorkspaceActionStatus::Refused);
    assert!(fixture.managed_worktree.is_dir());
}

#[tokio::test]
async fn external_path_errors_without_remove() {
    let fixture = Fixture::new();
    let err = remove_project_worktree(
        fixture.repo(),
        fixture.external_worktree.to_string_lossy().into_owned(),
    )
    .await
    .expect_err("external refused");
    assert!(err.contains("managed") || err.contains("Buzz"));
    assert!(fixture.external_worktree.is_dir());
}

#[tokio::test]
async fn main_worktree_errors_without_remove() {
    let fixture = Fixture::new();
    let err = remove_project_worktree(fixture.repo(), fixture.repo())
        .await
        .expect_err("main refused");
    assert!(err.to_lowercase().contains("main"));
    assert!(fixture.repository.is_dir());
}

#[tokio::test]
async fn unlisted_path_errors_without_remove() {
    let fixture = Fixture::new();
    let ghost = fixture.managed_root.join("crew-bbbbbbbbbbbb");
    fs::create_dir_all(&ghost).expect("ghost dir");
    let err = remove_project_worktree(fixture.repo(), ghost.to_string_lossy().into_owned())
        .await
        .expect_err("unlisted refused");
    assert!(err.contains("not registered") || err.contains("managed"));
    assert!(ghost.is_dir());
}

#[tokio::test]
async fn non_buzz_branch_under_managed_root_errors() {
    let fixture = Fixture::new();
    let path = fixture.managed_root.join("crew-not-buzz");
    git(
        &fixture.repository,
        &[
            "worktree",
            "add",
            "-b",
            "feature/other",
            path.to_str().expect("UTF-8"),
            "HEAD",
        ],
    );
    let err = remove_project_worktree(fixture.repo(), path.to_string_lossy().into_owned())
        .await
        .expect_err("non-buzz refused");
    assert!(err.contains("managed") || err.contains("Buzz") || err.contains("branch"));
    assert!(path.is_dir());
}

#[tokio::test]
async fn missing_path_cannot_canonicalize() {
    let fixture = Fixture::new();
    let missing = fixture.managed_root.join("crew-missingmissing");
    let err = remove_project_worktree(fixture.repo(), missing.to_string_lossy().into_owned())
        .await
        .expect_err("missing refused");
    assert!(err.contains("not accessible"));
}

#[tokio::test]
async fn prepare_guard_rejects_before_remove() {
    let fixture = Fixture::new();
    let err = prepare_managed_removal(
        &fixture.repo(),
        fixture.external_worktree.to_str().expect("UTF-8"),
    )
    .await
    .expect_err("prepare fails");
    assert!(!err.is_empty());
    assert!(fixture.external_worktree.is_dir());
}

#[tokio::test]
async fn prune_with_no_broken_worktrees_succeeds() {
    let fixture = Fixture::new();
    let result = prune_project_worktrees(fixture.repo())
        .await
        .expect("prune succeeds");
    assert_eq!(result.status, ThreadWorkspaceActionStatus::Completed);
    assert!(fixture.managed_worktree.is_dir());
}

fn git(cwd: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .expect("git starts");
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
