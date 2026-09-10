use super::*;
use nostr::Kind;
fn seed(path: &Path, owner: &Keys, agent: &str) -> Event {
    let event = EventBuilder::new(Kind::Custom(KIND_MANAGED_AGENT as u16), "{}")
        .tags([Tag::parse(["d", agent]).unwrap()])
        .sign_with_keys(owner)
        .unwrap();
    let conn = open_retention_db(path).unwrap();
    retain_event(
        &conn,
        &RetainedEvent {
            kind: KIND_MANAGED_AGENT,
            pubkey: owner.public_key().to_hex(),
            d_tag: agent.into(),
            content: event.content.clone(),
            created_at: event.created_at.as_secs() as i64,
            raw_event: event.as_json(),
            pending_sync: false,
        },
    )
    .unwrap();
    event
}
#[test]
fn committed_pair_replays_exact_ids_after_outer_receipt_is_lost() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("retention.db");
    let owner = Keys::generate();
    let agent = Keys::generate().public_key().to_hex();
    let head = seed(&path, &owner, &agent);
    let id = uuid::Uuid::new_v4().to_string();
    let first =
        enqueue_agent_offboarding_at(&path, &owner, &agent, &id, Some(&head.id.to_hex())).unwrap();
    let replay =
        enqueue_agent_offboarding_at(&path, &owner, &agent, &id, Some(&head.id.to_hex())).unwrap();
    assert_eq!(first, replay);
    let conn = open_retention_db(&path).unwrap();
    assert!(get_retained_event(
        &conn,
        KIND_MANAGED_AGENT,
        &owner.public_key().to_hex(),
        &agent
    )
    .unwrap()
    .is_none());
    assert!(
        get_retained_event(
            &conn,
            KIND_IA_ARCHIVE_REQUEST,
            &owner.public_key().to_hex(),
            &agent
        )
        .unwrap()
        .unwrap()
        .pending_sync
    );
}
#[test]
fn second_event_failure_rolls_back_head_purge_and_first_event() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("retention.db");
    let owner = Keys::generate();
    let agent = Keys::generate().public_key().to_hex();
    let head = seed(&path, &owner, &agent);
    let conn = open_retention_db(&path).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_archive BEFORE INSERT ON persona_events WHEN NEW.kind=9035 BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    assert!(
        enqueue_agent_offboarding_at(&path, &owner, &agent, &id, Some(&head.id.to_hex())).is_err()
    );
    let key = owner.public_key().to_hex();
    assert!(get_retained_event(&conn, KIND_MANAGED_AGENT, &key, &agent)
        .unwrap()
        .is_some());
    assert!(get_retained_event(
        &conn,
        5,
        &key,
        &tombstone_retention_d_tag(KIND_MANAGED_AGENT, &agent)
    )
    .unwrap()
    .is_none());
}
#[test]
fn changed_head_cannot_be_purged_by_previous_delete_intent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("retention.db");
    let owner = Keys::generate();
    let agent = Keys::generate().public_key().to_hex();
    seed(&path, &owner, &agent);
    assert!(enqueue_agent_offboarding_at(
        &path,
        &owner,
        &agent,
        &uuid::Uuid::new_v4().to_string(),
        None
    )
    .is_err());
    let conn = open_retention_db(&path).unwrap();
    assert!(get_retained_event(
        &conn,
        KIND_MANAGED_AGENT,
        &owner.public_key().to_hex(),
        &agent
    )
    .unwrap()
    .is_some());
}
#[test]
fn another_intent_cannot_replace_unacknowledged_pair() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("retention.db");
    let owner = Keys::generate();
    let agent = Keys::generate().public_key().to_hex();
    let first = enqueue_agent_offboarding_at(
        &path,
        &owner,
        &agent,
        &uuid::Uuid::new_v4().to_string(),
        None,
    )
    .unwrap();
    assert!(enqueue_agent_offboarding_at(
        &path,
        &owner,
        &agent,
        &uuid::Uuid::new_v4().to_string(),
        None
    )
    .is_err());
    let conn = open_retention_db(&path).unwrap();
    let row = get_retained_event(
        &conn,
        KIND_IA_ARCHIVE_REQUEST,
        &owner.public_key().to_hex(),
        &agent,
    )
    .unwrap()
    .unwrap();
    assert_eq!(event(&row).unwrap().id.to_hex(), first.archive_id);
}
