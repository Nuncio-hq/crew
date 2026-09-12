use super::*;
use crate::managed_agents::recap_capability::{
    RecapAuthBinding, RecapCertificationParts, RecapExecutableIdentity, RecapGuarantees,
    RecapSelection,
};
use rusqlite::Connection;
use std::path::PathBuf;

fn parts(version: &str, fingerprint: &str, platform: &str) -> RecapCertificationParts {
    RecapCertificationParts {
        runtime_id: "claude".into(),
        executable: RecapExecutableIdentity {
            resolved_path: PathBuf::from("/staging/claude"),
            version: version.into(),
            fingerprint: fingerprint.into(),
            platform: platform.into(),
        },
        selection: RecapSelection {
            model: "fixture-model".into(),
            profile: None,
            profile_digest: None,
            profile_identity: None,
            auth_available: true,
        },
        auth: RecapAuthBinding {
            service: "fixture-keyring".into(),
            reference: "fixture-auth".into(),
        },
        guarantees: RecapGuarantees {
            one_shot: true,
            tool_isolation: true,
            state_isolation: true,
            process_containment: true,
        },
        effective_model: "fixture-model".into(),
        output_digest: "a".repeat(64),
        tool_probe_digest: "b".repeat(64),
    }
}

#[test]
fn certification_rows_are_keyed_by_exact_binary_identity() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(RECAP_CERTIFICATION_SCHEMA).unwrap();
    let first = parts("fixture-1", &"a".repeat(64), "macos-aarch64");
    let second = parts("fixture-2", &"a".repeat(64), "macos-aarch64");
    let third = parts("fixture-1", &"b".repeat(64), "macos-aarch64");
    persist_recap_certification(&conn, &first, &"c".repeat(64), 1).unwrap();
    persist_recap_certification(&conn, &second, &"d".repeat(64), 2).unwrap();
    persist_recap_certification(&conn, &third, &"e".repeat(64), 3).unwrap();

    assert!(get_recap_certification(
        &conn,
        "claude",
        &"a".repeat(64),
        "fixture-1",
        "macos-aarch64"
    )
    .unwrap()
    .is_some());
    assert!(get_recap_certification(
        &conn,
        "claude",
        &"a".repeat(64),
        "fixture-2",
        "macos-aarch64"
    )
    .unwrap()
    .is_some());
    assert!(get_recap_certification(
        &conn,
        "claude",
        &"b".repeat(64),
        "fixture-1",
        "macos-aarch64"
    )
    .unwrap()
    .is_some());
    assert!(get_recap_certification(
        &conn,
        "claude",
        &"a".repeat(64),
        "fixture-1",
        "linux-x86_64"
    )
    .unwrap()
    .is_none());
}

#[test]
fn stored_row_matches_every_admission_field_and_ownership_digest() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(RECAP_CERTIFICATION_SCHEMA).unwrap();
    let expected = parts("fixture-1", &"a".repeat(64), "macos-aarch64");
    let ownership = "c".repeat(64);
    persist_recap_certification(&conn, &expected, &ownership, 1).unwrap();
    let stored = get_recap_certification(
        &conn,
        "claude",
        &expected.executable.fingerprint,
        &expected.executable.version,
        &expected.executable.platform,
    )
    .unwrap()
    .unwrap();
    assert!(stored.matches(&expected, &ownership));

    let mut changed = expected.clone();
    changed.tool_probe_digest = "d".repeat(64);
    assert!(!stored.matches(&changed, &ownership));
    assert!(!stored.matches(&expected, &"e".repeat(64)));
}

#[test]
fn malformed_ownership_digest_is_not_persisted() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(RECAP_CERTIFICATION_SCHEMA).unwrap();
    let expected = parts("fixture-1", &"a".repeat(64), "macos-aarch64");
    assert!(persist_recap_certification(&conn, &expected, "short", 1).is_err());
    assert!(get_recap_certification(
        &conn,
        "claude",
        &expected.executable.fingerprint,
        &expected.executable.version,
        &expected.executable.platform,
    )
    .unwrap()
    .is_none());
}
