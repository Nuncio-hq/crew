//! Admission and capability helpers for Crew conditional publications.
//!
//! The signed event remains the durable operation. This module only classifies
//! the requested compare-and-write mode and validates the bounded wire envelope
//! before the relay enters the existing replacement transaction.

use buzz_core::kind::{KIND_GIT_REPO_ANNOUNCEMENT, KIND_PROJECT, KIND_REPO_WIKI_PAGE};
use buzz_core::wiki_page::parse_wiki_d_tag;
use nostr::Event;
use uuid::Uuid;

use super::ingest::IngestError;

/// NIP-11 capability for the conditional publication transaction.
pub const EXTENSION: &str = "crew-conditional-publication-v1";
/// NIP-11 capability for transactional Project channel associations.
pub const PROJECT_LINK_EXTENSION: &str = "crew-project-channel-link-v1";

/// Maximum serialized size of one signed Wiki event.
pub const MAX_SIGNED_EVENT_BYTES: usize = 192 * 1024;
/// Maximum number of pages in one verified Wiki publication.
pub const MAX_PUBLICATION_PAGES: usize = 256;
/// Maximum live reserved Wiki event bytes for one owner in one community.
pub const MAX_WIKI_LIVE_BYTES: i64 = buzz_db::replaceable::MAX_WIKI_LIVE_BYTES;
/// Maximum live reserved Wiki events for one owner in one community.
pub const MAX_WIKI_LIVE_EVENTS: i64 = buzz_db::replaceable::MAX_WIKI_LIVE_EVENTS;

const SOURCE_MARKER_TAGS: &[&str] = &[
    "source-kind",
    "wiki-manifest",
    "wiki-slug",
    "wiki-snapshot",
    "wiki-source-files",
    "wiki-version",
];
const INVALID: &str = "invalid: conditional publication";

/// Conditional compare precondition selected by the signed `expected-revision`
/// tag.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConditionalRevision {
    /// The coordinate must have no live head, while an exact current replay is
    /// still accepted by the database transaction.
    ExpectedMissing,
    /// The coordinate's live head must have this exact event ID.
    ExpectedRevision(Vec<u8>),
}

/// Publication mode selected from the signed event envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicationMode {
    /// Existing NIP-33 last-write-wins semantics.
    Legacy,
    /// The D-079 compare-and-write contract.
    Conditional(ConditionalRevision),
}

impl PublicationMode {
    /// Whether this mode uses the conditional contract.
    #[must_use]
    pub fn is_conditional(&self) -> bool {
        matches!(self, Self::Conditional(_))
    }

    /// Convert to the database precondition used by the replacement seam.
    #[must_use]
    pub fn precondition(
        &self,
    ) -> Option<buzz_db::replaceable::ParameterizedReplacePrecondition<'_>> {
        match self {
            Self::Legacy => None,
            Self::Conditional(ConditionalRevision::ExpectedMissing) => {
                Some(buzz_db::replaceable::ParameterizedReplacePrecondition::ExpectedMissing)
            }
            Self::Conditional(ConditionalRevision::ExpectedRevision(revision)) => Some(
                buzz_db::replaceable::ParameterizedReplacePrecondition::ExpectedRevision(
                    revision.as_slice(),
                ),
            ),
        }
    }
}

/// Add the conditional capabilities only after the operator enables the full
/// transaction and all writers have been upgraded or quiesced.
pub fn advertise(extensions: &mut Option<Vec<String>>, enabled: bool) {
    if !enabled {
        return;
    }
    let values = extensions.get_or_insert_with(Vec::new);
    for extension in [EXTENSION, PROJECT_LINK_EXTENSION] {
        if !values.iter().any(|current| current == extension) {
            values.push(extension.to_owned());
        }
    }
}

