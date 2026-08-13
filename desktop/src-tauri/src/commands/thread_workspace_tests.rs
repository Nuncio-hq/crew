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
async fn ignored_only_local_state_refuses_thread_removal_and_preserves_file() {
    let fixture = Fixture::new();
    fs::write(fixture.worktree.join(".gitignore"), "ignored-local/\n").expect("gitignore");
    git(&fixture.worktree, &["add", ".gitignore"]);
    git(&fixture.worktree, &["commit", "-m", "ignore"]);
    let secret_dir = fixture.worktree.join("ignored-local");
    fs::create_dir_all(&secret_dir).expect("ignored dir");
    fs::write(secret_dir.join("secret.txt"), "secret").expect("secret");

    let plain = Command::new("git")
        .arg("-C")
        .arg(&fixture.worktree)
        .args(["status", "--porcelain"])
        .output()
        .expect("status");
    assert!(plain.status.success());
    assert!(String::from_utf8_lossy(&plain.stdout).trim().is_empty());

    let (repository, branch, root) = fixture.args();
    let result = remove_thread_worktree(
        repository,
        fixture.worktree.to_string_lossy().into_owned(),
        branch,
        root,
    )
    .await
    .expect("command returns");

    assert_eq!(result.status, ThreadWorkspaceActionStatus::Refused);
    assert!(result.message.to_lowercase().contains("ignored"));
    assert!(fixture.worktree.is_dir());
    assert!(secret_dir.join("secret.txt").is_file());
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
    // Branch is retained for reattach.
    git(
        &fixture.repository,
        &["show-ref", "--verify", &format!("refs/heads/{branch}")],
    );

    let deleted = delete_thread_branch(repository, branch, root)
        .await
        .expect("delete command succeeds");
    assert_eq!(deleted.status, ThreadWorkspaceActionStatus::Completed);

    // Recreating the deterministic branch name without the ownership config
    // must not let a stale thread panel delete that new branch.
    git(&fixture.repository, &["branch", fixture.branch.as_str()]);
    let (repository, branch, root) = fixture.args();
    let recreated = delete_thread_branch(repository, branch.clone(), root).await;
    assert!(recreated.is_err());
    git(
        &fixture.repository,
        &["show-ref", "--verify", &format!("refs/heads/{branch}")],
    );
}

#[tokio::test]
async fn active_lease_refuses_thread_worktree_removal() {
    let fixture = Fixture::new();
    let common = fixture.repository.join(".git");
    let shared = buzz_worktree::try_acquire_shared(&common, &fixture.root).expect("shared");
    let (repository, branch, root) = fixture.args();
    let result = remove_thread_worktree(
        repository,
        fixture.worktree.to_string_lossy().into_owned(),
        branch,
        root,
    )
    .await
    .expect("command returns");
    assert_eq!(result.status, ThreadWorkspaceActionStatus::Refused);
    assert!(result.message.to_lowercase().contains("agent"));
    assert!(fixture.worktree.is_dir());
    drop(shared);
}

#[tokio::test]
async fn successful_thread_eviction_retains_branch_and_advances_generation() {
    let fixture = Fixture::new();
    let common = fixture.repository.join(".git");
    buzz_worktree::adopt_or_create_record(
        &common,
        &fixture.root,
        "11111111-1111-1111-1111-111111111111",
        None,
        &fixture.branch,
        fixture.worktree.to_str().expect("utf8"),
        None,
    )
    .expect("record");
    let (repository, branch, root) = fixture.args();
    let result = remove_thread_worktree(
        repository,
        fixture.worktree.to_string_lossy().into_owned(),
        branch.clone(),
        root.clone(),
    )
    .await
    .expect("remove");
    assert_eq!(result.status, ThreadWorkspaceActionStatus::Completed);
    assert!(!fixture.worktree.exists());
    git(
        &fixture.repository,
        &["show-ref", "--verify", &format!("refs/heads/{branch}")],
    );
    let record = buzz_worktree::read_lifecycle_record(&common, &root)
        .expect("read")
        .expect("record");
    assert_eq!(record.eviction_generation, 1);
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
