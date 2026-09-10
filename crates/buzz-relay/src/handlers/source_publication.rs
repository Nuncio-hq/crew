//! Capability gate for source-bound Wiki publications.

use buzz_core::kind::KIND_REPO_WIKI_PAGE;
use nostr::Event;

use super::ingest::IngestError;

const MAX_SIGNED_EVENT_BYTES: usize = 192 * 1024;
const UNSUPPORTED: &str = "unsupported: crew-conditional-publication-v1";
const SOURCE_BOUND_TAGS: &[&str] = &[
    "expected-revision",
    "source-kind",
    "wiki-snapshot",
    "wiki-source-files",
    "wiki-version",
];

/// Gate source-bound Wiki events before persistence.
///
/// Legacy unmarked Wiki events remain on the existing path. Once conditional
/// publication is enabled, the transactional policy performs the complete
/// source-bound validation. With the capability disabled, explicit source
/// markers are rejected rather than falling through to legacy LWW; reserved
/// immutable addresses remain available to the guarded create/replay path,
/// including its exact-replay marker when the capability is rolled back.
pub(crate) fn validate(event: &Event, conditional_enabled: bool) -> Result<(), IngestError> {
    if u32::from(event.kind.as_u16()) != KIND_REPO_WIKI_PAGE {
        return Ok(());
    }
    let explicit_source_marker = SOURCE_BOUND_TAGS.iter().any(|name| has_tag(event, name));
    if !explicit_source_marker && !has_reserved_address(event) {
        return Ok(());
    }
    if !conditional_enabled && !explicit_source_marker {
        return Ok(());
    }
    let bytes = serde_json::to_vec(event).map_err(|_| {
        IngestError::Rejected("invalid: source-bound Wiki event is not serializable".into())
    })?;
    if bytes.len() > MAX_SIGNED_EVENT_BYTES {
        return Err(IngestError::Rejected(
            "invalid: source-bound Wiki event exceeds 192 KiB".into(),
        ));
    }
    // Reserved immutable addresses are guarded by the always-on create/replay
    // transaction, so rollback must not disable their exact-replay path even
    // when the advertised conditional capability is off.
    if conditional_enabled || has_reserved_address(event) {
        Ok(())
    } else {
        Err(IngestError::Rejected(UNSUPPORTED.into()))
    }
}

fn has_tag(event: &Event, name: &str) -> bool {
    event
        .tags
        .iter()
        .any(|tag| tag.as_slice().first().is_some_and(|value| value == name))
}

fn has_reserved_address(event: &Event) -> bool {
    event.tags.iter().any(|tag| {
        let values = tag.as_slice();
        values.first().is_some_and(|name| name == "d")
            && values.get(1).is_some_and(|value| {
                value
                    .split_once('/')
                    .is_some_and(|(_, slug)| is_reserved_slug(slug))
            })
    })
}

fn is_reserved_slug(slug: &str) -> bool {
    let Some(digest) = slug
        .strip_prefix("p1-")
        .or_else(|| slug.strip_prefix("m1-"))
    else {
        return false;
    };
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};

    fn wiki(tags: Vec<Vec<&str>>, content: &str) -> Event {
        let keys = Keys::generate();
        let tags: Vec<Tag> = tags
            .into_iter()
            .map(|tag| Tag::parse(tag).expect("valid test tag"))
            .collect();
        EventBuilder::new(Kind::Custom(KIND_REPO_WIKI_PAGE as u16), content)
            .tags(tags)
            .sign_with_keys(&keys)
            .expect("signed test event")
    }

    #[test]
    fn legacy_unmarked_wiki_and_other_kinds_keep_the_existing_path() {
        assert!(validate(&wiki(vec![], "legacy"), false).is_ok());
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, "ordinary")
            .sign_with_keys(&keys)
            .expect("signed test event");
        assert!(validate(&event, false).is_ok());
    }

    #[test]
    fn every_source_marker_is_rejected_before_generic_replaceable_storage() {
        for name in SOURCE_BOUND_TAGS {
            let event = wiki(vec![vec![*name, "1"]], "source");
            assert!(matches!(
                validate(&event, false),
                Err(IngestError::Rejected(reason)) if reason == UNSUPPORTED
            ));
        }
        let reserved = wiki(
            vec![vec![
                "d",
                "repo/p1-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ]],
            "page",
        );
        assert!(validate(&reserved, false).is_ok());
        let reserved_replay = wiki(
            vec![
                vec![
                    "d",
                    "repo/p1-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                ],
                vec!["expected-revision", "b"],
            ],
            "replay",
        );
        assert!(validate(&reserved_replay, false).is_ok());
    }

    #[test]
    fn only_well_formed_immutable_slugs_are_reserved() {
        for prefix in ["p1-", "m1-"] {
            let valid = wiki(
                vec![vec!["d", &format!("repo/{prefix}{}", "a".repeat(64))]],
                "page",
            );
            assert!(validate(&valid, false).is_ok());
        }

        for suffix in ["short".to_owned(), "a".repeat(63), "g".repeat(64)] {
            let event = wiki(vec![vec!["d", &format!("repo/p1-{suffix}")]], "legacy slug");
            assert!(validate(&event, false).is_ok());
        }
    }

    #[test]
    fn source_event_size_is_bounded_before_unsupported_outcome() {
        let event = wiki(
            vec![vec!["wiki-source-files", "1"]],
            &"x".repeat(MAX_SIGNED_EVENT_BYTES),
        );
        assert!(matches!(
            validate(&event, false),
            Err(IngestError::Rejected(reason))
                if reason == "invalid: source-bound Wiki event exceeds 192 KiB"
        ));
    }
}
