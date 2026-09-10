//! Schema v2 compatibility, crash atomicity, and retention-pin invariants.
//!
//! Every assertion here runs against a real SQLite file through the shipping
//! `OperationStore`; the crash cases run the real transaction in an owned
//! subprocess that exits inside the production checkpoint.

use super::super::{created, fixture, scope};
use super::{supersede, wiki_id, wiki_intent, wiki_payload};
use crate::owner_operations::{
    Limits, Operation, OperationKind, OperationScope, OperationStatus, OperationStore,
    OperationUpdate, StoreError, WikiSuccessorResult,
};
use rusqlite::Connection;
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;

fn journal_path(dir: &TempDir) -> std::path::PathBuf {
    dir.path()
        .canonicalize()
        .expect("canonical fixture path")
        .join("recovery.db")
}

fn insert_v1_row(connection: &Connection, operation: &Operation) {
    let record_json = serde_json::to_string(operation).expect("record JSON");
    connection
        .execute(
            "INSERT INTO operations(owner,community,id,kind,resource_key,revision,created_at,updated_at,status,reconciled,record_json,bytes,initial_digest) \
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            rusqlite::params![
                &operation.scope.owner,
                &operation.scope.community,
                &operation.id,
                crate::owner_operations::storage::kind_key(operation.kind),
                &operation.resource_key,
                operation.revision,
                operation.created_at,
                operation.updated_at,
                serde_json::to_string(&operation.status).expect("status JSON"),
                operation.reconciled,
                &record_json,
                record_json.len() as i64,
                vec![7u8; 32],
            ],
        )
        .expect("insert v1 row");
}

fn user_version(path: &std::path::Path) -> i64 {
    let connection = Connection::open(path).expect("open journal");
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .expect("user_version")
}

fn v1_rows(owner: &OperationScope) -> Vec<Operation> {
    vec![
        Operation {
            version: 1,
            scope: owner.clone(),
            id: wiki_id(101),
            kind: OperationKind::WikiPublication,
            resource_key: "repo/wiki".into(),
            revision: 4,
            created_at: 10,
            updated_at: 11,
            status: OperationStatus::Reconciling,
            reconciled: false,
            payload: wiki_payload("repo/wiki", "claim"),
        },
        Operation {
            version: 1,
            scope: scope('b', "https://two.example"),
            id: wiki_id(102),
            kind: OperationKind::ChannelCrewConfig,
            resource_key: "channel/one".into(),
            revision: 9,
            created_at: 20,
            updated_at: 21,
            status: OperationStatus::Complete,
            reconciled: true,
            payload: json!({"kind": "crew", "roles": ["owner"]}),
        },
    ]
}

