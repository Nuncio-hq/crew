//! Shared real signed-graph fixtures for the native Wiki publication tests.
//!
//! Every fixture goes through the production `crew-wiki` builder and the
//! production record validator, so a test can never accept a graph the
//! shipping code would reject.

use crate::commands::wiki_publication_record::{WikiPublicationProgress, WikiPublicationRecord};
use crate::owner_operations::{Operation, OperationKind, OperationScope, OperationStatus};
use crate::wiki_worker::WikiGeneration;
use crew_wiki::git_snapshot::RepoSnapshot;
use crew_wiki::publish::PageDraft;
use crew_wiki::snapshot_v1_build::{build_snapshot, SnapshotBuild, SnapshotPublication};
use crew_wiki::types::{PlannedPage, PlannedSection, WikiPlan};
use nostr::Keys;
use std::collections::BTreeMap;

/// Build one complete signed v1 publication for `repo_d`.
///
/// `snapshot_id = None` mirrors production regeneration: a fresh UUID gives
/// every page and the manifest a different immutable address.
pub(crate) fn publication(
    keys: &Keys,
    repo_d: &str,
    snapshot_id: Option<&str>,
) -> SnapshotPublication {
    publication_with_expected(keys, repo_d, snapshot_id, None)
}

/// The same fixture with an explicit conditional precondition.
///
/// `expected_revision` goes through the production builder so the signed head
/// really carries it; a test must never patch precondition metadata into an
/// already signed envelope.
pub(crate) fn publication_with_expected(
    keys: &Keys,
    repo_d: &str,
    snapshot_id: Option<&str>,
    expected_revision: Option<&str>,
) -> SnapshotPublication {
    let owner = keys.public_key().to_hex();
    let generation = generation();
    build_snapshot(SnapshotBuild {
        owner: &owner,
        repo_d,
        snapshot: &generation.snapshot,
        plan: &generation.plan,
        drafts: &generation.drafts,
        cadence: "manual",
        snapshot_id,
        expected_revision,
        created_at: 10,
        keys,
    })
    .expect("fixture publication")
}

/// Build a deterministic completed generation for production-bound acceptance.
///
/// The source, plan, and drafts are intentionally independent of signing. The
/// publication command owns the subsequent `build_snapshot` and journal
/// reservation boundary, so tests can exercise that same post-generation path.
pub(crate) fn generation() -> WikiGeneration {
    let commit = "a".repeat(40);
    let snapshot = RepoSnapshot {
        commit: commit.clone(),
        branch: "main".into(),
        source_revision: format!("git:{commit}"),
        files: vec!["src/main.rs".into()],
        contents: BTreeMap::from([(String::from("src/main.rs"), String::from("fn main() {}\n"))]),
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
        commit,
        language: "en".into(),
        content: "# Overview\n".into(),
    }];
    WikiGeneration {
        snapshot,
        plan,
        drafts,
    }
}

pub(crate) fn coordinate(keys: &Keys, repo_d: &str) -> String {
    format!("30617:{}:{repo_d}", keys.public_key().to_hex())
}

/// Wrap a signed publication in the durable recovery record, validated by the
/// production intent validator.
pub(crate) fn record(
    publication: SnapshotPublication,
    coordinate: &str,
    keys: &Keys,
) -> WikiPublicationRecord {
    let record = WikiPublicationRecord {
        version: 1,
        coordinate: coordinate.to_owned(),
        snapshot_id: publication.snapshot_id,
        source_revision: publication.source_revision,
        expected_revision: publication.expected_revision,
        head: publication.head,
        manifest: publication.manifest,
        pages: publication.pages,
        cadence: "manual".into(),
        attempts: 0,
        progress: WikiPublicationProgress::Preparing,
        head_attempted: false,
        cancel_requested: false,
        reconcile_only: false,
        reconciliation: None,
        retry_at: 0,
        last_error: None,
        lease: None,
    };
    record
        .validate_intent(keys.public_key())
        .expect("valid fixture record");
    record
}

/// An in-memory `Operation` shaped exactly like a freshly created journal row.
pub(crate) fn operation(
    scope: OperationScope,
    id: String,
    coordinate: &str,
    record: &WikiPublicationRecord,
    now: i64,
) -> Operation {
    Operation {
        version: 1,
        scope,
        id,
        kind: OperationKind::WikiPublication,
        resource_key: coordinate.to_owned(),
        revision: 0,
        created_at: now,
        updated_at: now,
        status: OperationStatus::Preparing,
        reconciled: false,
        payload: serde_json::to_value(record).expect("record JSON"),
    }
}
