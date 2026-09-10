use super::*;

fn wiki_payload(coordinate: &str, page: &str) -> serde_json::Value {
    json!({
        "version": 1,
        "coordinate": coordinate,
        "snapshot_id": format!("snapshot-{page}"),
        "source_revision": format!("source-{page}"),
        "expected_revision": "absent",
        "head": {"id": format!("head-{page}"), "content": "exact-head"},
        "manifest": {"id": format!("manifest-{page}"), "content": "exact-manifest"},
        "pages": [{"id": format!("page-{page}"), "content": "exact-page"}],
        "cadence": "manual",
        "progress": {"phase": "pages", "confirmed": 1},
        "head_attempted": false,
        "reconciliation": null,
        "reconcile_only": false,
        "retry_at": 0,
        "last_error": null,
        "lease": null
    })
}

fn wiki_request(id: &str, resource: &str, payload: serde_json::Value) -> NewOperation {
    NewOperation {
        id: id.into(),
        kind: OperationKind::WikiPublication,
        resource_key: resource.into(),
        payload,
    }
}

fn superseded_payload(mut payload: serde_json::Value) -> serde_json::Value {
    let object = payload.as_object_mut().expect("Wiki payload object");
    object.insert(
        "reconciliation".into(),
        json!({"proof":"superseded", "current_head_id": "b"}),
    );
    object.insert("reconcile_only".into(), json!(true));
    object.insert("retry_at".into(), json!(0));
    object.insert("last_error".into(), json!(null));
    object.insert("lease".into(), json!(null));
    payload
}

fn wiki_id(number: u128) -> String {
    Uuid::from_u128(number).hyphenated().to_string()
}

fn wiki_intent(id: &str, resource: &str, page: &str) -> NewOperation {
    wiki_request(id, resource, wiki_payload(resource, page))
}

fn supersede(payload: serde_json::Value) -> OperationUpdate {
    OperationUpdate {
        status: OperationStatus::Superseded,
        reconciled: true,
        payload: superseded_payload(payload),
    }
}

