//! Crew conditional publication policy on the existing NIP-33 persistence seam.
use buzz_core::kind::{KIND_GIT_REPO_ANNOUNCEMENT, KIND_PROJECT, KIND_REPO_WIKI_PAGE};
use buzz_db::replaceable::ParameterizedReplacePrecondition;
use nostr::Event;

use super::ingest::IngestError;

/// Capability emitted only for a homogeneous deployment with all guarantees enabled.
pub(crate) const EXTENSION: &str = "crew-conditional-publication-v1";
/// Additional opt-in Project association guarantees require both deployment flags.
pub(crate) const PROJECT_EXTENSION: &str = "crew-project-channel-link-v1";

/// Validated request policy; conversion borrows its decoded revision bytes.
pub(crate) enum PublicationPolicy {
    Legacy,
    Missing,
    Revision([u8; 32]),
    LegacyWikiHead,
    ReplayOnly,
}

impl PublicationPolicy {
    pub(crate) fn precondition(&self) -> ParameterizedReplacePrecondition<'_> {
        match self {
            Self::Legacy => ParameterizedReplacePrecondition::Unconditional,
            Self::Missing => ParameterizedReplacePrecondition::ExpectedMissing,
            Self::Revision(id) => ParameterizedReplacePrecondition::ExpectedRevision(id),
            Self::ReplayOnly => ParameterizedReplacePrecondition::ExactReplayOnly,
            Self::LegacyWikiHead => {
                ParameterizedReplacePrecondition::RejectIfLiveHeadHasTag("wiki-version", "1")
            }
        }
    }
}

fn invalid(message: &str) -> IngestError {
    IngestError::Rejected(format!("invalid: {message}"))
}

fn singleton<'a>(event: &'a Event, name: &str) -> Result<Option<&'a str>, IngestError> {
    let mut found = None;
    for tag in event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|s| s == name))
    {
        if found.is_some() || tag.as_slice().len() != 2 {
            return Err(invalid("malformed conditional publication tag"));
        }
        found = tag.as_slice().get(1).map(String::as_str);
    }
    Ok(found)
}

