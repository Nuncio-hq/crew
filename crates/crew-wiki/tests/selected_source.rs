//! Owned fixtures bind native-selected root access; no real user folders are read.
#![cfg(unix)]
use crew_wiki::source_access::SelectedSourceRoot;
use crew_wiki::source_snapshot::{source_hash, SourceReference};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "crew-selected-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn reference(content: &str) -> SourceReference {
    (
        "source.rs".into(),
        source_hash(content.as_bytes()),
        content.len() as u64,
        1,
        1,
    )
}
#[cfg(target_os = "linux")]
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
#[cfg(target_os = "linux")]
fn commit(root: &Path) {
    git(root, &["add", "--all"]);
    git(
        root,
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

#[test]
fn selected_folder_returns_exact_bytes_and_signed_range() {
    let fixture = Fixture::new();
    let content = "first\r\nsecond\n";
    std::fs::write(fixture.0.join("source.rs"), content).unwrap();
    let root = SelectedSourceRoot::open_native_selection(&fixture.0).unwrap();
    let mut source = reference(content);
    source.3 = 2;
    source.4 = 2;
    let file = root
        .read_verified_reference(&format!("folder:{}", "a".repeat(64)), &source, deadline())
        .expect("exact authorized bytes");
    assert_eq!(file.content, content);
    assert_eq!((file.start_line, file.end_line), (2, 2));
}

#[test]
fn changed_folder_and_invalid_range_return_unavailable() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("source.rs"), "original\n").unwrap();
    let root = SelectedSourceRoot::open_native_selection(&fixture.0).unwrap();
    let revision = format!("folder:{}", "a".repeat(64));
    let original = reference("original\n");
    std::fs::write(fixture.0.join("source.rs"), "modified\n").unwrap();
    assert!(root
        .read_verified_reference(&revision, &original, deadline())
        .is_err());
    let mut invalid = reference("modified\n");
    invalid.4 = 2;
    assert!(root
        .read_verified_reference(&revision, &invalid, deadline())
        .is_err());
}

#[test]
fn selected_root_replacement_cannot_reauthorize_new_directory() {
    let fixture = Fixture::new();
    let selected = fixture.0.join("selected");
    std::fs::create_dir(&selected).unwrap();
    std::fs::write(selected.join("source.rs"), "same bytes\n").unwrap();
    let root = SelectedSourceRoot::open_native_selection(&selected).unwrap();
    std::fs::rename(&selected, fixture.0.join("original")).unwrap();
    std::fs::create_dir(&selected).unwrap();
    std::fs::write(selected.join("source.rs"), "same bytes\n").unwrap();
    assert!(root
        .read_verified_reference(
            &format!("folder:{}", "a".repeat(64)),
            &reference("same bytes\n"),
            deadline()
        )
        .is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn git_source_uses_historical_objects_after_head_moves_and_disk_changes() {
    let fixture = Fixture::new();
    git(&fixture.0, &["init", "--quiet"]);
    git(&fixture.0, &["config", "user.name", "Fixture"]);
    git(
        &fixture.0,
        &["config", "user.email", "fixture@example.invalid"],
    );
    std::fs::write(fixture.0.join("source.rs"), "original\n").unwrap();
    commit(&fixture.0);
    let revision = format!("git:{}", git(&fixture.0, &["rev-parse", "HEAD"]));
    let root = SelectedSourceRoot::open_native_selection(&fixture.0).unwrap();
    std::fs::write(fixture.0.join("source.rs"), "future\n").unwrap();
    commit(&fixture.0);
    std::fs::remove_file(fixture.0.join("source.rs")).unwrap();
    assert_eq!(
        root.read_verified_reference(&revision, &reference("original\n"), deadline())
            .unwrap()
            .content,
        "original\n"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn git_source_is_unavailable_when_retained_fd_cwd_is_unsupported() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("source.rs"), "source\n").unwrap();
    let root = SelectedSourceRoot::open_native_selection(&fixture.0).unwrap();
    assert!(root
        .read_verified_reference(
            &format!("git:{}", "a".repeat(40)),
            &reference("source\n"),
            deadline(),
        )
        .is_err());
}

#[test]
fn expired_caller_deadline_never_starts_a_new_capture_budget() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("source.rs"), "source\n").unwrap();
    let root = SelectedSourceRoot::open_native_selection(&fixture.0).unwrap();
    assert!(root
        .read_verified_reference(
            &format!("folder:{}", "a".repeat(64)),
            &reference("source\n"),
            Instant::now() - Duration::from_secs(1)
        )
        .is_err());
}

#[test]
fn source_refuses_symlink_components_and_leaf() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let selected = fixture.0.join("selected");
    let outside = fixture.0.join("outside");
    std::fs::create_dir(&selected).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("source.rs"), "outside\n").unwrap();
    symlink(outside.join("source.rs"), selected.join("source.rs")).unwrap();
    symlink(&outside, selected.join("linked")).unwrap();
    let root = SelectedSourceRoot::open_native_selection(&selected).unwrap();
    let revision = format!("folder:{}", "a".repeat(64));
    let mut source = reference("outside\n");
    assert!(root
        .read_verified_reference(&revision, &source, deadline())
        .is_err());
    source.0 = "linked/source.rs".into();
    assert!(root
        .read_verified_reference(&revision, &source, deadline())
        .is_err());
}

#[test]
fn selected_source_checks_complete_hash_size_and_locator() {
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("source.rs"), "source\n").unwrap();
    let root = SelectedSourceRoot::open_native_selection(&fixture.0).unwrap();
    let revision = format!("folder:{}", "a".repeat(64));
    for source in [
        ("source.rs".into(), "a".repeat(64), 7, 1, 1),
        ("source.rs".into(), source_hash(b"source\n"), 6, 1, 1),
        ("../source.rs".into(), source_hash(b"source\n"), 7, 1, 1),
        ("/source.rs".into(), source_hash(b"source\n"), 7, 1, 1),
    ] {
        assert!(root
            .read_verified_reference(&revision, &source, deadline())
            .is_err());
    }
}