/// Classify and validate a source publication before any persistence.
///
/// Conditional tags are parsed even when the capability is disabled so a
/// malformed signed request is reported as invalid. A disabled relay then
/// rejects a well-formed opt-in instead of silently falling back to the legacy
/// replacement path.
pub(crate) fn classify(event: &Event, enabled: bool) -> Result<PublicationMode, IngestError> {
    let kind = u32::from(event.kind.as_u16());
    if !matches!(
        kind,
        KIND_GIT_REPO_ANNOUNCEMENT | KIND_PROJECT | KIND_REPO_WIKI_PAGE
    ) {
        return Ok(PublicationMode::Legacy);
    }

    // Bound the complete signed envelope before inspecting source metadata.
    // This applies to legacy-shaped rows too: a reserved immutable address or
    // partial v1 marker must not be able to bypass the Wiki storage contract.
    if kind == KIND_REPO_WIKI_PAGE {
        validate_signed_event_size(event)?;
    }

    let expected = parse_expected_revision(event)?;
    let source_marked = kind == KIND_REPO_WIKI_PAGE && has_any_tag(event, SOURCE_MARKER_TAGS);
    let reserved = kind == KIND_REPO_WIKI_PAGE && has_reserved_immutable_address(event);
    let conditional_requested = expected.is_some() || source_marked || reserved;

    if !conditional_requested {
        return Ok(PublicationMode::Legacy);
    }

    // Coordinate shape is part of the signed conditional envelope. Parse it
    // even while the deployment capability is disabled so malformed requests
    // are invalid rather than being hidden behind the capability response.
    match kind {
        KIND_GIT_REPO_ANNOUNCEMENT => validate_repository_coordinate(event)?,
        KIND_PROJECT => {
            singleton_tag(event, "d", "Project")?
                .ok_or_else(|| invalid("Project publication requires one d tag"))?;
        }
        _ => {}
    }
    // Parse the complete Wiki shape before the capability gate as well. A
    // disabled relay must report malformed signed opt-ins as invalid rather
    // than hide duplicate tags, a wrong owner, or a malformed source payload
    // behind the generic unsupported response.
    let wiki_shape = if kind == KIND_REPO_WIKI_PAGE {
        Some(validate_wiki_event(
            event,
            source_marked,
            reserved,
            expected.is_some(),
        )?)
    } else {
        None
    };
    if !enabled {
        return Err(unsupported());
    }

    match kind {
        KIND_GIT_REPO_ANNOUNCEMENT | KIND_PROJECT => {}
        KIND_REPO_WIKI_PAGE => {
            let Some(wiki) = wiki_shape else {
                return Err(invalid("Wiki shape is unavailable"));
            };
            if !wiki.is_toc && expected.is_some() {
                return Err(invalid(
                    "expected-revision is only valid on the Wiki _toc head",
                ));
            }
            if wiki.is_toc
                && (source_marked || has_tag(event, "wiki-version"))
                && expected.is_none()
            {
                return Err(invalid(
                    "a v1 Wiki _toc must carry one expected-revision tag",
                ));
            }
        }
        _ => unreachable!("kind was checked above"),
    }

    let precondition = match expected {
        Some(ExpectedRevisionValue::Absent) | None => ConditionalRevision::ExpectedMissing,
        Some(ExpectedRevisionValue::Id(id)) => ConditionalRevision::ExpectedRevision(id),
    };
    Ok(PublicationMode::Conditional(precondition))
}

fn validate_signed_event_size(event: &Event) -> Result<(), IngestError> {
    let bytes = serde_json::to_vec(event).map_err(|_| invalid("Wiki event is not serializable"))?;
    if bytes.len() > MAX_SIGNED_EVENT_BYTES {
        return Err(invalid("Wiki event exceeds the signed 192 KiB bound"));
    }
    Ok(())
}

