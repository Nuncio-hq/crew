use super::*;
use crate::channel_crew_config::prepare::{self, Input, Preparation};
use buzz_core_pkg::crew_role::CrewConfigDraft;

#[test]
fn channel_crew_native_revalidates_changed_bindings_not_retained_unresolved_entries() {
    let key = nostr::Keys::generate().public_key().to_hex();
    let old = "```crew\nassignments: {bad-key: Ghost}\ndefinitions: {Review: Inspect}\n```";
    let new=format!("```crew\nassignments: {{bad-key: Ghost, {key}: Review}}\ndefinitions: {{Review: Inspect}}\ncontact: {key}\n```");
    assert_eq!(changed_members(old, &new).unwrap(), vec![key]);
    assert!(changed_members(&new, &new).unwrap().is_empty());
}

#[test]
fn channel_crew_native_latest_uses_canonical_relay_order_at_tied_timestamp() {
    let keys = nostr::Keys::generate();
    let channel = uuid::Uuid::new_v4();
    let mut events = vec!["one", "two"]
        .into_iter()
        .map(|text| {
            crate::events::build_set_canvas(channel, text)
                .unwrap()
                .custom_created_at(nostr::Timestamp::from(2000))
                .sign_with_keys(&keys)
                .unwrap()
        })
        .collect::<Vec<_>>();
    events.sort_by_key(|event| event.id);
    let expected = events[0].id;
    assert_eq!(
        validated_head(events, &channel.to_string())
            .unwrap()
            .unwrap()
            .id,
        expected
    );
}

#[test]
fn channel_crew_native_future_head_does_not_advance_signed_canvas_clock() {
    let keys = nostr::Keys::generate();
    let channel = uuid::Uuid::new_v4().to_string();
    let current = crate::events::build_set_canvas(uuid::Uuid::parse_str(&channel).unwrap(), "old")
        .unwrap()
        .custom_created_at(nostr::Timestamp::from(2000))
        .sign_with_keys(&keys)
        .unwrap();
    let known = BTreeSet::new();
    let id = current.id.to_hex();
    let ready = prepare::prepare(
        Input {
            keys: &keys,
            channel_id: &channel,
            relay_url: "http://fixture.invalid",
            expected_head: Some(&id),
            current: Some(&current),
            known_members: &known,
            now: 1000,
        },
        &CrewConfigDraft::default(),
    )
    .unwrap();
    let Preparation::Ready(payload) = ready else {
        panic!("prepared")
    };
    assert_eq!(payload.canvas.created_at.as_secs(), 1000);
}