#[test]
fn durable_core_schema_migration_is_idempotent_and_preserves_unresolved_claims() {
    let dir = tempfile::tempdir().expect("fixture directory");
    let path = journal_path(&dir);
    let owner = scope('a', "https://one.example");
    let rows = v1_rows(&owner);

    let connection = Connection::open(&path).expect("open journal");
    connection
        .execute_batch(include_str!("schema.sql"))
        .expect("v1 schema");
    for operation in &rows {
        insert_v1_row(&connection, operation);
    }
    // Running the migration twice must leave exactly the same schema: an
    // interrupted opener retries it on the next start.
    for _ in 0..2 {
        connection
            .execute_batch(include_str!("migration_1_to_2.sql"))
            .expect("idempotent migration");
    }
    drop(connection);
    assert_eq!(user_version(&path), 2);

    let mut store = OperationStore::open(&path, Limits::default()).expect("reopen migrated");
    for operation in &rows {
        assert_eq!(
            store.load(&operation.scope, &operation.id).unwrap(),
            operation.clone(),
            "the migration must preserve every scoped row byte-for-byte"
        );
    }
    // The unresolved claim survived, so a different draft for the same
    // coordinate is still refused instead of adopting the retained one.
    assert_eq!(
        store.create(&owner, wiki_intent(&wiki_id(103), "repo/wiki", "other"), 30),
        Err(StoreError::Conflict)
    );
    assert_eq!(store.load(&owner, &wiki_id(103)), Err(StoreError::Missing));
    assert_eq!(
        store
            .connection
            .query_row(
                "SELECT count(*) FROM wiki_publication_successors",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0,
        "the migration adds no relation rows of its own"
    );
}

/// The opener's accepted set is the whole compatibility contract: a build
/// without the successor relation ends that set at `1`, so it refuses a
/// migrated journal through this very branch instead of ignoring pins it
/// cannot honor (D-079).
#[test]
fn durable_core_open_refuses_schema_versions_outside_its_supported_set() {
    assert_eq!(
        crate::owner_operations::storage::SUPPORTED_SCHEMA_VERSIONS.to_vec(),
        vec![0_i64, 1, 2]
    );
    assert_eq!(crate::owner_operations::storage::CURRENT_SCHEMA_VERSION, 2);
    for version in [3_i64, 99] {
        let dir = tempfile::tempdir().expect("fixture directory");
        let path = journal_path(&dir);
        let connection = Connection::open(&path).expect("open journal");
        connection
            .execute_batch(&format!(
                "PRAGMA user_version={version}; CREATE TABLE evidence(value TEXT); \
                 INSERT INTO evidence VALUES('keep');"
            ))
            .expect("future schema");
        drop(connection);
        assert!(matches!(
            OperationStore::open(&path, Limits::default()),
            Err(StoreError::Version)
        ));
        let connection = Connection::open(&path).expect("reopen");
        assert_eq!(
            connection
                .query_row("SELECT value FROM evidence", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "keep",
            "an unsupported journal is never reset"
        );
    }
}

fn child_test_name(name: &str) -> String {
    format!(
        "{}::{name}",
        module_path!()
            .split("::")
            .skip(1)
            .collect::<Vec<_>>()
            .join("::")
    )
}

fn run_crash_child(name: &str, stage: &str, envs: &[(&str, &str)]) {
    use std::process::Command;
    use std::time::{Duration, Instant};
    let child_name = child_test_name(name);
    let mut command = Command::new(std::env::current_exe().expect("test binary"));
    command
        .args(["--exact", child_name.as_str(), "--ignored"])
        .env("CREW_OPERATION_CRASH_AT", stage)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("spawn crash fixture");
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().expect("child status") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill crash fixture");
            child.wait().expect("reap crash fixture");
            panic!("owned crash fixture exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(
        status.code(),
        Some(77),
        "the fixture must exit in-checkpoint"
    );
}

#[test]
#[ignore = "owned subprocess crash fixture; invoked by durable_core_crash_around_the_migration_commit_is_atomic"]
fn crash_migration_child() {
    let path = std::env::var("CREW_OPERATION_TEST_DB").expect("owned crash fixture DB");
    OperationStore::open(Path::new(&path), Limits::default()).expect("migrating open");
}

#[test]
fn durable_core_crash_around_the_migration_commit_is_atomic() {
    for stage in ["before-migration-commit", "after-migration-commit"] {
        let dir = tempfile::tempdir().expect("fixture directory");
        let path = journal_path(&dir);
        let owner = scope('a', "https://one.example");
        let rows = v1_rows(&owner);
        let connection = Connection::open(&path).expect("open journal");
        connection
            .execute_batch(include_str!("schema.sql"))
            .expect("v1 schema");
        for operation in &rows {
            insert_v1_row(&connection, operation);
        }
        drop(connection);

        let db = path.display().to_string();
        run_crash_child(
            "crash_migration_child",
            stage,
            &[("CREW_OPERATION_TEST_DB", db.as_str())],
        );

        assert_eq!(
            user_version(&path),
            if stage == "before-migration-commit" {
                1
            } else {
                2
            },
            "the migration commits atomically or not at all"
        );
        // Either way the journal is recoverable and complete.
        let store = OperationStore::open(&path, Limits::default()).expect("recovering open");
        assert_eq!(
            store
                .connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );
        for operation in &rows {
            assert_eq!(
                store.load(&operation.scope, &operation.id).unwrap(),
                operation.clone()
            );
        }
    }
}

#[test]
#[ignore = "owned subprocess crash fixture; invoked by durable_core_crash_around_the_successor_commit_is_atomic"]
fn crash_successor_child() {
    let path = std::env::var("CREW_OPERATION_TEST_DB").expect("owned crash fixture DB");
    let id = std::env::var("CREW_OPERATION_TEST_ID").expect("predecessor ID");
    let successor = std::env::var("CREW_OPERATION_TEST_SUCCESSOR").expect("successor ID");
    let owner = scope('a', "https://one.example");
    let mut store =
        OperationStore::open(Path::new(&path), Limits::default()).expect("open crash fixture");
    let predecessor = store.load(&owner, &id).expect("predecessor");
    store
        .replace_wiki_with_successor(
            &owner,
            &id,
            predecessor.revision,
            supersede(predecessor.payload.clone()),
            wiki_intent(&successor, &predecessor.resource_key, "b"),
            200,
        )
        .expect("atomic replacement");
}

#[test]
fn durable_core_crash_around_the_successor_commit_is_atomic() {
    for stage in ["before-commit", "after-commit"] {
        let dir = tempfile::tempdir().expect("fixture directory");
        let path = journal_path(&dir);
        let owner = scope('a', "https://one.example");
        let mut store = OperationStore::open(&path, Limits::default()).expect("journal");
        let predecessor = created(
            store
                .create(&owner, wiki_intent(&wiki_id(110), "repo/wiki", "a"), 100)
                .expect("reserve predecessor"),
        );
        drop(store);

        let db = path.display().to_string();
        let successor_id = wiki_id(111);
        run_crash_child(
            "crash_successor_child",
            stage,
            &[
                ("CREW_OPERATION_TEST_DB", db.as_str()),
                ("CREW_OPERATION_TEST_ID", predecessor.id.as_str()),
                ("CREW_OPERATION_TEST_SUCCESSOR", successor_id.as_str()),
            ],
        );

        let store = OperationStore::open(&path, Limits::default()).expect("recovering open");
        let links: i64 = store
            .connection
            .query_row(
                "SELECT count(*) FROM wiki_publication_successors",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let recovered = store.load(&owner, &predecessor.id).expect("predecessor");
        if stage == "before-commit" {
            assert_eq!(recovered, predecessor, "the old claim is retained intact");
            assert!(!recovered.reconciled);
            assert_eq!(store.load(&owner, &wiki_id(111)), Err(StoreError::Missing));
            assert_eq!(links, 0);
        } else {
            assert_eq!(recovered.revision, predecessor.revision + 1);
            assert!(recovered.reconciled);
            assert_eq!(recovered.status, OperationStatus::Superseded);
            let successor = store.load(&owner, &wiki_id(111)).expect("successor");
            assert!(!successor.reconciled);
            assert_eq!(links, 1);
            assert_eq!(
                store
                    .connection
                    .query_row(
                        "SELECT predecessor_revision FROM wiki_publication_successors \
                         WHERE owner=?1 AND community=?2 AND predecessor_id=?3",
                        rusqlite::params![owner.owner, owner.community, predecessor.id],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap(),
                recovered.revision as i64,
                "the link records the post-retirement revision"
            );
        }
    }
}

#[test]
fn durable_core_age_trimming_removes_unpinned_history_but_never_an_active_pin() {
    let (_dir, mut store) = fixture(Limits {
        terminal_age_secs: 10,
        ..Limits::default()
    });
    let owner = scope('a', "https://one.example");
    let stale = created(
        store
            .create(&owner, wiki_intent(&wiki_id(120), "repo/stale", "a"), 100)
            .unwrap(),
    );
    store
        .compare_and_swap(
            &owner,
            &stale.id,
            stale.revision,
            OperationUpdate {
                status: OperationStatus::Complete,
                reconciled: true,
                payload: stale.payload.clone(),
            },
            100,
        )
        .unwrap();
    let pinned = created(
        store
            .create(&owner, wiki_intent(&wiki_id(121), "repo/pinned", "a"), 100)
            .unwrap(),
    );
    let relation = store
        .replace_wiki_with_successor(
            &owner,
            &pinned.id,
            pinned.revision,
            supersede(pinned.payload.clone()),
            wiki_intent(&wiki_id(122), "repo/pinned", "b"),
            100,
        )
        .unwrap();

    // Any later mutation runs the transactional trim well past the age window.
    store
        .create(&owner, wiki_intent(&wiki_id(123), "repo/later", "a"), 1_000)
        .unwrap();
    assert_eq!(store.load(&owner, &stale.id), Err(StoreError::Missing));
    assert_eq!(
        store.load(&owner, &pinned.id).unwrap(),
        relation.predecessor,
        "an active pin is protected from every trim path"
    );
    assert_eq!(
        store.remove_reconciled(&owner, &pinned.id, relation.predecessor.revision),
        Err(StoreError::Pinned)
    );

    // Once the immediate successor reconciles, the predecessor resumes
    // ordinary retention and the same age window reclaims it. There is no
    // recursive pin chain and no orphaned relation row.
    store
        .compare_and_swap(
            &owner,
            &relation.successor.id,
            relation.successor.revision,
            OperationUpdate {
                status: OperationStatus::Complete,
                reconciled: true,
                payload: relation.successor.payload.clone(),
            },
            1_001,
        )
        .unwrap();
    assert_eq!(store.load(&owner, &pinned.id), Err(StoreError::Missing));
    assert_eq!(
        store
            .connection
            .query_row(
                "SELECT count(*) FROM wiki_publication_successors",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0,
        "a resolved relation must not keep consuming owner quota"
    );
}

#[test]
fn durable_core_successor_relation_metadata_counts_against_the_owner_byte_quota() {
    fn replace(
        limits: Limits,
    ) -> (
        TempDir,
        OperationStore,
        Result<WikiSuccessorResult, StoreError>,
    ) {
        let (dir, mut store) = fixture(limits);
        let owner = scope('a', "https://one.example");
        let old = created(
            store
                .create(&owner, wiki_intent(&wiki_id(130), "repo/wiki", "a"), 100)
                .expect("reserve predecessor"),
        );
        let result = store.replace_wiki_with_successor(
            &owner,
            &old.id,
            old.revision,
            supersede(old.payload.clone()),
            wiki_intent(&wiki_id(131), "repo/wiki", "b"),
            101,
        );
        (dir, store, result)
    }

    let owner = scope('a', "https://one.example");
    let (_dir, measured, result) = replace(Limits::default());
    result.expect("baseline replacement");
    let record_bytes: i64 = measured
        .connection
        .query_row(
            "SELECT coalesce(sum(length(CAST(record_json AS BLOB))),0) FROM operations WHERE owner=?1",
            [owner.owner.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    let link_bytes = crate::owner_operations::mutations::successor_link_bytes(
        &measured.connection,
        &owner.owner,
    )
    .expect("link metadata size");
    assert!(link_bytes > 0, "the relation carries bounded metadata");

    let (_dir, exact, exact_result) = replace(Limits {
        bytes_per_owner: (record_bytes + link_bytes) as usize,
        ..Limits::default()
    });
    exact_result.expect("a budget that covers records and relation metadata");
    drop(exact);

    let (_dir, tight, tight_result) = replace(Limits {
        bytes_per_owner: (record_bytes + link_bytes - 1) as usize,
        ..Limits::default()
    });
    assert_eq!(
        tight_result,
        Err(StoreError::Quota),
        "relation metadata must be counted, and a pinned predecessor is never evicted to make room"
    );
    let retained = tight
        .load(&owner, &wiki_id(130))
        .expect("old claim retained");
    assert!(
        !retained.reconciled,
        "a failed replacement rolls back the claim"
    );
    assert_eq!(retained.revision, 0);
    assert_eq!(tight.load(&owner, &wiki_id(131)), Err(StoreError::Missing));
    assert_eq!(
        tight
            .connection
            .query_row(
                "SELECT count(*) FROM wiki_publication_successors",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn durable_core_wiki_successor_lookup_binds_scope_and_post_retirement_revision() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let old = created(
        store
            .create(&owner, wiki_intent(&wiki_id(140), "repo/wiki", "a"), 100)
            .unwrap(),
    );
    assert_eq!(
        store.wiki_successor(&owner, &old.id, old.revision),
        Ok(None)
    );

    let relation = store
        .replace_wiki_with_successor(
            &owner,
            &old.id,
            old.revision,
            supersede(old.payload.clone()),
            wiki_intent(&wiki_id(141), "repo/wiki", "b"),
            101,
        )
        .unwrap();
    assert_eq!(
        store.wiki_successor(&owner, &old.id, old.revision),
        Ok(Some(relation.clone()))
    );
    assert_eq!(
        store.wiki_successor(&owner, &old.id, old.revision + 1),
        Err(StoreError::Conflict),
        "the recorded revision is post-retirement; only the request's own is accepted"
    );
    assert_eq!(
        store.wiki_successor(&owner, &old.id, u64::MAX),
        Err(StoreError::Invalid)
    );
    for wrong in [
        scope('b', "https://one.example"),
        scope('a', "https://two.example"),
    ] {
        assert_eq!(
            store.wiki_successor(&wrong, &old.id, old.revision),
            Ok(None),
            "a foreign scope must never resolve another owner's relation"
        );
    }

    let completed = store
        .compare_and_swap(
            &owner,
            &relation.successor.id,
            relation.successor.revision,
            OperationUpdate {
                status: OperationStatus::Complete,
                reconciled: true,
                payload: relation.successor.payload.clone(),
            },
            102,
        )
        .unwrap();
    let after = store
        .wiki_successor(&owner, &old.id, old.revision)
        .unwrap()
        .expect("a reconciled successor is still recoverable");
    assert_eq!(after.successor, completed);
    assert_eq!(after.predecessor, relation.predecessor);
}
