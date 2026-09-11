//! Production builders for signed Crew Wiki snapshot v1 publications.
//!
//! The verifier in [`crate::snapshot_v1`] remains the protocol authority.  This
//! module only assembles and signs events, then runs that verifier over the
//! complete result before returning it to a caller.  In particular, a caller
//! must persist the returned events before sending any of them to a relay.

use crate::git_snapshot::RepoSnapshot;
use crate::publish::PageDraft;
use crate::snapshot_v1::{projection, verify_snapshot, verify_snapshot_index, SnapshotManifest};
use crate::snapshot_v1_validation::{canonical, digest, hex, MAX_EVENT_BYTES};
use crate::source_snapshot::{source_reference, SourceReference};
use crate::types::WikiPlan;
use crate::WikiError;
use buzz_core::kind::KIND_REPO_WIKI_PAGE;
use nostr::{Event, EventBuilder, Keys, Kind, Tag, Tags, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Maximum number of pages in one immutable Wiki snapshot.
pub const MAX_SNAPSHOT_PAGES: usize = 256;
/// Maximum serialized bytes across all events in one publication.
pub const MAX_PUBLICATION_BYTES: usize = 64 * 1024 * 1024;

/// Exact signed events that comprise one v1 publication.
///
/// `pages` and `manifest` are immutable dependencies of `head`.  A retry must
/// reuse these event values byte-for-byte; it must never sign a replacement
/// page or manifest after the operation has entered the journal.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SnapshotPublication {
    /// The addressable `_toc` head.  It is the only conditional write.
    pub head: Event,
    /// The immutable manifest event referenced by `head`.
    pub manifest: Event,
    /// Immutable page events in manifest traversal order.
    pub pages: Vec<Event>,
    /// UUID binding this manifest and every page together.
    pub snapshot_id: String,
    /// Exact captured source revision bound by the manifest and pages.
    pub source_revision: String,
    /// Live head ID required by the conditional TOC write, or `absent`.
    pub expected_revision: String,
}

/// Inputs for constructing one signed v1 publication.
pub struct SnapshotBuild<'a> {
    /// Repository owner public key in lowercase hex.
    pub owner: &'a str,
    /// NIP-34 repository identifier.
    pub repo_d: &'a str,
    /// Immutable captured source tree used for source references.
    pub snapshot: &'a RepoSnapshot,
    /// Deterministic section/page traversal.
    pub plan: &'a WikiPlan,
    /// Generated page content matching `plan`.
    pub drafts: &'a [PageDraft],
    /// Head refresh cadence.
    pub cadence: &'a str,
    /// Optional v4 UUID.  `None` creates a fresh snapshot ID.
    pub snapshot_id: Option<&'a str>,
    /// Current live `_toc` ID, or `None` for initial creation.
    pub expected_revision: Option<&'a str>,
    /// One timestamp used for every event in this publication.
    pub created_at: u64,
    /// Owner key used to sign every event.
    pub keys: &'a Keys,
}

