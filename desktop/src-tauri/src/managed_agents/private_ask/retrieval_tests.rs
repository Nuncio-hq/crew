//! Retrieval, bound to the question.
//!
//! Each test names the production line whose removal makes it fail. The
//! snapshots are really signed and really verified — retrieval never sees an
//! unverified page — and the `read` closure is the viewer-granted reader the
//! native shell supplies, stubbed only at the byte boundary this module
//! defines it at.

use super::*;
use crew_wiki::snapshot_v1_build::{build_snapshot, SnapshotBuild, SnapshotPublication};
use nostr::{Keys, SecretKey};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

fn owner_keys() -> Keys {
    let mut bytes = [0_u8; 32];
    bytes[31] = 7;
    Keys::new(SecretKey::from_slice(&bytes).expect("owner key"))
}

/// One signed snapshot with two pages on distinct sources, so a question can
/// be pointed at either. `pairs` is (slug, title, body, source path, source
/// body) per page.
fn publication(pairs: &[(&str, &str, &str, &str, &str)]) -> SnapshotPublication {
    use crew_wiki::git_snapshot::RepoSnapshot;
    use crew_wiki::publish::PageDraft;
    use crew_wiki::types::{PlannedPage, PlannedSection, WikiPlan};

    let keys = owner_keys();
    let owner = keys.public_key().to_hex();
    let commit = "b".repeat(40);
    let snapshot = RepoSnapshot {
        commit: commit.clone(),
        branch: "main".into(),
        files: pairs.iter().map(|pair| pair.3.to_owned()).collect(),
        contents: pairs
            .iter()
            .map(|pair| (pair.3.to_owned(), pair.4.to_owned()))
            .collect::<BTreeMap<_, _>>(),
        source_revision: format!("git:{commit}"),
        omissions: Vec::new(),
    };
    let plan = WikiPlan {
        language: "en".into(),
        sections: vec![PlannedSection {
            id: "overview".into(),
            title: "Overview".into(),
            pages: pairs
                .iter()
                .map(|pair| PlannedPage {
                    slug: pair.0.into(),
                    title: pair.1.into(),
                    section: "overview".into(),
                    source_files: vec![pair.3.to_owned()],
                })
                .collect(),
        }],
    };
    let drafts: Vec<PageDraft> = pairs
        .iter()
        .map(|pair| PageDraft {
            slug: pair.0.into(),
            title: pair.1.into(),
            section: "overview".into(),
            source_files: vec![pair.3.to_owned()],
            commit: commit.clone(),
            language: "en".into(),
            content: pair.2.to_owned(),
        })
        .collect();
    build_snapshot(SnapshotBuild {
        owner: &owner,
        repo_d: "repo-a",
        snapshot: &snapshot,
        plan: &plan,
        drafts: &drafts,
        cadence: "manual",
        snapshot_id: Some("12345678-1234-4234-9234-123456789abc"),
        expected_revision: None,
        created_at: 10,
        keys: &keys,
    })
    .expect("a signed snapshot")
}

fn verified(publication: &SnapshotPublication) -> VerifiedSnapshot {
    let owner = owner_keys().public_key().to_hex();
    let head = serde_json::to_value(&publication.head).expect("head");
    let manifest = serde_json::to_value(&publication.manifest).expect("manifest");
    let pages: Vec<serde_json::Value> = publication
        .pages
        .iter()
        .map(|page| serde_json::to_value(page).expect("page"))
        .collect();
    crew_wiki::snapshot_v1::verify_snapshot(&owner, "repo-a", &head, &manifest, &pages)
        .expect("the fixture snapshot verifies")
}

/// The viewer-granted reader: looks up the source body by path and reports
/// its real line range, as `read_verified_reference` does for a whole file.
fn reading(
    files: &BTreeMap<String, String>,
) -> impl Fn(&str, &SourceReference) -> Result<VerifiedSourceFile, String> + '_ {
    move |_, reference| {
        let content = files
            .get(&reference.0)
            .cloned()
            .ok_or_else(|| "unreadable".to_string())?;
        Ok(VerifiedSourceFile {
            start_line: 1,
            end_line: content.lines().count() as u64,
            content,
        })
    }
}

fn two_page_snapshot() -> (VerifiedSnapshot, BTreeMap<String, String>) {
    let pairs = [
        (
            "apples",
            "Apples",
            "# Apples\nHow the orchard rows are grafted.\n",
            "src/apples.rs",
            "pub fn graft() {}\n",
        ),
        (
            "bridges",
            "Bridges",
            "# Bridges\nSuspension cables carry the deck load.\n",
            "src/bridges.rs",
            "pub fn anchor() {}\n",
        ),
    ];
    let files: BTreeMap<String, String> =
        pairs.iter().map(|pair| (pair.3.into(), pair.4.into())).collect();
    (verified(&publication(&pairs)), files)
}

