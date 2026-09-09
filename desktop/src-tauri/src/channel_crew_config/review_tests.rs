use super::{validate, Payload};
use crate::channel_crew_config::{
    prepare::{prepare_cleanup, Input, Preparation},
    tests::Fixture,
};

#[test]
fn channel_crew_review_record_rejects_noncanonical_and_inconsistent_state() {
    let (_fixture, operation) = Fixture::new();
    for (name, field, value) in [
        (
            "same-origin path",
            "relay_url",
            serde_json::json!("http://fixture.invalid/path"),
        ),
        ("false applied", "outcome", serde_json::json!("applied")),
        (
            "notice before canvas",
            "announcement_acknowledged",
            serde_json::json!(true),
        ),
        (
            "noncanonical head",
            "expected_head",
            serde_json::json!("ABCDEF0123456789".repeat(4)),
        ),
        (
            "noncanonical lease",
            "lease",
            serde_json::json!({"worker":"ABCDEF01-2345-6789-ABCD-EF0123456789","expires_at":1060}),
        ),
    ] {
        let mut value_json = operation.payload.clone();
        value_json[field] = value;
        let payload: Payload = serde_json::from_value(value_json).unwrap();
        assert!(validate(&operation, &payload).is_err(), "{name}");
    }
}

#[test]
fn channel_crew_review_foreign_cleanup_without_matching_member_is_unchanged() {
    let keys = nostr::Keys::generate();
    let channel = uuid::Uuid::new_v4().to_string();
    let foreign = crate::events::build_set_canvas(
        uuid::Uuid::parse_str(&channel).unwrap(),
        "human prose\n```crew\ndefinitions: {Review: Inspect}\n```",
    )
    .unwrap()
    .sign_with_keys(&nostr::Keys::generate())
    .unwrap();
    let id = foreign.id.to_hex();
    let known = Default::default();
    let result = prepare_cleanup(
        Input {
            keys: &keys,
            channel_id: &channel,
            relay_url: "http://fixture.invalid",
            expected_head: Some(&id),
            current: Some(&foreign),
            known_members: &known,
            now: 1000,
        },
        &[keys.public_key().to_hex()],
    )
    .unwrap();
    assert!(
        !matches!(
            result,
            Preparation::Ready(_) | Preparation::ReviewRequired(_) | Preparation::Conflict(_)
        ),
        "unrelated foreign canvas requires no cleanup write"
    );
}
