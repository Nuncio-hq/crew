//! Native verification of signed Wiki snapshot membership.
//!
//! Cryptographic validity does not establish relay acceptance or current access.
//! Callers must separately establish those properties before opening source files.
use crate::snapshot_v1_validation::{
    canonical, digest, hex, invalid, metadata, tag, valid_repo, valid_revision, valid_slug,
    valid_sources, valid_uuid, verify_event, SignedEvent,
};
use crate::source_snapshot::SourceReference;
use crate::WikiError;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Canonical section identity, display title and logical page order.
pub type SnapshotSection = (String, String, Vec<String>);
/// Canonical logical slug, encoded slug, event ID, envelope digest, metadata and source references.
pub type SnapshotPageReference = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Vec<SourceReference>,
);
/// Fixed-order canonical manifest; integers and strings retain their exact wire meaning.
pub type SnapshotManifest = (
    u8,
    String,
    String,
    String,
    String,
    Option<String>,
    Vec<SnapshotSection>,
    Vec<SnapshotPageReference>,
);

/// Verified signed index; it does not assert relay acceptance or current access.
#[derive(Debug)]
pub struct VerifiedSnapshotIndex {
    head: SignedEvent,
    manifest_event: SignedEvent,
    manifest: SnapshotManifest,
}

/// One page authenticated against an immutable verified manifest.
#[derive(Debug)]
pub struct VerifiedSnapshotPage {
    event: SignedEvent,
    reference: SnapshotPageReference,
}

impl VerifiedSnapshotPage {
    /// Exact signed event ID.
    pub fn event_id(&self) -> &str {
        &self.event.id
    }
    /// Exact signed Markdown, without normalization or trimming.
    pub fn content(&self) -> &str {
        &self.event.content
    }
    /// Authenticated source locators; local byte verification is still required.
    pub fn source_references(&self) -> &[SourceReference] {
        &self.reference.7
    }
}

impl VerifiedSnapshotIndex {
    /// Exact signed head ID; its existence alone does not prove relay acceptance.
    pub fn head_event_id(&self) -> &str {
        &self.head.id
    }
    /// Exact immutable manifest event ID.
    pub fn manifest_event_id(&self) -> &str {
        &self.manifest_event.id
    }
    /// Typed revision bound by every page in this manifest.
    pub fn source_revision(&self) -> &str {
        &self.manifest.4
    }
    /// Canonical authenticated metadata and membership.
    pub fn manifest(&self) -> &SnapshotManifest {
        &self.manifest
    }

    /// Authenticate one signed page and its exact inclusion before deriving locators.
    pub fn verify_page(&self, raw: &Value) -> Result<VerifiedSnapshotPage, WikiError> {
        let event = verify_event(raw)?;
        let reference = self
            .manifest
            .7
            .iter()
            .find(|r| r.2 == event.id)
            .ok_or_else(invalid)?;
        common_tags(&event, &reference.1, &self.manifest)?;
        let envelope = (
            1_u8,
            &self.manifest.1,
            &self.manifest.2,
            &self.manifest.3,
            &self.manifest.4,
            &reference.0,
            &reference.4,
            &reference.5,
            &reference.6,
            &reference.7,
            &event.content,
        );
        if digest(&envelope)? != reference.3 || reference.1 != format!("p1-{}", reference.3) {
            return Err(invalid());
        }
        let source_json = canonical(&reference.7)?;
        for (name, value) in [
            ("wiki-slug", &reference.0),
            ("title", &reference.4),
            ("section", &reference.5),
            ("language", &reference.6),
            ("wiki-source-files", &source_json),
        ] {
            if tag(&event, name, 2)?[1] != *value {
                return Err(invalid());
            }
        }
        let mut seen = BTreeSet::new();
        let expected: Vec<Vec<String>> = reference
            .7
            .iter()
            .filter(|s| seen.insert(&s.0))
            .map(|s| vec!["source".into(), s.0.clone()])
            .collect();
        let actual: Vec<_> = event
            .tags
            .iter()
            .filter(|t| t.first().is_some_and(|v| v == "source"))
            .cloned()
            .collect();
        if actual != expected {
            return Err(invalid());
        }
        Ok(VerifiedSnapshotPage {
            event,
            reference: reference.clone(),
        })
    }
}

/// A complete verified display snapshot, separate from native access authorization.
#[derive(Debug)]
pub struct VerifiedSnapshot {
    index: VerifiedSnapshotIndex,
    pages: Vec<VerifiedSnapshotPage>,
}
impl VerifiedSnapshot {
    /// Authenticated index used by all pages.
    pub fn index(&self) -> &VerifiedSnapshotIndex {
        &self.index
    }
    /// Authenticated pages in section traversal order.
    pub fn pages(&self) -> &[VerifiedSnapshotPage] {
        &self.pages
    }
}

/// Verify the complete signed snapshot for one exact repository owner/identifier.
pub fn verify_snapshot(
    owner: &str,
    repo_d: &str,
    head: &Value,
    manifest: &Value,
    pages: &[Value],
) -> Result<VerifiedSnapshot, WikiError> {
    if pages.len() > 256 {
        return Err(invalid());
    }
    let index = verify_snapshot_index(owner, repo_d, head, manifest)?;
    if pages.len() != index.manifest.7.len() {
        return Err(invalid());
    }
    let mut by_id = BTreeMap::new();
    for raw in pages {
        let page = index.verify_page(raw)?;
        if by_id.insert(page.event.id.clone(), page).is_some() {
            return Err(invalid());
        }
    }
    let mut verified = Vec::with_capacity(pages.len());
    for reference in &index.manifest.7 {
        verified.push(by_id.remove(&reference.2).ok_or_else(invalid)?);
    }
    Ok(VerifiedSnapshot {
        index,
        pages: verified,
    })
}