fn retrieve_with(
    question: &str,
    snapshot: &VerifiedSnapshot,
    files: &BTreeMap<String, String>,
) -> Retrieval {
    retrieve(
        question,
        snapshot,
        Some("git"),
        Instant::now() + Duration::from_secs(30),
        reading(files),
    )
}

/// The "fixed hit" mutation bound: a question that names the second page must
/// not come back with the first page's content. Swap retrieval for "always the
/// first page" and this fails — which is exactly what the old canned grounding
/// did.
#[test]
fn the_question_decides_which_pages_are_consulted() {
    let (snapshot, files) = two_page_snapshot();

    let retrieval = retrieve_with(
        "How does a suspension bridge anchor its cables?",
        &snapshot,
        &files,
    );

    assert!(!retrieval.insufficient());
    assert_eq!(retrieval.pages.len(), 1);
    assert_eq!(retrieval.pages[0].slug, "bridges");
    // The other page is a hit with a score of zero — never a candidate, and
    // not even recorded as omitted: omission is for hits the bounds dropped.
    assert!(retrieval.manifest.omitted_pages.is_empty());
    assert_eq!(retrieval.manifest.included_pages[0].slug, "bridges");
    assert!(retrieval.manifest.included_pages[0].score > 0);
    // Its source was read through the grant and grounded.
    assert_eq!(retrieval.grounding.len(), 1);
    assert_eq!(retrieval.grounding[0].path(), "src/bridges.rs");
    assert_eq!(
        retrieval.manifest.included_sources[0].path,
        "src/bridges.rs"
    );
}

/// The honest "not enough source" outcome: a question the snapshot cannot
/// cover is not answered with the best of a bad lot.
#[test]
fn a_question_with_no_evidence_is_insufficient_not_answered_with_the_first_page() {
    let (snapshot, files) = two_page_snapshot();

    let retrieval = retrieve_with("How are glaciers calibrated?", &snapshot, &files);

    assert!(retrieval.insufficient());
    assert!(retrieval.pages.is_empty());
    assert!(retrieval.grounding.is_empty());
    assert!(retrieval.manifest.included_pages.is_empty());
    assert!(retrieval.manifest.included_sources.is_empty());
    assert!(retrieval.manifest.source_grant);
}

/// Stopwords and noise are not retrieval signal: a question made of them is
/// the same as no question.
#[test]
fn a_question_without_terms_is_insufficient() {
    let (snapshot, files) = two_page_snapshot();

    let retrieval = retrieve_with("what is the it?", &snapshot, &files);

    assert!(retrieval.insufficient());
    // The manifest still records that a grant existed — the statement is
    // about coverage, not about access.
    assert!(retrieval.manifest.source_grant);
}

/// Without a viewer grant there are no source bytes to read; page hits still
/// carry the Wiki's own text and the manifest says why sources are absent.
#[test]
fn without_a_source_grant_sources_are_omitted_not_read() {
    let (snapshot, files) = two_page_snapshot();

    let retrieval = retrieve(
        "How does a suspension bridge anchor its cables?",
        &snapshot,
        None,
        Instant::now() + Duration::from_secs(30),
        |_, _| -> Result<VerifiedSourceFile, String> {
            panic!("no grant means no read is even attempted")
        },
    );

    assert!(!retrieval.insufficient(), "the page hit still grounds the ask");
    assert_eq!(retrieval.pages.len(), 1);
    assert!(retrieval.grounding.is_empty());
    assert!(!retrieval.manifest.source_grant);
    assert_eq!(retrieval.manifest.omitted_sources.len(), 1);
    assert_eq!(retrieval.manifest.omitted_sources[0].path, "src/bridges.rs");
    let _ = files;
}

/// A grant anchored to a different checkout mode reads bytes the snapshot
/// never signed for — its sources are omitted, not grounded.
#[test]
fn a_grant_in_another_checkout_mode_reads_nothing() {
    let (snapshot, files) = two_page_snapshot();

    let retrieval = retrieve(
        "How does a suspension bridge anchor its cables?",
        &snapshot,
        Some("folder"),
        Instant::now() + Duration::from_secs(30),
        reading(&files),
    );

    assert_eq!(retrieval.pages.len(), 1);
    assert!(retrieval.grounding.is_empty());
    assert_eq!(retrieval.manifest.omitted_sources.len(), 1);
}

