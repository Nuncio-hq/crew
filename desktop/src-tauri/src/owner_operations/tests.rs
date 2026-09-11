use super::*;
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;
use uuid::Uuid;

fn scope(owner: char, community: &str) -> OperationScope {
    OperationScope {
        owner: owner.to_string().repeat(64),
        community: community.into(),
    }
}

fn fixture(limits: Limits) -> (TempDir, OperationStore) {
    let dir = tempfile::tempdir().expect("owned fixture directory");
    let path = dir
        .path()
        .canonicalize()
        .expect("fixture canonical path")
        .join("recovery.db");
    let store = OperationStore::open(&path, limits).expect("healthy SQLite fixture");
    assert_eq!(
        store
            .connection
            .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
            .expect("healthy SQLite connection"),
        1
    );
    (dir, store)
}

fn request(resource: &str) -> NewOperation {
    NewOperation {
        id: Uuid::new_v4().to_string(),
        kind: OperationKind::ProjectChange,
        resource_key: resource.into(),
        payload: json!({"signedEvent": {"id":"exact-event", "sig":"exact-signature"}, "step":"prepared"}),
    }
}

fn created(value: CreateResult) -> Operation {
    match value {
        CreateResult::Created(op) => op,
        other => panic!("expected created operation: {other:?}"),
    }
}

#[test]
fn durable_core_roundtrips_exact_event_after_reopen() {
    let (dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let new = request("30621:a:project");
    let expected = new.payload.clone();
    let operation = created(
        store
            .create(&owner, new, 100)
            .expect("create operation before publication"),
    );
    drop(store);
    let store = OperationStore::open(
        &dir.path().canonicalize().unwrap().join("recovery.db"),
        Limits::default(),
    )
    .expect("reopen");
    let read = store
        .load(&owner, &operation.id)
        .expect("durable operation");
    assert_eq!(read.payload, expected);
    assert_eq!(read.revision, 0);
    assert!(!read.reconciled);
}

#[test]
fn durable_core_scope_isolation_owner_and_community() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let op = created(
        store
            .create(&owner, request("project"), 100)
            .expect("create"),
    );
    for wrong in [
        scope('b', "https://one.example"),
        scope('a', "https://two.example"),
    ] {
        assert_eq!(store.load(&wrong, &op.id), Err(StoreError::Missing));
    }
    assert_eq!(store.load(&owner, &op.id).unwrap(), op);
}

#[test]
fn durable_core_cas_two_connections_preserves_whole_winner() {
    let (dir, mut first) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let op = created(
        first
            .create(&owner, request("project"), 100)
            .expect("create"),
    );
    let mut second = OperationStore::open(
        &dir.path().canonicalize().unwrap().join("recovery.db"),
        Limits::default(),
    )
    .unwrap();
    let update = |step| OperationUpdate {
        status: OperationStatus::Pending,
        reconciled: false,
        payload: json!({"step":step,"signedEvent":op.payload["signedEvent"]}),
    };
    let winner = first
        .compare_and_swap(&owner, &op.id, 0, update("winner"), 101)
        .expect("first CAS");
    assert_eq!(
        second.compare_and_swap(&owner, &op.id, 0, update("loser"), 102),
        Err(StoreError::Conflict)
    );
    assert_eq!(second.load(&owner, &op.id).unwrap(), winner);
}

#[test]
fn durable_core_canceled_and_superseded_unresolved_keep_resource_claim() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    for status in [
        OperationStatus::Failed,
        OperationStatus::Canceled,
        OperationStatus::Superseded,
    ] {
        let resource = format!("project-{status:?}");
        let op = created(
            store
                .create(&owner, request(&resource), 100)
                .expect("create"),
        );
        let updated = store
            .compare_and_swap(
                &owner,
                &op.id,
                0,
                OperationUpdate {
                    status,
                    reconciled: false,
                    payload: op.payload,
                },
                101,
            )
            .unwrap();
        assert_eq!(
            store.create(&owner, request(&resource), 102).unwrap(),
            CreateResult::Existing(updated.clone())
        );
        assert_eq!(
            store.remove_reconciled(&owner, &op.id, 1),
            Err(StoreError::Unreconciled)
        );
        assert_eq!(store.load(&owner, &op.id).unwrap(), updated);
    }
}