#[test]
fn durable_core_v1_migration_preserves_scoped_rows_and_is_reopenable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap().join("recovery.db");
    let owner = scope('a', "https://one.example");
    let first = Operation {
        version: 1,
        scope: owner.clone(),
        id: wiki_id(1),
        kind: OperationKind::WikiPublication,
        resource_key: "repo/wiki".into(),
        revision: 7,
        created_at: 11,
        updated_at: 19,
        status: OperationStatus::Reconciling,
        reconciled: false,
        payload: wiki_payload("repo/wiki", "migration"),
    };
    let operations = vec![
        first,
        Operation {
            version: 1,
            scope: scope('b', "https://one.example"),
            id: wiki_id(2),
            kind: OperationKind::ProjectChange,
            resource_key: "project/one".into(),
            revision: 3,
            created_at: 21,
            updated_at: 22,
            status: OperationStatus::Pending,
            reconciled: false,
            payload: json!({"kind":"project"}),
        },
        Operation {
            version: 1,
            scope: scope('a', "https://two.example"),
            id: wiki_id(3),
            kind: OperationKind::ThreadHandoff,
            resource_key: "thread/one".into(),
            revision: 1,
            created_at: 31,
            updated_at: 32,
            status: OperationStatus::Complete,
            reconciled: true,
            payload: json!({"kind":"thread"}),
        },
        Operation {
            version: 1,
            scope: scope('b', "https://two.example"),
            id: wiki_id(4),
            kind: OperationKind::ChannelCrewConfig,
            resource_key: "channel/one".into(),
            revision: 2,
            created_at: 41,
            updated_at: 42,
            status: OperationStatus::Canceled,
            reconciled: true,
            payload: json!({"kind":"crew"}),
        },
    ];
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(include_str!("schema.sql"))
        .unwrap();
    for operation in &operations {
        let record_json = serde_json::to_string(operation).unwrap();
        connection
            .execute(
                "INSERT INTO operations(owner,community,id,kind,resource_key,revision,created_at,updated_at,status,reconciled,record_json,bytes,initial_digest) \
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                rusqlite::params![
                    &operation.scope.owner,
                    &operation.scope.community,
                    &operation.id,
                    super::storage::kind_key(operation.kind),
                    &operation.resource_key,
                    operation.revision,
                    operation.created_at,
                    operation.updated_at,
                    serde_json::to_string(&operation.status).unwrap(),
                    operation.reconciled,
                    &record_json,
                    record_json.len() as i64,
                    vec![7u8; 32],
                ],
            )
            .unwrap();
    }
    drop(connection);

    let store = OperationStore::open(&path, Limits::default()).unwrap();
    assert_eq!(
        store
            .connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        2
    );
    for operation in &operations {
        assert_eq!(
            store.load(&operation.scope, &operation.id).unwrap(),
            operation.clone()
        );
    }
    assert_eq!(
        store
            .connection
            .query_row(
                "SELECT count(*) FROM wiki_publication_successors",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    drop(store);
    let reopened = OperationStore::open(&path, Limits::default()).unwrap();
    for operation in &operations {
        assert_eq!(
            reopened.load(&operation.scope, &operation.id).unwrap(),
            operation.clone()
        );
    }
}

#[test]
fn durable_core_wiki_successor_is_atomic_and_idempotent_after_lost_ipc() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let resource = "repo/wiki";
    let old = created(
        store
            .create(&owner, wiki_intent(&wiki_id(10), resource, "a"), 100)
            .unwrap(),
    );
    let successor_intent = wiki_intent(&wiki_id(11), resource, "b");
    let retirement = supersede(old.payload.clone());
    let result = store
        .replace_wiki_with_successor(
            &owner,
            &old.id,
            old.revision,
            retirement,
            successor_intent,
            101,
        )
        .unwrap();
    assert_eq!(result.predecessor.id, old.id);
    assert_eq!(result.predecessor.revision, 1);
    assert!(result.predecessor.reconciled);
    assert_eq!(result.predecessor.status, OperationStatus::Superseded);
    assert_eq!(result.successor.id, wiki_id(11));
    assert_eq!(result.successor.revision, 0);
    assert!(!result.successor.reconciled);
    assert_eq!(store.load(&owner, &old.id).unwrap(), result.predecessor);
    assert_eq!(
        store.load(&owner, &result.successor.id).unwrap(),
        result.successor
    );
    assert_eq!(
        store
            .connection
            .query_row(
                "SELECT count(*) FROM wiki_publication_successors WHERE owner=?1 AND community=?2",
                rusqlite::params![owner.owner, owner.community],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );

    // The caller can safely retry the exact successor intent after losing the
    // first IPC response, even though the predecessor revision advanced.
    let retry = store
        .replace_wiki_with_successor(
            &owner,
            &old.id,
            old.revision,
            supersede(old.payload.clone()),
            wiki_intent(&wiki_id(11), resource, "b"),
            102,
        )
        .unwrap();
    assert_eq!(retry, result);

    // A fast worker may reconcile the child before the caller retries. The
    // relation remains recoverable even though the resolved child no longer
    // contributes an active predecessor pin.
    let completed = store
        .compare_and_swap(
            &owner,
            &result.successor.id,
            result.successor.revision,
            OperationUpdate {
                status: OperationStatus::Complete,
                reconciled: true,
                payload: result.successor.payload.clone(),
            },
            103,
        )
        .unwrap();
    let retry_after_completion = store
        .replace_wiki_with_successor(
            &owner,
            &old.id,
            old.revision,
            supersede(old.payload.clone()),
            wiki_intent(&wiki_id(11), resource, "b"),
            104,
        )
        .unwrap();
    assert_eq!(retry_after_completion.predecessor, result.predecessor);
    assert_eq!(retry_after_completion.successor, completed);
}

#[test]
fn durable_core_wiki_successor_rejects_signed_graph_or_progress_changes() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let resource = "repo/wiki";
    let old = created(
        store
            .create(&owner, wiki_intent(&wiki_id(20), resource, "a"), 100)
            .unwrap(),
    );

    for changed in [
        json!({"coordinate": "other-repo"}),
        json!({"head": {"id": "changed", "content": "exact-head"}}),
        json!({"progress": {"phase": "pages", "confirmed": 99}}),
        json!({"head_attempted": true}),
    ] {
        let mut payload = old.payload.clone();
        let object = payload.as_object_mut().unwrap();
        for (key, value) in changed.as_object().unwrap() {
            object.insert(key.clone(), value.clone());
        }
        let error = store.replace_wiki_with_successor(
            &owner,
            &old.id,
            old.revision,
            supersede(payload),
            wiki_intent(&wiki_id(21), resource, "b"),
            101,
        );
        assert_eq!(error, Err(StoreError::Invalid));
        assert_eq!(store.load(&owner, &old.id).unwrap(), old);
        assert_eq!(store.load(&owner, &wiki_id(21)), Err(StoreError::Missing));
    }

    let mut no_proof = old.payload.clone();
    no_proof["reconciliation"] = json!({"current_head_id": "b"});
    assert_eq!(
        store.replace_wiki_with_successor(
            &owner,
            &old.id,
            old.revision,
            OperationUpdate {
                status: OperationStatus::Superseded,
                reconciled: true,
                payload: no_proof,
            },
            wiki_intent(&wiki_id(21), resource, "b"),
            101,
        ),
        Err(StoreError::Invalid)
    );
}