/// A source read that fails lands in `omitted` with its locator intact — the
/// manifest is the coverage record, not a fatal error.
#[test]
fn an_unreadable_source_is_omitted_and_named() {
    let (snapshot, _files) = two_page_snapshot();

    let retrieval = retrieve(
        "How does a suspension bridge anchor its cables?",
        &snapshot,
        Some("git"),
        Instant::now() + Duration::from_secs(30),
        |_, _| -> Result<VerifiedSourceFile, String> { Err("gone".into()) },
    );

    assert_eq!(retrieval.pages.len(), 1);
    assert!(retrieval.grounding.is_empty());
    assert_eq!(
        retrieval.manifest.omitted_sources[0].path,
        "src/bridges.rs"
    );
}

/// A read that returns the wrong bytes is refused by `GroundedSource`'s own
/// hash check and lands in `omitted` — retrieval never grounds unverified
/// bytes.
#[test]
fn tampered_source_bytes_never_reach_the_grounding() {
    let (snapshot, _files) = two_page_snapshot();

    let retrieval = retrieve(
        "How does a suspension bridge anchor its cables?",
        &snapshot,
        Some("git"),
        Instant::now() + Duration::from_secs(30),
        |_, _| -> Result<VerifiedSourceFile, String> {
            Ok(VerifiedSourceFile {
                content: "pub fn forged() {}\n".into(),
                start_line: 1,
                end_line: 1,
            })
        },
    );

    assert!(retrieval.grounding.is_empty());
    assert_eq!(retrieval.manifest.omitted_sources.len(), 1);
}

/// A page larger than its prompt bound is excerpted to the windows the
/// question hit, never dropped silently and never carried whole.
#[test]
fn a_large_page_is_excerpted_to_its_matching_windows() {
    let padding = "unrelated filler line\n".repeat(4000);
    let body = format!("# Big\n{padding}the keelhaul protocol is described here\n{padding}");
    let pairs = [
        (
            "big",
            "Big Page",
            body.as_str(),
            "src/big.rs",
            "pub fn keelhaul() {}\n",
        ),
        (
            "small",
            "Small",
            "# Small\nnothing relevant\n",
            "src/small.rs",
            "pub fn small() {}\n",
        ),
    ];
    let files: BTreeMap<String, String> =
        pairs.iter().map(|pair| (pair.3.into(), pair.4.into())).collect();
    let snapshot = verified(&publication(&pairs));

    let retrieval = retrieve_with("what is the keelhaul protocol?", &snapshot, &files);

    assert_eq!(retrieval.pages.len(), 1);
    let page = &retrieval.pages[0];
    assert!(page.excerpted, "a page past the bound is excerpted");
    assert!(page.content.len() <= PAGE_BYTES);
    assert!(
        page.content.contains("keelhaul protocol"),
        "the excerpt keeps the window the question hit"
    );
}

/// The page bound is a bound: more matching pages than the prompt may carry
/// land in `omitted_pages` with their scores, so the viewer sees the cut.
#[test]
fn pages_past_the_bound_are_omitted_and_scored() {
    let mut pairs: Vec<(String, String, String, String, String)> = Vec::new();
    for index in 0..9 {
        pairs.push((
            format!("page-{index}"),
            format!("Keelhaul Page {index}"),
            format!("# {index}\nkeelhaul cable rigging {index}\n"),
            format!("src/file{index}.rs"),
            format!("pub fn f{index}() {{}}\n"),
        ));
    }
    let pairs_ref: Vec<(&str, &str, &str, &str, &str)> = pairs
        .iter()
        .map(|p| (p.0.as_str(), p.1.as_str(), p.2.as_str(), p.3.as_str(), p.4.as_str()))
        .collect();
    let files: BTreeMap<String, String> = pairs
        .iter()
        .map(|pair| (pair.3.clone(), pair.4.clone()))
        .collect();
    let snapshot = verified(&publication(&pairs_ref));

    let retrieval = retrieve_with("keelhaul cable rigging", &snapshot, &files);

    assert_eq!(retrieval.pages.len(), MAX_PAGES);
    assert_eq!(retrieval.manifest.included_pages.len(), MAX_PAGES);
    assert_eq!(
        retrieval.manifest.omitted_pages.len(),
        pairs.len() - MAX_PAGES
    );
    assert!(
        retrieval
            .manifest
            .omitted_pages
            .iter()
            .all(|page| page.score > 0),
        "an omitted page was a real hit the bound dropped, not noise"
    );
}

