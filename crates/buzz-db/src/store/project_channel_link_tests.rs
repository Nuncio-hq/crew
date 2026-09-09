//! Pure delta checks plus owned-PostgreSQL transaction proofs (added below).
use super::*;
use nostr::{EventBuilder, Keys, Kind, Tag};

fn tags(values: &[&[&str]]) -> Vec<Vec<String>> {
    values
        .iter()
        .map(|tag| tag.iter().map(|value| (*value).to_string()).collect())
        .collect()
}
fn project(values: &[Vec<String>]) -> Event {
    EventBuilder::new(Kind::Custom(30621), "")
        .tags(
            values
                .iter()
                .map(|value| Tag::parse(value.clone()).unwrap()),
        )
        .sign_with_keys(&Keys::generate())
        .unwrap()
}

#[test]
fn project_link_delta_union_covers_home_creation_and_preserves_role_movement() {
    let home = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let related = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let current = tags(&[
        &["d", "project"],
        &["buzz-channel", home],
        &["buzz-related-channel", related],
    ]);
    assert_eq!(
        newly_associated(&[], &project(&current)).unwrap(),
        vec![
            Uuid::parse_str(home).unwrap(),
            Uuid::parse_str(related).unwrap()
        ]
    );
    let moved = tags(&[
        &["d", "project"],
        &["buzz-channel", related],
        &["buzz-related-channel", home],
        &["future-metadata", "preserved"],
    ]);
    assert!(newly_associated(&current, &project(&moved))
        .unwrap()
        .is_empty());
}

#[test]
fn project_link_delta_preserves_unchanged_legacy_values_but_rejects_new_invalid_ids() {
    let old = tags(&[
        &["d", "project"],
        &["buzz-related-channel", "legacy-opaque"],
    ]);
    let mut unchanged = old.clone();
    unchanged.push(vec!["unknown-metadata".into(), "new value".into()]);
    assert!(newly_associated(&old, &project(&unchanged))
        .unwrap()
        .is_empty());
    unchanged.push(vec![
        "buzz-related-channel".into(),
        "AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA".into(),
    ]);
    assert!(newly_associated(&old, &project(&unchanged)).is_err());
}

#[test]
fn project_link_delta_rejects_duplicate_home_and_never_truncates_associations() {
    let duplicate = tags(&[
        &["d", "project"],
        &["buzz-channel", "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"],
        &["buzz-channel", "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"],
    ]);
    assert!(newly_associated(&[], &project(&duplicate)).is_err());
    let mut over = tags(&[&["d", "project"]]);
    for index in 1..=65 {
        over.push(vec![
            "buzz-related-channel".into(),
            Uuid::from_u128(index).to_string(),
        ]);
    }
    assert!(newly_associated(&[], &project(&over)).is_err());
    over.pop();
    assert_eq!(newly_associated(&[], &project(&over)).unwrap().len(), 64);
    over.push(vec!["buzz-channel".into(), Uuid::from_u128(65).to_string()]);
    assert_eq!(newly_associated(&[], &project(&over)).unwrap().len(), 65);
}