#[test]
fn durable_core_owner_quota_crosses_communities_without_eviction() {
    let (_dir, mut store) = fixture(Limits {
        pending_per_owner: 1,
        ..Limits::default()
    });
    let owner = scope('a', "https://one.example");
    let op = created(
        store
            .create(&owner, request("project"), 100)
            .expect("create"),
    );
    assert_eq!(
        store.create(&scope('a', "https://two.example"), request("other"), 101),
        Err(StoreError::Quota)
    );
    assert_eq!(store.load(&owner, &op.id).unwrap(), op);
    assert!(matches!(
        store.create(&scope('b', "https://one.example"), request("project"), 101),
        Ok(CreateResult::Created(_))
    ));
}

#[test]
fn durable_core_reconciliation_releases_claim_and_allows_explicit_removal() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let op = created(store.create(&owner, request("project"), 100).unwrap());
    let done = store
        .compare_and_swap(
            &owner,
            &op.id,
            0,
            OperationUpdate {
                status: OperationStatus::Canceled,
                reconciled: true,
                payload: op.payload,
            },
            101,
        )
        .unwrap();
    assert!(matches!(
        store.create(&owner, request("project"), 102),
        Ok(CreateResult::Created(_))
    ));
    assert_eq!(
        store.remove_reconciled(&owner, &done.id, 0),
        Err(StoreError::Conflict)
    );
    store.remove_reconciled(&owner, &done.id, 1).unwrap();
    assert_eq!(store.load(&owner, &done.id), Err(StoreError::Missing));
}

#[test]
fn durable_core_remove_reconciled_targets_one_id_and_keeps_unrelated_records() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let first = created(store.create(&owner, request("first"), 100).unwrap());
    let second = created(store.create(&owner, request("second"), 101).unwrap());
    let first = store
        .compare_and_swap(
            &owner,
            &first.id,
            0,
            OperationUpdate {
                status: OperationStatus::Superseded,
                reconciled: true,
                payload: first.payload,
            },
            102,
        )
        .unwrap();
    let second = store
        .compare_and_swap(
            &owner,
            &second.id,
            0,
            OperationUpdate {
                status: OperationStatus::Superseded,
                reconciled: true,
                payload: second.payload,
            },
            103,
        )
        .unwrap();

    store
        .remove_reconciled(&owner, &first.id, first.revision)
        .unwrap();
    assert_eq!(store.load(&owner, &first.id), Err(StoreError::Missing));
    assert_eq!(store.load(&owner, &second.id).unwrap(), second);
}

#[test]
fn durable_core_create_distinguishes_id_replay_and_id_conflict() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let new = request("project");
    let replay = NewOperation {
        id: new.id.clone(),
        kind: new.kind,
        resource_key: new.resource_key.clone(),
        payload: new.payload.clone(),
    };
    let op = created(store.create(&owner, new, 100).unwrap());
    assert_eq!(
        store.create(&owner, replay, 101).unwrap(),
        CreateResult::Existing(op.clone())
    );
    let different = NewOperation {
        id: op.id.clone(),
        kind: op.kind,
        resource_key: op.resource_key.clone(),
        payload: json!({"changed":true}),
    };
    assert_eq!(
        store.create(&owner, different, 102),
        Err(StoreError::Conflict)
    );
}

#[test]
fn durable_core_does_not_adopt_a_different_unresolved_resource_intent() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let first = request("channel");
    let first_id = first.id.clone();
    let first_operation = created(store.create(&owner, first, 100).unwrap());

    let mut second = request("channel");
    second.payload = json!({
        "signedEvent": {"id": "different-event", "sig": "different-signature"},
        "step": "a-different-draft"
    });
    assert_eq!(
        store.create(&owner, second, 101),
        Err(StoreError::Conflict),
        "a resource claim must not transfer the first draft to a second save"
    );
    assert_eq!(store.load(&owner, &first_id).unwrap(), first_operation);
}

#[test]
fn durable_core_large_update_rolls_back_whole_record() {
    let (_dir, mut store) = fixture(Limits {
        bytes_per_operation: 1024,
        ..Limits::default()
    });
    let owner = scope('a', "https://one.example");
    let op = created(store.create(&owner, request("project"), 100).unwrap());
    assert_eq!(
        store.compare_and_swap(
            &owner,
            &op.id,
            0,
            OperationUpdate {
                status: OperationStatus::Pending,
                reconciled: false,
                payload: json!({"text":"x".repeat(2048)})
            },
            101
        ),
        Err(StoreError::Quota)
    );
    assert_eq!(store.load(&owner, &op.id).unwrap(), op);
}

