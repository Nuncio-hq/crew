//! The read-allow-list fence: no read root may cover the directory that holds
//! every run root.
//!
//! These exercise the predicate directly rather than through the built policy
//! text, so they assert the same fact on every platform — the non-macOS branch
//! has no policy to build but applies the same predicate.

use super::read_roots_clear_of_run_roots;
use super::PrivateAskFailure;
use std::path::{Path, PathBuf};

/// A temporary tree canonicalized once, so `/var` vs `/private/var` on macOS
/// cannot make an ancestor look unrelated.
fn tree() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().canonicalize().expect("canonical");
    (directory, root)
}

/// `run_root` is `<run roots>/<attempt>`; the fence protects its parent.
fn run_root(root: &Path, runs: &str) -> PathBuf {
    let runs = root.join(runs);
    std::fs::create_dir_all(&runs).expect("run roots");
    let run = runs.join("attempt");
    std::fs::create_dir_all(&run).expect("run root");
    run
}

#[test]
fn a_runtime_installed_above_the_run_roots_is_refused() {
    let (_directory, root) = tree();
    let run = run_root(&root, "agents/recap-runs");

    // The runtime's own directory is the tree that contains every run root: its
    // read allowance would be a read allowance for every other attempt.
    assert_eq!(
        read_roots_clear_of_run_roots(&run, &root, &[]).unwrap_err(),
        PrivateAskFailure::ProcessContainmentUnverified
    );
}

#[test]
fn a_runtime_directory_equal_to_the_run_roots_is_refused() {
    let (_directory, root) = tree();
    let run = run_root(&root, "recap-runs");
    let run_roots = run.parent().expect("run roots").to_path_buf();

    // Equality, not only a strict ancestor: the same disclosure.
    assert_eq!(
        read_roots_clear_of_run_roots(&run, &run_roots, &[]).unwrap_err(),
        PrivateAskFailure::ProcessContainmentUnverified
    );
}

#[test]
fn a_sibling_whose_name_shares_a_prefix_is_not_an_ancestor() {
    let (_directory, root) = tree();
    let run = run_root(&root, "recap-runs");
    let sibling = root.join("recap-runs-installed");
    std::fs::create_dir(&sibling).expect("sibling");

    // A string prefix comparison would refuse this one. Component-wise does not.
    read_roots_clear_of_run_roots(&run, &sibling, &[])
        .expect("a sibling installation is not an ancestor");
}

#[test]
fn a_runtime_inside_the_run_roots_is_not_refused_by_this_fence() {
    let (_directory, root) = tree();
    let run = run_root(&root, "recap-runs");
    let inside = run.join("inside");
    std::fs::create_dir(&inside).expect("inside");

    // Deliberately asymmetric: only the ancestor direction discloses other
    // attempts, and the probe's own fixtures sit this way.
    read_roots_clear_of_run_roots(&run, &inside, &[])
        .expect("a directory inside the run roots discloses no sibling attempt");
}

#[test]
fn an_interpreter_read_root_above_the_run_roots_is_refused() {
    let (_directory, root) = tree();
    let run = run_root(&root, "agents/recap-runs");
    let installation = root.join("install");
    std::fs::create_dir(&installation).expect("install");

    // The runtime itself is placed correctly; the interpreter prefix is not.
    // Checking only the runtime directory would let this one through.
    assert_eq!(
        read_roots_clear_of_run_roots(&run, &installation, std::slice::from_ref(&root))
            .unwrap_err(),
        PrivateAskFailure::ProcessContainmentUnverified
    );
    read_roots_clear_of_run_roots(&run, &installation, std::slice::from_ref(&installation))
        .expect("an interpreter prefix beside the run roots is allowed");
}

#[test]
fn a_read_root_that_cannot_be_resolved_is_refused() {
    let (_directory, root) = tree();
    let run = run_root(&root, "recap-runs");

    // Refused rather than assumed unrelated: an unresolvable path is not
    // evidence that it sits outside the run roots.
    assert_eq!(
        read_roots_clear_of_run_roots(&run, &root.join("absent"), &[]).unwrap_err(),
        PrivateAskFailure::ProcessContainmentUnverified
    );
}

#[test]
fn a_symlinked_read_root_cannot_hide_an_ancestor() {
    let (_directory, root) = tree();
    let run = run_root(&root, "agents/recap-runs");
    let disguise = root.join("install-link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&root, &disguise).expect("symlink");

    // Canonicalized on the candidate side too, so a link pointing above the run
    // roots is the same disclosure by another name.
    #[cfg(unix)]
    assert_eq!(
        read_roots_clear_of_run_roots(&run, &disguise, &[]).unwrap_err(),
        PrivateAskFailure::ProcessContainmentUnverified
    );
}

#[test]
fn a_run_root_with_no_parent_is_refused() {
    // `/` is not a run root this recipe produced.
    assert_eq!(
        read_roots_clear_of_run_roots(Path::new("/"), Path::new("/usr"), &[]).unwrap_err(),
        PrivateAskFailure::ProcessContainmentUnverified
    );
}
