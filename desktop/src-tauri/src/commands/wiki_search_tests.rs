use super::*;
use nostr::{EventBuilder, Keys, Kind, SecretKey, Tag};

fn keys(seed: u8) -> Keys {
    let mut bytes = [0_u8; 32];
    bytes[31] = seed;
    Keys::new(SecretKey::from_slice(&bytes).expect("test key"))
}

fn tag(name: &str, value: impl Into<String>) -> Tag {
    Tag::parse(vec![name.to_owned(), value.into()]).expect("test tag")
}

#[test]
fn search_event_guard_binds_kind_owner_coordinate_and_generation() {
    let owner_keys = keys(8);
    let owner = owner_keys.public_key().to_hex();
    let snapshot_id = "12345678-1234-4234-9234-123456789abc";
    let coordinate = format!("30617:{owner}:repo");
    let event = EventBuilder::new(Kind::Custom(WIKI_EVENT_KIND), "needle")
        .tags(vec![
            tag("d", "repo/intro"),
            tag("a", coordinate.clone()),
            tag("wiki-version", "1"),
            tag("wiki-snapshot", snapshot_id),
        ])
        .sign_with_keys(&owner_keys)
        .expect("search event");
    assert!(validate_search_event(&event, &owner, "repo", snapshot_id).is_ok());

    let foreign = EventBuilder::new(Kind::Custom(WIKI_EVENT_KIND), "needle")
        .tags(vec![
            tag("d", "repo/intro"),
            tag("a", coordinate),
            tag("wiki-version", "1"),
            tag("wiki-snapshot", snapshot_id),
        ])
        .sign_with_keys(&keys(9))
        .expect("foreign search event");
    assert!(validate_search_event(&foreign, &owner, "repo", snapshot_id).is_err());
}

#[test]
fn search_snapshot_identity_requires_a_lowercase_v4_uuid() {
    assert!(valid_snapshot_id("12345678-1234-4234-9234-123456789abc"));
    assert!(!valid_snapshot_id("12345678-1234-5234-9234-123456789abc"));
    assert!(!valid_snapshot_id("12345678-1234-4234-9234-123456789ABC"));
    assert!(!valid_snapshot_id("not-a-snapshot"));
}

#[test]
fn search_page_ids_require_unique_lowercase_event_ids() {
    assert!(valid_event_id(&"a".repeat(64)));
    assert!(!valid_event_id(&"A".repeat(64)));
    assert!(!valid_event_id(&"a".repeat(63)));
}
