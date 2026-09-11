//! Producer invariants that native regeneration and cadence updates rely on.
//!
//! These replace the deleted JavaScript generation tests: the addresses in a
//! v1 publication are immutable, so a regeneration must move *every* one of
//! them, while a cadence-only rewrite must move *none* of them.

use super::tests::fixture;
use crate::git_snapshot::RepoSnapshot;
use crate::publish::PageDraft;
use crate::snapshot_v1_build::{
    build_cadence_update, build_snapshot, verify_publication, SnapshotBuild, SnapshotPublication,
};
use crate::types::WikiPlan;
use nostr::{Event, Keys};

fn d_tag(event: &Event) -> String {
    let tags: Vec<_> = event
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|name| name == "d"))
        .collect();
    assert_eq!(tags.len(), 1, "every Wiki event carries one d tag");
    tags[0].as_slice()[1].clone()
}

fn tag_value(event: &Event, name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|tag| tag.as_slice().first().is_some_and(|value| value == name))
        .and_then(|tag| tag.as_slice().get(1).cloned())
}

fn build(
    keys: &Keys,
    snapshot: &RepoSnapshot,
    plan: &WikiPlan,
    drafts: &[PageDraft],
    snapshot_id: Option<&str>,
) -> SnapshotPublication {
    build_snapshot(SnapshotBuild {
        owner: &keys.public_key().to_hex(),
        repo_d: "Repo.demo",
        snapshot,
        plan,
        drafts,
        cadence: "manual",
        snapshot_id,
        expected_revision: None,
        created_at: 10,
        keys,
    })
    .expect("build")
}

/// Explicit Regenerate captures fresh source and builds a complete graph with
/// a new snapshot UUID, *including when the source bytes are unchanged*, so a
/// retired address can never be selected for replay again (D-079).
#[test]
fn regeneration_from_identical_source_moves_every_immutable_address() {
    let (keys, snapshot, plan, drafts) = fixture();
    let first = build(&keys, &snapshot, &plan, &drafts, None);
    let second = build(&keys, &snapshot, &plan, &drafts, None);

    assert_ne!(first.snapshot_id, second.snapshot_id);
    assert_ne!(first.manifest.id, second.manifest.id);
    assert_ne!(d_tag(&first.manifest), d_tag(&second.manifest));
    assert_eq!(first.pages.len(), second.pages.len());
    for (old, new) in first.pages.iter().zip(second.pages.iter()) {
        assert_eq!(
            old.content, new.content,
            "the source bytes are deliberately unchanged"
        );
        assert_ne!(old.id, new.id, "a regenerated page needs a fresh address");
        assert_ne!(d_tag(old), d_tag(new));
    }
    // The head keeps its replaceable coordinate; only its dependencies move.
    assert_eq!(d_tag(&first.head), d_tag(&second.head));
    assert_ne!(first.head.id, second.head.id);
    assert_eq!(
        tag_value(&second.head, "wiki-snapshot").as_deref(),
        Some(second.snapshot_id.as_str())
    );
    verify_publication(&keys.public_key().to_hex(), "Repo.demo", &second)
        .expect("regenerated publication verifies");
}

/// The freshness guard in the native regenerate command is only meaningful
/// because reusing a snapshot UUID reproduces the *same* addresses.
#[test]
fn reusing_a_snapshot_uuid_reproduces_the_same_immutable_addresses() {
    let (keys, snapshot, plan, drafts) = fixture();
    let id = Some("12345678-1234-4234-9234-123456789abc");
    let first = build(&keys, &snapshot, &plan, &drafts, id);
    let second = build(&keys, &snapshot, &plan, &drafts, id);

    assert_eq!(first.snapshot_id, second.snapshot_id);
    assert_eq!(first.manifest.id, second.manifest.id);
    assert_eq!(first.head.id, second.head.id);
    assert_eq!(
        first
            .pages
            .iter()
            .map(|page| page.id.to_hex())
            .collect::<Vec<_>>(),
        second
            .pages
            .iter()
            .map(|page| page.id.to_hex())
            .collect::<Vec<_>>()
    );
}

