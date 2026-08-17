use std::{fs, path::PathBuf};

use tokio::process::Command;
use uuid::Uuid;

use crate::thread_workspace::BaseSource;
use crate::thread_workspace::{
    ensure_planned_thread_worktree, ensure_thread_worktree, parse_project_workspace,
    plan_thread_worktree, EnsureKind, ProjectWorkspace,
};

#[test]
fn parses_encoded_project_workspace_context() {
    let content = "[ctx]: <buzz://project-workspace?repo=github.com%2Facme%2Fapp&path=%2Ftmp%2Fapp> \"Project\"\n\nFix it";
    let workspace = parse_project_workspace(content)
        .expect("valid context")
        .expect("context present");
    assert_eq!(workspace.repo_address, "github.com/acme/app");
    assert_eq!(workspace.local_path, PathBuf::from("/tmp/app"));
    assert_eq!(workspace.binding, Default::default());
}

#[test]
fn absent_ws_and_base_are_todays_isolated_worktree() {
    let content = "buzz://project-workspace?repo=acme%2Fapp&path=%2Ftmp%2Fapp";
    let workspace = parse_project_workspace(content).unwrap().unwrap();
    assert!(workspace.binding.is_default_new_worktree());
}

#[test]
fn parses_ws_main_and_branch_and_base() {
    let main = parse_project_workspace(
        "buzz://project-workspace?repo=acme%2Fapp&path=%2Ftmp%2Fapp&ws=main",
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        main.binding,
        crate::thread_workspace::WorkspaceBindingSpec::Main
    );

    let branch = parse_project_workspace(
        "buzz://project-workspace?repo=acme%2Fapp&path=%2Ftmp%2Fapp&ws=branch:feature%2Fx",
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        branch.binding,
        crate::thread_workspace::WorkspaceBindingSpec::ExistingBranch {
            name: "feature/x".into()
        }
    );

    let base = parse_project_workspace(
        "buzz://project-workspace?repo=acme%2Fapp&path=%2Ftmp%2Fapp&base=release",
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        base.binding,
        crate::thread_workspace::WorkspaceBindingSpec::NewWorktree {
            base: Some("release".into())
        }
    );
}

#[test]
fn malformed_branch_param_is_a_named_error() {
    assert!(parse_project_workspace(
        "buzz://project-workspace?repo=acme%2Fapp&path=%2Ftmp%2Fapp&ws=branch:"
    )
    .is_err());
}

#[test]
fn unknown_ws_fails_closed_to_default() {
    let workspace = parse_project_workspace(
        "buzz://project-workspace?repo=acme%2Fapp&path=%2Ftmp%2Fapp&ws=cowork",
    )
    .unwrap()
    .unwrap();
    assert!(workspace.binding.is_default_new_worktree());
    assert_eq!(workspace.mode, crate::thread_workspace::WorkspaceMode::Git);
}

#[test]
fn mode_folder_selects_cowork_without_worktree_binding() {
    let workspace = parse_project_workspace(
        "buzz://project-workspace?repo=acme%2Fapp&path=%2Ftmp%2Fapp&mode=folder",
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        workspace.mode,
        crate::thread_workspace::WorkspaceMode::Folder
    );
    assert!(workspace.binding.is_default_new_worktree());
}

#[test]
fn rejects_relative_workspace_path() {
    let content = "buzz://project-workspace?repo=acme%2Fapp&path=relative";
    assert!(parse_project_workspace(content).is_err());
}