/// Authenticate head and manifest before deriving page queries or membership.
pub fn verify_snapshot_index(
    owner: &str,
    repo_d: &str,
    head: &Value,
    manifest: &Value,
) -> Result<VerifiedSnapshotIndex, WikiError> {
    if !hex(owner, 64) || !valid_repo(repo_d) {
        return Err(invalid());
    }
    let head = verify_event(head)?;
    let manifest_event = verify_event(manifest)?;
    let manifest: SnapshotManifest =
        serde_json::from_str(&manifest_event.content).map_err(|_| invalid())?;
    if canonical(&manifest)? != manifest_event.content
        || manifest.2 != owner
        || manifest.3 != repo_d
    {
        return Err(invalid());
    }
    validate_manifest(&manifest)?;
    let hash = digest(&manifest)?;
    common_tags(&head, "_toc", &manifest)?;
    common_tags(&manifest_event, &format!("m1-{hash}"), &manifest)?;
    let reference = tag(&head, "wiki-manifest", 3)?;
    if reference[1] != manifest_event.id || reference[2] != hash {
        return Err(invalid());
    }
    let expected = tag(&head, "expected-revision", 2)?;
    if expected[1] != "absent" && !hex(&expected[1], 64) {
        return Err(invalid());
    }
    if head.content != projection(&manifest)? {
        return Err(invalid());
    }
    Ok(VerifiedSnapshotIndex {
        head,
        manifest_event,
        manifest,
    })
}

fn common_tags(
    event: &SignedEvent,
    slug: &str,
    manifest: &SnapshotManifest,
) -> Result<(), WikiError> {
    let folder = manifest.4.starts_with("folder:");
    if event.pubkey != manifest.2
        || (folder
            && event
                .tags
                .iter()
                .any(|t| t.first().is_some_and(|v| v == "branch")))
    {
        return Err(invalid());
    }
    let coordinate = format!("30617:{}:{}", manifest.2, manifest.3);
    let d = format!("{}/{slug}", manifest.3);
    let commit = if folder {
        manifest.4.as_str()
    } else {
        manifest.4.strip_prefix("git:").ok_or_else(invalid)?
    };
    for (name, value) in [
        ("d", d.as_str()),
        ("a", coordinate.as_str()),
        ("wiki-version", "1"),
        ("wiki-snapshot", manifest.1.as_str()),
        ("source-kind", if folder { "folder" } else { "git" }),
        ("commit", commit),
    ] {
        if tag(event, name, 2)?[1] != value {
            return Err(invalid());
        }
    }
    Ok(())
}

fn validate_manifest(m: &SnapshotManifest) -> Result<(), WikiError> {
    if m.0 != 1
        || !valid_uuid(&m.1)
        || !hex(&m.2, 64)
        || !valid_repo(&m.3)
        || !valid_revision(&m.4)
        || m.5.as_ref().is_some_and(|id| !hex(id, 64))
        || m.7.is_empty()
        || m.7.len() > 256
    {
        return Err(invalid());
    }
    let mut traversal = Vec::new();
    let mut sections = BTreeSet::new();
    let mut membership = BTreeMap::new();
    for (id, title, slugs) in &m.6 {
        if !metadata(id) || !metadata(title) || !sections.insert(id) {
            return Err(invalid());
        }
        for slug in slugs {
            if !valid_slug(slug) || membership.insert(slug, id).is_some() {
                return Err(invalid());
            }
            traversal.push(slug);
        }
    }
    if traversal.len() != m.7.len() {
        return Err(invalid());
    }
    let mut ids = BTreeSet::new();
    for (reference, logical) in m.7.iter().zip(traversal) {
        if &reference.0 != logical
            || !hex(&reference.3, 64)
            || reference.1 != format!("p1-{}", reference.3)
            || !hex(&reference.2, 64)
            || !ids.insert(&reference.2)
            || !metadata(&reference.4)
            || !metadata(&reference.6)
            || membership.get(logical).copied() != Some(&reference.5)
            || !valid_sources(&reference.7)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct HeadProjection<'a> {
    sections: Vec<SectionProjection<'a>>,
}
#[derive(Serialize)]
struct SectionProjection<'a> {
    id: &'a str,
    title: &'a str,
    pages: Vec<PageProjection<'a>>,
}
#[derive(Serialize)]
struct PageProjection<'a> {
    slug: &'a str,
    title: &'a str,
}
fn projection(manifest: &SnapshotManifest) -> Result<String, WikiError> {
    let mut sections = Vec::new();
    let by_slug: BTreeMap<_, _> = manifest.7.iter().map(|p| (&p.0, p)).collect();
    for (id, title, slugs) in &manifest.6 {
        let mut pages = Vec::new();
        for slug in slugs {
            let page = by_slug.get(slug).ok_or_else(invalid)?;
            pages.push(PageProjection {
                slug: &page.1,
                title: &page.4,
            });
        }
        sections.push(SectionProjection { id, title, pages });
    }
    canonical(&HeadProjection { sections })
}