#[test]
fn durable_core_future_schema_is_not_reset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap().join("recovery.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("PRAGMA user_version=99; CREATE TABLE evidence(value TEXT); INSERT INTO evidence VALUES('keep');").unwrap();
    drop(conn);
    assert!(matches!(
        OperationStore::open(&path, Limits::default()),
        Err(StoreError::Version)
    ));
    let conn = Connection::open(&path).unwrap();
    assert_eq!(
        conn.query_row("SELECT value FROM evidence", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "keep"
    );
}

#[cfg(unix)]
#[test]
fn durable_core_rejects_symlink_database_and_sets_private_permissions() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let target = base.join("target.db");
    let link = base.join("recovery.db");
    symlink(&target, &link).unwrap();
    assert!(matches!(
        OperationStore::open(&link, Limits::default()),
        Err(StoreError::UnsafePath)
    ));
    assert!(!target.exists());
    std::fs::remove_file(link).unwrap();
    let (_dir, store) = fixture(Limits::default());
    let path = store.connection.path().unwrap();
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn durable_core_reconciled_only_retention_never_evicts_ambiguous_terminal() {
    let (_dir, mut store) = fixture(Limits {
        terminal_per_owner: 1,
        terminal_age_secs: 10,
        ..Limits::default()
    });
    let owner = scope('a', "https://one.example");
    let pending = created(store.create(&owner, request("ambiguous"), 1).unwrap());
    store
        .compare_and_swap(
            &owner,
            &pending.id,
            0,
            OperationUpdate {
                status: OperationStatus::Canceled,
                reconciled: false,
                payload: pending.payload,
            },
            2,
        )
        .unwrap();
    let old = created(store.create(&owner, request("done1"), 3).unwrap());
    store
        .compare_and_swap(
            &owner,
            &old.id,
            0,
            OperationUpdate {
                status: OperationStatus::Complete,
                reconciled: true,
                payload: old.payload,
            },
            4,
        )
        .unwrap();
    let newer = created(store.create(&owner, request("done2"), 5).unwrap());
    store
        .compare_and_swap(
            &owner,
            &newer.id,
            0,
            OperationUpdate {
                status: OperationStatus::Complete,
                reconciled: true,
                payload: newer.payload,
            },
            6,
        )
        .unwrap();
    assert_eq!(store.load(&owner, &old.id), Err(StoreError::Missing));
    assert!(!store.load(&owner, &pending.id).unwrap().reconciled);
    assert!(store.load(&owner, &newer.id).unwrap().reconciled);
}

#[test]
fn durable_core_simultaneous_connections_have_one_cas_winner() {
    use std::sync::{Arc, Barrier};
    let (dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let op = created(store.create(&owner, request("project"), 100).unwrap());
    let barrier = Arc::new(Barrier::new(3));
    let handles: Vec<_> = (0..2)
        .map(|index| {
            let path = dir.path().canonicalize().unwrap().join("recovery.db");
            let scope = owner.clone();
            let id = op.id.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut store = OperationStore::open(&path, Limits::default()).unwrap();
                barrier.wait();
                store.compare_and_swap(
                    &scope,
                    &id,
                    0,
                    OperationUpdate {
                        status: OperationStatus::Pending,
                        reconciled: false,
                        payload: json!({"writer":index}),
                    },
                    101,
                )
            })
        })
        .collect();
    barrier.wait();
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(StoreError::Conflict))
            .count(),
        1
    );
    assert_eq!(store.load(&owner, &op.id).unwrap().revision, 1);
}

