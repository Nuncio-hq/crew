use std::{fs, path::PathBuf, process::Command};

use tempfile::TempDir;

use super::project_worktree_cleanup::{
    prepare_managed_removal, prune_project_worktrees, remove_project_worktree,
};
use super::thread_workspace::ThreadWorkspaceActionStatus;

const CHANNEL: &str = "11111111-1111-1111-1111-111111111111";
const OTHER_CHANNEL: &str = "22222222-2222-2222-2222-222222222222";

struct Fixture {
    _temp: TempDir,
    repository: PathBuf,
    managed_root: PathBuf,
    managed_worktree: PathBuf,
    external_worktree: PathBuf,
    root: String,
    common_git: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temp dir");
        let root_dir = temp.path();
        let repository = root_dir.join("crew");
        let managed_root = root_dir.join(".buzz-worktrees");
        let managed_worktree = managed_root.join("crew-aaaaaaaaaaaa");
        let external_worktree = root_dir.join(".worktrees").join("crew-docs-fork");
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
        let root = "a".repeat(64);
        git(
            &repository,
            &[
                "config",
                "branch.buzz/aaaaaaaaaaaa.buzzThreadRoot",
                root.as_str(),
            ],
        );
        let common_git = git_common(&repository);
        buzz_worktree::adopt_or_create_record(
            &common_git,
            &root,
            CHANNEL,
            None,
            "buzz/aaaaaaaaaaaa",
            managed_worktree.to_str().expect("UTF-8"),
            None,
        )
        .expect("lifecycle record");
        Self {
            _temp: temp,
            repository,
            managed_root,
            managed_worktree,
            external_worktree,
            root,
            common_git,
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
    let result =
        remove_project_worktree(fixture.repo(), fixture.managed_path(), CHANNEL.to_string())
            .await
            .expect("remove succeeds");
    assert_eq!(result.status, ThreadWorkspaceActionStatus::Completed);
    assert!(!fixture.managed_worktree.exists());
    let record = buzz_worktree::read_lifecycle_record(&fixture.common_git, &fixture.root)
        .expect("read")
        .expect("record retained");
    assert_eq!(record.eviction_generation, 1);
}

#[tokio::test]
async fn dirty_managed_worktree_refuses_without_remove() {
    let fixture = Fixture::new();
    fs::write(fixture.managed_worktree.join("dirty.txt"), "dirty").expect("dirty");
    let result =
        remove_project_worktree(fixture.repo(), fixture.managed_path(), CHANNEL.to_string())
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
        CHANNEL.to_string(),
    )
    .await
    .expect_err("external refused");
    assert!(err.contains("managed") || err.contains("Buzz"));
    assert!(fixture.external_worktree.is_dir());
}

#[tokio::test]
async fn main_worktree_errors_without_remove() {
    let fixture = Fixture::new();
    let err = remove_project_worktree(fixture.repo(), fixture.repo(), CHANNEL.to_string())
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
    let err = remove_project_worktree(
        fixture.repo(),
        ghost.to_string_lossy().into_owned(),
        CHANNEL.to_string(),
    )
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
    let err = remove_project_worktree(
        fixture.repo(),
        path.to_string_lossy().into_owned(),
        CHANNEL.to_string(),
    )
    .await
    .expect_err("non-buzz refused");
    assert!(err.contains("managed") || err.contains("Buzz") || err.contains("branch"));
    assert!(path.is_dir());
}

