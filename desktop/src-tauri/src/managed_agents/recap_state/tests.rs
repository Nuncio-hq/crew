#![cfg(unix)]
use super::*;
use std::io::Read;
use std::os::unix::fs::{symlink, PermissionsExt};

fn canonical_tempdir() -> tempfile::TempDir {
    // macOS /var is an alias for /private/var; production accepts canonical bases only.
    tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

#[test]
fn owned_run_has_private_durable_manifest_and_unlinked_input() {
    let base = canonical_tempdir();
    let run = OwnedRecapRun::create(base.path(), 100).unwrap();
    assert_eq!(
        std::fs::metadata(run.path()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(
        run.path().join(MANIFEST).is_file(),
        "a private directory alone is not durable ownership"
    );
    let mut input = run.input(b"synthetic source").unwrap();
    assert!(
        !run.path().join("input").exists(),
        "unlink stdin after opening so later path writes cannot grow it"
    );
    let mut text = String::new();
    input.read_to_string(&mut text).unwrap();
    assert_eq!(text, "synthetic source");
}

#[test]
fn oversized_input_is_rejected_before_file_creation() {
    let base = canonical_tempdir();
    let run = OwnedRecapRun::create(base.path(), 100).unwrap();
    assert_eq!(
        run.input(&vec![b'x'; RECAP_INPUT_LIMIT + 1]).unwrap_err(),
        RecapStateFailure::InputLimit
    );
    assert!(!run.path().join("input").exists());
}

#[test]
fn cleanup_refuses_pending_process_and_preserves_its_retry_record() {
    let base = canonical_tempdir();
    let mut run = OwnedRecapRun::create(base.path(), 100).unwrap();
    let path = run.path().to_owned();
    run.mark_process_pending().unwrap();
    assert_eq!(run.cleanup(), Err(RecapStateFailure::ProcessPending));
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(path.join(MANIFEST)).unwrap()).unwrap();
    assert_eq!(manifest.phase, Phase::ProcessMayBeRunning);
}

#[test]
fn cleanup_removes_only_the_finished_owned_generation() {
    let base = canonical_tempdir();
    let untouched = base.path().join("employee-sentinel");
    std::fs::write(&untouched, b"unchanged").unwrap();
    let mut run = OwnedRecapRun::create(base.path(), 100).unwrap();
    let path = run.path().to_owned();
    run.mark_process_pending().unwrap();
    run.mark_finished().unwrap();
    run.cleanup().unwrap();
    assert!(!path.exists());
    assert_eq!(std::fs::read(untouched).unwrap(), b"unchanged");
}

#[test]
fn recovery_removes_expired_owned_roots_but_not_unknown_or_running_roots() {
    let base = canonical_tempdir();
    let expired = OwnedRecapRun::create(base.path(), 100).unwrap();
    let mut pending = OwnedRecapRun::create(base.path(), 100).unwrap();
    pending.mark_process_pending().unwrap();
    let fresh = OwnedRecapRun::create(base.path(), 100 + RECOVERY_AGE_SECONDS).unwrap();
    let unknown = base.path().join("recap-runs/unknown");
    std::fs::create_dir(&unknown).unwrap();
    let report = recover_recap_runs(base.path(), 100 + RECOVERY_AGE_SECONDS).unwrap();
    assert_eq!(report.removed, 1);
    assert_eq!(report.pending_process, 1);
    assert!(!expired.path().exists());
    assert!(pending.path().exists());
    assert!(fresh.path().exists());
    assert!(unknown.exists());
}

#[test]
fn symlink_replacement_is_never_followed_or_removed_as_owned_state() {
    let base = canonical_tempdir();
    let outside = canonical_tempdir();
    std::fs::write(outside.path().join("sentinel"), b"keep").unwrap();
    let run = OwnedRecapRun::create(base.path(), 100).unwrap();
    let original = run.path().to_owned();
    std::fs::rename(&original, base.path().join("saved-owned-root")).unwrap();
    symlink(outside.path(), &original).unwrap();
    assert_eq!(run.cleanup(), Err(RecapStateFailure::Ownership));
    assert!(std::fs::symlink_metadata(&original)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read(outside.path().join("sentinel")).unwrap(),
        b"keep"
    );
}

#[test]
fn shared_or_symlinked_parent_is_not_a_private_run_namespace() {
    for symlink_parent in [false, true] {
        let base = canonical_tempdir();
        let outside = canonical_tempdir();
        let parent = base.path().join("recap-runs");
        if symlink_parent {
            symlink(outside.path(), &parent).unwrap();
        } else {
            std::fs::create_dir(&parent).unwrap();
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(matches!(
            OwnedRecapRun::create(base.path(), 100),
            Err(RecapStateFailure::Ownership)
        ));
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}

#[test]
fn recovery_preserves_forged_generation_and_symlinked_manifest() {
    let base = canonical_tempdir();
    let known = OwnedRecapRun::create(base.path(), 100).unwrap();
    let contents = serde_json::to_vec(&known.manifest).unwrap();
    let other = base
        .path()
        .join("recap-runs")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir(&other).unwrap();
    std::fs::set_permissions(&other, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(other.join(MANIFEST), &contents).unwrap();
    let linked = base
        .path()
        .join("recap-runs")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir(&linked).unwrap();
    std::fs::set_permissions(&linked, std::fs::Permissions::from_mode(0o700)).unwrap();
    symlink(known.path().join(MANIFEST), linked.join(MANIFEST)).unwrap();
    let report = recover_recap_runs(base.path(), 100 + RECOVERY_AGE_SECONDS).unwrap();
    assert!(
        other.exists(),
        "copied manifest does not own another directory"
    );
    assert!(
        linked.exists(),
        "manifest symlinks are not ownership records"
    );
    assert!(report.preserved_unknown > 0);
}

#[test]
fn input_rejects_replaced_manifest_and_does_not_follow_an_existing_link() {
    let base = canonical_tempdir();
    let run = OwnedRecapRun::create(base.path(), 100).unwrap();
    let outside = base.path().join("sentinel");
    std::fs::write(&outside, b"keep").unwrap();
    symlink(&outside, run.path().join("input")).unwrap();
    assert!(run.input(b"overwrite").is_err());
    assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
    std::fs::remove_file(run.path().join("input")).unwrap();
    std::fs::remove_file(run.path().join(MANIFEST)).unwrap();
    symlink(&outside, run.path().join(MANIFEST)).unwrap();
    assert!(matches!(
        run.input(b"text"),
        Err(RecapStateFailure::Ownership)
    ));
}

#[test]
fn persisted_phase_mismatch_fences_stale_owner() {
    let base = canonical_tempdir();
    let mut run = OwnedRecapRun::create(base.path(), 100).unwrap();
    let mut changed = run.manifest.clone();
    changed.phase = Phase::ProcessMayBeRunning;
    std::fs::write(
        run.path().join(MANIFEST),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    assert_eq!(run.mark_finished(), Err(RecapStateFailure::Ownership));
    assert_eq!(run.cleanup(), Err(RecapStateFailure::Ownership));
}

#[test]
fn aliased_base_is_rejected_before_creating_any_outside_state() {
    let staging = canonical_tempdir();
    let outside = canonical_tempdir();
    let base = staging.path().join("agents");
    symlink(outside.path(), &base).unwrap();
    assert!(matches!(
        OwnedRecapRun::create(&base, 100),
        Err(RecapStateFailure::Ownership)
    ));
    assert!(
        !outside.path().join("recap-runs").exists(),
        "base validation must happen before mkdir"
    );
}

#[test]
fn aliased_recovery_base_cannot_delete_outside_owned_generations() {
    let staging = canonical_tempdir();
    let outside = canonical_tempdir();
    let run = OwnedRecapRun::create(outside.path(), 100).unwrap();
    let base = staging.path().join("agents");
    symlink(outside.path(), &base).unwrap();
    assert!(matches!(
        recover_recap_runs(&base, 100 + RECOVERY_AGE_SECONDS),
        Err(RecapStateFailure::Ownership)
    ));
    assert!(run.path().exists());
}

#[test]
fn cleanup_failure_is_propagated_after_other_expired_roots_are_attempted() {
    let base = canonical_tempdir();
    let _first = OwnedRecapRun::create(base.path(), 100).unwrap();
    let _second = OwnedRecapRun::create(base.path(), 100).unwrap();
    let mut attempts = 0;
    let mut failed_path = None;
    let result = recover_with_cleanup(base.path(), 100 + RECOVERY_AGE_SECONDS, |run| {
        attempts += 1;
        if attempts == 1 {
            failed_path = Some(run.path().to_owned());
            Err(RecapStateFailure::Io)
        } else {
            run.cleanup()
        }
    });
    assert!(matches!(result, Err(RecapStateFailure::Io)));
    assert_eq!(attempts, 2);
    assert!(failed_path.unwrap().join(MANIFEST).exists());
    assert_eq!(
        std::fs::read_dir(base.path().join("recap-runs"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn recovery_reports_when_the_scan_budget_leaves_unexamined_entries() {
    let base = canonical_tempdir();
    let run = OwnedRecapRun::create(base.path(), 100).unwrap();
    run.cleanup().unwrap();
    for id in 0..1025 {
        std::fs::create_dir(base.path().join("recap-runs").join(format!("unknown-{id}"))).unwrap();
    }
    let report = recover_recap_runs(base.path(), 100 + RECOVERY_AGE_SECONDS).unwrap();
    assert!(report.scan_limited);
    assert_eq!(report.preserved_unknown, 1024);
}
