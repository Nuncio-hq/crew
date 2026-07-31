use std::{fs, path::PathBuf};

use tokio::process::Command;
use uuid::Uuid;

use crate::thread_workspace::{ensure_thread_worktree, parse_project_workspace, ProjectWorkspace};

#[test]
fn parses_encoded_project_workspace_context() {
    let content = "[ctx]: <buzz://project-workspace?repo=github.com%2Facme%2Fapp&path=%2Ftmp%2Fapp> \"Project\"\n\nFix it";
    let workspace = parse_project_workspace(content)
        .expect("valid context")
        .expect("context present");
    assert_eq!(workspace.repo_address, "github.com/acme/app");
    assert_eq!(workspace.local_path, PathBuf::from("/tmp/app"));
}

#[test]
fn rejects_relative_workspace_path() {
    let content = "buzz://project-workspace?repo=acme%2Fapp&path=relative";
    assert!(parse_project_workspace(content).is_err());
}

#[tokio::test]
async fn create_and_reuse_return_stable_verified_metadata() {
    let (fixture, workspace, base_revision) = git_fixture().await;
    let root = "a".repeat(64);

    let created = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("create succeeds");
    fs::write(created.worktree_path.join("AGENT.md"), "thread change")
        .expect("thread worktree change");
    run_git(&created.worktree_path, &["add", "AGENT.md"]).await;
    run_git(&created.worktree_path, &["commit", "-m", "thread change"]).await;
    let reused = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("reuse succeeds");

    assert_eq!(created, reused);
    assert_eq!(created.root_event_id, root);
    assert_eq!(created.branch, "buzz/aaaaaaaaaaaa");
    assert_eq!(created.worktree_name, "project-aaaaaaaaaaaa");
    assert_eq!(created.base_revision, base_revision);
    assert!(created.worktree_path.join("README.md").is_file());

    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn concurrent_ensure_calls_converge_on_one_worktree() {
    let (fixture, workspace, _) = git_fixture().await;
    let root = "b".repeat(64);
    let (first, second) = tokio::join!(
        ensure_thread_worktree(&workspace, &root),
        ensure_thread_worktree(&workspace, &root)
    );
    let first = first.expect("first ensure succeeds");
    let second = second.expect("second ensure converges");
    assert_eq!(first, second);
    assert!(first.worktree_path.join("README.md").is_file());

    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn concurrent_distinct_roots_create_distinct_worktrees() {
    let (fixture, workspace, base_revision) = git_fixture().await;
    let first_root = "c".repeat(64);
    let second_root = "d".repeat(64);
    let (first, second) = tokio::join!(
        ensure_thread_worktree(&workspace, &first_root),
        ensure_thread_worktree(&workspace, &second_root)
    );
    let first = first.expect("first root succeeds");
    let second = second.expect("second root succeeds");

    assert_ne!(first.worktree_path, second.worktree_path);
    assert_ne!(first.branch, second.branch);
    assert_eq!(first.root_event_id, first_root);
    assert_eq!(second.root_event_id, second_root);
    assert_eq!(first.base_revision, base_revision);
    assert_eq!(second.base_revision, base_revision);
    assert!(first.worktree_path.join("README.md").is_file());
    assert!(second.worktree_path.join("README.md").is_file());

    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn concurrent_distinct_roots_with_the_same_prefix_cannot_both_succeed() {
    let (fixture, workspace, _) = git_fixture().await;
    let first_root = format!("{}{}", "e".repeat(12), "1".repeat(52));
    let colliding_root = format!("{}{}", "e".repeat(12), "2".repeat(52));

    let (first, collision) = tokio::join!(
        ensure_thread_worktree(&workspace, &first_root),
        ensure_thread_worktree(&workspace, &colliding_root)
    );

    let (winner_root, winner) = exactly_one_winner(first_root, first, colliding_root, collision);
    let reused = ensure_thread_worktree(&workspace, &winner_root)
        .await
        .expect("winning root remains reusable");
    assert_eq!(reused, winner);
    assert_recorded_winner(&workspace, "buzz/eeeeeeeeeeee", &winner_root).await;

    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn legacy_worktree_without_root_metadata_is_adopted_for_reuse() {
    let (fixture, workspace, _) = git_fixture().await;
    let root = "f".repeat(64);
    let created = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("initial create succeeds");
    let key = "branch.buzz/ffffffffffff.buzzThreadRoot";
    remove_root_identity(&workspace, &root, key).await;

    let (first, second) = tokio::join!(
        ensure_thread_worktree(&workspace, &root),
        ensure_thread_worktree(&workspace, &root)
    );
    assert_eq!(first.expect("first legacy reuse succeeds"), created);
    assert_eq!(second.expect("second legacy reuse succeeds"), created);
    let recorded = git_output(
        &workspace.local_path,
        &["config", "--local", "--get-all", key],
    )
    .await;
    assert!(
        recorded
            .lines()
            .all(|value| value.eq_ignore_ascii_case(&root)),
        "same-root adoption may duplicate values but never change identity"
    );

    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn concurrent_same_prefix_legacy_adoptions_cannot_both_succeed() {
    let (fixture, workspace, _) = git_fixture().await;
    let first_root = format!("{}{}", "9".repeat(12), "1".repeat(52));
    let colliding_root = format!("{}{}", "9".repeat(12), "2".repeat(52));
    ensure_thread_worktree(&workspace, &first_root)
        .await
        .expect("initial create succeeds");
    let key = "branch.buzz/999999999999.buzzThreadRoot";
    remove_root_identity(&workspace, &first_root, key).await;

    let (first, collision) = tokio::join!(
        ensure_thread_worktree(&workspace, &first_root),
        ensure_thread_worktree(&workspace, &colliding_root)
    );

    let (winner_root, winner) = exactly_one_winner(first_root, first, colliding_root, collision);
    let reused = ensure_thread_worktree(&workspace, &winner_root)
        .await
        .expect("winning legacy root remains reusable");
    assert_eq!(reused, winner);
    assert_recorded_winner(&workspace, "buzz/999999999999", &winner_root).await;

    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

fn exactly_one_winner(
    first_root: String,
    first: anyhow::Result<crate::thread_workspace::ThreadWorkspace>,
    second_root: String,
    second: anyhow::Result<crate::thread_workspace::ThreadWorkspace>,
) -> (String, crate::thread_workspace::ThreadWorkspace) {
    match (first, second) {
        (Ok(winner), Err(_)) => (first_root, winner),
        (Err(_), Ok(winner)) => (second_root, winner),
        (Ok(_), Ok(_)) => panic!("same-prefix roots must never both claim one worktree"),
        (Err(first_error), Err(second_error)) => {
            panic!("one same-prefix root must win: {first_error}; {second_error}")
        }
    }
}

async fn assert_recorded_winner(workspace: &ProjectWorkspace, branch: &str, winner_root: &str) {
    let key = format!("branch.{branch}.buzzThreadRoot");
    let recorded = git_output(
        &workspace.local_path,
        &["config", "--local", "--get-all", &key],
    )
    .await;
    assert!(
        !recorded.is_empty()
            && recorded
                .lines()
                .all(|value| value.eq_ignore_ascii_case(winner_root)),
        "only the durable winner may be recorded in branch config"
    );
}

async fn remove_root_identity(workspace: &ProjectWorkspace, root: &str, config_key: &str) {
    run_git(
        &workspace.local_path,
        &["config", "--local", "--unset-all", config_key],
    )
    .await;
    let common_git = git_output(&workspace.local_path, &["rev-parse", "--git-common-dir"]).await;
    let common_git = PathBuf::from(common_git);
    let common_git = if common_git.is_absolute() {
        common_git
    } else {
        workspace.local_path.join(common_git)
    };
    let claim = common_git
        .join("buzz-thread-workspace-roots")
        .join(format!("{}.root", &root[..12]));
    fs::remove_file(claim).expect("remove current-version claim to model a legacy worktree");
}

async fn git_fixture() -> (PathBuf, ProjectWorkspace, String) {
    let fixture = std::env::temp_dir().join(format!("buzz-worktree-test-{}", Uuid::new_v4()));
    let repo = fixture.join("project");
    fs::create_dir_all(&repo).expect("fixture directory");
    run_git(&repo, &["init", "-b", "main"]).await;
    run_git(&repo, &["config", "user.email", "test@example.com"]).await;
    run_git(&repo, &["config", "user.name", "Test"]).await;
    fs::write(repo.join("README.md"), "fixture").expect("fixture file");
    run_git(&repo, &["add", "README.md"]).await;
    run_git(&repo, &["commit", "-m", "fixture"]).await;
    let base_revision = git_output(&repo, &["rev-parse", "HEAD"]).await;

    let workspace = ProjectWorkspace {
        repo_address: "fixture/project".to_string(),
        local_path: repo,
    };
    (fixture, workspace, base_revision)
}

async fn run_git(cwd: &std::path::Path, args: &[&str]) {
    let _ = git_output(cwd, args).await;
}

async fn git_output(cwd: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .await
        .expect("git starts");
    assert!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output is UTF-8")
        .trim()
        .to_string()
}