/// Compatibility seam used by the Wiki validator. It uses the same classifier
/// as the main ingest path, including the deployment gate.
pub(crate) fn validate(event: &Event, enabled: bool) -> Result<(), IngestError> {
    let _ = classify(event, enabled)?;
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ExpectedRevisionValue {
    Absent,
    Id(Vec<u8>),
}

fn parse_expected_revision(event: &Event) -> Result<Option<ExpectedRevisionValue>, IngestError> {
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| {
            tag.as_slice()
                .first()
                .is_some_and(|name| name == "expected-revision")
        })
        .collect();
    if tags.is_empty() {
        return Ok(None);
    }
    if tags.len() != 1 || tags[0].as_slice().len() != 2 {
        return Err(invalid(
            "expected-revision must be exactly one two-element tag",
        ));
    }
    let value = tags[0].as_slice().get(1).map(String::as_str).unwrap_or("");
    if value == "absent" {
        return Ok(Some(ExpectedRevisionValue::Absent));
    }
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid(
            "expected-revision must be `absent` or a 64-character lowercase event ID",
        ));
    }
    let bytes = hex::decode(value).map_err(|_| invalid("expected-revision is not hexadecimal"))?;
    Ok(Some(ExpectedRevisionValue::Id(bytes)))
}

fn validate_repository_coordinate(event: &Event) -> Result<(), IngestError> {
    let d = singleton_tag(event, "d", "repository")?
        .ok_or_else(|| invalid("repository announcement requires one d tag"))?;
    if !valid_repo_id(d) {
        return Err(invalid("repository d tag is malformed"));
    }
    Ok(())
}

struct WikiShape {
    is_toc: bool,
}

fn validate_wiki_event(
    event: &Event,
    source_marked: bool,
    reserved: bool,
    has_expected: bool,
) -> Result<WikiShape, IngestError> {
    let d = singleton_tag(event, "d", "Wiki")?
        .ok_or_else(|| invalid("Wiki event requires one d tag"))?;
    let parsed = parse_wiki_d_tag(d).map_err(|_| invalid("Wiki d tag is malformed"))?;
    let a = singleton_tag(event, "a", "Wiki")?
        .ok_or_else(|| invalid("Wiki event requires one repository a tag"))?;
    let (owner, repo) = parse_repo_coordinate(a)?;
    if repo != parsed.repo_d {
        return Err(invalid("Wiki a tag repository does not match d"));
    }
    if owner != event.pubkey.to_hex() {
        return Err(invalid("Wiki event must be signed by the repository owner"));
    }
    let immutable = is_immutable_slug(parsed.slug.as_str());
    if reserved && !immutable {
        return Err(invalid(
            "reserved Wiki address must use a p1- or m1- digest slug",
        ));
    }
    if source_marked {
        validate_v1_common_tags(
            event,
            parsed.is_toc(),
            owner.as_str(),
            parsed.repo_d.as_str(),
        )?;
        if parsed.is_toc() {
            validate_toc_tags(event)?;
        } else if parsed.slug.starts_with("p1-") {
            validate_page_tags(event)?;
        } else if parsed.slug.starts_with("m1-") {
            validate_manifest_event(event)?;
        } else {
            return Err(invalid(
                "v1 Wiki pages and manifests require immutable slugs",
            ));
        }
    }
    if has_expected && !parsed.is_toc() {
        return Err(invalid(
            "expected-revision is only valid on the Wiki _toc head",
        ));
    }
    Ok(WikiShape {
        is_toc: parsed.is_toc(),
    })
}

fn validate_v1_common_tags(
    event: &Event,
    is_toc: bool,
    owner: &str,
    repo: &str,
) -> Result<(), IngestError> {
    let version = exact_tag(event, "wiki-version", 2)?
        .ok_or_else(|| invalid("v1 Wiki event requires wiki-version=1"))?;
    if version[1] != "1" {
        return Err(invalid("wiki-version must be 1"));
    }
    let snapshot = exact_tag(event, "wiki-snapshot", 2)?
        .ok_or_else(|| invalid("v1 Wiki event requires one wiki-snapshot tag"))?;
    let snapshot_id = &snapshot[1];
    let uuid = Uuid::parse_str(snapshot_id)
        .map_err(|_| invalid("wiki-snapshot must be a canonical UUID"))?;
    if uuid.to_string() != *snapshot_id {
        return Err(invalid("wiki-snapshot must be a lowercase canonical UUID"));
    }
    let source_kind = exact_tag(event, "source-kind", 2)?
        .ok_or_else(|| invalid("v1 Wiki event requires one source-kind tag"))?;
    if source_kind[1] != "git" && source_kind[1] != "folder" {
        return Err(invalid("source-kind must be git or folder"));
    }
    let commit = exact_tag(event, "commit", 2)?
        .ok_or_else(|| invalid("v1 Wiki event requires one commit tag"))?;
    if !valid_commit(source_kind[1].as_str(), commit[1].as_str()) {
        return Err(invalid("v1 Wiki commit tag is malformed"));
    }
    // Keep these arguments explicit at the validation seam so a future change
    // cannot accidentally validate tags for a different repository.
    let _ = (is_toc, owner, repo);
    Ok(())
}

