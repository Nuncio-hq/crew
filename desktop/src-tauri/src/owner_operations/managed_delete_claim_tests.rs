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

fn cascade_fence(pubkey: &str) -> serde_json::Value {
    json!({
        "pubkey": pubkey,
        "name": "agent",
        "created_at": "created",
        "relay_url": "wss://relay.example",
        "backend_agent_id": null
    })
}

fn cascade_child_payload(parent_id: &str, persona_id: &str, pubkey: &str) -> serde_json::Value {
    json!({
        "version": 1,
        "fence": cascade_fence(pubkey),
        "channels": [],
        "local_removed": false,
        "key_removed": false,
        "tombstone_enqueued": false,
        "failures": 0,
        "last_error": null,
        "cascade_parent": parent_id,
        "cascade_persona_id": persona_id,
        "cascade": null
    })
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

#[test]
fn cascade_coordinator_claim_covers_every_prepared_target() {
    let (_dir, mut store) = fixture();
    let first = "b".repeat(64);
    let second = "c".repeat(64);
    let parent_id = uuid::Uuid::new_v4().to_string();
    let first_child_id = uuid::Uuid::new_v4().to_string();
    let second_child_id = uuid::Uuid::new_v4().to_string();
    let target = |pubkey: &str, operation_id: &str| {
        json!({
            "operation_id": operation_id,
            "persona_id": "persona-1",
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
            "last_error": null,
            "settled": false
        })
    };
    let payload = json!({
        "version": 1,
        "fence": {
            "pubkey": first.clone(),
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
        "last_error": null,
        "cascade": {
            "persona": {
                "id": "persona-1",
                "d_tag": "persona-1",
                "created_at": "created",
                "updated_at": "updated"
            },
            "targets": [
                target(&first, &first_child_id),
                target(&second, &second_child_id)
            ],
            "persona_removed": false
        }
    });
    let operation = NewOperation {
        id: parent_id,
        kind: OperationKind::ManagedAgentDelete,
        resource_key: first,
        payload,
    };
    store
        .create(&scope('a', "https://one.example"), operation, 1)
        .unwrap();

    // Before the cascade-aware claim parser this checked only the parent's
    // top-level resource key, so the second target could start concurrently.
    assert!(store.managed_agent_delete_is_pending(&second).unwrap());
}

#[test]
fn cascade_coordinator_and_first_child_may_share_the_managed_key() {
    let (_dir, mut store) = fixture();
    let owner = scope('a', "https://one.example");
    let pubkey = "f".repeat(64);
    let persona_id = "persona-same-key";
    let parent_id = uuid::Uuid::new_v4().to_string();
    let child_id = uuid::Uuid::new_v4().to_string();
    let parent_payload = json!({
        "version": 1,
        "fence": cascade_fence(&pubkey),
        "channels": [],
        "local_removed": false,
        "key_removed": false,
        "tombstone_enqueued": false,
        "failures": 0,
        "last_error": null,
        "cascade": {
            "persona": {
                "id": persona_id,
                "d_tag": persona_id,
                "created_at": "created",
                "updated_at": "updated"
            },
            "targets": [{
                "operation_id": child_id,
                "persona_id": persona_id,
                "fence": cascade_fence(&pubkey),
                "channels": [],
                "local_removed": false,
                "key_removed": false,
                "tombstone_enqueued": false,
                "failures": 0,
                "last_error": null,
                "settled": false
            }],
            "persona_removed": false
        }
    });
    let child_payload = cascade_child_payload(&parent_id, persona_id, &pubkey);
    let operations = store
        .create_managed_agent_delete_batch(
            &owner,
            vec![
                NewOperation {
                    id: parent_id.clone(),
                    kind: OperationKind::ManagedAgentDelete,
                    resource_key: pubkey.clone(),
                    payload: parent_payload,
                },
                NewOperation {
                    id: child_id,
                    kind: OperationKind::ManagedAgentDelete,
                    resource_key: pubkey.clone(),
                    payload: child_payload,
                },
            ],
            1,
        )
        .expect("the coordinator and its first child are one atomic prepared set");
    assert_eq!(operations.len(), 2);
    assert!(store.managed_agent_delete_is_pending(&pubkey).unwrap());
    assert_eq!(store.list_managed_agent_deletions().unwrap().len(), 1);
    assert!(matches!(
        store.create(&owner, request(&pubkey), 2),
        Err(StoreError::Conflict)
    ));
}

#[test]
fn zero_target_persona_coordinator_keeps_tombstone_retry_claim() {
    let (_dir, mut store) = fixture();
    let owner = scope('a', "https://one.example");
    let persona_id = "persona-without-agents";
    let resource_key = persona_cascade_coordinator_resource_key(persona_id);
    let operation = NewOperation {
        id: uuid::Uuid::new_v5(
            &crate::owner_operations::PERSONA_CASCADE_NAMESPACE,
            format!("persona:{persona_id}").as_bytes(),
        )
        .to_string(),
        kind: OperationKind::ManagedAgentDelete,
        resource_key: resource_key.clone(),
        payload: json!({
            "version": 1,
            "fence": {
                "pubkey": resource_key,
                "name": "persona-cascade",
                "created_at": "created",
                "relay_url": "wss://persona-coordinator.invalid",
                "backend_agent_id": null
            },
            "channels": [],
            "local_removed": false,
            "key_removed": false,
            "tombstone_enqueued": false,
            "failures": 0,
            "last_error": null,
            "cascade": {
                "persona": {
                    "id": persona_id,
                    "d_tag": persona_id,
                    "created_at": "created",
                    "updated_at": "updated"
                },
                "targets": [],
                "coordinator_only": true,
                "persona_removed": false
            }
        }),
    };
    let operation = match store.create(&owner, operation, 1).unwrap() {
        CreateResult::Created(operation) => operation,
        other => panic!("expected a new coordinator: {other:?}"),
    };
    assert!(store
        .managed_agent_delete_is_pending(&operation.resource_key)
        .unwrap());
    let mut payload = operation.payload;
    payload["local_removed"] = json!(true);
    payload["key_removed"] = json!(true);
    payload["tombstone_enqueued"] = json!(true);
    payload["cascade"]["persona_removed"] = json!(true);
    let completed = store
        .compare_and_swap(
            &owner,
            &operation.id,
            operation.revision,
            OperationUpdate {
                status: OperationStatus::Complete,
                reconciled: true,
                payload,
            },
            2,
        )
        .unwrap();
    assert!(completed.reconciled);
    assert!(!store
        .managed_agent_delete_is_pending(&completed.resource_key)
        .unwrap());
}

#[test]
fn cascade_coordinator_cannot_claim_terminal_before_persona_removal() {
    let (_dir, mut store) = fixture();
    let first = "b".repeat(64);
    let parent_id = uuid::Uuid::new_v4().to_string();
    let child_id = uuid::Uuid::new_v4().to_string();
    let payload = json!({
        "version": 1,
        "fence": {
            "pubkey": first.clone(),
            "name": "agent",
            "created_at": "created",
            "relay_url": "wss://relay.example",
            "backend_agent_id": null
        },
        "channels": [],
        "local_removed": true,
        "key_removed": true,
        "tombstone_enqueued": true,
        "failures": 0,
        "last_error": null,
        "cascade": {
            "persona": {
                "id": "persona-1",
                "d_tag": "persona-1",
                "created_at": "created",
                "updated_at": "updated"
            },
            "targets": [{
                "operation_id": child_id,
                "persona_id": "persona-1",
                "fence": {
                    "pubkey": first.clone(),
                    "name": "agent",
                    "created_at": "created",
                    "relay_url": "wss://relay.example",
                    "backend_agent_id": null
                },
                "channels": [],
                "local_removed": true,
                "key_removed": true,
                "tombstone_enqueued": true,
                "failures": 0,
                "last_error": null,
                "settled": true
            }],
            "persona_removed": false
        }
    });
    let owner = scope('a', "https://one.example");
    let operation = match store
        .create(
            &owner,
            NewOperation {
                id: parent_id.clone(),
                kind: OperationKind::ManagedAgentDelete,
                resource_key: first,
                payload,
            },
            1,
        )
        .unwrap()
    {
        CreateResult::Created(operation) => operation,
        other => panic!("expected a new cascade coordinator: {other:?}"),
    };
    assert!(matches!(
        store.compare_and_swap(
            &operation.scope,
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
}