#[tokio::test]
async fn missing_path_cannot_canonicalize() {
    let fixture = Fixture::new();
    let missing = fixture.managed_root.join("crew-missingmissing");
    let err = remove_project_worktree(
        fixture.repo(),
        missing.to_string_lossy().into_owned(),
        CHANNEL.to_string(),
    )
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

#[tokio::test]
async fn exclusive_lease_holder_refuses_eviction() {
    let fixture = Fixture::new();
    let shared =
        buzz_worktree::try_acquire_shared(&fixture.common_git, &fixture.root).expect("shared");
    let result =
        remove_project_worktree(fixture.repo(), fixture.managed_path(), CHANNEL.to_string())
            .await
            .expect("command returns");
    assert_eq!(result.status, ThreadWorkspaceActionStatus::Refused);
    assert!(result.message.to_lowercase().contains("agent"));
    assert!(fixture.managed_worktree.is_dir());
    drop(shared);
    let result =
        remove_project_worktree(fixture.repo(), fixture.managed_path(), CHANNEL.to_string())
            .await
            .expect("evict after release");
    assert_eq!(result.status, ThreadWorkspaceActionStatus::Completed);
    assert!(!fixture.managed_worktree.exists());
}

#[tokio::test]
async fn other_channel_cannot_evict() {
    let fixture = Fixture::new();
    let result = remove_project_worktree(
        fixture.repo(),
        fixture.managed_path(),
        OTHER_CHANNEL.to_string(),
    )
    .await
    .expect("command returns");
    assert_eq!(result.status, ThreadWorkspaceActionStatus::Refused);
    assert!(result.message.to_lowercase().contains("another channel"));
    assert!(fixture.managed_worktree.is_dir());
}

#[tokio::test]
async fn legacy_without_lifecycle_record_cannot_evict() {
    let fixture = Fixture::new();
    let path =
        buzz_worktree::lifecycle_record_path(&fixture.common_git, &fixture.root).expect("path");
    fs::remove_file(&path).expect("delete record");
    let result =
        remove_project_worktree(fixture.repo(), fixture.managed_path(), CHANNEL.to_string())
            .await
            .expect("command returns");
    assert_eq!(result.status, ThreadWorkspaceActionStatus::Refused);
    assert!(result.message.to_lowercase().contains("verif"));
    assert!(fixture.managed_worktree.is_dir());
}

#[tokio::test]
async fn missing_root_claim_cannot_evict() {
    let fixture = Fixture::new();
    git(
        &fixture.repository,
        &[
            "config",
            "--unset",
            "branch.buzz/aaaaaaaaaaaa.buzzThreadRoot",
        ],
    );
    let result =
        remove_project_worktree(fixture.repo(), fixture.managed_path(), CHANNEL.to_string())
            .await
            .expect("command returns");
    assert_eq!(result.status, ThreadWorkspaceActionStatus::Refused);
    assert!(
        result.message.to_lowercase().contains("legacy")
            || result.message.to_lowercase().contains("root")
    );
    assert!(fixture.managed_worktree.is_dir());
}

#[tokio::test]
async fn ignored_only_local_state_refuses_eviction_and_preserves_checkout() {
    let fixture = Fixture::new();
    plant_ignored_secret(&fixture.managed_worktree);
    assert!(
        git_status_porcelain(&fixture.managed_worktree)
            .trim()
            .is_empty(),
        "plain status must be empty for this fixture"
    );

    let result =
        remove_project_worktree(fixture.repo(), fixture.managed_path(), CHANNEL.to_string())
            .await
            .expect("command returns");
    assert_eq!(result.status, ThreadWorkspaceActionStatus::Refused);
    assert!(
        result.message.to_lowercase().contains("ignored"),
        "refusal should name ignored state: {}",
        result.message
    );
    assert!(fixture.managed_worktree.is_dir());
    assert!(
        fixture
            .managed_worktree
            .join("ignored-local/secret.txt")
            .is_file(),
        "ignored file must survive refused eviction"
    );
}

fn plant_ignored_secret(worktree: &std::path::Path) {
    fs::write(worktree.join(".gitignore"), "ignored-local/\n").expect("gitignore");
    git(worktree, &["add", ".gitignore"]);
    git(worktree, &["commit", "-m", "ignore local"]);
    let dir = worktree.join("ignored-local");
    fs::create_dir_all(&dir).expect("ignored dir");
    fs::write(dir.join("secret.txt"), "secret").expect("secret");
}

fn git_status_porcelain(cwd: &std::path::Path) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["status", "--porcelain"])
        .output()
        .expect("status");
    assert!(output.status.success());
    String::from_utf8(output.stdout).expect("utf8")
}

fn git_common(repo: &std::path::Path) -> std::path::PathBuf {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .expect("git common");
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("utf8");
    let path = std::path::PathBuf::from(text.trim());
    if path.is_absolute() {
        path
    } else {
        repo.join(path)
    }
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
