//! The production producer must refuse what it cannot observe.

use super::*;

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

/// One ledger directory holding the named entries.
fn ledger(directory: &Path, entries: &[(&str, &[u8])]) -> std::path::PathBuf {
    let path = directory.join("session-ledger");
    std::fs::create_dir_all(&path).unwrap();
    for (name, bytes) in entries {
        std::fs::write(path.join(name), bytes).unwrap();
    }
    path
}

#[test]
fn a_capture_reads_the_sessions_own_ledger_entries() {
    let directory = tempdir();
    let path = ledger(
        directory.path(),
        &[("b.json", b"{\"turn\":2}"), ("a.json", b"{\"turn\":1}")],
    );
    let snapshot = SessionSnapshot::capture(&path, Some(4321)).unwrap();
    assert_eq!(snapshot.entry_count, 2);
    assert_eq!(snapshot.acp_pid, Some(4321));

    // Sorted by name, so readdir order cannot change the digest.
    let mut expected = Sha256::new();
    for (name, bytes) in [
        ("a.json", b"{\"turn\":1}".as_slice()),
        ("b.json", b"{\"turn\":2}".as_slice()),
    ] {
        expected.update(name.as_bytes());
        expected.update([0]);
        expected.update(bytes.len().to_le_bytes());
        expected.update([0]);
        expected.update(bytes);
    }
    assert_eq!(snapshot.ledger_digest, hex::encode(expected.finalize()));
}

#[test]
fn an_unobservable_ledger_directory_is_refused_rather_than_digested_as_empty() {
    let directory = tempdir();

    // Absent directory — an agent that never ran is not an observed session.
    assert_eq!(
        SessionSnapshot::capture(&directory.path().join("absent"), Some(1)),
        Err(PrivateAskFailure::SessionObservationUnavailable)
    );
    // A file is not a ledger directory.
    let file = directory.path().join("ledger.json");
    std::fs::write(&file, b"{}").unwrap();
    assert_eq!(
        SessionSnapshot::capture(&file, Some(1)),
        Err(PrivateAskFailure::SessionObservationUnavailable)
    );
    // An empty directory. THE case: "digest of nothing" would certify every
    // agent that has never written a ledger entry.
    let empty = ledger(directory.path(), &[]);
    assert_eq!(
        SessionSnapshot::capture(&empty, Some(1)),
        Err(PrivateAskFailure::SessionObservationUnavailable)
    );
    // A directory holding only a write in progress and a subdirectory still
    // has zero entries.
    std::fs::write(empty.join("pending.tmp"), b"{}").unwrap();
    std::fs::create_dir(empty.join("nested.json")).unwrap();
    assert_eq!(
        SessionSnapshot::capture(&empty, Some(1)),
        Err(PrivateAskFailure::SessionObservationUnavailable)
    );
}

#[test]
fn an_oversized_ledger_entry_is_refused_rather_than_truncated() {
    let directory = tempdir();
    let path = ledger(directory.path(), &[("a.json", b"{}")]);
    let big = vec![b'x'; SESSION_ARTEFACT_LIMIT as usize + 1];
    std::fs::write(path.join("big.json"), &big).unwrap();
    assert_eq!(
        SessionSnapshot::capture(&path, Some(1)),
        Err(PrivateAskFailure::SessionObservationUnavailable)
    );
}

#[test]
fn the_ledger_directory_derivation_mirrors_the_harnesss_own() {
    // Pins the mirror: `session_ledger_dir_for_scope` in `buzz-acp` joins the
    // first 16 hex characters of the relay-url digest and the lowercased agent
    // pubkey under the ledger base. If either side moves, this fails rather
    // than making every private Ask look like an agent that never ran.
    let base = Path::new("/tmp/ledger-base");
    let relay = "wss://relay.example/";
    let pubkey = "AB".repeat(32);
    let relay_hash = hex::encode(Sha256::digest(relay.as_bytes()));
    assert_eq!(
        session_ledger_dir_under(base, relay, &pubkey),
        base.join(&relay_hash[..16])
            .join(pubkey.to_ascii_lowercase())
    );
}

#[test]
fn a_session_with_no_live_process_cannot_be_verified() {
    let directory = tempdir();
    let path = ledger(directory.path(), &[("a.json", b"{}")]);
    // PID zero is not a process; it must not masquerade as one.
    let absent = SessionSnapshot::capture(&path, Some(0)).unwrap();
    assert_eq!(absent.acp_pid, None);
    let evidence = SessionIsolationEvidence::observe(absent.clone(), absent, 99, 99).unwrap();
    assert!(!evidence.is_verified());
}

#[test]
fn a_lineage_without_a_parent_or_a_desktop_is_refused() {
    let directory = tempdir();
    let path = ledger(directory.path(), &[("a.json", b"{}")]);
    let snapshot = SessionSnapshot::capture(&path, Some(11)).unwrap();
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
    let path = ledger(directory.path(), &[("a.json", b"{\"turn\":1}")]);
    let before = SessionSnapshot::capture(&path, Some(11)).unwrap();

    let unchanged = SessionSnapshot::capture(&path, Some(11)).unwrap();
    assert!(
        SessionIsolationEvidence::observe(before.clone(), unchanged, 99, 99)
            .unwrap()
            .is_verified()
    );

    // The ledger bytes moved.
    std::fs::write(path.join("a.json"), b"{\"turn\":2}").unwrap();
    let changed = SessionSnapshot::capture(&path, Some(11)).unwrap();
    assert!(
        !SessionIsolationEvidence::observe(before.clone(), changed, 99, 99)
            .unwrap()
            .is_verified()
    );

    // A new entry appeared beside the original one.
    std::fs::write(path.join("a.json"), b"{\"turn\":1}").unwrap();
    std::fs::write(path.join("b.json"), b"{\"turn\":2}").unwrap();
    let appended = SessionSnapshot::capture(&path, Some(11)).unwrap();
    assert!(
        !SessionIsolationEvidence::observe(before.clone(), appended, 99, 99)
            .unwrap()
            .is_verified()
    );

    // The session restarted under a new PID.
    std::fs::remove_file(path.join("b.json")).unwrap();
    let restarted = SessionSnapshot::capture(&path, Some(12)).unwrap();
    assert!(
        !SessionIsolationEvidence::observe(before.clone(), restarted, 99, 99)
            .unwrap()
            .is_verified()
    );

    // The child was adopted by something other than the desktop.
    let same = SessionSnapshot::capture(&path, Some(11)).unwrap();
    assert!(!SessionIsolationEvidence::observe(before, same, 98, 99)
        .unwrap()
        .is_verified());
}