#[test]
fn durable_core_busy_is_bounded_and_retryable() {
    let (dir, store) = fixture(Limits::default());
    let mut second = OperationStore::open(
        &dir.path().canonicalize().unwrap().join("recovery.db"),
        Limits {
            busy_timeout_ms: 0,
            ..Limits::default()
        },
    )
    .unwrap();
    store.connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert_eq!(
        second.create(&scope('a', "https://one.example"), request("project"), 100),
        Err(StoreError::Busy)
    );
    store.connection.execute_batch("ROLLBACK").unwrap();
    assert!(second
        .create(&scope('a', "https://one.example"), request("project"), 101)
        .is_ok());
    assert_eq!(
        second
            .connection
            .pragma_query_value(None, "temp_store", |row| row.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        second
            .connection
            .pragma_query_value(None, "synchronous", |row| row.get::<_, i64>(0))
            .unwrap(),
        2
    );
}

pub(super) fn crash_checkpoint(stage: &str) {
    if std::env::var("CREW_OPERATION_CRASH_AT").as_deref() == Ok(stage) {
        std::process::exit(77);
    }
}

#[test]
#[ignore = "owned subprocess crash fixture; invoked by durable_core_crash_atomicity"]
fn crash_process_child() {
    let path = std::env::var("CREW_OPERATION_TEST_DB").expect("owned crash fixture DB");
    let id = std::env::var("CREW_OPERATION_TEST_ID").expect("fixture operation ID");
    let mut store = OperationStore::open(Path::new(&path), Limits::default()).unwrap();
    store
        .compare_and_swap(
            &scope('a', "https://one.example"),
            &id,
            0,
            OperationUpdate {
                status: OperationStatus::Pending,
                reconciled: false,
                payload: json!({"step":"after-crash"}),
            },
            101,
        )
        .unwrap();
}

#[test]
fn durable_core_crash_atomicity_before_and_after_commit() {
    use std::process::Command;
    use std::time::{Duration, Instant};
    for stage in ["before-commit", "after-commit"] {
        let (dir, mut store) = fixture(Limits::default());
        let owner = scope('a', "https://one.example");
        let op = created(store.create(&owner, request("project"), 100).unwrap());
        let path = dir.path().canonicalize().unwrap().join("recovery.db");
        let test_name = format!(
            "{}::crash_process_child",
            module_path!()
                .split("::")
                .skip(1)
                .collect::<Vec<_>>()
                .join("::")
        );
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &test_name, "--ignored"])
            .env("CREW_OPERATION_CRASH_AT", stage)
            .env("CREW_OPERATION_TEST_DB", &path)
            .env("CREW_OPERATION_TEST_ID", &op.id)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("owned crash fixture exceeded deadline");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(status.code(), Some(77));
        drop(store);
        let recovered = OperationStore::open(&path, Limits::default())
            .unwrap()
            .load(&owner, &op.id)
            .unwrap();
        if stage == "before-commit" {
            assert_eq!(recovered, op);
        } else {
            assert_eq!(recovered.revision, 1);
            assert_eq!(recovered.payload, json!({"step":"after-crash"}));
        }
    }
}

#[test]
fn durable_core_listing_is_bounded_scoped_and_paginates_without_payloads() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let other = scope('a', "https://two.example");
    let mut ids = Vec::new();
    for resource in ["one", "two", "three"] {
        ids.push(created(store.create(&owner, request(resource), 100).unwrap()).id);
    }
    store.create(&other, request("other"), 100).unwrap();
    ids.sort();
    let first = store.list(&owner, None, 2).unwrap();
    assert_eq!(
        first.iter().map(|op| op.id.clone()).collect::<Vec<_>>(),
        ids[..2]
    );
    let second = store.list(&owner, Some(&first[1].id), 2).unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].id, ids[2]);
    assert!(serde_json::to_value(&first).unwrap()[0]
        .get("payload")
        .is_none());
    // The insertion sequence orders native history selection; it is not part
    // of the renderer contract and must never reach the IPC wire.
    assert!(
        serde_json::to_value(&first).unwrap()[0]
            .get("sequence")
            .is_none(),
        "the native ordering sequence stays internal"
    );
    assert!(
        first.iter().all(|summary| summary.sequence > 0),
        "every listed row carries its durable insertion sequence"
    );
    assert_eq!(store.list(&owner, None, 101), Err(StoreError::Invalid));
    assert_eq!(store.list(&owner, None, 0), Err(StoreError::Invalid));
    assert!(store
        .list(&scope('b', "https://one.example"), None, 2)
        .unwrap()
        .is_empty());
}

#[test]
fn durable_core_corrupt_byte_metadata_cannot_bypass_read_limit() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let op = created(store.create(&owner, request("one"), 100).unwrap());
    store
        .connection
        .execute_batch("PRAGMA ignore_check_constraints=ON")
        .unwrap();
    store
        .connection
        .execute("UPDATE operations SET bytes=1 WHERE id=?1", [&op.id])
        .unwrap();
    assert_eq!(store.load(&owner, &op.id), Err(StoreError::Corrupt));
}

#[test]
fn durable_core_create_replays_initial_intent_after_progress_changes() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let initial = request("repo");
    let id = initial.id.clone();
    let payload = initial.payload.clone();
    store.create(&owner, initial, 100).unwrap();
    let progressed = store
        .compare_and_swap(
            &owner,
            &id,
            0,
            OperationUpdate {
                status: OperationStatus::Pending,
                reconciled: false,
                payload: json!({"step":"published-page-one"}),
            },
            101,
        )
        .unwrap();
    let replay = || NewOperation {
        id: id.clone(),
        kind: OperationKind::ProjectChange,
        resource_key: "repo".into(),
        payload: payload.clone(),
    };
    assert_eq!(
        store.create(&owner, replay(), 102).unwrap(),
        CreateResult::Existing(progressed)
    );
    let mut changed = replay();
    changed.payload["step"] = json!("different-intent");
    assert_eq!(
        store.create(&owner, changed, 103),
        Err(StoreError::Conflict)
    );
}