/// Build, sign, and verify a complete v1 publication.
pub fn build_snapshot(input: SnapshotBuild<'_>) -> Result<SnapshotPublication, WikiError> {
    validate_build_inputs(&input)?;
    let snapshot_id = input
        .snapshot_id
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if !crate::snapshot_v1_validation::valid_uuid(&snapshot_id) {
        return Err(fail("invalid Wiki snapshot UUID"));
    }
    let expected_revision = input
        .expected_revision
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "absent".to_owned());
    validate_expected_revision(&expected_revision)?;

    let drafts_by_slug = index_drafts(input.plan, input.drafts, &input.snapshot.commit)?;
    let mut sections = Vec::with_capacity(input.plan.sections.len());
    let mut page_events = Vec::with_capacity(input.drafts.len());
    let mut page_references = Vec::with_capacity(input.drafts.len());

    for section in &input.plan.sections {
        let mut slugs = Vec::with_capacity(section.pages.len());
        for planned in &section.pages {
            let draft = drafts_by_slug
                .get(planned.slug.as_str())
                .ok_or_else(|| fail("Wiki plan and generated pages differ"))?;
            slugs.push(planned.slug.clone());
            let sources = full_source_references(input.snapshot, &draft.source_files)?;
            let envelope = (
                1_u8,
                &snapshot_id,
                input.owner,
                input.repo_d,
                &input.snapshot.source_revision,
                &draft.slug,
                &draft.title,
                &draft.section,
                &draft.language,
                &sources,
                &draft.content,
            );
            let envelope_digest = digest(&envelope)?;
            let encoded_slug = format!("p1-{envelope_digest}");
            let tags = page_tags(
                input.owner,
                input.repo_d,
                &snapshot_id,
                input.snapshot,
                &draft.slug,
                &encoded_slug,
                &draft.title,
                &draft.section,
                &draft.language,
                &sources,
            )?;
            let event = sign_event(draft.content.clone(), tags, input.created_at, input.keys)?;
            page_references.push((
                draft.slug.clone(),
                encoded_slug,
                event.id.to_hex(),
                envelope_digest,
                draft.title.clone(),
                draft.section.clone(),
                draft.language.clone(),
                sources,
            ));
            page_events.push(event);
        }
        sections.push((section.id.clone(), section.title.clone(), slugs));
    }

    if page_events.is_empty() || page_events.len() > MAX_SNAPSHOT_PAGES {
        return Err(fail("Wiki publication page count is outside its bounds"));
    }
    let manifest: SnapshotManifest = (
        1,
        snapshot_id.clone(),
        input.owner.to_owned(),
        input.repo_d.to_owned(),
        input.snapshot.source_revision.clone(),
        None,
        sections,
        page_references,
    );
    let manifest_content = canonical(&manifest)?;
    let manifest_digest = digest(&manifest)?;
    let manifest_tags = common_tags(
        input.owner,
        input.repo_d,
        &snapshot_id,
        input.snapshot,
        &format!("m1-{manifest_digest}"),
    )?;
    let manifest_event = sign_event(
        manifest_content,
        manifest_tags,
        input.created_at,
        input.keys,
    )?;

    let head_content = projection(&manifest)?;
    let head_tags = head_tags(
        input.owner,
        input.repo_d,
        &snapshot_id,
        input.snapshot,
        &manifest_event,
        &manifest_digest,
        &expected_revision,
        input.cadence,
    )?;
    let head = sign_event(head_content, head_tags, input.created_at, input.keys)?;
    let publication = SnapshotPublication {
        head,
        manifest: manifest_event,
        pages: page_events,
        snapshot_id,
        source_revision: input.snapshot.source_revision.clone(),
        expected_revision,
    };
    verify_publication(input.owner, input.repo_d, &publication)?;
    Ok(publication)
}

/// Build a cadence-only replacement for an already verified v1 snapshot.
///
/// The supplied page and manifest events are checked as dependencies and then
/// cloned into the returned publication.  Only the head's existing `cadence`
/// tag and `expected-revision` tag are changed; every other tag keeps its exact
/// order and value, including tags unknown to this version of Crew.
#[allow(clippy::too_many_arguments)]
pub fn build_cadence_update(
    owner: &str,
    repo_d: &str,
    current_head: &Event,
    manifest: &Event,
    pages: &[Event],
    cadence: &str,
    created_at: u64,
    keys: &Keys,
) -> Result<SnapshotPublication, WikiError> {
    if keys.public_key().to_hex() != owner || !hex(owner, 64) {
        return Err(fail("Wiki signing owner does not match the native key"));
    }
    let head_value = event_value(current_head)?;
    let manifest_value = event_value(manifest)?;
    let page_values: Vec<Value> = pages.iter().map(event_value).collect::<Result<_, _>>()?;
    let verified = verify_snapshot(owner, repo_d, &head_value, &manifest_value, &page_values)?;
    crate::snapshot_v1_validation::valid_revision(verified.index().source_revision())
        .then_some(())
        .ok_or_else(|| fail("Wiki source revision is invalid"))?;
    validate_cadence(cadence)?;
    if current_head.created_at.as_secs() >= created_at {
        return Err(fail("Wiki cadence update timestamp must advance the head"));
    }
    let mut tags: Vec<Tag> = current_head.tags.iter().cloned().collect();
    replace_single_tag(&mut tags, "cadence", cadence, true)?;
    replace_single_tag(
        &mut tags,
        "expected-revision",
        &current_head.id.to_hex(),
        false,
    )?;
    let next_head = sign_event(current_head.content.clone(), tags, created_at, keys)?;
    let next_head_value = event_value(&next_head)?;
    verify_snapshot_index(owner, repo_d, &next_head_value, &manifest_value)?;
    let manifest_tuple = verified.index().manifest();
    let expected_revision = current_head.id.to_hex();
    let publication = SnapshotPublication {
        head: next_head,
        manifest: manifest.clone(),
        pages: pages.to_vec(),
        snapshot_id: manifest_tuple.1.clone(),
        source_revision: manifest_tuple.4.clone(),
        expected_revision,
    };
    verify_publication(owner, repo_d, &publication)?;
    Ok(publication)
}

