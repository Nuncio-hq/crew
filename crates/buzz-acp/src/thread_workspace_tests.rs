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
async fn concurrent_ensure_calls_converge_on_one_worktree() {
    let fixture = std::env::temp_dir().join(format!("buzz-worktree-test-{}", Uuid::new_v4()));
    let repo = fixture.join("project");
    fs::create_dir_all(&repo).expect("fixture directory");
    run_git(&repo, &["init", "-b", "main"]).await;
    run_git(&repo, &["config", "user.email", "test@example.com"]).await;
    run_git(&repo, &["config", "user.name", "Test"]).await;
    fs::write(repo.join("README.md"), "fixture").expect("fixture file");
    run_git(&repo, &["add", "README.md"]).await;
    run_git(&repo, &["commit", "-m", "fixture"]).await;

    let workspace = ProjectWorkspace {
        repo_address: "fixture/project".to_string(),
        local_path: repo,
    };
    let root = "a".repeat(64);
    let (first, second) = tokio::join!(
        ensure_thread_worktree(&workspace, &root),
        ensure_thread_worktree(&workspace, &root)
    );
    let first = first.expect("first ensure succeeds");
    let second = second.expect("second ensure converges");
    assert_eq!(first, second);
    assert!(first.join("README.md").is_file());

    fs::remove_dir_all(&fixture).expect("fixture cleanup");
}

async fn run_git(cwd: &std::path::Path, args: &[&str]) {
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
}