#[tokio::test]
async fn missing_git_workspace_folder_is_a_typed_recover_error() {
    let missing = PathBuf::from("/definitely-missing-nuncio-217/crew");
    let workspace = ProjectWorkspace {
        repo_address: "30617:ab:crew".into(),
        local_path: missing.clone(),
        binding: Default::default(),
        mode: crate::thread_workspace::WorkspaceMode::Git,
    };
    let error = plan_thread_worktree(&workspace, &"a".repeat(64))
        .await
        .expect_err("a deleted Project folder must fail planning");
    let missing_folder = error
        .downcast_ref::<crate::thread_workspace::ThreadWorkspaceMissing>()
        .expect("missing folder must be a typed recover error, not opaque anyhow");
    assert_eq!(missing_folder.path, missing);
    let message = error.to_string();
    assert!(
        message.contains("Project workspace does not exist"),
        "log copy must keep the existing missing-folder prefix: {message}"
    );
    assert!(
        message.contains("Pick a workspace again"),
        "recover copy must tell the owner they can pick a folder again: {message}"
    );
}

#[tokio::test]
async fn missing_folder_workspace_is_a_typed_recover_error() {
    let missing = PathBuf::from("/definitely-missing-nuncio-217/docs");
    let workspace = ProjectWorkspace {
        repo_address: "30617:ab:docs".into(),
        local_path: missing.clone(),
        binding: Default::default(),
        mode: crate::thread_workspace::WorkspaceMode::Folder,
    };
    let error = plan_thread_worktree(&workspace, &"b".repeat(64))
        .await
        .expect_err("a deleted Cowork folder must fail planning");
    let missing_folder = error
        .downcast_ref::<crate::thread_workspace::ThreadWorkspaceMissing>()
        .expect("missing folder must be a typed recover error, not opaque anyhow");
    assert_eq!(missing_folder.path, missing);
    assert!(
        error.to_string().contains("Pick a workspace again"),
        "recover copy must tell the owner they can pick a folder again: {error}"
    );
}

#[tokio::test]
async fn plan_folder_workspace_skips_git_and_uses_shadow_history() {
    let fixture = std::env::temp_dir().join(format!("buzz-cowork-plan-{}", Uuid::new_v4()));
    let folder = fixture.join("docs");
    let history = fixture.join("history");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("notes.txt"), "hello").unwrap();
    std::env::set_var(buzz_cowork::HISTORY_DIR_ENV, &history);
    let workspace = ProjectWorkspace {
        repo_address: "30617:ab:docs".into(),
        local_path: folder.clone(),
        binding: Default::default(),
        mode: crate::thread_workspace::WorkspaceMode::Folder,
    };
    let root = "d".repeat(64);
    let plan = plan_thread_worktree(&workspace, &root)
        .await
        .expect("folder plan");
    assert_eq!(plan.worktree_path, folder.canonicalize().unwrap());
    assert_eq!(
        plan.checkout_kind,
        crate::thread_workspace::CheckoutKind::Folder
    );
    assert!(plan.checkout_kind.skips_lifecycle_record());
    assert!(
        plan.common_git
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".git")),
        "shadow git-dir should be named *.git: {}",
        plan.common_git.display()
    );
    assert!(!folder.join(".git").exists(), "folder must stay byte-clean");
    let (ensured, kind) = ensure_planned_thread_worktree(&plan).await.unwrap();
    assert_eq!(kind, EnsureKind::AlreadyPresent);
    assert_eq!(ensured.worktree_path, plan.worktree_path);
    std::env::remove_var(buzz_cowork::HISTORY_DIR_ENV);
    let _ = fs::remove_dir_all(&fixture);
}

