use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "crew-wiki-capability-{}-{}",
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
fn coordinate() -> String {
    format!("30617:{}:folder", "a".repeat(64))
}

#[test]
fn captured_file_mutation_before_validation_is_a_visible_failure() {
    let root = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "before\n").unwrap();
    let result = capture_inner(&root.0, &coordinate(), &mut |point, _| {
        if point == CapturePoint::AfterReadAll {
            std::fs::write(root.0.join("source.rs"), "after\n").unwrap();
        }
    });
    assert!(result.is_err());
}

#[test]
fn adding_a_selected_file_before_validation_invalidates_the_snapshot() {
    let root = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "before\n").unwrap();
    let result = capture_inner(&root.0, &coordinate(), &mut |point, _| {
        if point == CapturePoint::AfterReadAll {
            std::fs::write(root.0.join("added.rs"), "new\n").unwrap();
        }
    });
    assert!(result.is_err());
}

#[test]
fn leaf_symlink_swap_is_rejected_before_opening_the_target() {
    let root = Fixture::new();
    let outside = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "source\n").unwrap();
    std::fs::write(outside.0.join("secret.rs"), "outside\n").unwrap();
    let mut target_opened = false;
    let result = capture_inner(&root.0, &coordinate(), &mut |point, path| {
        if path != "source.rs" {
            return;
        }
        if point == CapturePoint::BeforeOpenFile {
            std::fs::remove_file(root.0.join("source.rs")).unwrap();
            std::os::unix::fs::symlink(outside.0.join("secret.rs"), root.0.join("source.rs"))
                .unwrap();
        }
        if point == CapturePoint::AfterOpenFile {
            target_opened = true;
        }
    });
    assert!(result.is_err());
    assert!(
        !target_opened,
        "no-follow must reject before opening an outside target"
    );
}

#[test]
fn intermediate_symlink_swap_is_rejected_before_opening_the_target() {
    let root = Fixture::new();
    let outside = Fixture::new();
    std::fs::create_dir(root.0.join("src")).unwrap();
    std::fs::write(root.0.join("src/source.rs"), "source\n").unwrap();
    std::fs::write(outside.0.join("secret.rs"), "outside\n").unwrap();
    let mut target_opened = false;
    let result = capture_inner(&root.0, &coordinate(), &mut |point, path| {
        if path != "src" {
            return;
        }
        if point == CapturePoint::BeforeOpenDirectory {
            std::fs::rename(root.0.join("src"), root.0.join("old-src")).unwrap();
            std::os::unix::fs::symlink(&outside.0, root.0.join("src")).unwrap();
        }
        if point == CapturePoint::AfterOpenDirectory {
            target_opened = true;
        }
    });
    assert!(result.is_err());
    assert!(
        !target_opened,
        "directory no-follow must reject before opening an outside target"
    );
}

// APFS refuses creation of these names; Linux exercises the filesystem case.
#[cfg(target_os = "linux")]
#[test]
fn non_utf8_entry_names_fail_instead_of_being_lossily_renamed() {
    use std::os::unix::ffi::OsStringExt;
    let root = Fixture::new();
    let name = std::ffi::OsString::from_vec(vec![255, b'.', b'r', b's']);
    std::fs::write(root.0.join(name), "source\n").unwrap();
    assert!(capture_folder(&root.0, &coordinate()).is_err());
}

#[test]
fn trusted_bound_root_alias_is_canonicalized_once() {
    let root = Fixture::new();
    let alias = Fixture::new();
    std::fs::write(root.0.join("source.rs"), "source\n").unwrap();
    std::os::unix::fs::symlink(&root.0, alias.0.join("bound")).unwrap();
    let snapshot = capture_folder(&alias.0.join("bound"), &coordinate()).unwrap();
    assert_eq!(snapshot.read("source.rs", None).unwrap(), "source\n");
}

#[test]
fn invalid_entry_bytes_are_rejected_before_path_construction() {
    assert!(crate::source_snapshot::source_path_from_bytes(&[255, b'.', b'r', b's']).is_err());
    assert_eq!(
        crate::source_snapshot::source_path_from_bytes("nguồn\nfile.rs".as_bytes()).unwrap(),
        "nguồn\nfile.rs"
    );
}
