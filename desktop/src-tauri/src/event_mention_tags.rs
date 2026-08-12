//! Mention and `p-removed` tag builders for signed event construction.
//!
//! Child module of `events` so it can share `tag` / `check_pubkey` without a
//! crate-level cycle, and so the edit-as-undo mention surface has a real
//! boundary under the file-size ratchet.

use nostr::Tag;

use super::{check_pubkey, tag};

/// Maximum mention count — matches buzz-sdk.
const MAX_MENTIONS: usize = 50;

pub(super) fn mention_tags(mentions: &[&str]) -> Result<Vec<Tag>, String> {
    if mentions.len() > MAX_MENTIONS {
        return Err(format!("too many mentions (max {MAX_MENTIONS})"));
    }
    let mut seen = std::collections::HashSet::new();
    let mut tags = Vec::new();
    for &hex in mentions {
        check_pubkey(hex)?;
        let lower = hex.to_ascii_lowercase();
        if seen.insert(lower.clone()) {
            tags.push(tag(vec!["p", &lower])?);
        }
    }
    Ok(tags)
}

pub(super) fn removed_mention_tags(mentions: &[&str]) -> Result<Vec<Tag>, String> {
    if mentions.len() > MAX_MENTIONS {
        return Err(format!("too many removed mentions (max {MAX_MENTIONS})"));
    }
    let mut seen = std::collections::HashSet::new();
    let mut tags = Vec::new();
    for &hex in mentions {
        check_pubkey(hex)?;
        let lower = hex.to_ascii_lowercase();
        if seen.insert(lower.clone()) {
            tags.push(tag(vec!["p-removed", &lower])?);
        }
    }
    Ok(tags)
}

pub(super) fn mention_reference_tags(
    mentions: &[Vec<String>],
    tags: &mut Vec<Tag>,
) -> Result<(), String> {
    for mention in mentions {
        if mention.first().map(String::as_str) != Some("mention") {
            return Err(format!(
                "mention reference tags must use 'mention' prefix (got {:?})",
                mention.first()
            ));
        }
        let Some(pubkey) = mention.get(1) else {
            return Err("mention reference tag missing pubkey".into());
        };
        check_pubkey(pubkey)?;
        tags.push(tag(vec!["mention", &pubkey.to_ascii_lowercase()])?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use nostr::{EventId, Keys};
    use uuid::Uuid;

    use super::super::{build_message_edit, MessageEditTags};

    // ── build_message_edit `p`-tag emission (lane 8ace8eed) ──────────────
    //
    // The composer diffs the edited body's mentions against the original and
    // hands `build_message_edit` only the *newly added* pubkeys. These tests
    // pin the builder's contract given that contract: emit a `p` per added
    // mention (deduped, lowercased), and none when the added set is empty
    // (typo-fix edit) — so an unchanged mention set re-wakes nobody.

    const CH_ID: &str = "11111111-1111-4111-8111-111111111111";
    const ALICE_HEX: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const BOB_HEX: &str = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";

    fn edit_tags(mentions: &[&str], removed: &[&str]) -> Vec<Vec<String>> {
        let channel = Uuid::parse_str(CH_ID).unwrap();
        let target =
            EventId::from_hex("d24da132115ca0a46233cf4c2ad8338fbf914250cbcaa9181a6dd59533cb5ac1")
                .unwrap();
        let builder = build_message_edit(
            channel,
            target,
            "hi @alice",
            MessageEditTags {
                media: &[],
                custom_emoji: &[],
                mentions,
                mention_refs: None,
                removed_mentions: removed,
            },
            false,
        )
        .unwrap();
        let secret = nostr::SecretKey::from_hex(
            "0000000000000000000000000000000000000000000000000000000000000003",
        )
        .unwrap();
        let event = builder.sign_with_keys(&Keys::new(secret)).unwrap();
        event.tags.iter().map(|t| t.as_slice().to_vec()).collect()
    }

    #[test]
    fn edit_with_added_mention_emits_p_tag() {
        let tags = edit_tags(&[ALICE_HEX], &[]);
        assert_eq!(tags[0][0], "h");
        assert_eq!(tags[1][0], "e");
        // The `p` tag rides right after the `e` tag (insertion order).
        assert_eq!(tags[2], vec!["p".to_string(), ALICE_HEX.to_string()]);
    }

    #[test]
    fn edit_with_no_added_mentions_emits_no_p_tag() {
        // Typo-fix edit: mention set unchanged, so the composer passes `&[]`.
        // The edit event must carry no `p` tag and re-wake nobody.
        let tags = edit_tags(&[], &[]);
        assert!(
            !tags
                .iter()
                .any(|t| t.first().map(String::as_str) == Some("p")),
            "unchanged-mention edit must not emit any `p` tag, got {tags:?}"
        );
    }

    #[test]
    fn edit_mentions_are_deduped_and_lowercased() {
        let alice_upper = ALICE_HEX.to_ascii_uppercase();
        let tags = edit_tags(&[ALICE_HEX, &alice_upper, BOB_HEX], &[]);
        let p_tags: Vec<&Vec<String>> = tags
            .iter()
            .filter(|t| t.first().map(String::as_str) == Some("p"))
            .collect();
        // ALICE appears twice (mixed case) but collapses to one lowercase tag.
        assert_eq!(
            p_tags.len(),
            2,
            "duplicate mention must collapse, got {p_tags:?}"
        );
        assert_eq!(p_tags[0], &vec!["p".to_string(), ALICE_HEX.to_string()]);
        assert_eq!(p_tags[1], &vec!["p".to_string(), BOB_HEX.to_string()]);
    }

    #[test]
    fn edit_with_removed_mention_emits_p_removed_tag() {
        let tags = edit_tags(&[], &[ALICE_HEX]);
        assert!(
            tags.iter()
                .any(|t| t.as_slice() == ["p-removed", ALICE_HEX]),
            "removed mention must emit p-removed, got {tags:?}"
        );
        assert!(
            !tags
                .iter()
                .any(|t| t.first().map(String::as_str) == Some("p")),
            "removal-only edit must not emit `p` tags, got {tags:?}"
        );
    }

    #[test]
    fn edit_removed_mentions_are_deduped_and_lowercased() {
        let alice_upper = ALICE_HEX.to_ascii_uppercase();
        let tags = edit_tags(&[], &[ALICE_HEX, &alice_upper, BOB_HEX]);
        let removed: Vec<&Vec<String>> = tags
            .iter()
            .filter(|t| t.first().map(String::as_str) == Some("p-removed"))
            .collect();
        assert_eq!(
            removed.len(),
            2,
            "duplicate removed must collapse, got {removed:?}"
        );
        assert_eq!(
            removed[0],
            &vec!["p-removed".to_string(), ALICE_HEX.to_string()]
        );
        assert_eq!(
            removed[1],
            &vec!["p-removed".to_string(), BOB_HEX.to_string()]
        );
    }
}