#[tokio::test]
async fn plan_thread_worktree_resolves_identity_without_creating_checkout() {
    let (fixture, workspace, _) = git_fixture().await;
    let root = "c".repeat(64);

    let plan = plan_thread_worktree(&workspace, &root)
        .await
        .expect("plan succeeds");

    assert_eq!(plan.root_event_id, root);
    assert_eq!(plan.branch, "buzz/cccccccccccc");
    assert!(
        !plan.worktree_path.exists(),
        "plan must not create the managed checkout"
    );
    assert!(plan.common_git.is_dir(), "common git dir must resolve");

    let (ensured, kind) = ensure_planned_thread_worktree(&plan)
        .await
        .expect("ensure under caller-held lease");
    assert_eq!(kind, EnsureKind::Created);
    assert!(ensured.worktree_path.is_dir());

    let (_, reuse_kind) = ensure_planned_thread_worktree(&plan)
        .await
        .expect("idempotent ensure");
    assert_eq!(reuse_kind, EnsureKind::AlreadyPresent);

    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn ws_main_plans_canonical_checkout_without_creating_a_worktree() {
    let (fixture, mut workspace, _) = git_fixture().await;
    workspace.binding = crate::thread_workspace::WorkspaceBindingSpec::Main;
    let root = "d".repeat(64);
    let plan = plan_thread_worktree(&workspace, &root)
        .await
        .expect("plan succeeds");
    assert_eq!(
        plan.worktree_path,
        fs::canonicalize(&workspace.local_path).expect("canonicalize fixture")
    );
    assert_eq!(
        plan.checkout_kind,
        crate::thread_workspace::CheckoutKind::MainCheckout
    );
    assert!(!plan.worktree_path.join(".buzz-worktrees").exists());

    let (ensured, kind) = ensure_planned_thread_worktree(&plan)
        .await
        .expect("main checkout ensures");
    assert_eq!(kind, EnsureKind::AlreadyPresent);
    assert_eq!(
        ensured.worktree_path,
        fs::canonicalize(&workspace.local_path).expect("canonicalize fixture")
    );
    let parent = workspace
        .local_path
        .parent()
        .unwrap()
        .join(".buzz-worktrees");
    if parent.exists() {
        let entries: Vec<_> = fs::read_dir(&parent).unwrap().collect();
        assert!(
            entries.is_empty()
                || entries.iter().all(|entry| {
                    entry.as_ref().ok().is_none_or(|value| {
                        value.path().file_name() != Some("project-dddddddddddd".as_ref())
                    })
                }),
            "ws=main must not create a managed worktree"
        );
    }
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn ws_branch_reuses_existing_worktree_and_attaches_when_absent() {
    let (fixture, mut workspace, _) = git_fixture().await;
    run_git(&workspace.local_path, &["branch", "feature-x"]).await;
    workspace.binding = crate::thread_workspace::WorkspaceBindingSpec::ExistingBranch {
        name: "feature-x".into(),
    };
    let root_a = "e".repeat(64);
    let (first, kind) = ensure_planned_thread_worktree(
        &plan_thread_worktree(&workspace, &root_a)
            .await
            .expect("plan"),
    )
    .await
    .expect("attach creates a shared worktree");
    assert_eq!(kind, EnsureKind::AttachedExisting);
    assert_eq!(first.branch, "feature-x");
    assert_ne!(first.worktree_path, workspace.local_path);

    let root_b = "f".repeat(64);
    let (second, reuse_kind) = ensure_planned_thread_worktree(
        &plan_thread_worktree(&workspace, &root_b)
            .await
            .expect("plan"),
    )
    .await
    .expect("second thread reuses the shared worktree");
    assert_eq!(reuse_kind, EnsureKind::AlreadyPresent);
    assert_eq!(second.worktree_path, first.worktree_path);
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

#[tokio::test]
async fn requested_base_is_recorded_on_the_plan() {
    let (fixture, mut workspace, remote) = remote_git_fixture("main").await;
    run_git(&workspace.local_path, &["branch", "release"]).await;
    run_git(&workspace.local_path, &["push", "-u", "origin", "release"]).await;
    workspace.binding = crate::thread_workspace::WorkspaceBindingSpec::NewWorktree {
        base: Some("release".into()),
    };
    let plan = plan_thread_worktree(&workspace, &"9".repeat(64))
        .await
        .expect("plan");
    assert_eq!(
        plan.workspace_base.requested_base.as_deref(),
        Some("release")
    );
    let _ = remote;
    fs::remove_dir_all(&fixture).expect("fixture cleanup");
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
        binding: Default::default(),
        mode: crate::thread_workspace::WorkspaceMode::Git,
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
            binding: Default::default(),
            mode: crate::thread_workspace::WorkspaceMode::Git,
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
