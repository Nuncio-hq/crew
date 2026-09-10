//! Source revision regressions bind the production snapshot reader.

#![cfg(unix)]

use crew_wiki::git_snapshot::RepoSnapshot;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "crew-wiki-pinned-source-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).expect("create unique owned Git fixture");
        git(&path, &["init", "--quiet"]);
        git(&path, &["config", "user.name", "Wiki source test"]);
        git(
            &path,
            &["config", "user.email", "wiki-test@example.invalid"],
        );
        Self(path)
    }

    fn commit(&self) {
        git(&self.0, &["add", "--all"]);
        git(
            &self.0,
            &[
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-s",
                "--quiet",
                "-m",
                "fixture",
            ],
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove only owned fixture");
    }
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .expect("run fixture Git");
    assert!(output.status.success(), "fixture Git failed: {output:?}");
}

#[test]
fn pinned_snapshot_never_reads_dirty_worktree_bytes() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("source.rs"), "committed source\n").unwrap();
    fixture.commit();
    let snapshot = RepoSnapshot::from_git(&fixture.0).expect("capture committed revision");
    std::fs::write(fixture.0.join("source.rs"), "dirty future source\n").unwrap();
    assert_eq!(
        snapshot
            .read("source.rs", Some(&fixture.0))
            .expect("captured source"),
        "committed source\n"
    );
}

#[test]
fn pinned_snapshot_preserves_unicode_and_newline_paths() {
    let fixture = Fixture::new();
    let path = "nguồn\nfile.rs";
    std::fs::write(fixture.0.join(path), "exact content\n").unwrap();
    fixture.commit();
    let snapshot = RepoSnapshot::from_git(&fixture.0).expect("capture unusual valid UTF-8 path");
    assert_eq!(snapshot.files, vec![path.to_owned()]);
    assert_eq!(
        snapshot
            .read(path, Some(&fixture.0))
            .expect("captured source"),
        "exact content\n"
    );
}

#[test]
fn pinned_snapshot_retains_full_source_past_old_truncation_limit() {
    let fixture = Fixture::new();
    let content = format!("{}\nlast source line\n", "x".repeat(8_100));
    std::fs::write(fixture.0.join("source.rs"), &content).unwrap();
    fixture.commit();
    let snapshot = RepoSnapshot::from_git(&fixture.0).expect("capture full source");
    assert_eq!(
        snapshot
            .read("source.rs", Some(&fixture.0))
            .expect("captured source"),
        content
    );
}

#[test]
fn pinned_snapshot_remains_exact_after_head_moves_and_file_is_removed() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("source.rs"), "first\n").unwrap();
    fixture.commit();
    let snapshot = RepoSnapshot::from_git(&fixture.0).unwrap();
    let revision = snapshot.source_revision.clone();
    git(&fixture.0, &["checkout", "--quiet", "-b", "later"]);
    std::fs::remove_file(fixture.0.join("source.rs")).unwrap();
    std::fs::write(fixture.0.join("later.rs"), "later\n").unwrap();
    fixture.commit();
    assert_eq!(
        snapshot.read("source.rs", Some(&fixture.0)).unwrap(),
        "first\n"
    );
    assert_eq!(snapshot.source_revision, revision);
    assert!(snapshot.read("later.rs", Some(&fixture.0)).is_err());
}

#[test]
fn pinned_snapshot_detached_head_and_explicit_remote_default_never_guess() {
    use crew_wiki::source_snapshot::{capture_git, GitSourceChoice};
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("source.rs"), "source\n").unwrap();
    fixture.commit();
    git(&fixture.0, &["checkout", "--quiet", "--detach"]);
    let detached = RepoSnapshot::from_git(&fixture.0).unwrap();
    assert!(detached.branch.is_empty());
    assert_eq!(detached.source_revision, format!("git:{}", detached.commit));
    assert!(capture_git(&fixture.0, GitSourceChoice::RemoteDefault("origin".into())).is_err());
    git(
        &fixture.0,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/source.git",
        ],
    );
    git(
        &fixture.0,
        &["update-ref", "refs/remotes/origin/trunk", "HEAD"],
    );
    git(
        &fixture.0,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
        ],
    );
    let remote = capture_git(&fixture.0, GitSourceChoice::RemoteDefault("origin".into())).unwrap();
    assert_eq!(remote.branch, "trunk");
    assert_eq!(remote.commit, detached.commit);
}