fn validate_toc_tags(event: &Event) -> Result<(), IngestError> {
    let manifest = exact_tag(event, "wiki-manifest", 3)?
        .ok_or_else(|| invalid("v1 Wiki _toc requires one wiki-manifest tag"))?;
    if !is_lower_hex(&manifest[1], 64) || !is_lower_hex(&manifest[2], 64) {
        return Err(invalid(
            "wiki-manifest must contain a 64-character event ID and digest",
        ));
    }
    let cadence = exact_tag(event, "cadence", 2)?
        .ok_or_else(|| invalid("v1 Wiki _toc requires one cadence tag"))?;
    if !matches!(
        cadence[1].as_str(),
        "manual" | "on-push" | "daily" | "weekly"
    ) {
        return Err(invalid("Wiki cadence is malformed"));
    }
    let expected = exact_tag(event, "expected-revision", 2)?
        .ok_or_else(|| invalid("v1 Wiki _toc requires one expected-revision tag"))?;
    if expected[1] != "absent" && !is_lower_hex(&expected[1], 64) {
        return Err(invalid("Wiki _toc expected-revision is malformed"));
    }
    Ok(())
}

fn validate_page_tags(event: &Event) -> Result<(), IngestError> {
    for name in [
        "wiki-slug",
        "title",
        "section",
        "language",
        "wiki-source-files",
    ] {
        let tag = exact_tag(event, name, 2)?
            .ok_or_else(|| invalid("v1 Wiki page metadata is incomplete"))?;
        if tag[1].is_empty() {
            return Err(invalid("v1 Wiki page metadata must not be empty"));
        }
    }
    let source_files = event
        .tags
        .iter()
        .find(|tag| {
            tag.as_slice()
                .first()
                .is_some_and(|name| name == "wiki-source-files")
        })
        .and_then(|tag| tag.as_slice().get(1))
        .ok_or_else(|| invalid("v1 Wiki page source metadata is missing"))?;
    let parsed: serde_json::Value = serde_json::from_str(source_files)
        .map_err(|_| invalid("wiki-source-files must be canonical JSON"))?;
    if !parsed.is_array() || event.content.contains('\0') {
        return Err(invalid("wiki-source-files or page content is malformed"));
    }
    Ok(())
}

fn validate_manifest_event(event: &Event) -> Result<(), IngestError> {
    if event.content.len() > MAX_SIGNED_EVENT_BYTES {
        return Err(invalid("Wiki manifest exceeds the signed event bound"));
    }
    let _: serde_json::Value = serde_json::from_str(&event.content)
        .map_err(|_| invalid("v1 Wiki manifest content must be JSON"))?;
    Ok(())
}

fn singleton_tag<'a>(
    event: &'a Event,
    name: &str,
    label: &str,
) -> Result<Option<&'a str>, IngestError> {
    let matches: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|value| value == name))
        .collect();
    if matches.len() > 1 {
        return Err(invalid(&format!(
            "{label} must have exactly one {name} tag"
        )));
    }
    let Some(tag) = matches.first() else {
        return Ok(None);
    };
    if tag.as_slice().len() != 2 {
        return Err(invalid(&format!(
            "{name} tag must have exactly two elements"
        )));
    }
    Ok(tag.as_slice().get(1).map(String::as_str))
}

