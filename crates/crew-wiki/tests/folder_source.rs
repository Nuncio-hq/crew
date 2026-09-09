//! Real-folder fixtures exercise the production source capture boundary.

#![cfg(unix)]

use crew_wiki::source_folder::capture_folder;
use crew_wiki::source_snapshot::{source_hash, SourceOmissionReason};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Folder(PathBuf);
impl Folder {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "crew-wiki-folder-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Folder {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn coordinate() -> String {
    format!("30617:{}:folder-demo", "a".repeat(64))
}

#[test]
fn folder_capture_has_exact_content_identity_without_git_or_branch() {
    let folder = Folder::new();
    std::fs::write(folder.0.join("source.rs"), "source\n").unwrap();
    let captured = capture_folder(&folder.0, &coordinate()).unwrap();
    let manifest = serde_json::json!([
        1,
        coordinate(),
        [["source.rs", source_hash(b"source\n"), 7]],
        []
    ]);
    let expected = format!(
        "folder:{}",
        source_hash(&serde_json::to_vec(&manifest).unwrap())
    );
    assert_eq!(captured.source_revision, expected);
    assert_eq!(captured.commit, expected);
    assert!(captured.branch.is_empty());
    assert_eq!(captured.read("source.rs", None).unwrap(), "source\n");
    assert!(!folder.0.join(".git").exists());
}

#[test]
fn folder_revisions_change_with_bytes_while_prior_capture_stays_exact() {
    let folder = Folder::new();
    std::fs::write(folder.0.join("source.rs"), "first\n").unwrap();
    let before = capture_folder(&folder.0, &coordinate()).unwrap();
    std::fs::write(folder.0.join("source.rs"), "second\n").unwrap();
    let after = capture_folder(&folder.0, &coordinate()).unwrap();
    assert_ne!(before.source_revision, after.source_revision);
    assert_eq!(before.read("source.rs", None).unwrap(), "first\n");
}

#[cfg(unix)]
#[test]
fn folder_capture_never_follows_leaf_or_intermediate_symlinks() {
    let folder = Folder::new();
    std::fs::write(folder.0.join("source.rs"), "source\n").unwrap();
    std::os::unix::fs::symlink("/etc/passwd", folder.0.join("secret.rs")).unwrap();
    std::os::unix::fs::symlink("/etc", folder.0.join("linked")).unwrap();
    let captured = capture_folder(&folder.0, &coordinate()).unwrap();
    assert_eq!(captured.files, vec!["source.rs"]);
    for name in ["secret.rs", "linked"] {
        assert!(captured
            .omissions
            .iter()
            .any(|o| o.path == name && o.reason == SourceOmissionReason::Symlink));
    }
}

#[test]
fn missing_bound_folder_is_unavailable() {
    let folder = Folder::new();
    assert!(capture_folder(&folder.0.join("missing"), &coordinate()).is_err());
}