#[test]
fn pinned_snapshot_uses_committed_steering_and_reports_coverage() {
    use crew_wiki::source_snapshot::SourceOmissionReason;
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.0.join(".crew")).unwrap();
    let steering = r#"{"language":"vi","pages":[{"slug":"build","source_files":["Makefile"]}]}"#;
    std::fs::write(fixture.0.join(".crew/wiki.json"), steering).unwrap();
    std::fs::write(fixture.0.join("Makefile"), "all:\n\ttrue\n").unwrap();
    std::fs::write(fixture.0.join(".env"), "PRIVATE_TEST_VALUE=fixture\n").unwrap();
    fixture.commit();
    std::fs::write(fixture.0.join(".crew/wiki.json"), "dirty invalid JSON").unwrap();
    let snapshot = RepoSnapshot::from_git(&fixture.0).unwrap();
    assert!(snapshot.contents.contains_key("Makefile"));
    assert!(!snapshot.contents.contains_key(".env"));
    assert!(snapshot
        .omissions
        .iter()
        .any(|item| item.path == ".env" && item.reason == SourceOmissionReason::Excluded));
    assert_eq!(
        crew_wiki::steering::load_captured_steering(&snapshot)
            .unwrap()
            .unwrap()
            .language
            .as_deref(),
        Some("vi")
    );
}

#[test]
fn pinned_source_ranges_hash_complete_bytes_and_do_not_invent_empty_lines() {
    use crew_wiki::source_snapshot::{source_hash, source_reference};
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("source.rs"), "one\r\ntwo\n").unwrap();
    std::fs::write(fixture.0.join("empty.rs"), "").unwrap();
    fixture.commit();
    let snapshot = RepoSnapshot::from_git(&fixture.0).unwrap();
    let reference = source_reference(&snapshot, "source.rs", 1, 2).unwrap();
    assert_eq!(reference.1, source_hash(b"one\r\ntwo\n"));
    assert_eq!(reference.2, 9);
    assert!(source_reference(&snapshot, "source.rs", 1, 3).is_err());
    assert!(source_reference(&snapshot, "empty.rs", 1, 1).is_err());
    assert!(source_reference(&snapshot, "missing.rs", 1, 1).is_err());
}

#[cfg(unix)]
#[test]
fn pinned_snapshot_never_follows_symlinks_and_omits_binary_or_oversized_files() {
    use crew_wiki::source_snapshot::{SourceOmissionReason, MAX_SOURCE_FILE_BYTES};
    let fixture = Fixture::new();
    std::os::unix::fs::symlink("/etc/passwd", fixture.0.join("link.rs")).unwrap();
    std::fs::write(fixture.0.join("binary.rs"), [0, 1, 2]).unwrap();
    std::fs::write(
        fixture.0.join("large.rs"),
        vec![b'x'; MAX_SOURCE_FILE_BYTES + 1],
    )
    .unwrap();
    fixture.commit();
    let snapshot = RepoSnapshot::from_git(&fixture.0).unwrap();
    for (path, reason) in [
        ("link.rs", SourceOmissionReason::Symlink),
        ("binary.rs", SourceOmissionReason::Binary),
        ("large.rs", SourceOmissionReason::Oversized),
    ] {
        assert!(snapshot.read(path, Some(&fixture.0)).is_err());
        assert!(snapshot
            .omissions
            .iter()
            .any(|item| item.path == path && item.reason == reason));
    }
}

fn git_value(root: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn corrupt_blob_bytes_cannot_be_labeled_with_the_original_git_revision() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("source.rs"), "first\n").unwrap();
    fixture.commit();
    let original = git_value(&fixture.0, &["rev-parse", "HEAD:source.rs"]);
    std::fs::write(fixture.0.join("other.rs"), "other\n").unwrap();
    let replacement = git_value(&fixture.0, &["hash-object", "-w", "other.rs"]);
    let object = |oid: &str| {
        fixture
            .0
            .join(".git/objects")
            .join(&oid[..2])
            .join(&oid[2..])
    };
    std::fs::remove_file(object(&original)).unwrap();
    std::fs::copy(object(&replacement), object(&original)).unwrap();
    assert!(RepoSnapshot::from_git(&fixture.0).is_err());
}

#[test]
fn missing_git_blob_never_falls_back_to_an_existing_worktree_file() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("source.rs"), "source\n").unwrap();
    fixture.commit();
    let oid = git_value(&fixture.0, &["rev-parse", "HEAD:source.rs"]);
    std::fs::remove_file(
        fixture
            .0
            .join(".git/objects")
            .join(&oid[..2])
            .join(&oid[2..]),
    )
    .unwrap();
    assert!(fixture.0.join("source.rs").exists());
    assert!(RepoSnapshot::from_git(&fixture.0).is_err());
}

