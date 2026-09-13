use super::*;
use crate::owner_operations::{OperationScope, OperationStatus};

fn operation(payload: &Payload) -> Operation {
    Operation {
        version: 1,
        scope: OperationScope {
            owner: "a".repeat(64),
            community: "https://example.com".into(),
        },
        id: "00000000-0000-0000-0000-000000000001".into(),
        kind: OperationKind::ManagedAgentDelete,
        resource_key: payload.fence.pubkey.clone(),
        revision: 0,
        created_at: 1,
        updated_at: 1,
        status: OperationStatus::Preparing,
        reconciled: false,
        payload: serde_json::to_value(payload).unwrap(),
    }
}

fn payload() -> Payload {
    Payload::new(
        RecordFence {
            pubkey: "b".repeat(64),
            name: "agent".into(),
            created_at: "created".into(),
            relay_url: "wss://relay.example".into(),
            backend_agent_id: None,
        },
        vec!["00000000-0000-0000-0000-000000000002".into()],
        &"b".repeat(64),
    )
    .unwrap()
}

#[test]
fn journal_records_channel_operation_before_local_removal() {
    let payload = payload();
    assert!(!payload.local_removed);
    assert_eq!(payload.channels.len(), 1);
    assert!(!payload.channels[0].operation_id.is_empty());
    validate_operation(&operation(&payload), &payload).unwrap();
}

#[test]
fn journal_rejects_duplicate_channel_and_key_before_local_removal() {
    let mut duplicate_payload = payload();
    duplicate_payload
        .channels
        .push(duplicate_payload.channels[0].clone());
    assert!(validate_operation(&operation(&duplicate_payload), &duplicate_payload).is_err());
    let mut key_payload = payload();
    key_payload.key_removed = true;
    assert!(validate_operation(&operation(&key_payload), &key_payload).is_err());
}

#[test]
fn review_state_never_counts_as_settled() {
    let mut payload = payload();
    payload.channels[0].review_required = true;
    assert!(!payload.all_settled());
}

#[test]
fn tombstone_progress_is_fenced_after_key_cleanup() {
    let mut payload = payload();
    payload.tombstone_enqueued = true;
    assert!(validate_operation(&operation(&payload), &payload).is_err());

    payload.key_removed = true;
    assert!(validate_operation(&operation(&payload), &payload).is_err());

    payload.local_removed = true;
    assert!(validate_operation(&operation(&payload), &payload).is_ok());
}

#[test]
fn older_records_default_tombstone_progress_to_pending() {
    let payload = payload();
    let mut encoded = serde_json::to_value(&payload).unwrap();
    encoded
        .as_object_mut()
        .unwrap()
        .remove("tombstone_enqueued");
    let decoded: Payload = serde_json::from_value(encoded).unwrap();
    assert!(!decoded.tombstone_enqueued);
}

#[test]
fn error_bound_is_utf8_byte_safe() {
    let bounded = bounded_error("é".repeat(MAX_ERROR_BYTES));
    assert!(bounded.len() <= MAX_ERROR_BYTES);
    assert!(std::str::from_utf8(bounded.as_bytes()).is_ok());
}