/// A cadence rewrite must retain the exact manifest and page events; only the
/// head's mutable metadata may change.
#[test]
fn cadence_update_preserves_exact_manifest_and_page_addresses() {
    let (keys, snapshot, plan, drafts) = fixture();
    let owner = keys.public_key().to_hex();
    let first = build(&keys, &snapshot, &plan, &drafts, None);
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
    .expect("cadence update");

    assert_eq!(next.manifest, first.manifest);
    assert_eq!(next.pages, first.pages);
    assert_eq!(next.snapshot_id, first.snapshot_id);
    assert_eq!(next.source_revision, first.source_revision);
    assert_eq!(d_tag(&next.head), d_tag(&first.head));
    assert_eq!(
        tag_value(&next.head, "wiki-manifest"),
        tag_value(&first.head, "wiki-manifest"),
        "the manifest reference must survive a cadence rewrite"
    );
    assert_eq!(next.head.content, first.head.content);
    assert_eq!(tag_value(&next.head, "cadence").as_deref(), Some("daily"));
    assert_eq!(
        next.expected_revision,
        first.head.id.to_hex(),
        "a cadence rewrite is conditional on the exact head it replaces"
    );

    // Every tag other than the two mutable ones keeps its exact position and
    // value, including tags this version of Crew does not interpret.
    let mutable = ["cadence", "expected-revision"];
    let stable = |event: &Event| -> Vec<Vec<String>> {
        event
            .tags
            .iter()
            .map(|tag| tag.as_slice().to_vec())
            .filter(|tag| {
                !tag.first()
                    .is_some_and(|name| mutable.contains(&name.as_str()))
            })
            .collect()
    };
    assert_eq!(stable(&next.head), stable(&first.head));
}

#[test]
fn cadence_update_rejects_a_non_advancing_timestamp_or_unknown_cadence() {
    let (keys, snapshot, plan, drafts) = fixture();
    let owner = keys.public_key().to_hex();
    let first = build(&keys, &snapshot, &plan, &drafts, None);
    let update = |cadence: &str, created_at: u64| {
        build_cadence_update(
            &owner,
            "Repo.demo",
            &first.head,
            &first.manifest,
            &first.pages,
            cadence,
            created_at,
            &keys,
        )
    };
    assert!(
        update("daily", 10).is_err(),
        "the head timestamp must advance"
    );
    assert!(update("daily", 9).is_err());
    assert!(update("every-second", 11).is_err(), "unknown cadence");
    assert!(update("daily", 11).is_ok());
}

#[test]
fn cadence_update_rejects_a_foreign_signer_or_swapped_dependency() {
    let (keys, snapshot, plan, drafts) = fixture();
    let owner = keys.public_key().to_hex();
    let first = build(&keys, &snapshot, &plan, &drafts, None);
    let other = Keys::generate();
    assert!(
        build_cadence_update(
            &owner,
            "Repo.demo",
            &first.head,
            &first.manifest,
            &first.pages,
            "daily",
            11,
            &other,
        )
        .is_err(),
        "the signing key must be the repository owner"
    );

    let foreign = build(&keys, &snapshot, &plan, &drafts, None);
    assert!(
        build_cadence_update(
            &owner,
            "Repo.demo",
            &first.head,
            &foreign.manifest,
            &foreign.pages,
            "daily",
            11,
            &keys,
        )
        .is_err(),
        "a cadence rewrite must not adopt another snapshot's dependencies"
    );
}

#[test]
fn draft_metadata_must_match_its_plan_and_captured_source() {
    let (keys, snapshot, plan, drafts) = fixture();
    let owner = keys.public_key().to_hex();
    let attempt = |drafts: &[PageDraft]| {
        build_snapshot(SnapshotBuild {
            owner: &owner,
            repo_d: "Repo.demo",
            snapshot: &snapshot,
            plan: &plan,
            drafts,
            cadence: "manual",
            snapshot_id: None,
            expected_revision: None,
            created_at: 10,
            keys: &keys,
        })
    };
    assert!(attempt(&drafts).is_ok());

    let mut wrong_language = drafts.clone();
    wrong_language[0].language = "fr".into();
    assert!(
        attempt(&wrong_language).is_err(),
        "language must match plan"
    );

    let mut wrong_commit = drafts.clone();
    wrong_commit[0].commit = "b".repeat(40);
    assert!(
        attempt(&wrong_commit).is_err(),
        "a draft must be bound to the captured revision"
    );

    let mut wrong_section = drafts.clone();
    wrong_section[0].section = "other".into();
    assert!(attempt(&wrong_section).is_err());

    let mut embedded_nul = drafts.clone();
    embedded_nul[0].content = "# Overview\0\n".into();
    assert!(attempt(&embedded_nul).is_err(), "NUL content is rejected");

    let mut duplicated = drafts.clone();
    duplicated.push(drafts[0].clone());
    assert!(
        attempt(&duplicated).is_err(),
        "a duplicate slug must never reach a signed page"
    );

    assert!(
        attempt(&[]).is_err(),
        "a publication needs at least one page"
    );
}
