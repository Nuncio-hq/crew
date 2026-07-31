use std::{fs, path::PathBuf, process::Command};

use tempfile::TempDir;

use super::{delete_thread_branch, remove_thread_worktree, ThreadWorkspaceActionStatus};

struct Fixture {
    _temp: TempDir,
    repository: PathBuf,
    worktree: PathBuf,
    branch: String,
    root: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temp dir");
        let repository = temp.path().join("project");
        let worktree = temp.path().join("project-thread");
        fs::create_dir_all(&repository).expect("repository dir");
        git(&repository, &["init", "-b", "main"]);
        git(&repository, &["config", "user.email", "test@example.com"]);
        git(&repository, &["config", "user.name", "Test"]);
        fs::write(repository.join("README.md"), "fixture").expect("fixture file");
        git(&repository, &["add", "README.md"]);
        git(&repository, &["commit", "-m", "fixture"]);
        let root = "a".repeat(64);
        let branch = "buzz/aaaaaaaaaaaa".to_string();
        git(
            &repository,
            &[
                "worktree",
                "add",
                "-b",
                branch.as_str(),
                worktree.to_str().expect("UTF-8 worktree"),
                "HEAD",
            ],
        );
        git(
            &repository,
            &[
                "config",
                "--add",
                "branch.buzz/aaaaaaaaaaaa.buzzThreadRoot",
                root.as_str(),
            ],
        );
        let common_git = repository.join(".git");
        let claims = common_git.join("buzz-thread-workspace-roots");
        fs::create_dir_all(&claims).expect("claim dir");
        fs::write(claims.join("aaaaaaaaaaaa.root"), format!("{root}\n")).expect("root claim");
        Self {
            _temp: temp,
            repository,
            worktree,
            branch,
            root,
        }
    }

    fn args(&self) -> (String, String, String) {
        (
            self.repository.to_string_lossy().into_owned(),
            self.branch.clone(),
            self.root.clone(),
        )
    }
}

#[tokio::test]
async fn dirty_worktree_returns_typed_refusal() {
    let fixture = Fixture::new();
    fs::write(fixture.worktree.join("dirty.txt"), "dirty").expect("dirty file");
    let (repository, branch, root) = fixture.args();

    let result = remove_thread_worktree(
        repository,
        fixture.worktree.to_string_lossy().into_owned(),
        branch,
        root,
    )
    .await
    .expect("command succeeds");

    assert_eq!(result.status, ThreadWorkspaceActionStatus::Refused);
    assert!(fixture.worktree.is_dir());
}

#[tokio::test]
async fn clean_worktree_can_be_removed_then_branch_deleted() {
    let fixture = Fixture::new();
    let (repository, branch, root) = fixture.args();
    let removed = remove_thread_worktree(
        repository.clone(),
        fixture.worktree.to_string_lossy().into_owned(),
        branch.clone(),
        root.clone(),
    )
    .await
    .expect("remove command succeeds");
    assert_eq!(removed.status, ThreadWorkspaceActionStatus::Completed);
    assert!(!fixture.worktree.exists());

    let deleted = delete_thread_branch(repository, branch, root)
        .await
        .expect("delete command succeeds");
    assert_eq!(deleted.status, ThreadWorkspaceActionStatus::Completed);

    let (repository, branch, root) = fixture.args();
    let deleted_again = delete_thread_branch(repository, branch, root)
        .await
        .expect("repeat delete succeeds");
    assert_eq!(deleted_again.status, ThreadWorkspaceActionStatus::NotFound);
}

#[tokio::test]
async fn checked_out_branch_returns_typed_refusal() {
    let fixture = Fixture::new();
    let (repository, branch, root) = fixture.args();

    let result = delete_thread_branch(repository, branch, root)
        .await
        .expect("command succeeds");

    assert_eq!(result.status, ThreadWorkspaceActionStatus::Refused);
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
