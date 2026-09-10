use super::*;
use serde_json::json;

fn scope(owner: char) -> OperationScope {
    OperationScope {
        owner: owner.to_string().repeat(64),
        community: "https://one.example".into(),
    }
}
fn request(pubkey: &str) -> NewOperation {
    NewOperation {
        id: uuid::Uuid::new_v4().to_string(),
        kind: OperationKind::ManagedAgentDelete,
        resource_key: pubkey.into(),
        payload: json!({"version":1}),
    }
}
fn fixture() -> (tempfile::TempDir, OperationStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = OperationStore::open(
        &dir.path().canonicalize().unwrap().join("recovery.db"),
        Limits::default(),
    )
    .unwrap();
    (dir, store)
}
#[test]
fn delete_claim_is_global_and_only_reconciliation_releases_it() {
    let (_dir, mut store) = fixture();
    let pubkey = "c".repeat(64);
    let CreateResult::Created(mut op) = store.create(&scope('a'), request(&pubkey), 1).unwrap()
    else {
        panic!("new intent")
    };
    for status in [
        OperationStatus::Failed,
        OperationStatus::Canceled,
        OperationStatus::Superseded,
    ] {
        op = store
            .compare_and_swap(
                &scope('a'),
                &op.id,
                op.revision,
                OperationUpdate {
                    status,
                    reconciled: false,
                    payload: op.payload.clone(),
                },
                2,
            )
            .unwrap();
        assert!(store.managed_agent_delete_is_pending(&pubkey).unwrap());
        assert!(matches!(
            store.create(&scope('b'), request(&pubkey), 3),
            Err(StoreError::Busy)
        ));
    }
    store
        .compare_and_swap(
            &scope('a'),
            &op.id,
            op.revision,
            OperationUpdate {
                status: OperationStatus::Complete,
                reconciled: true,
                payload: op.payload,
            },
            4,
        )
        .unwrap();
    assert!(!store.managed_agent_delete_is_pending(&pubkey).unwrap());
    assert!(matches!(
        store.create(&scope('b'), request(&pubkey), 5),
        Ok(CreateResult::Created(_))
    ));
}
#[test]
fn corrupt_sql_release_bit_does_not_release_delete_claim() {
    let (_dir, mut store) = fixture();
    let pubkey = "c".repeat(64);
    store.create(&scope('a'), request(&pubkey), 1).unwrap();
    store
        .connection
        .execute("UPDATE operations SET reconciled=1", [])
        .unwrap();
    assert!(matches!(
        store.managed_agent_delete_is_pending(&pubkey),
        Err(StoreError::Corrupt)
    ));
    assert!(matches!(
        store.create(&scope('b'), request(&pubkey), 2),
        Err(StoreError::Corrupt)
    ));
}
#[test]
fn v1_database_migrates_without_losing_existing_records() {
    let (dir, mut store) = fixture();
    let mut other = request("repository");
    other.kind = OperationKind::ProjectChange;
    let id = other.id.clone();
    store.create(&scope('a'), other, 1).unwrap();
    store
        .connection
        .execute_batch("DROP INDEX unresolved_managed_agent_delete; PRAGMA user_version=1;")
        .unwrap();
    drop(store);
    let store = OperationStore::open(
        &dir.path().canonicalize().unwrap().join("recovery.db"),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        store.load(&scope('a'), &id).unwrap().resource_key,
        "repository"
    );
    assert_eq!(
        store
            .connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert!(!store
        .managed_agent_delete_is_pending(&"c".repeat(64))
        .unwrap());
}
#[test]
fn missing_v2_claim_index_fails_closed_on_reopen() {
    let (dir, store) = fixture();
    store
        .connection
        .execute_batch("DROP INDEX unresolved_managed_agent_delete;")
        .unwrap();
    drop(store);
    assert!(matches!(
        OperationStore::open(
            &dir.path().canonicalize().unwrap().join("recovery.db"),
            Limits::default()
        ),
        Err(StoreError::Corrupt)
    ));
}
#[test]
fn delete_record_limit_and_canonical_pubkey_are_native() {
    let (_dir, mut store) = fixture();
    assert!(matches!(
        store.create(&scope('a'), request(&"C".repeat(64)), 1),
        Err(StoreError::Invalid)
    ));
    let mut oversized = request(&"c".repeat(64));
    oversized.payload = json!({"body":"x".repeat(65536)});
    assert!(matches!(
        store.create(&scope('a'), oversized, 1),
        Err(StoreError::Quota)
    ));
}
