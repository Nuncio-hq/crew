use super::*;
use serde_json::json;

fn scope(community: &str) -> OperationScope {
    OperationScope {
        owner: "a".repeat(64),
        community: community.into(),
    }
}
fn new(kind: OperationKind, resource: &str, body: String) -> NewOperation {
    let payload = if kind == OperationKind::ManagedAgentDelete {
        json!({
            "version": 1,
            "fence": {
                "pubkey": resource,
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
            "body": body
        })
    } else {
        json!({"body":body})
    };
    NewOperation {
        id: uuid::Uuid::new_v4().to_string(),
        kind,
        resource_key: resource.into(),
        payload,
    }
}
fn created(result: CreateResult) -> Operation {
    match result {
        CreateResult::Created(op) => op,
        _ => panic!("expected new operation"),
    }
}
fn fixture(limits: Limits) -> (tempfile::TempDir, OperationStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = OperationStore::open(
        &dir.path().canonicalize().unwrap().join("recovery.db"),
        limits,
    )
    .unwrap();
    (dir, store)
}

#[test]
fn durable_core_channel_crew_quota_is_100_per_community() {
    let (_dir, mut store) = fixture(Limits::default());
    for community in ["https://one.example", "https://two.example"] {
        let owner = scope(community);
        for index in 0..100 {
            store
                .create(
                    &owner,
                    new(
                        OperationKind::ChannelCrewConfig,
                        &format!("channel-{index}"),
                        String::new(),
                    ),
                    100,
                )
                .unwrap();
        }
        assert!(matches!(
            store.create(
                &owner,
                new(OperationKind::ChannelCrewConfig, "overflow", String::new()),
                100
            ),
            Err(StoreError::Quota)
        ));
        assert_eq!(store.list(&owner, None, 100).unwrap().len(), 100);
    }
}

#[test]
fn durable_core_channel_crew_does_not_consume_or_remove_other_kind_quota() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope("https://one.example");
    store
        .create(
            &owner,
            new(OperationKind::ChannelCrewConfig, "crew", String::new()),
            100,
        )
        .unwrap();
    for index in 0..16 {
        store
            .create(
                &owner,
                new(
                    OperationKind::ProjectChange,
                    &format!("project-{index}"),
                    String::new(),
                ),
                100,
            )
            .unwrap();
    }
    assert!(matches!(
        store.create(
            &owner,
            new(
                OperationKind::WikiPublication,
                "wiki-overflow",
                String::new()
            ),
            100
        ),
        Err(StoreError::Quota)
    ));
    assert!(store
        .create(
            &owner,
            new(
                OperationKind::ChannelCrewConfig,
                "crew-second",
                String::new()
            ),
            100
        )
        .is_ok());
}

#[test]
fn durable_core_channel_crew_complete_record_boundary_create_update_load() {
    const CAP: usize = 1024 * 1024;
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope("https://one.example");
    let template = created(
        store
            .create(
                &owner,
                new(OperationKind::ChannelCrewConfig, "probe", String::new()),
                100,
            )
            .unwrap(),
    );
    let size = serde_json::to_vec(&template).unwrap().len();
    let exact = created(
        store
            .create(
                &owner,
                new(
                    OperationKind::ChannelCrewConfig,
                    "exact",
                    "x".repeat(CAP - size),
                ),
                100,
            )
            .unwrap(),
    );
    assert_eq!(serde_json::to_vec(&exact).unwrap().len(), CAP);
    assert!(store.load(&owner, &exact.id).is_ok());
    assert!(matches!(
        store.create(
            &owner,
            new(
                OperationKind::ChannelCrewConfig,
                "above",
                "x".repeat(CAP - size + 1)
            ),
            100
        ),
        Err(StoreError::Quota)
    ));
    let mut update_shape = exact.clone();
    update_shape.revision = 1;
    update_shape.updated_at = 101;
    update_shape.status = OperationStatus::Pending;
    update_shape.payload = json!({"body":""});
    let overhead = serde_json::to_vec(&update_shape).unwrap().len();
    let update = |bytes| OperationUpdate {
        status: OperationStatus::Pending,
        reconciled: false,
        payload: json!({"body":"x".repeat(bytes)}),
    };
    assert!(matches!(
        store.compare_and_swap(&owner, &exact.id, 0, update(CAP - overhead + 1), 101),
        Err(StoreError::Quota)
    ));
    assert_eq!(store.load(&owner, &exact.id).unwrap().revision, 0);
    let updated = store
        .compare_and_swap(&owner, &exact.id, 0, update(CAP - overhead), 101)
        .unwrap();
    assert_eq!(serde_json::to_vec(&updated).unwrap().len(), CAP);
    let mut corrupt = updated.clone();
    corrupt.payload = json!({"body":"x".repeat(CAP-overhead+1)});
    let raw = serde_json::to_string(&corrupt).unwrap();
    store
        .connection
        .execute(
            "UPDATE operations SET record_json=?1,bytes=?2 WHERE id=?3",
            rusqlite::params![raw, raw.len() as i64, corrupt.id],
        )
        .unwrap();
    assert!(matches!(
        store.load(&owner, &corrupt.id),
        Err(StoreError::Corrupt)
    ));
}