fn exact_tag<'a>(
    event: &'a Event,
    name: &str,
    width: usize,
) -> Result<Option<&'a [String]>, IngestError> {
    let matches: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|value| value == name))
        .collect();
    if matches.len() > 1 {
        return Err(invalid(&format!("{name} tag must occur exactly once")));
    }
    let Some(tag) = matches.first() else {
        return Ok(None);
    };
    if tag.as_slice().len() != width {
        return Err(invalid(&format!(
            "{name} tag must have exactly {width} elements"
        )));
    }
    Ok(Some(tag.as_slice()))
}

fn parse_repo_coordinate(value: &str) -> Result<(String, String), IngestError> {
    let mut parts = value.splitn(3, ':');
    if parts.next() != Some("30617") {
        return Err(invalid("Wiki a tag must name a kind:30617 repository"));
    }
    let owner = parts.next().unwrap_or("");
    let repo = parts.next().unwrap_or("");
    if !is_lower_hex(owner, 64) || !valid_repo_id(repo) {
        return Err(invalid("Wiki a tag repository coordinate is malformed"));
    }
    Ok((owner.to_owned(), repo.to_owned()))
}

fn valid_repo_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('.')
        && !value.contains("..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_commit(source_kind: &str, value: &str) -> bool {
    match source_kind {
        "git" => is_lower_hex(value, 40) || is_lower_hex(value, 64),
        "folder" => value
            .strip_prefix("folder:")
            .is_some_and(|hash| is_lower_hex(hash, 64)),
        _ => false,
    }
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn is_immutable_slug(slug: &str) -> bool {
    ["p1-", "m1-"].iter().any(|prefix| {
        slug.strip_prefix(prefix)
            .is_some_and(|digest| is_lower_hex(digest, 64))
    })
}

/// Whether a Wiki coordinate is one of the content-addressed p1-/m1-
/// addresses whose accepted event cannot be replaced in place.
pub(crate) fn is_reserved_immutable_d_tag(d_tag: &str) -> bool {
    d_tag
        .rsplit_once('/')
        .is_some_and(|(_, slug)| is_immutable_slug(slug))
}

fn has_any_tag(event: &Event, names: &[&str]) -> bool {
    names.iter().any(|name| has_tag(event, name))
}

fn has_tag(event: &Event, name: &str) -> bool {
    event
        .tags
        .iter()
        .any(|tag| tag.as_slice().first().is_some_and(|value| value == name))
}

fn has_reserved_immutable_address(event: &Event) -> bool {
    event.tags.iter().any(|tag| {
        let parts = tag.as_slice();
        parts.first().is_some_and(|name| name == "d")
            && parts
                .get(1)
                .is_some_and(|value| is_reserved_immutable_d_tag(value))
    })
}

fn invalid(reason: &str) -> IngestError {
    IngestError::Rejected(format!("{INVALID}: {reason}"))
}

fn unsupported() -> IngestError {
    IngestError::Rejected(format!(
        "unsupported: {EXTENSION} is not enabled on this relay"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag, Timestamp};

    fn signed(kind: u32, tags: &[&[&str]]) -> Event {
        let keys = Keys::generate();
        let tags = tags
            .iter()
            .map(|parts| Tag::parse(*parts).expect("test tag"))
            .collect::<Vec<_>>();
        EventBuilder::new(Kind::Custom(kind as u16), "")
            .tags(tags)
            .sign_with_keys(&keys)
            .expect("signed test event")
    }

    fn v1_page(keys: &Keys, owner: &str, source_kind: &str, commit: &str) -> Event {
        let a = format!("30617:{owner}:repo");
        let snapshot = Uuid::new_v4().to_string();
        let page_digest = "a".repeat(64);
        let page_d = format!("repo/p1-{page_digest}");
        EventBuilder::new(Kind::Custom(KIND_REPO_WIKI_PAGE as u16), "# Overview")
            .tags([
                Tag::parse(["d", page_d.as_str()]).expect("d tag"),
                Tag::parse(["a", a.as_str()]).expect("a tag"),
                Tag::parse(["wiki-version", "1"]).expect("version tag"),
                Tag::parse(["wiki-snapshot", snapshot.as_str()]).expect("snapshot tag"),
                Tag::parse(["source-kind", source_kind]).expect("source kind tag"),
                Tag::parse(["commit", commit]).expect("commit tag"),
                Tag::parse(["wiki-slug", "overview"]).expect("slug tag"),
                Tag::parse(["title", "Overview"]).expect("title tag"),
                Tag::parse(["section", "guide"]).expect("section tag"),
                Tag::parse(["language", "en"]).expect("language tag"),
                Tag::parse(["wiki-source-files", "[\"README.md\"]"]).expect("source files tag"),
            ])
            .custom_created_at(Timestamp::now())
            .sign_with_keys(keys)
            .expect("signed Wiki page")
    }

    #[test]
    fn capability_advertisement_is_default_off_and_idempotent() {
        let mut extensions = Some(vec!["nip-er".to_owned()]);
        advertise(&mut extensions, false);
        assert_eq!(
            extensions.as_deref(),
            Some(["nip-er".to_owned()].as_slice())
        );
        advertise(&mut extensions, true);
        advertise(&mut extensions, true);
        let values = extensions.expect("extensions");
        assert_eq!(values.iter().filter(|v| *v == EXTENSION).count(), 1);
        assert_eq!(
            values
                .iter()
                .filter(|v| *v == PROJECT_LINK_EXTENSION)
                .count(),
            1
        );
    }

    #[test]
    fn expected_revision_selects_missing_or_exact_revision() {
        let absent = signed(30617, &[&["d", "repo"], &["expected-revision", "absent"]]);
        assert_eq!(
            classify(&absent, true).unwrap(),
            PublicationMode::Conditional(ConditionalRevision::ExpectedMissing)
        );
        let id = "ab".repeat(32);
        let exact = signed(30617, &[&["d", "repo"], &["expected-revision", &id]]);
        assert_eq!(
            classify(&exact, true).unwrap(),
            PublicationMode::Conditional(ConditionalRevision::ExpectedRevision(
                hex::decode(id).unwrap()
            ))
        );
    }

    #[test]
    fn disabled_opt_in_never_falls_back() {
        let event = signed(30617, &[&["d", "repo"], &["expected-revision", "absent"]]);
        assert!(
            matches!(classify(&event, false), Err(IngestError::Rejected(reason)) if reason.starts_with("unsupported:"))
        );
    }

    #[test]
    fn malformed_expected_revision_is_invalid_even_when_disabled() {
        let event = signed(30617, &[&["d", "repo"], &["expected-revision", "UPPER"]]);
        assert!(
            matches!(classify(&event, false), Err(IngestError::Rejected(reason)) if reason.starts_with("invalid:"))
        );
    }

    #[test]
    fn unconditioned_legacy_repository_keeps_legacy_coordinate_validation() {
        let event = signed(30617, &[]);
        assert_eq!(classify(&event, false).unwrap(), PublicationMode::Legacy);
    }

    #[test]
    fn conditional_project_requires_one_unambiguous_d_tag() {
        let missing = signed(30621, &[&["expected-revision", "absent"]]);
        assert!(matches!(
            classify(&missing, true),
            Err(IngestError::Rejected(reason)) if reason.starts_with("invalid:")
        ));

        let duplicate = signed(
            30621,
            &[
                &["d", "project"],
                &["d", "other"],
                &["expected-revision", "absent"],
            ],
        );
        assert!(matches!(
            classify(&duplicate, true),
            Err(IngestError::Rejected(reason)) if reason.starts_with("invalid:")
        ));
    }

    #[test]
    fn full_v1_opt_in_is_unsupported_when_capability_is_disabled() {
        let keys = Keys::generate();
        let event = v1_page(&keys, &keys.public_key().to_hex(), "git", &"a".repeat(40));
        assert!(matches!(
            classify(&event, false),
            Err(IngestError::Rejected(reason)) if reason.starts_with("unsupported:")
        ));
    }

    #[test]
    fn full_signed_wiki_envelope_is_bounded_before_shape_validation() {
        let oversized = EventBuilder::new(
            Kind::Custom(KIND_REPO_WIKI_PAGE as u16),
            "x".repeat(MAX_SIGNED_EVENT_BYTES),
        )
        .sign_with_keys(&Keys::generate())
        .unwrap();
        assert!(matches!(
            classify(&oversized, false),
            Err(IngestError::Rejected(reason)) if reason.contains("192 KiB")
        ));
    }

    #[test]
    fn immutable_wiki_address_is_conditional_when_enabled() {
        let keys = Keys::generate();
        let owner = keys.public_key().to_hex();
        let d = format!("repo/p1-{}", "a".repeat(64));
        let a = format!("30617:{owner}:repo");
        let event = EventBuilder::new(Kind::Custom(KIND_REPO_WIKI_PAGE as u16), "page")
            .tags([
                Tag::parse(["d", d.as_str()]).unwrap(),
                Tag::parse(["a", a.as_str()]).unwrap(),
                Tag::parse(["commit", "a".repeat(40).as_str()]).unwrap(),
            ])
            .sign_with_keys(&keys)
            .unwrap();
        assert_eq!(
            classify(&event, true).unwrap(),
            PublicationMode::Conditional(ConditionalRevision::ExpectedMissing)
        );
        assert!(
            matches!(classify(&event, false), Err(IngestError::Rejected(reason)) if reason.starts_with("unsupported:"))
        );
    }

    #[test]
    fn conditional_publication_is_default_off_for_repository_project_and_wiki() {
        let repository = signed(
            KIND_GIT_REPO_ANNOUNCEMENT,
            &[&["d", "repo"], &["expected-revision", "absent"]],
        );
        let project = signed(
            KIND_PROJECT,
            &[&["d", "Project"], &["expected-revision", "absent"]],
        );
        let wiki_keys = Keys::generate();
        let wiki = v1_page(
            &wiki_keys,
            &wiki_keys.public_key().to_hex(),
            "git",
            &"b".repeat(40),
        );
        for event in [&repository, &project, &wiki] {
            assert!(matches!(
                classify(event, false),
                Err(IngestError::Rejected(reason)) if reason.starts_with("unsupported:")
            ));
        }
    }

    #[test]
    fn conditional_wiki_rejects_wrong_owner_and_duplicate_coordinate_tags() {
        let keys = Keys::generate();
        let wrong_owner = v1_page(&keys, &"c".repeat(64), "git", &"a".repeat(40));
        assert!(matches!(
            classify(&wrong_owner, true),
            Err(IngestError::Rejected(reason)) if reason.starts_with("invalid:")
        ));

        let owner = keys.public_key().to_hex();
        let a = format!("30617:{owner}:repo");
        let snapshot = Uuid::new_v4().to_string();
        let commit = "a".repeat(40);
        let duplicate = EventBuilder::new(Kind::Custom(KIND_REPO_WIKI_PAGE as u16), "page")
            .tags([
                Tag::parse(["d", "repo/overview"]).expect("d"),
                Tag::parse(["d", "repo/other"]).expect("duplicate d"),
                Tag::parse(["a", a.as_str()]).expect("a"),
                Tag::parse(["wiki-version", "1"]).expect("version"),
                Tag::parse(["wiki-snapshot", snapshot.as_str()]).expect("snapshot"),
                Tag::parse(["source-kind", "git"]).expect("source"),
                Tag::parse(["commit", commit.as_str()]).expect("commit"),
                Tag::parse(["wiki-slug", "overview"]).expect("slug"),
                Tag::parse(["title", "Overview"]).expect("title"),
                Tag::parse(["section", "guide"]).expect("section"),
                Tag::parse(["language", "en"]).expect("language"),
                Tag::parse(["wiki-source-files", "[]"]).expect("files"),
            ])
            .sign_with_keys(&keys)
            .expect("signed duplicate");
        assert!(matches!(
            classify(&duplicate, true),
            Err(IngestError::Rejected(reason)) if reason.starts_with("invalid:")
        ));
    }
}