fn validate_build_inputs(input: &SnapshotBuild<'_>) -> Result<(), WikiError> {
    if !hex(input.owner, 64)
        || input.keys.public_key().to_hex() != input.owner
        || !crate::snapshot_v1_validation::valid_repo(input.repo_d)
        || !crate::snapshot_v1_validation::valid_revision(&input.snapshot.source_revision)
        || input.snapshot.files.len() > crate::source_snapshot::MAX_SOURCE_FILES
        || input.drafts.len() > MAX_SNAPSHOT_PAGES
    {
        return Err(fail("invalid Wiki publication input"));
    }
    validate_cadence(input.cadence)?;
    if input.created_at > 9_007_199_254_740_991 {
        return Err(fail("Wiki publication timestamp is outside the safe range"));
    }
    if input.snapshot.source_revision.starts_with("git:") {
        let commit = input
            .snapshot
            .source_revision
            .strip_prefix("git:")
            .ok_or_else(|| fail("invalid Git source revision"))?;
        if input.snapshot.commit != commit {
            return Err(fail(
                "Wiki snapshot commit differs from its source revision",
            ));
        }
    }
    if input.drafts.is_empty() {
        return Err(fail("Wiki publication requires at least one page"));
    }
    Ok(())
}

fn validate_cadence(cadence: &str) -> Result<(), WikiError> {
    buzz_core::wiki_page::WikiCadence::parse(cadence)
        .map(|_| ())
        .map_err(|_| fail("invalid Wiki cadence"))
}

fn validate_expected_revision(value: &str) -> Result<(), WikiError> {
    if value != "absent" && !hex(value, 64) {
        return Err(fail("invalid Wiki expected revision"));
    }
    Ok(())
}

fn index_drafts<'a>(
    plan: &WikiPlan,
    drafts: &'a [PageDraft],
    snapshot_commit: &str,
) -> Result<BTreeMap<&'a str, &'a PageDraft>, WikiError> {
    let mut planned_by_slug = BTreeMap::new();
    for section in &plan.sections {
        for planned in &section.pages {
            if planned_by_slug
                .insert(planned.slug.as_str(), planned)
                .is_some()
            {
                return Err(fail("Wiki plan contains duplicate or missing pages"));
            }
        }
    }
    if planned_by_slug.len() != drafts.len() {
        return Err(fail("Wiki plan contains duplicate or missing pages"));
    }
    let mut indexed = BTreeMap::new();
    for draft in drafts {
        let planned = planned_by_slug
            .get(draft.slug.as_str())
            .ok_or_else(|| fail("Wiki generated page does not match its plan"))?;
        if draft.slug.is_empty()
            || !crate::snapshot_v1_validation::valid_slug(&draft.slug)
            || draft.title != planned.title
            || draft.section != planned.section
            || draft.source_files != planned.source_files
            || draft.language != plan.language
            || draft.commit != snapshot_commit
            || draft.title.is_empty()
            || draft.section.is_empty()
            || draft.content.trim().is_empty()
            || draft.content.len() > MAX_EVENT_BYTES
            || draft.content.contains('\0')
            || draft.source_files.iter().any(|path| path.contains('\0'))
        {
            return Err(fail("Wiki generated page does not match its plan"));
        }
        if indexed.insert(draft.slug.as_str(), draft).is_some() {
            return Err(fail("Wiki generated pages contain a duplicate slug"));
        }
    }
    if indexed.len() != planned_by_slug.len() {
        return Err(fail("Wiki plan contains duplicate or missing pages"));
    }
    Ok(indexed)
}

