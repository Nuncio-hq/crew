use super::super::*;
use serde_json::json;

fn scope(owner: char, community: &str) -> OperationScope {
    OperationScope {
        owner: owner.to_string().repeat(64),
        community: community.to_string(),
    }
}

fn request(pubkey: &str) -> NewOperation {
    NewOperation {
        id: uuid::Uuid::new_v4().to_string(),
        kind: OperationKind::ManagedAgentDelete,
        resource_key: pubkey.to_string(),
        payload: json!({
            "version": 1,
            "fence": {
                "pubkey": pubkey,
                "name": "agent",
                "created_at": "created",
                "relay_url": "wss://relay.example",
                "backend_agent_id": null
            },
            "channels": [],
            "local_removed": false,
            "key_removed": false,
            "tombstone_enqueued": false,
            "failures": 0,
            "last_error": null
        }),
    }
}

fn fixture() -> (tempfile::TempDir, OperationStore) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap().join("recovery.db");
    (dir, OperationStore::open(&path, Limits::default()).unwrap())
}

#[test]
fn unresolved_delete_claim_is_global_across_communities() {
    let (_dir, mut store) = fixture();
    let pubkey = "c".repeat(64);
    let CreateResult::Created(operation) = store
        .create(&scope('a', "https://one.example"), request(&pubkey), 1)
        .unwrap()
    else {
        panic!("expected a new deletion intent");
    };
    assert!(store.managed_agent_delete_is_pending(&pubkey).unwrap());
    assert!(matches!(
        store.create(&scope('b', "https://two.example"), request(&pubkey), 2),
        Err(StoreError::Busy)
    ));

    let mut complete_payload = operation.payload.clone();
    complete_payload["local_removed"] = json!(true);
    complete_payload["key_removed"] = json!(true);
    complete_payload["tombstone_enqueued"] = json!(true);
    let updated = store
        .compare_and_swap(
            &scope('a', "https://one.example"),
            &operation.id,
            operation.revision,
            OperationUpdate {
                status: OperationStatus::Complete,
                reconciled: true,
                payload: complete_payload,
            },
            3,
        )
        .unwrap();
    assert!(updated.reconciled);
    assert!(!store.managed_agent_delete_is_pending(&pubkey).unwrap());
    assert!(matches!(
        store.create(&scope('b', "https://two.example"), request(&pubkey), 4),
        Ok(CreateResult::Created(_))
    ));
}

#[test]
fn malformed_global_claim_fails_closed() {
    let (dir, mut store) = fixture();
    let pubkey = "c".repeat(64);
    store
        .create(&scope('a', "https://one.example"), request(&pubkey), 1)
        .unwrap();
    store
        .connection
        .execute("UPDATE operations SET reconciled=1", [])
        .unwrap();
    assert!(matches!(
        store.managed_agent_delete_is_pending(&pubkey),
        Err(StoreError::Corrupt)
    ));
    drop(store);
    let path = dir.path().canonicalize().unwrap().join("recovery.db");
    let reopened = OperationStore::open(&path, Limits::default()).unwrap();
    assert!(matches!(
        reopened.managed_agent_delete_is_pending(&pubkey),
        Err(StoreError::Corrupt)
    ));
}

#[test]
fn renderer_cannot_forge_terminal_delete_progress() {
    let (_dir, mut store) = fixture();
    let pubkey = "d".repeat(64);
    let scope = scope('a', "https://one.example");
    let CreateResult::Created(operation) = store.create(&scope, request(&pubkey), 1).unwrap()
    else {
        panic!("expected a new deletion intent");
    };
    assert!(matches!(
        store.compare_and_swap(
            &scope,
            &operation.id,
            operation.revision,
            OperationUpdate {
                status: OperationStatus::Complete,
                reconciled: true,
                payload: operation.payload,
            },
            2,
        ),
        Err(StoreError::Invalid)
    ));
    assert!(store.managed_agent_delete_is_pending(&pubkey).unwrap());
}

#[test]
fn global_delete_list_is_redacted_and_load_is_scope_independent() {
    let (_dir, mut store) = fixture();
    let pubkey = "e".repeat(64);
    let community = "https://two.example";
    let scope = scope('b', community);
    let CreateResult::Created(operation) = store.create(&scope, request(&pubkey), 1).unwrap()
    else {
        panic!("expected a new deletion intent");
    };
    let summaries = store.list_managed_agent_deletions().unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].community, community);
    assert_eq!(summaries[0].resource_key, pubkey);
    let loaded = store
        .load_managed_agent_delete_any_scope(&operation.id)
        .unwrap();
    assert_eq!(loaded.scope, scope);
    assert_eq!(loaded.payload, operation.payload);
}