#[test]
fn durable_core_corrupt_metadata_is_bounded_in_load_and_list() {
    for column in ["kind", "status", "resource_key"] {
        let (_dir, mut store) = fixture(Limits::default());
        let owner = scope('a', "https://one.example");
        let op = created(store.create(&owner, request("repo"), 100).unwrap());
        store
            .connection
            .execute(
                &format!("UPDATE operations SET {column}=?1 WHERE id=?2"),
                rusqlite::params!["x".repeat(1024 * 1024), op.id],
            )
            .unwrap();
        assert_eq!(store.load(&owner, &op.id), Err(StoreError::Corrupt));
        assert!(
            matches!(store.list(&owner, None, 10), Err(StoreError::Corrupt)),
            "corrupt metadata must be rejected"
        );
    }
}

#[test]
fn durable_core_corrupt_byte_metadata_cannot_bypass_owner_admission() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let op = created(store.create(&owner, request("first"), 100).unwrap());
    store
        .connection
        .execute_batch("PRAGMA ignore_check_constraints=ON")
        .unwrap();
    store
        .connection
        .execute("UPDATE operations SET bytes=1 WHERE id=?1", [&op.id])
        .unwrap();
    assert!(matches!(
        store.create(&owner, request("second"), 101),
        Err(StoreError::Corrupt)
    ));
}

#[test]
fn durable_core_backward_clock_preserves_monotonic_snapshot() {
    let (_dir, mut store) = fixture(Limits::default());
    let scope = scope('a', "https://relay.example");
    let op = created(store.create(&scope, request("clock"), 100).unwrap());
    let updated = store
        .compare_and_swap(
            &scope,
            &op.id,
            0,
            OperationUpdate {
                status: OperationStatus::Complete,
                reconciled: true,
                payload: op.payload,
            },
            90,
        )
        .unwrap();
    assert_eq!(updated.updated_at, 100);
    assert_eq!(store.load(&scope, &op.id).unwrap(), updated);
}

#[test]
fn durable_core_byte_pressure_evicts_oldest_reconciled_with_incoming_headroom() {
    let (_dir, mut store) = fixture(Limits {
        bytes_per_owner: 5000,
        ..Limits::default()
    });
    let scope = scope('a', "https://relay.example");
    let mut ids = Vec::new();
    for index in 0..3 {
        let mut new = request(&format!("history-{index}"));
        new.payload = json!({"data": "x".repeat(900)});
        let op = created(store.create(&scope, new, 100 + index).unwrap());
        store
            .compare_and_swap(
                &scope,
                &op.id,
                0,
                OperationUpdate {
                    status: OperationStatus::Complete,
                    reconciled: true,
                    payload: op.payload,
                },
                100 + index,
            )
            .unwrap();
        ids.push(op.id);
    }
    // Three legitimate small records nearly fill the scaled owner budget.
    let mut new = request("pending");
    new.payload = json!({"data": "p".repeat(1400)});
    let pending = created(store.create(&scope, new, 104).unwrap());
    assert!(matches!(
        store.load(&scope, &ids[0]),
        Err(StoreError::Missing)
    ));
    assert!(store.load(&scope, &ids[2]).unwrap().reconciled);
    assert!(!store.load(&scope, &pending.id).unwrap().reconciled);
}

#[test]
fn durable_core_constraint_errors_distinguish_conflict_from_corruption() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE fixture (id TEXT PRIMARY KEY, unique_value TEXT UNIQUE, n INTEGER CHECK(n>0)); INSERT INTO fixture VALUES('a','b',1);").unwrap();
    for sql in [
        "INSERT INTO fixture VALUES('a','c',1)",
        "INSERT INTO fixture VALUES('c','b',1)",
    ] {
        assert_eq!(
            super::storage::sql_error(conn.execute(sql, []).unwrap_err()),
            StoreError::Conflict
        );
    }
    assert_ne!(
        super::storage::sql_error(
            conn.execute("INSERT INTO fixture VALUES('c','d',0)", [])
                .unwrap_err()
        ),
        StoreError::Conflict
    );
}

#[path = "wiki_successor_tests.rs"]
mod wiki_successor_tests;