/*
 * Keep this check beside signing.  A body bound alone does not account for
 * authenticated tags, IDs and signatures, and a later `Value` conversion
 * would allocate the complete event before discovering an oversized result.
 */
fn ensure_event_size(event: &Event) -> Result<(), WikiError> {
    struct Budget(usize);
    impl std::io::Write for Budget {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(std::io::Error::other("signed Wiki event exceeds limit"));
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Budget(MAX_EVENT_BYTES), event)
        .map_err(|_| fail("Wiki event exceeds its 192 KiB bound"))
}

fn full_source_references(
    snapshot: &RepoSnapshot,
    paths: &[String],
) -> Result<Vec<SourceReference>, WikiError> {
    let mut references = Vec::with_capacity(paths.len());
    let mut seen = BTreeSet::new();
    for path in paths {
        if !seen.insert(path) {
            return Err(fail("Wiki page contains a duplicate source path"));
        }
        let content = snapshot
            .contents
            .get(path)
            .ok_or_else(|| fail("Wiki page source is unavailable in the captured revision"))?;
        let lines = if content.is_empty() {
            0
        } else {
            content.split_terminator('\n').count() as u64
        };
        if lines == 0 {
            return Err(fail("Wiki page source has no addressable lines"));
        }
        references.push(source_reference(snapshot, path, 1, lines)?);
    }
    references.sort();
    Ok(references)
}

fn common_tags(
    owner: &str,
    repo_d: &str,
    snapshot_id: &str,
    snapshot: &RepoSnapshot,
    slug: &str,
) -> Result<Vec<Tag>, WikiError> {
    let source_kind = source_kind(snapshot)?;
    let commit = commit_tag(snapshot)?;
    let coordinate = format!("30617:{owner}:{repo_d}");
    let d = format!("{repo_d}/{slug}");
    let mut tags = vec![
        tag2("d", d)?,
        tag2("a", coordinate)?,
        tag2("wiki-version", "1")?,
        tag2("wiki-snapshot", snapshot_id)?,
        tag2("source-kind", source_kind)?,
        tag2("commit", commit)?,
    ];
    if source_kind == "git" && !snapshot.branch.is_empty() {
        tags.push(tag2("branch", snapshot.branch.clone())?);
    }
    Ok(tags)
}

#[allow(clippy::too_many_arguments)]
fn page_tags(
    owner: &str,
    repo_d: &str,
    snapshot_id: &str,
    snapshot: &RepoSnapshot,
    logical_slug: &str,
    encoded_slug: &str,
    title: &str,
    section: &str,
    language: &str,
    sources: &[SourceReference],
) -> Result<Vec<Tag>, WikiError> {
    let mut tags = common_tags(owner, repo_d, snapshot_id, snapshot, encoded_slug)?;
    tags.extend([
        tag2("wiki-slug", logical_slug)?,
        tag2("title", title)?,
        tag2("section", section)?,
        tag2("language", language)?,
        tag2("wiki-source-files", canonical(sources)?)?,
    ]);
    let mut seen = BTreeSet::new();
    for source in sources {
        if seen.insert(&source.0) {
            tags.push(tag2("source", source.0.clone())?);
        }
    }
    Ok(tags)
}

#[allow(clippy::too_many_arguments)]
fn head_tags(
    owner: &str,
    repo_d: &str,
    snapshot_id: &str,
    snapshot: &RepoSnapshot,
    manifest: &Event,
    manifest_digest: &str,
    expected_revision: &str,
    cadence: &str,
) -> Result<Vec<Tag>, WikiError> {
    let mut tags = common_tags(owner, repo_d, snapshot_id, snapshot, "_toc")?;
    tags.push(tag3(
        "wiki-manifest",
        manifest.id.to_hex(),
        manifest_digest.to_owned(),
    )?);
    tags.push(tag2("cadence", cadence)?);
    tags.push(tag2("expected-revision", expected_revision)?);
    Ok(tags)
}

