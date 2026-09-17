//! The production producer must refuse what it cannot observe.

use super::*;

fn session(
    directory: &Path,
    ledger: &[u8],
    sequence: &[u8],
) -> (std::path::PathBuf, std::path::PathBuf) {
    let ledger_path = directory.join("ledger.json");
    let sequence_path = directory.join("observer-sequence");
    std::fs::write(&ledger_path, ledger).unwrap();
    std::fs::write(&sequence_path, sequence).unwrap();
    (ledger_path, sequence_path)
}

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

#[test]
fn a_capture_reads_the_sessions_own_bytes() {
    let directory = tempdir();
    let (ledger, sequence) = session(directory.path(), b"{\"turn\":1}", b"42\n");
    let snapshot = SessionSnapshot::capture(&ledger, &sequence, Some(4321)).unwrap();
    assert_eq!(
        snapshot.ledger_digest,
        hex::encode(Sha256::digest(b"{\"turn\":1}"))
    );
    assert_eq!(snapshot.observer_sequence, 42);
    assert_eq!(snapshot.acp_pid, Some(4321));
}

#[test]
fn an_unobservable_session_is_refused_rather_than_digested_as_empty() {
    let directory = tempdir();
    let (ledger, sequence) = session(directory.path(), b"{}", b"1");

    // Absent ledger.
    assert_eq!(
        SessionSnapshot::capture(&directory.path().join("absent"), &sequence, Some(1)),
        Err(PrivateAskFailure::SessionObservationUnavailable)
    );
    // A directory is not a ledger.
    assert_eq!(
        SessionSnapshot::capture(directory.path(), &sequence, Some(1)),
        Err(PrivateAskFailure::SessionObservationUnavailable)
    );
    // An observer sequence that is empty, non-numeric, or not valid UTF-8.
    for spoiled in [
        b"".as_slice(),
        b"  ".as_slice(),
        b"soon".as_slice(),
        &[0xff, 0xfe],
    ] {
        let path = directory.path().join("spoiled-sequence");
        std::fs::write(&path, spoiled).unwrap();
        assert_eq!(
            SessionSnapshot::capture(&ledger, &path, Some(1)),
            Err(PrivateAskFailure::SessionObservationUnavailable),
            "sequence {spoiled:?} must not be observable"
        );
    }
}

#[test]
fn a_session_with_no_live_process_cannot_be_verified() {
    let directory = tempdir();
    let (ledger, sequence) = session(directory.path(), b"{}", b"7");
    // PID zero is not a process; it must not masquerade as one.
    let absent = SessionSnapshot::capture(&ledger, &sequence, Some(0)).unwrap();
    assert_eq!(absent.acp_pid, None);
    let evidence = SessionIsolationEvidence::observe(absent.clone(), absent, 99, 99).unwrap();
    assert!(!evidence.is_verified());
}

#[test]
fn a_lineage_without_a_parent_or_a_desktop_is_refused() {
    let directory = tempdir();
    let (ledger, sequence) = session(directory.path(), b"{}", b"7");
    let snapshot = SessionSnapshot::capture(&ledger, &sequence, Some(11)).unwrap();
    for (parent, desktop) in [(0, 99), (99, 0), (0, 0)] {
        assert_eq!(
            SessionIsolationEvidence::observe(snapshot.clone(), snapshot.clone(), parent, desktop),
            Err(PrivateAskFailure::SessionObservationUnavailable)
        );
    }
}

#[test]
fn a_session_that_changed_under_the_ask_is_not_verified() {
    let directory = tempdir();
    let (ledger, sequence) = session(directory.path(), b"{\"turn\":1}", b"7");
    let before = SessionSnapshot::capture(&ledger, &sequence, Some(11)).unwrap();

    let unchanged = SessionSnapshot::capture(&ledger, &sequence, Some(11)).unwrap();
    assert!(
        SessionIsolationEvidence::observe(before.clone(), unchanged, 99, 99)
            .unwrap()
            .is_verified()
    );

    // The ledger bytes moved.
    std::fs::write(&ledger, b"{\"turn\":2}").unwrap();
    let changed = SessionSnapshot::capture(&ledger, &sequence, Some(11)).unwrap();
    assert!(
        !SessionIsolationEvidence::observe(before.clone(), changed, 99, 99)
            .unwrap()
            .is_verified()
    );

    // The observer advanced.
    std::fs::write(&ledger, b"{\"turn\":1}").unwrap();
    std::fs::write(&sequence, b"8").unwrap();
    let advanced = SessionSnapshot::capture(&ledger, &sequence, Some(11)).unwrap();
    assert!(
        !SessionIsolationEvidence::observe(before.clone(), advanced, 99, 99)
            .unwrap()
            .is_verified()
    );

    // The session restarted under a new PID.
    std::fs::write(&sequence, b"7").unwrap();
    let restarted = SessionSnapshot::capture(&ledger, &sequence, Some(12)).unwrap();
    assert!(
        !SessionIsolationEvidence::observe(before.clone(), restarted, 99, 99)
            .unwrap()
            .is_verified()
    );

    // The child was adopted by something other than the desktop.
    let same = SessionSnapshot::capture(&ledger, &sequence, Some(11)).unwrap();
    assert!(!SessionIsolationEvidence::observe(before, same, 98, 99)
        .unwrap()
        .is_verified());
}
