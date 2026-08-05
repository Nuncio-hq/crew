use std::{fs, path::PathBuf};

use tokio::process::Command;
use uuid::Uuid;

use crate::thread_workspace::BaseSource;
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
async fn remote_ahead_tip_is_used_as_the_new_worktree_base() {
    let (fixture, workspace, remote) = remote_git_fixture("main").await;
    let remote_tip = push_remote_commit(&fixture, &remote, "main").await;

    let created = ensure_thread_worktree(&workspace, &"1".repeat(64))
        .await
        .expect("create succeeds");

    assert_eq!(created.base_revision, remote_tip);
    assert_eq!(created.base_source, BaseSource::Remote);
    assert_eq!(created.remote_default_branch.as_deref(), Some("main"));
    assert_eq!(created.commits_behind_remote, Some(0));
    assert_eq!(
        git_output(&created.worktree_path, &["rev-parse", "HEAD"]).await,
        remote_tip
    );
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn fetch_failure_falls_back_to_local_head_without_blocking_creation() {
    let (fixture, workspace, remote) = remote_git_fixture("main").await;
    let local_head = git_output(&workspace.local_path, &["rev-parse", "HEAD"]).await;
    fs::rename(&remote, fixture.join("remote-unavailable.git")).expect("disable remote");

    let created = ensure_thread_worktree(&workspace, &"2".repeat(64))
        .await
        .expect("fallback create succeeds");

    assert_eq!(created.base_revision, local_head);
    assert_eq!(created.base_source, BaseSource::LocalFallback);
    assert_eq!(created.remote_default_branch.as_deref(), Some("main"));
    assert_eq!(created.commits_behind_remote, None);
    assert_eq!(
        git_output(&created.worktree_path, &["rev-parse", "HEAD"]).await,
        local_head
    );
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn reused_worktree_reports_its_actual_distance_from_the_latest_remote_tip() {
    let (fixture, workspace, remote) = remote_git_fixture("main").await;
    let root = "8".repeat(64);
    let created = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("initial create succeeds");
    let original_head = git_output(&created.worktree_path, &["rev-parse", "HEAD"]).await;
    push_remote_commit(&fixture, &remote, "main").await;

    let reused = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("reuse succeeds");

    assert_eq!(
        git_output(&reused.worktree_path, &["rev-parse", "HEAD"]).await,
        original_head
    );
    assert_eq!(reused.base_source, BaseSource::Remote);
    assert_eq!(reused.commits_behind_remote, Some(1));
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn detached_remote_head_reuses_the_configured_remote_default_branch() {
    let (fixture, workspace, remote) = remote_git_fixture("main").await;
    let head = git_output(&workspace.local_path, &["rev-parse", "HEAD"]).await;
    fs::write(remote.join("HEAD"), format!("{head}\n")).expect("detach remote HEAD");
    run_git(
        &workspace.local_path,
        &["symbolic-ref", "--delete", "refs/remotes/origin/HEAD"],
    )
    .await;

    let created = ensure_thread_worktree(&workspace, &"3".repeat(64))
        .await
        .expect("fallback create succeeds");

    assert_eq!(created.base_source, BaseSource::Remote);
    assert_eq!(created.remote_default_branch.as_deref(), Some("main"));
    assert_eq!(created.base_revision, head);
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn remote_default_branch_is_not_assumed_to_be_main() {
    let (fixture, workspace, _) = remote_git_fixture("trunk").await;

    let created = ensure_thread_worktree(&workspace, &"4".repeat(64))
        .await
        .expect("create succeeds");

    assert_eq!(created.base_source, BaseSource::Remote);
    assert_eq!(created.remote_default_branch.as_deref(), Some("trunk"));
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
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
async fn clean_foreign_branch_recovers_without_losing_its_commit() {
    let (fixture, workspace, _) = git_fixture().await;
    let root = "5".repeat(64);
    let created = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("initial create succeeds");
    let feature_commit = checkout_feature_with_commit(&created.worktree_path).await;

    let recovered = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("clean foreign branch recovers");

    assert_eq!(recovered.branch, "buzz/555555555555");
    assert_eq!(
        git_output(&created.worktree_path, &["symbolic-ref", "--short", "HEAD"]).await,
        "buzz/555555555555"
    );
    assert_eq!(
        git_output(&created.worktree_path, &["rev-parse", "feature"]).await,
        feature_commit,
        "recovery must preserve the foreign branch commit"
    );
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn dirty_foreign_branch_refuses_recovery_with_actionable_error() {
    let (fixture, workspace, _) = git_fixture().await;
    let root = "6".repeat(64);
    let created = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("initial create succeeds");
    checkout_feature_with_commit(&created.worktree_path).await;
    let dirty_path = created.worktree_path.join("DIRTY.md");
    fs::write(&dirty_path, "do not discard").expect("dirty file");

    let error = ensure_thread_worktree(&workspace, &root)
        .await
        .expect_err("dirty foreign branch must not recover");

    assert_branch_conflict_error(
        &error,
        &created.worktree_path,
        "feature",
        "buzz/666666666666",
    );
    assert_eq!(
        git_output(&created.worktree_path, &["symbolic-ref", "--short", "HEAD"]).await,
        "feature"
    );
    assert_eq!(
        fs::read_to_string(dirty_path).expect("dirty file remains"),
        "do not discard"
    );
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn merge_in_progress_refuses_foreign_branch_recovery() {
    let (fixture, workspace, base_revision) = git_fixture().await;
    let root = "7".repeat(64);
    let created = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("initial create succeeds");
    run_git(&created.worktree_path, &["checkout", "-b", "feature"]).await;
    let git_dir =
        PathBuf::from(git_output(&created.worktree_path, &["rev-parse", "--git-dir"]).await);
    let merge_head = git_dir.join("MERGE_HEAD");
    fs::write(&merge_head, format!("{base_revision}\n")).expect("merge marker");
    assert_eq!(
        git_output(&created.worktree_path, &["status", "--porcelain"]).await,
        "",
        "merge marker fixture must keep the worktree otherwise clean"
    );

    let error = ensure_thread_worktree(&workspace, &root)
        .await
        .expect_err("merge in progress must not recover");

    assert_branch_conflict_error(
        &error,
        &created.worktree_path,
        "feature",
        "buzz/777777777777",
    );
    assert!(merge_head.is_file(), "recovery must preserve MERGE_HEAD");
    assert_eq!(
        git_output(&created.worktree_path, &["symbolic-ref", "--short", "HEAD"]).await,
        "feature"
    );
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn reachable_clean_detached_head_recovers() {
    let (fixture, workspace, _) = git_fixture().await;
    let root = "8".repeat(64);
    let created = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("initial create succeeds");
    let expected_commit =
        git_output(&created.worktree_path, &["rev-parse", "buzz/888888888888"]).await;
    let feature_commit = checkout_feature_with_commit(&created.worktree_path).await;
    let detached_commit = detach_head(&created.worktree_path).await;
    assert_eq!(detached_commit, feature_commit);
    assert!(
        !named_refs_containing(&created.worktree_path, &detached_commit)
            .await
            .is_empty(),
        "fixture commit must remain reachable from a named ref"
    );

    let recovered = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("reachable clean detached HEAD recovers");

    assert_eq!(recovered.branch, "buzz/888888888888");
    assert_eq!(
        git_output(&created.worktree_path, &["symbolic-ref", "--short", "HEAD"]).await,
        "buzz/888888888888"
    );
    assert_eq!(
        git_output(&created.worktree_path, &["rev-parse", "HEAD"]).await,
        expected_commit
    );
    assert_eq!(
        git_output(&created.worktree_path, &["rev-parse", "feature"]).await,
        detached_commit,
        "recovery must preserve the detached commit through its named ref"
    );
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn unreachable_detached_commit_refuses_recovery_and_names_commit() {
    let (fixture, workspace, _) = git_fixture().await;
    let root = "a".repeat(64);
    let created = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("initial create succeeds");
    detach_head(&created.worktree_path).await;
    fs::write(created.worktree_path.join("DETACHED.md"), "preserve commit").expect("detached file");
    run_git(&created.worktree_path, &["add", "DETACHED.md"]).await;
    run_git(
        &created.worktree_path,
        &["commit", "-m", "unreachable detached commit"],
    )
    .await;
    let detached_commit = git_output(&created.worktree_path, &["rev-parse", "HEAD"]).await;
    assert_eq!(
        git_output(&created.worktree_path, &["status", "--porcelain"]).await,
        "",
        "fixture must be clean before recovery"
    );
    assert_eq!(
        named_refs_containing(&created.worktree_path, &detached_commit).await,
        "",
        "fixture commit must not be reachable from a named branch"
    );

    let error = ensure_thread_worktree(&workspace, &root)
        .await
        .expect_err("unreachable detached commit must not recover");

    assert_branch_conflict_error(
        &error,
        &created.worktree_path,
        &detached_commit,
        "buzz/aaaaaaaaaaaa",
    );
    assert!(
        error.to_string().contains("detached"),
        "error must identify detached HEAD: {error}"
    );
    assert!(
        error.to_string().contains("not reachable")
            && error.to_string().contains("preserve it with a branch"),
        "error must explain how to preserve the detached commit: {error}"
    );
    assert_eq!(
        git_output(&created.worktree_path, &["rev-parse", "HEAD"]).await,
        detached_commit,
        "recovery must preserve the unreachable commit"
    );
    assert!(
        git_output(&created.worktree_path, &["branch", "--show-current"])
            .await
            .is_empty(),
        "HEAD must remain detached"
    );
    assert_eq!(
        fs::read_to_string(created.worktree_path.join("DETACHED.md"))
            .expect("detached file remains"),
        "preserve commit"
    );
    assert_eq!(
        named_refs_containing(&created.worktree_path, &detached_commit).await,
        "",
        "refusing recovery must not create a named ref"
    );
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn dirty_detached_head_refuses_recovery() {
    let (fixture, workspace, _) = git_fixture().await;
    let root = "c".repeat(64);
    let created = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("initial create succeeds");
    let detached_commit = detach_head(&created.worktree_path).await;
    let dirty_path = created.worktree_path.join("DIRTY-DETACHED.md");
    fs::write(&dirty_path, "do not discard").expect("dirty detached file");

    let error = ensure_thread_worktree(&workspace, &root)
        .await
        .expect_err("dirty detached HEAD must not recover");

    assert_branch_conflict_error(
        &error,
        &created.worktree_path,
        &detached_commit,
        "buzz/cccccccccccc",
    );
    assert_eq!(
        git_output(&created.worktree_path, &["rev-parse", "HEAD"]).await,
        detached_commit
    );
    assert!(
        git_output(&created.worktree_path, &["branch", "--show-current"])
            .await
            .is_empty(),
        "HEAD must remain detached"
    );
    assert_eq!(
        fs::read_to_string(dirty_path).expect("dirty detached file remains"),
        "do not discard"
    );
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn reattach_failure_reports_the_path_conflict_stderr() {
    let (fixture, workspace, _) = git_fixture().await;
    let root = "0".repeat(64);
    let created = ensure_thread_worktree(&workspace, &root)
        .await
        .expect("initial create succeeds");
    run_git(
        &workspace.local_path,
        &[
            "worktree",
            "remove",
            created.worktree_path.to_str().expect("worktree UTF-8"),
        ],
    )
    .await;
    fs::create_dir_all(&created.worktree_path).expect("blocking directory");
    fs::write(created.worktree_path.join("BLOCKER"), "occupied")
        .expect("non-empty blocking directory");

    let error = ensure_thread_worktree(&workspace, &root)
        .await
        .expect_err("reattach must fail while the path exists");
    let message = error.to_string();

    assert!(
        message.contains("already exists"),
        "reattach stderr must describe the path conflict: {message}"
    );
    assert!(
        message.contains(created.worktree_path.to_string_lossy().as_ref()),
        "reattach stderr must name the conflicting path: {message}"
    );
    assert!(
        !message.contains("a branch named 'buzz/000000000000' already exists"),
        "reattach failure must not report the earlier create stderr: {message}"
    );
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

fn assert_branch_conflict_error(
    error: &anyhow::Error,
    worktree_path: &std::path::Path,
    current_branch: &str,
    expected_branch: &str,
) {
    let message = error.to_string();
    assert!(
        message.contains(worktree_path.to_string_lossy().as_ref()),
        "error must name the worktree path: {message}"
    );
    assert!(
        message.contains(current_branch),
        "error must name the current branch: {message}"
    );
    assert!(
        message.contains(expected_branch),
        "error must name the expected branch: {message}"
    );
    assert!(
        message.contains("git -C") && message.contains("checkout"),
        "error must include a runnable checkout command: {message}"
    );
}

async fn checkout_feature_with_commit(worktree_path: &std::path::Path) -> String {
    run_git(worktree_path, &["checkout", "-b", "feature"]).await;
    fs::write(worktree_path.join("FEATURE.md"), "preserved").expect("feature file");
    run_git(worktree_path, &["add", "FEATURE.md"]).await;
    run_git(worktree_path, &["commit", "-m", "feature commit"]).await;
    git_output(worktree_path, &["rev-parse", "feature"]).await
}

async fn detach_head(worktree_path: &std::path::Path) -> String {
    let head = git_output(worktree_path, &["rev-parse", "HEAD"]).await;
    run_git(worktree_path, &["checkout", "--detach", &head]).await;
    head
}

async fn named_refs_containing(worktree_path: &std::path::Path, commit: &str) -> String {
    let contains = format!("--contains={commit}");
    git_output(
        worktree_path,
        &[
            "for-each-ref",
            "--count=1",
            contains.as_str(),
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )
    .await
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

async fn remote_git_fixture(default_branch: &str) -> (PathBuf, ProjectWorkspace, PathBuf) {
    let fixture = std::env::temp_dir().join(format!("buzz-remote-test-{}", Uuid::new_v4()));
    let remote = fixture.join("remote.git");
    let repo = fixture.join("project");
    fs::create_dir_all(&fixture).expect("fixture directory");
    run_git(
        &fixture,
        &["init", "--bare", remote.to_str().expect("remote UTF-8")],
    )
    .await;
    fs::create_dir_all(&repo).expect("repository directory");
    run_git(&repo, &["init", "-b", default_branch]).await;
    run_git(&repo, &["config", "user.email", "test@example.com"]).await;
    run_git(&repo, &["config", "user.name", "Test"]).await;
    fs::write(repo.join("README.md"), "fixture").expect("fixture file");
    run_git(&repo, &["add", "README.md"]).await;
    run_git(&repo, &["commit", "-m", "fixture"]).await;
    run_git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            remote.to_str().expect("remote UTF-8"),
        ],
    )
    .await;
    run_git(&repo, &["push", "-u", "origin", default_branch]).await;
    run_git(
        &fixture,
        &[
            "--git-dir",
            remote.to_str().expect("remote UTF-8"),
            "symbolic-ref",
            "HEAD",
            &format!("refs/heads/{default_branch}"),
        ],
    )
    .await;
    run_git(&repo, &["remote", "set-head", "origin", default_branch]).await;
    (
        fixture,
        ProjectWorkspace {
            repo_address: "fixture/project".to_string(),
            local_path: repo,
        },
        remote,
    )
}

async fn push_remote_commit(
    fixture: &std::path::Path,
    remote: &std::path::Path,
    branch: &str,
) -> String {
    let publisher = fixture.join("publisher");
    run_git(
        fixture,
        &[
            "clone",
            "--branch",
            branch,
            remote.to_str().expect("remote UTF-8"),
            publisher.to_str().expect("publisher UTF-8"),
        ],
    )
    .await;
    run_git(
        &publisher,
        &["config", "user.email", "publisher@example.com"],
    )
    .await;
    run_git(&publisher, &["config", "user.name", "Publisher"]).await;
    fs::write(publisher.join("REMOTE.md"), "remote ahead").expect("remote file");
    run_git(&publisher, &["add", "REMOTE.md"]).await;
    run_git(&publisher, &["commit", "-m", "remote ahead"]).await;
    run_git(&publisher, &["push", "origin", branch]).await;
    git_output(&publisher, &["rev-parse", "HEAD"]).await
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
