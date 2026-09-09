//! Fail-closed admission for source-bound Wiki publications.
//!
//! The conditional NIP-33 transaction is not available on this relay head.
//! Rejecting source-bound events here prevents the generic replaceable path
//! from presenting an unsafe legacy write as a successful v1 publication.

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

/// Reject v1/source-bound Wiki events before any persistence or side effect.
///
/// Legacy unmarked Wiki events remain on the existing path. Once a source
/// marker or immutable v1 address appears, this relay has no safe conditional
/// transaction to apply, so accepting the event would be an unsafe fallback.
pub(crate) fn validate(event: &Event) -> Result<(), IngestError> {
    if u32::from(event.kind.as_u16()) != KIND_REPO_WIKI_PAGE {
        return Ok(());
    }
    let source_bound =
        SOURCE_BOUND_TAGS.iter().any(|name| has_tag(event, name)) || has_reserved_address(event);
    if !source_bound {
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
    Err(IngestError::Rejected(UNSUPPORTED.into()))
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
                    .is_some_and(|(_, slug)| slug.starts_with("p1-") || slug.starts_with("m1-"))
            })
    })
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
        assert!(validate(&wiki(vec![], "legacy")).is_ok());
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, "ordinary")
            .sign_with_keys(&keys)
            .expect("signed test event");
        assert!(validate(&event).is_ok());
    }

    #[test]
    fn every_source_marker_is_rejected_before_generic_replaceable_storage() {
        for name in SOURCE_BOUND_TAGS {
            let event = wiki(vec![vec![*name, "1"]], "source");
            assert!(matches!(
                validate(&event),
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
        assert!(matches!(
            validate(&reserved),
            Err(IngestError::Rejected(reason)) if reason == UNSUPPORTED
        ));
    }

    #[test]
    fn source_event_size_is_bounded_before_unsupported_outcome() {
        let event = wiki(
            vec![vec!["wiki-source-files", "1"]],
            &"x".repeat(MAX_SIGNED_EVENT_BYTES),
        );
        assert!(matches!(
            validate(&event),
            Err(IngestError::Rejected(reason))
                if reason == "invalid: source-bound Wiki event exceeds 192 KiB"
        ));
    }
}