#[test]
fn durable_core_channel_crew_and_other_kinds_share_owner_byte_cap() {
    let (_dir, mut store) = fixture(Limits {
        bytes_per_owner: 3500,
        ..Limits::default()
    });
    let first = scope("https://one.example");
    let second = scope("https://two.example");
    store
        .create(
            &first,
            new(OperationKind::ChannelCrewConfig, "crew", "x".repeat(1000)),
            100,
        )
        .unwrap();
    store
        .create(
            &second,
            new(OperationKind::ProjectChange, "repo", "x".repeat(1000)),
            100,
        )
        .unwrap();
    assert!(matches!(
        store.create(
            &second,
            new(OperationKind::ChannelCrewConfig, "more", "x".repeat(1000)),
            100
        ),
        Err(StoreError::Quota)
    ));
    assert_eq!(store.list(&first, None, 100).unwrap().len(), 1);
    assert_eq!(store.list(&second, None, 100).unwrap().len(), 1);
}

#[test]
fn durable_core_channel_crew_concurrent_admission_has_one_winner() {
    let (dir, mut store) = fixture(Limits::default());
    let owner = scope("https://one.example");
    for index in 0..99 {
        store
            .create(
                &owner,
                new(
                    OperationKind::ChannelCrewConfig,
                    &format!("channel-{index}"),
                    String::new(),
                ),
                100,
            )
            .unwrap();
    }
    let path = dir.path().canonicalize().unwrap().join("recovery.db");
    let second = OperationStore::open(&path, Limits::default()).unwrap();
    let start = std::sync::Arc::new(std::sync::Barrier::new(3));
    let handles = [store, second]
        .into_iter()
        .enumerate()
        .map(|(index, mut connection)| {
            let start = start.clone();
            let owner = owner.clone();
            std::thread::spawn(move || {
                start.wait();
                connection.create(
                    &owner,
                    new(
                        OperationKind::ChannelCrewConfig,
                        &format!("race-{index}"),
                        String::new(),
                    ),
                    100,
                )
            })
        })
        .collect::<Vec<_>>();
    start.wait();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Ok(CreateResult::Created(_))))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(StoreError::Quota)))
            .count(),
        1
    );
    let store = OperationStore::open(&path, Limits::default()).unwrap();
    assert_eq!(store.list(&owner, None, 100).unwrap().len(), 100);
}

#[test]
fn durable_core_other_kinds_keep_larger_record_admission() {
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope("https://one.example");
    let op = created(
        store
            .create(
                &owner,
                new(
                    OperationKind::WikiPublication,
                    "wiki",
                    "x".repeat(1024 * 1024),
                ),
                100,
            )
            .unwrap(),
    );
    assert!(serde_json::to_vec(&op).unwrap().len() > 1024 * 1024);
    assert_eq!(store.load(&owner, &op.id).unwrap(), op);
}

#[test]
fn durable_core_managed_agent_delete_record_is_bounded() {
    const CAP: usize = 64 * 1024;
    let (_dir, mut store) = fixture(Limits::default());
    let owner = scope("https://one.example");
    let template = created(
        store
            .create(
                &owner,
                new(
                    OperationKind::ManagedAgentDelete,
                    &"a".repeat(64),
                    String::new(),
                ),
                100,
            )
            .unwrap(),
    );
    let overhead = serde_json::to_vec(&template).unwrap().len();
    let exact = created(
        store
            .create(
                &owner,
                new(
                    OperationKind::ManagedAgentDelete,
                    &"b".repeat(64),
                    "x".repeat(CAP - overhead),
                ),
                100,
            )
            .unwrap(),
    );
    assert_eq!(serde_json::to_vec(&exact).unwrap().len(), CAP);
    assert!(matches!(
        store.create(
            &owner,
            new(
                OperationKind::ManagedAgentDelete,
                &"c".repeat(64),
                "x".repeat(CAP - overhead + 1),
            ),
            100,
        ),
        Err(StoreError::Quota)
    ));
}