/// Validate opt-in tags before persistence; reserved Wiki addresses stay guarded
/// even when the deployment disables its advertised capability.
pub(crate) fn policy(
    event: &Event,
    enabled: bool,
) -> Result<Option<PublicationPolicy>, IngestError> {
    let kind = u32::from(event.kind.as_u16());
    if !matches!(
        kind,
        KIND_GIT_REPO_ANNOUNCEMENT | KIND_PROJECT | KIND_REPO_WIKI_PAGE
    ) {
        return Ok(None);
    }
    let expected = singleton(event, "expected-revision")?;
    let version = if kind == KIND_REPO_WIKI_PAGE {
        singleton(event, "wiki-version")?
    } else {
        None
    };
    if !enabled && (expected.is_some() || version.is_some()) {
        return Err(IngestError::Rejected(format!("unsupported: {EXTENSION}")));
    }
    let revision = match expected {
        None => None,
        Some("absent") => Some(PublicationPolicy::Missing),
        Some(value) => {
            if value.len() != 64
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(invalid("bad expected publication revision"));
            }
            let mut id = [0; 32];
            hex::decode_to_slice(value, &mut id)
                .map_err(|_| invalid("bad expected publication revision"))?;
            Some(PublicationPolicy::Revision(id))
        }
    };
    if kind != KIND_REPO_WIKI_PAGE {
        return Ok(Some(revision.unwrap_or(PublicationPolicy::Legacy)));
    }
    let bytes = serde_json::to_vec(event).map_err(|_| invalid("invalid signed Wiki event"))?;
    if bytes.len() > 192 * 1024 {
        return Err(invalid("signed Wiki event exceeds 192 KiB"));
    }
    let d = singleton(event, "d")?.ok_or_else(|| invalid("missing Wiki coordinate"))?;
    let (_, slug) = d
        .split_once('/')
        .ok_or_else(|| invalid("bad Wiki coordinate"))?;
    let reserved = slug.starts_with("p1-") || slug.starts_with("m1-");
    if version.is_some_and(|value| value != "1") {
        return Err(invalid("unsupported Wiki version"));
    }
    if reserved || version.is_some() {
        let a = singleton(event, "a")?.ok_or_else(|| invalid("missing Wiki repository"))?;
        let (owner, _) = buzz_core::wiki_page::parse_wiki_repo_a_tag(a)
            .map_err(|_| invalid("invalid Wiki repository"))?;
        if owner != event.pubkey.to_hex() {
            return Err(invalid(
                "Wiki publication must be signed by the repository owner",
            ));
        }
    }
    if version.is_some() {
        let snapshot =
            singleton(event, "wiki-snapshot")?.ok_or_else(|| invalid("missing Wiki snapshot"))?;
        let parsed =
            uuid::Uuid::parse_str(snapshot).map_err(|_| invalid("invalid Wiki snapshot"))?;
        if parsed.get_version_num() != 4 || parsed.to_string() != snapshot {
            return Err(invalid("invalid Wiki snapshot"));
        }
    }
    if reserved {
        if slug.len() != 67
            || !slug[3..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("bad immutable Wiki coordinate"));
        }
        // No conditional tag can turn a reserved address into a replacement.
        return Ok(Some(
            if matches!(revision, Some(PublicationPolicy::Revision(_))) {
                PublicationPolicy::ReplayOnly
            } else {
                PublicationPolicy::Missing
            },
        ));
    }
    if slug == "_toc" && revision.is_some() && version.is_none() {
        return Err(invalid("conditional Wiki head requires wiki-version 1"));
    }
    if version.is_some() {
        if slug != "_toc" || revision.is_none() {
            return Err(invalid("v1 Wiki head requires expected revision"));
        }
    }
    Ok(Some(revision.unwrap_or(if slug == "_toc" {
        PublicationPolicy::LegacyWikiHead
    } else {
        PublicationPolicy::Legacy
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};

    fn wiki(keys: &Keys, owner: &Keys, slug: &str, extra: Vec<Vec<String>>, body: String) -> Event {
        let mut tags = vec![
            vec!["d".into(), format!("repo/{slug}")],
            vec!["a".into(), format!("30617:{}:repo", owner.public_key())],
            vec!["commit".into(), "a".repeat(40)],
        ];
        tags.extend(extra);
        EventBuilder::new(Kind::Custom(30623), body)
            .tags(tags.into_iter().map(|t| Tag::parse(t).unwrap()))
            .sign_with_keys(keys)
            .unwrap()
    }

    #[test]
    fn conditional_policy_reserved_address_guard_survives_disabled_flag_and_omitted_tags() {
        let keys = Keys::generate();
        let event = wiki(
            &keys,
            &keys,
            &format!("p1-{}", "a".repeat(64)),
            vec![],
            "page".into(),
        );
        assert!(matches!(
            policy(&event, false).unwrap().unwrap().precondition(),
            ParameterizedReplacePrecondition::ExpectedMissing
        ));
    }

    #[test]
    fn conditional_policy_disabled_rejects_opt_in_and_legacy_head_has_locked_tag_guard() {
        let keys = Keys::generate();
        let head = wiki(&keys, &keys, "_toc", vec![], "{}".into());
        assert!(matches!(
            policy(&head, false).unwrap().unwrap().precondition(),
            ParameterizedReplacePrecondition::RejectIfLiveHeadHasTag("wiki-version", "1")
        ));
        let opted = wiki(
            &keys,
            &keys,
            "_toc",
            vec![
                vec!["expected-revision".into(), "absent".into()],
                vec!["wiki-version".into(), "1".into()],
                vec!["wiki-snapshot".into(), uuid::Uuid::new_v4().to_string()],
            ],
            "{}".into(),
        );
        assert!(policy(&opted, false).is_err());
        assert!(matches!(
            policy(&opted, true).unwrap().unwrap().precondition(),
            ParameterizedReplacePrecondition::ExpectedMissing
        ));
    }

    #[test]
    fn conditional_policy_rejects_duplicate_and_malformed_expected_revision() {
        let keys = Keys::generate();
        for extra in [
            vec![vec!["expected-revision".into(), "ABC".into()]],
            vec![
                vec!["expected-revision".into(), "absent".into()],
                vec!["expected-revision".into(), "absent".into()],
            ],
        ] {
            assert!(policy(&wiki(&keys, &keys, "_toc", extra, "{}".into()), true).is_err());
        }
    }

    #[test]
    fn conditional_policy_rejects_foreign_author_reserved_wiki() {
        let keys = Keys::generate();
        let foreign = Keys::generate();
        let event = wiki(
            &keys,
            &foreign,
            &format!("p1-{}", "a".repeat(64)),
            vec![],
            "page".into(),
        );
        assert!(policy(&event, true).is_err());
    }

    #[test]
    fn conditional_policy_rejects_whole_signed_event_over_192kib() {
        let keys = Keys::generate();
        let event = wiki(
            &keys,
            &keys,
            &format!("p1-{}", "a".repeat(64)),
            vec![],
            "x".repeat(192 * 1024),
        );
        assert!(policy(&event, true).is_err());
    }

    #[test]
    fn conditional_policy_cannot_strip_v1_with_explicit_revision() {
        let keys = Keys::generate();
        let event = wiki(
            &keys,
            &keys,
            "_toc",
            vec![vec!["expected-revision".into(), "a".repeat(64)]],
            "{}".into(),
        );
        assert!(policy(&event, true).is_err());
    }

    #[test]
    fn conditional_policy_reserved_revision_never_creates_or_replaces() {
        let keys = Keys::generate();
        let event = wiki(
            &keys,
            &keys,
            &format!("p1-{}", "a".repeat(64)),
            vec![vec!["expected-revision".into(), "b".repeat(64)]],
            "page".into(),
        );
        assert!(matches!(
            policy(&event, true).unwrap().unwrap().precondition(),
            ParameterizedReplacePrecondition::ExactReplayOnly
        ));
    }
}