/// The same range named by two pages is one read and one citation target.
#[test]
fn a_source_shared_by_two_pages_is_read_once() {
    let shared = "pub fn shared() {}\n";
    let pairs = [
        (
            "first",
            "Keelhaul First",
            "# First\nkeelhaul notes\n",
            "src/shared.rs",
            shared,
        ),
        (
            "second",
            "Keelhaul Second",
            "# Second\nkeelhaul notes\n",
            "src/shared.rs",
            shared,
        ),
    ];
    let files: BTreeMap<String, String> =
        pairs.iter().map(|pair| (pair.3.into(), pair.4.into())).collect();
    let snapshot = verified(&publication(&pairs));
    let reads = std::cell::Cell::new(0usize);

    let retrieval = retrieve(
        "keelhaul",
        &snapshot,
        Some("git"),
        Instant::now() + Duration::from_secs(30),
        |revision, reference| {
            reads.set(reads.get() + 1);
            reading(&files)(revision, reference)
        },
    );

    assert_eq!(retrieval.pages.len(), 2);
    assert_eq!(retrieval.grounding.len(), 1);
    assert_eq!(reads.get(), 1, "one deduplicated read per range");
}

/// A deadline that has already passed reads nothing and omits every source —
/// the budget is enforced at the read, not at the report.
#[test]
fn an_expired_deadline_still_returns_a_bounded_manifest() {
    let (snapshot, files) = two_page_snapshot();
    let reads = std::cell::Cell::new(0usize);

    let retrieval = retrieve(
        "How does a suspension bridge anchor its cables?",
        &snapshot,
        Some("git"),
        Instant::now() - Duration::from_secs(1),
        |revision, reference| {
            reads.set(reads.get() + 1);
            reading(&files)(revision, reference)
        },
    );

    assert_eq!(reads.get(), 0, "no read happens after the deadline");
    assert!(retrieval.grounding.is_empty());
    assert_eq!(retrieval.manifest.omitted_sources.len(), 1);
}

/// More named sources than the read cap are omitted past the cap — the read
/// count is proportional to the question, not the page's reference list.
#[test]
fn source_reads_are_capped_at_the_per_question_bound() {
    let mut source_files = Vec::new();
    for index in 0..(MAX_SOURCES + 5) {
        source_files.push(format!("src/cap{index}.rs"));
    }
    let files: BTreeMap<String, String> = source_files
        .iter()
        .map(|path| (path.clone(), "pub fn cap() {}\n".to_string()))
        .collect();
    // One page naming every file: the cap, not the page, decides the reads.
    let keys = owner_keys();
    let owner = keys.public_key().to_hex();
    let commit = "b".repeat(40);
    let publication = {
        use crew_wiki::git_snapshot::RepoSnapshot;
        use crew_wiki::publish::PageDraft;
        use crew_wiki::types::{PlannedPage, PlannedSection, WikiPlan};
        let snapshot = RepoSnapshot {
            commit: commit.clone(),
            branch: "main".into(),
            files: source_files.clone(),
            contents: files.clone(),
            source_revision: format!("git:{commit}"),
            omissions: Vec::new(),
        };
        let plan = WikiPlan {
            language: "en".into(),
            sections: vec![PlannedSection {
                id: "overview".into(),
                title: "Overview".into(),
                pages: vec![PlannedPage {
                    slug: "capped".into(),
                    title: "Keelhaul".into(),
                    section: "overview".into(),
                    source_files: source_files.clone(),
                }],
            }],
        };
        let drafts = vec![PageDraft {
            slug: "capped".into(),
            title: "Keelhaul".into(),
            section: "overview".into(),
            source_files: source_files.clone(),
            commit: commit.clone(),
            language: "en".into(),
            content: "# Keelhaul\nkeelhaul every cable\n".into(),
        }];
        build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "repo-a",
            snapshot: &snapshot,
            plan: &plan,
            drafts: &drafts,
            cadence: "manual",
            snapshot_id: Some("12345678-1234-4234-9234-123456789abc"),
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
        .expect("a signed snapshot")
    };
    let snapshot = verified(&publication);
    let reads = std::cell::Cell::new(0usize);

    let retrieval = retrieve(
        "keelhaul",
        &snapshot,
        Some("git"),
        Instant::now() + Duration::from_secs(30),
        |revision, reference| {
            reads.set(reads.get() + 1);
            reading(&files)(revision, reference)
        },
    );

    assert_eq!(reads.get(), MAX_SOURCES);
    assert_eq!(retrieval.grounding.len(), MAX_SOURCES);
    assert_eq!(retrieval.manifest.omitted_sources.len(), 5);
}