#[test]
fn durable_core_wiki_successor_is_direct_only_and_respects_pins() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let resource = "repo/wiki";
    let a = created(
        store
            .create(&owner, wiki_intent(&wiki_id(30), resource, "a"), 100)
            .unwrap(),
    );
    let ab = store
        .replace_wiki_with_successor(
            &owner,
            &a.id,
            a.revision,
            supersede(a.payload.clone()),
            wiki_intent(&wiki_id(31), resource, "b"),
            101,
        )
        .unwrap();
    assert_eq!(
        store.remove_reconciled(&owner, &a.id, ab.predecessor.revision),
        Err(StoreError::Pinned)
    );

    let bc = store
        .replace_wiki_with_successor(
            &owner,
            &ab.successor.id,
            ab.successor.revision,
            supersede(ab.successor.payload.clone()),
            wiki_intent(&wiki_id(32), resource, "c"),
            102,
        )
        .unwrap();
    let links: Vec<(String, String)> = store
        .connection
        .prepare(
            "SELECT predecessor_id, successor_id FROM wiki_publication_successors \
             WHERE owner=?1 AND community=?2 ORDER BY predecessor_id",
        )
        .unwrap()
        .query_map(rusqlite::params![owner.owner, owner.community], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        links,
        vec![
            (ab.predecessor.id.clone(), ab.successor.id.clone()),
            (ab.successor.id.clone(), bc.successor.id.clone()),
        ]
    );
    assert_eq!(
        store.remove_reconciled(&owner, &a.id, ab.predecessor.revision),
        Ok(())
    );
    assert_eq!(store.load(&owner, &a.id), Err(StoreError::Missing));
    assert_eq!(
        store.remove_reconciled(&owner, &ab.successor.id, bc.predecessor.revision),
        Err(StoreError::Pinned)
    );
}