fn source_kind(snapshot: &RepoSnapshot) -> Result<&'static str, WikiError> {
    if snapshot.source_revision.starts_with("git:") {
        Ok("git")
    } else if snapshot.source_revision.starts_with("folder:") {
        Ok("folder")
    } else {
        Err(fail("invalid Wiki source kind"))
    }
}

fn commit_tag(snapshot: &RepoSnapshot) -> Result<String, WikiError> {
    if let Some(commit) = snapshot.source_revision.strip_prefix("git:") {
        Ok(commit.to_owned())
    } else {
        Ok(snapshot.source_revision.clone())
    }
}

fn tag2(name: &str, value: impl Into<String>) -> Result<Tag, WikiError> {
    Tag::parse([name.to_owned(), value.into()]).map_err(|_| fail("invalid Wiki event tag"))
}

fn tag3(name: &str, first: impl Into<String>, second: impl Into<String>) -> Result<Tag, WikiError> {
    Tag::parse([name.to_owned(), first.into(), second.into()])
        .map_err(|_| fail("invalid Wiki event tag"))
}

fn replace_single_tag(
    tags: &mut Vec<Tag>,
    name: &str,
    value: &str,
    append_if_missing: bool,
) -> Result<(), WikiError> {
    let mut matches = tags.iter_mut().filter(|tag| {
        tag.as_slice()
            .first()
            .is_some_and(|candidate| candidate == name)
    });
    if let Some(first) = matches.next() {
        if matches.next().is_some() || first.as_slice().len() != 2 {
            return Err(fail(
                "Wiki head contains duplicate or malformed mutable metadata",
            ));
        }
        *first = tag2(name, value)?;
        return Ok(());
    }
    if append_if_missing {
        tags.push(tag2(name, value)?);
        return Ok(());
    }
    Err(fail("Wiki head is missing its expected revision"))
}

fn sign_event(
    content: String,
    tags: Vec<Tag>,
    created_at: u64,
    keys: &Keys,
) -> Result<Event, WikiError> {
    let event = EventBuilder::new(Kind::Custom(KIND_REPO_WIKI_PAGE as u16), content)
        .tags(Tags::from_list(tags))
        .custom_created_at(Timestamp::from_secs(created_at))
        .sign_with_keys(keys)
        .map_err(|error| fail(&format!("could not sign Wiki event: {error}")))?;
    ensure_event_size(&event)?;
    Ok(event)
}

fn event_value(event: &Event) -> Result<Value, WikiError> {
    serde_json::to_value(event).map_err(|_| fail("could not encode signed Wiki event"))
}

fn verify_publication(
    owner: &str,
    repo_d: &str,
    publication: &SnapshotPublication,
) -> Result<(), WikiError> {
    if publication.pages.len() > MAX_SNAPSHOT_PAGES {
        return Err(fail("Wiki publication has too many pages"));
    }
    let head = event_value(&publication.head)?;
    let manifest = event_value(&publication.manifest)?;
    let pages: Vec<Value> = publication
        .pages
        .iter()
        .map(event_value)
        .collect::<Result<_, _>>()?;
    verify_snapshot(owner, repo_d, &head, &manifest, &pages)?;
    let mut bytes = 0_usize;
    for event in std::iter::once(&publication.head)
        .chain(std::iter::once(&publication.manifest))
        .chain(publication.pages.iter())
    {
        let encoded = serde_json::to_vec(event).map_err(|_| fail("could not size Wiki event"))?;
        if encoded.len() > MAX_EVENT_BYTES {
            return Err(fail("Wiki event exceeds its 192 KiB bound"));
        }
        bytes = bytes
            .checked_add(encoded.len())
            .ok_or_else(|| fail("Wiki publication size overflow"))?;
    }
    if bytes > MAX_PUBLICATION_BYTES {
        return Err(fail("Wiki publication exceeds its 64 MiB bound"));
    }
    if publication.expected_revision != expected_revision_from_head(&publication.head)? {
        return Err(fail("Wiki publication expected revision is inconsistent"));
    }
    Ok(())
}