#[test]
fn omitted_source_is_not_classified_as_an_empty_repository() {
    for (path, bytes) in [
        (".env", b"fixture=omitted\n".to_vec()),
        ("binary.rs", vec![0xff]),
        ("oversized.rs", vec![b'x'; 1024 * 1024 + 1]),
    ] {
        let fixture = Fixture::new();
        std::fs::write(fixture.0.join(path), bytes).unwrap();
        fixture.commit();
        let snapshot = RepoSnapshot::from_git(&fixture.0).unwrap();
        assert!(snapshot.files.is_empty());
        assert!(!snapshot.omissions.is_empty());
        assert!(
            !snapshot.is_empty_tree(),
            "worker must report unavailable coverage for {path}"
        );
    }
    let empty = Fixture::new();
    git(
        &empty.0,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-s",
            "--allow-empty",
            "--quiet",
            "-m",
            "empty fixture",
        ],
    );
    assert!(
        RepoSnapshot::from_git(&empty.0).unwrap().is_empty_tree(),
        "true empty remains distinct"
    );
}

fn assert_swapped_object_rejected(revision: &str) {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.0.join("sub")).unwrap();
    std::fs::write(fixture.0.join("sub/source.rs"), "first\n").unwrap();
    fixture.commit();
    let original = git_value(&fixture.0, &["rev-parse", revision]);
    let original_head = git_value(&fixture.0, &["rev-parse", "HEAD"]);
    std::fs::write(fixture.0.join("sub/source.rs"), "other\n").unwrap();
    fixture.commit();
    let replacement = git_value(&fixture.0, &["rev-parse", revision]);
    git(&fixture.0, &["update-ref", "HEAD", &original_head]);
    let object = |oid: &str| {
        fixture
            .0
            .join(".git/objects")
            .join(&oid[..2])
            .join(&oid[2..])
    };
    std::fs::remove_file(object(&original)).unwrap();
    std::fs::copy(object(&replacement), object(&original)).unwrap();
    assert!(
        RepoSnapshot::from_git(&fixture.0).is_err(),
        "swapped {revision}"
    );
}

#[test]
fn swapped_commit_is_rejected_by_actual_capture() {
    assert_swapped_object_rejected("HEAD");
}
#[test]
fn swapped_root_tree_is_rejected_by_actual_capture() {
    assert_swapped_object_rejected("HEAD^{tree}");
}
#[test]
fn swapped_nested_tree_is_rejected_by_actual_capture() {
    assert_swapped_object_rejected("HEAD:sub");
}

#[test]
fn malformed_and_duplicate_tree_entries_are_rejected_by_actual_capture() {
    for duplicate in [false, true] {
        let fixture = Fixture::new();
        std::fs::write(fixture.0.join("source.rs"), "source\n").unwrap();
        fixture.commit();
        let blob = git_value(&fixture.0, &["rev-parse", "HEAD:source.rs"]);
        let mut tree = if duplicate {
            b"100644 source.rs\0".to_vec()
        } else {
            b"999999 source.rs\0".to_vec()
        };
        tree.extend(
            (0..blob.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&blob[i..i + 2], 16).unwrap()),
        );
        if duplicate {
            tree.extend_from_within(..);
        }
        std::fs::write(fixture.0.join("raw-object"), tree).unwrap();
        let tree_id = git_value(
            &fixture.0,
            &[
                "hash-object",
                "--literally",
                "-t",
                "tree",
                "-w",
                "raw-object",
            ],
        );
        let commit = git_value(&fixture.0, &["cat-file", "commit", "HEAD"]);
        let (_, remaining) = commit.split_once('\n').unwrap();
        std::fs::write(
            fixture.0.join("raw-object"),
            format!("tree {tree_id}\n{remaining}\n"),
        )
        .unwrap();
        let commit_id = git_value(
            &fixture.0,
            &[
                "hash-object",
                "--literally",
                "-t",
                "commit",
                "-w",
                "raw-object",
            ],
        );
        git(&fixture.0, &["update-ref", "HEAD", &commit_id]);
        assert!(
            RepoSnapshot::from_git(&fixture.0).is_err(),
            "duplicate={duplicate}"
        );
    }
}