#[test]
fn durable_core_wiki_successor_rolls_back_when_terminal_cap_is_below_pins() {
    let (_dir, mut store) = fixture(Limits {
        terminal_per_owner: 1,
        ..Limits::default()
    });
    let owner = scope('a', "https://one.example");
    let first = created(
        store
            .create(&owner, wiki_intent(&wiki_id(40), "repo/one", "a"), 100)
            .unwrap(),
    );
    let first_result = store
        .replace_wiki_with_successor(
            &owner,
            &first.id,
            first.revision,
            supersede(first.payload.clone()),
            wiki_intent(&wiki_id(41), "repo/one", "b"),
            101,
        )
        .unwrap();
    let second = created(
        store
            .create(&owner, wiki_intent(&wiki_id(42), "repo/two", "a"), 102)
            .unwrap(),
    );
    assert_eq!(
        store.replace_wiki_with_successor(
            &owner,
            &second.id,
            second.revision,
            supersede(second.payload.clone()),
            wiki_intent(&wiki_id(43), "repo/two", "b"),
            103,
        ),
        Err(StoreError::Quota)
    );
    assert_eq!(store.load(&owner, &second.id).unwrap(), second);
    assert_eq!(store.load(&owner, &wiki_id(43)), Err(StoreError::Missing));
    assert_eq!(
        store.load(&owner, &first.id).unwrap(),
        first_result.predecessor
    );
    assert_eq!(
        store.load(&owner, &first_result.successor.id).unwrap(),
        first_result.successor
    );
    assert_eq!(
        store
            .connection
            .query_row(
                "SELECT count(*) FROM wiki_publication_successors WHERE owner=?1 AND community=?2",
                rusqlite::params![owner.owner, owner.community],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn durable_core_wiki_successor_caps_active_pins_at_sixteen() {
    let (_dir, mut store) = fixture(Limits {
        pending_per_owner: 32,
        ..Limits::default()
    });
    let owner = scope('a', "https://one.example");
    let mut last_old = None;
    for index in 0..17u128 {
        let old = created(
            store
                .create(
                    &owner,
                    wiki_intent(&wiki_id(60 + index * 2), &format!("repo/{index}"), "a"),
                    100 + index as i64,
                )
                .unwrap(),
        );
        let result = store.replace_wiki_with_successor(
            &owner,
            &old.id,
            old.revision,
            supersede(old.payload.clone()),
            wiki_intent(&wiki_id(61 + index * 2), &format!("repo/{index}"), "b"),
            101 + index as i64,
        );
        if index < 16 {
            assert!(result.is_ok(), "pin {} should be admitted", index + 1);
        } else {
            assert_eq!(result, Err(StoreError::Quota));
            last_old = Some(old);
        }
    }
    let old = last_old.unwrap();
    assert_eq!(store.load(&owner, &old.id).unwrap(), old);
    assert_eq!(store.load(&owner, &wiki_id(93)), Err(StoreError::Missing));
    assert_eq!(
        store
            .connection
            .query_row(
                "SELECT count(*) FROM wiki_publication_successors link \
                 JOIN operations successor ON successor.owner=link.owner \
                   AND successor.community=link.community AND successor.id=link.successor_id \
                 WHERE link.owner=?1 AND successor.reconciled=0",
                rusqlite::params![owner.owner],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        16
    );
}

#[test]
fn durable_core_wiki_successor_rejects_cross_scope_and_stale_or_conflicting_intents() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope('a', "https://one.example");
    let old = created(
        store
            .create(&owner, wiki_intent(&wiki_id(50), "repo/wiki", "a"), 100)
            .unwrap(),
    );
    let wrong_scope = scope('b', "https://one.example");
    assert_eq!(
        store.replace_wiki_with_successor(
            &wrong_scope,
            &old.id,
            old.revision,
            supersede(old.payload.clone()),
            wiki_intent(&wiki_id(51), "repo/wiki", "b"),
            101,
        ),
        Err(StoreError::Missing)
    );
    assert_eq!(
        store.replace_wiki_with_successor(
            &owner,
            &old.id,
            1,
            supersede(old.payload.clone()),
            wiki_intent(&wiki_id(51), "repo/wiki", "b"),
            101,
        ),
        Err(StoreError::Conflict)
    );
    assert_eq!(
        store.replace_wiki_with_successor(
            &owner,
            &old.id,
            old.revision,
            supersede(old.payload.clone()),
            wiki_intent(&wiki_id(51), "other/wiki", "b"),
            101,
        ),
        Err(StoreError::Invalid)
    );

    let existing = created(
        store
            .create(&owner, wiki_intent(&wiki_id(52), "repo/other", "x"), 102)
            .unwrap(),
    );
    assert_eq!(
        store.replace_wiki_with_successor(
            &owner,
            &old.id,
            old.revision,
            supersede(old.payload.clone()),
            wiki_intent(&existing.id, "repo/wiki", "b"),
            103,
        ),
        Err(StoreError::Conflict)
    );
    assert_eq!(store.load(&owner, &old.id).unwrap(), old);
    assert_eq!(store.load(&owner, &existing.id).unwrap(), existing);
}

#[path = "wiki_schema_tests.rs"]
mod wiki_schema_tests;