fn expected_revision_from_head(head: &Event) -> Result<String, WikiError> {
    let mut found: Option<String> = None;
    for tag in head.tags.iter() {
        if tag
            .as_slice()
            .first()
            .is_some_and(|name| name == "expected-revision")
        {
            if found.is_some() || tag.as_slice().len() != 2 {
                return Err(fail("Wiki head contains duplicate expected revision tags"));
            }
            found = tag.as_slice().get(1).cloned();
        }
    }
    found.ok_or_else(|| fail("Wiki head is missing its expected revision"))
}

fn fail(message: &str) -> WikiError {
    WikiError::Publish(message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_snapshot::RepoSnapshot;
    use crate::types::{PlannedPage, PlannedSection};
    use std::collections::BTreeMap;

    pub(super) fn fixture() -> (Keys, RepoSnapshot, WikiPlan, Vec<PageDraft>) {
        let keys = Keys::generate();
        let owner = keys.public_key().to_hex();
        let content = "fn main() {}\n".to_owned();
        let snapshot = RepoSnapshot {
            commit: "a".repeat(40),
            branch: "main".into(),
            source_revision: format!("git:{}", "a".repeat(40)),
            files: vec!["src/main.rs".into()],
            contents: BTreeMap::from([("src/main.rs".into(), content)]),
            omissions: Vec::new(),
        };
        let plan = WikiPlan {
            language: "en".into(),
            sections: vec![PlannedSection {
                id: "overview".into(),
                title: "Overview".into(),
                pages: vec![PlannedPage {
                    slug: "overview".into(),
                    title: "Overview".into(),
                    section: "overview".into(),
                    source_files: vec!["src/main.rs".into()],
                }],
            }],
        };
        let drafts = vec![PageDraft {
            slug: "overview".into(),
            title: "Overview".into(),
            section: "overview".into(),
            source_files: vec!["src/main.rs".into()],
            commit: snapshot.commit.clone(),
            language: "en".into(),
            content: "# Overview\n".into(),
        }];
        assert_eq!(owner, keys.public_key().to_hex());
        (keys, snapshot, plan, drafts)
    }

    #[test]
    fn producer_output_is_accepted_by_native_verifier() {
        let (keys, snapshot, plan, drafts) = fixture();
        let owner = keys.public_key().to_hex();
        let publication = build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "Repo.demo",
            snapshot: &snapshot,
            plan: &plan,
            drafts: &drafts,
            cadence: "manual",
            snapshot_id: Some("12345678-1234-4234-9234-123456789abc"),
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
        .expect("build");
        verify_publication(&owner, "Repo.demo", &publication).expect("verify");
        assert_eq!(publication.pages.len(), 1);
        assert_eq!(
            expected_revision_from_head(&publication.head).unwrap(),
            "absent"
        );
    }

    #[test]
    fn cadence_update_reuses_dependencies_and_only_changes_mutable_tags() {
        let (keys, snapshot, plan, drafts) = fixture();
        let owner = keys.public_key().to_hex();
        let first = build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "Repo.demo",
            snapshot: &snapshot,
            plan: &plan,
            drafts: &drafts,
            cadence: "manual",
            snapshot_id: None,
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
        .expect("build");
        let next = build_cadence_update(
            &owner,
            "Repo.demo",
            &first.head,
            &first.manifest,
            &first.pages,
            "daily",
            11,
            &keys,
        )
        .expect("cadence");
        assert_eq!(next.manifest, first.manifest);
        assert_eq!(next.pages, first.pages);
        assert_eq!(next.expected_revision, first.head.id.to_hex());
        assert_eq!(
            next.head
                .tags
                .iter()
                .find(|tag| tag.as_slice().first().is_some_and(|name| name == "cadence"))
                .expect("cadence")
                .as_slice(),
            ["cadence", "daily"]
        );
        verify_publication(&owner, "Repo.demo", &next).expect("verify update");
    }

    #[test]
    fn oversized_page_is_rejected_before_signing_result() {
        let (keys, snapshot, plan, mut drafts) = fixture();
        drafts[0].content = "x".repeat(MAX_EVENT_BYTES);
        let owner = keys.public_key().to_hex();
        assert!(build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "Repo.demo",
            snapshot: &snapshot,
            plan: &plan,
            drafts: &drafts,
            cadence: "manual",
            snapshot_id: None,
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
        .is_err());
    }

    #[test]
    fn blank_page_and_empty_source_are_rejected_at_the_production_builder() {
        let (keys, snapshot, plan, mut drafts) = fixture();
        let owner = keys.public_key().to_hex();
        drafts[0].content = " \n\t".into();
        assert!(build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "Repo.demo",
            snapshot: &snapshot,
            plan: &plan,
            drafts: &drafts,
            cadence: "manual",
            snapshot_id: None,
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
        .is_err());

        let mut empty_snapshot = snapshot;
        empty_snapshot
            .contents
            .insert("src/main.rs".into(), String::new());
        drafts[0].content = "# Overview\n".into();
        assert!(build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "Repo.demo",
            snapshot: &empty_snapshot,
            plan: &plan,
            drafts: &drafts,
            cadence: "manual",
            snapshot_id: None,
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
        .is_err());
    }

    #[test]
    fn mismatched_plan_source_and_draft_metadata_are_rejected() {
        let (keys, snapshot, plan, drafts) = fixture();
        let owner = keys.public_key().to_hex();

        let mut mismatched_plan = plan.clone();
        mismatched_plan.sections[0].pages[0].title = "Different".into();
        assert!(build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "Repo.demo",
            snapshot: &snapshot,
            plan: &mismatched_plan,
            drafts: &drafts,
            cadence: "manual",
            snapshot_id: None,
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
        .is_err());

        let mut mismatched_draft = drafts.clone();
        mismatched_draft[0].source_files = vec!["other.rs".into()];
        assert!(build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "Repo.demo",
            snapshot: &snapshot,
            plan: &plan,
            drafts: &mismatched_draft,
            cadence: "manual",
            snapshot_id: None,
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
        .is_err());
    }

    #[test]
    fn signed_event_limits_cover_ascii_multibyte_and_escaped_content() {
        let (keys, snapshot, plan, mut drafts) = fixture();
        let owner = keys.public_key().to_hex();
        let near_limit_ascii = MAX_EVENT_BYTES - 4096;
        drafts[0].content = "x".repeat(near_limit_ascii);
        let ascii = build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "Repo.demo",
            snapshot: &snapshot,
            plan: &plan,
            drafts: &drafts,
            cadence: "manual",
            snapshot_id: None,
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
        .expect("ASCII near-bound page remains valid");
        verify_publication(&owner, "Repo.demo", &ascii).expect("ASCII publication");

        drafts[0].content = "ế".repeat((MAX_EVENT_BYTES - 4096) / "ế".len());
        let multibyte = build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "Repo.demo",
            snapshot: &snapshot,
            plan: &plan,
            drafts: &drafts,
            cadence: "manual",
            snapshot_id: None,
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
        .expect("multibyte near-bound page remains valid");
        verify_publication(&owner, "Repo.demo", &multibyte).expect("multibyte publication");

        drafts[0].content = "quoted \"text\"\\with\\slashes\nand a newline".into();
        let escaped = build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "Repo.demo",
            snapshot: &snapshot,
            plan: &plan,
            drafts: &drafts,
            cadence: "manual",
            snapshot_id: None,
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
        .expect("escaped signed content remains valid");
        assert_eq!(escaped.pages[0].content, drafts[0].content);
        verify_publication(&owner, "Repo.demo", &escaped).expect("escaped publication");
    }

    #[test]
    fn oversized_multibyte_page_is_rejected_before_publication() {
        let (keys, snapshot, plan, mut drafts) = fixture();
        let owner = keys.public_key().to_hex();
        drafts[0].content = "ế".repeat(MAX_EVENT_BYTES / "ế".len() + 1);
        assert!(build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "Repo.demo",
            snapshot: &snapshot,
            plan: &plan,
            drafts: &drafts,
            cadence: "manual",
            snapshot_id: None,
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
        .is_err());
    }
}

#[cfg(test)]
#[path = "snapshot_v1_build_regeneration_tests.rs"]
mod regeneration_tests;
