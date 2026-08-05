//! Cross-process lease semantics (subprocess, not merely threads).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use buzz_worktree::{try_acquire_exclusive, try_acquire_shared};
use tempfile::TempDir;

fn root() -> String {
    "b".repeat(64)
}

fn lease_child_bin() -> PathBuf {
    // Prefer the Cargo-built sibling bin next to the test executable.
    let mut path = std::env::current_exe().expect("current test exe");
    path.pop(); // deps/
    if path.ends_with("deps") {
        path.pop();
    }
    path.push("buzz-worktree-lease-child");
    if path.exists() {
        return path;
    }
    // Fallback: target/{debug,release}/buzz-worktree-lease-child from CARGO_MANIFEST_DIR.
    let mut fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fallback.pop(); // crates
    fallback.pop(); // repo root
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    fallback.push("target");
    fallback.push(profile);
    fallback.push("buzz-worktree-lease-child");
    fallback
}

fn run_child(common: &Path, mode: &str) -> i32 {
    let bin = lease_child_bin();
    assert!(
        bin.exists(),
        "missing lease child binary at {} — build buzz-worktree first",
        bin.display()
    );
    let status = Command::new(bin)
        .env("BUZZ_WORKTREE_LEASE_CHILD", mode)
        .env(
            "BUZZ_WORKTREE_LEASE_COMMON",
            common.to_string_lossy().as_ref(),
        )
        .env("BUZZ_WORKTREE_LEASE_ROOT", root())
        .status()
        .expect("spawn lease child");
    status.code().unwrap_or(3)
}

#[test]
fn subprocess_exclusive_refuses_while_parent_holds_shared() {
    let temp = TempDir::new().unwrap();
    let common = temp.path().to_path_buf();
    let shared = try_acquire_shared(&common, &root()).expect("parent shared");

    assert_eq!(
        run_child(&common, "exclusive"),
        2,
        "child must observe Busy while parent holds shared"
    );

    drop(shared);
    assert_eq!(
        run_child(&common, "exclusive"),
        0,
        "child must acquire exclusive after parent releases"
    );
}

#[test]
fn subprocess_shared_refuses_while_parent_holds_exclusive() {
    let temp = TempDir::new().unwrap();
    let common = temp.path().to_path_buf();
    let exclusive = try_acquire_exclusive(&common, &root()).expect("parent exclusive");

    assert_eq!(
        run_child(&common, "shared"),
        2,
        "child must observe Busy while parent holds exclusive"
    );

    drop(exclusive);
    assert_eq!(
        run_child(&common, "shared"),
        0,
        "child must acquire shared after parent releases"
    );
}

#[test]
fn subprocess_holder_exit_releases_for_peer() {
    let temp = TempDir::new().unwrap();
    let common = temp.path().to_path_buf();
    // Seed the lease directory/file in-process so the child only contends on
    // the advisory lock, not on first-time path creation.
    {
        let seed = try_acquire_shared(&common, &root()).expect("seed shared");
        drop(seed);
    }
    let bin = lease_child_bin();
    let mut child = Command::new(&bin)
        .env("BUZZ_WORKTREE_LEASE_CHILD", "shared")
        .env(
            "BUZZ_WORKTREE_LEASE_COMMON",
            common.to_string_lossy().as_ref(),
        )
        .env("BUZZ_WORKTREE_LEASE_ROOT", root())
        .env("BUZZ_WORKTREE_LEASE_HOLD_MS", "500")
        .spawn()
        .expect("spawn holding child");

    // Wait until the child holds the shared lease (exclusive must be Busy).
    let mut saw_busy = false;
    for _ in 0..40 {
        thread::sleep(Duration::from_millis(25));
        if run_child(&common, "exclusive") == 2 {
            saw_busy = true;
            break;
        }
    }
    assert!(saw_busy, "child never held a shared lease");

    let status = child.wait().expect("wait holder");
    assert!(status.success());
    assert_eq!(run_child(&common, "exclusive"), 0);
}
