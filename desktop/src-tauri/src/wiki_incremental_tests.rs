use super::{all_pages_reused, generate_planned_pages};
use crew_wiki::generate::Generator;
use crew_wiki::git_snapshot::RepoSnapshot;
use crew_wiki::publish::PageDraft;
use crew_wiki::snapshot_v1::SnapshotManifest;
use crew_wiki::snapshot_v1_build::{build_snapshot, SnapshotBuild, SnapshotPublication};
use crew_wiki::types::{PlannedPage, PlannedSection, WikiPlan};
use crew_wiki::WikiError;
use nostr::{Keys, SecretKey};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct CountingGenerator {
    calls: Arc<Mutex<Vec<String>>>,
}

impl Generator for CountingGenerator {
    fn generate(
        &self,
        page: &PlannedPage,
        _snapshot: &RepoSnapshot,
        _language: &str,
    ) -> Result<String, WikiError> {
        self.calls
            .lock()
            .expect("counting generator lock")
            .push(page.slug.clone());
        Ok(format!("# generated {}\n", page.slug))
    }
}

fn keys() -> Keys {
    let mut bytes = [0_u8; 32];
    bytes[31] = 7;
    Keys::new(SecretKey::from_slice(&bytes).expect("test key"))
}

fn snapshot(
    commit_byte: char,
    page_zero: &str,
    page_one: Option<&str>,
    steering: Option<&str>,
) -> RepoSnapshot {
    let commit = commit_byte.to_string().repeat(40);
    let mut files = vec!["src/page-0.rs".to_owned()];
    let mut contents = BTreeMap::from([("src/page-0.rs".to_owned(), page_zero.to_owned())]);
    if let Some(page_one) = page_one {
        files.push("src/page-1.rs".to_owned());
        contents.insert("src/page-1.rs".to_owned(), page_one.to_owned());
    }
    if let Some(steering) = steering {
        files.push(".crew/wiki.json".to_owned());
        contents.insert(".crew/wiki.json".to_owned(), steering.to_owned());
    }
    files.sort();
    RepoSnapshot {
        commit: commit.clone(),
        branch: "main".into(),
        files,
        contents,
        source_revision: format!("git:{commit}"),
        omissions: Vec::new(),
    }
}

fn plan(language: &str, include_page_one: bool) -> WikiPlan {
    let mut pages = vec![PlannedPage {
        slug: "page-0".into(),
        title: "Page 0".into(),
        section: "overview".into(),
        source_files: vec!["src/page-0.rs".into()],
    }];
    if include_page_one {
        pages.push(PlannedPage {
            slug: "page-1".into(),
            title: "Page 1".into(),
            section: "overview".into(),
            source_files: vec!["src/page-1.rs".into()],
        });
    }
    WikiPlan {
        language: language.into(),
        sections: vec![PlannedSection {
            id: "overview".into(),
            title: "Overview".into(),
            pages,
        }],
    }
}

fn old_drafts(plan: &WikiPlan, commit: &str) -> Vec<PageDraft> {
    plan.sections
        .iter()
        .flat_map(|section| section.pages.iter())
        .map(|page| PageDraft {
            slug: page.slug.clone(),
            title: page.title.clone(),
            section: page.section.clone(),
            source_files: page.source_files.clone(),
            commit: commit.to_owned(),
            language: plan.language.clone(),
            content: format!("# old {}\n", page.slug),
        })
        .collect()
}

fn publication(snapshot: &RepoSnapshot, plan: &WikiPlan) -> SnapshotPublication {
    let keys = keys();
    let owner = keys.public_key().to_hex();
    build_snapshot(SnapshotBuild {
        owner: &owner,
        repo_d: "repo",
        snapshot,
        plan,
        drafts: &old_drafts(plan, &snapshot.commit),
        cadence: "manual",
        snapshot_id: Some("12345678-1234-4234-9234-123456789abc"),
        expected_revision: None,
        created_at: 10,
        keys: &keys,
    })
    .expect("valid previous publication")
}

struct Run {
    drafts: Vec<PageDraft>,
    calls: Vec<String>,
    factories: usize,
}

fn run(snapshot: &RepoSnapshot, plan: &WikiPlan, previous: &SnapshotPublication) -> Run {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let factories = Arc::new(AtomicUsize::new(0));
    let factory_calls = factories.clone();
    let generator_calls = calls.clone();
    let drafts = generate_planned_pages(
        snapshot,
        plan,
        Some(previous),
        None,
        Instant::now(),
        move || {
            factory_calls.fetch_add(1, Ordering::Relaxed);
            Ok(Box::new(CountingGenerator {
                calls: generator_calls,
            }))
        },
    )
    .expect("incremental generation");
    let calls = calls.lock().expect("counting generator lock").clone();
    Run {
        drafts,
        calls,
        factories: factories.load(Ordering::Relaxed),
    }
}

#[test]
fn unchanged_verified_snapshot_reuses_every_page_without_constructing_generator() {
    let old_snapshot = snapshot('a', "fn zero() {}\n", Some("fn one() {}\n"), None);
    let old_plan = plan("en", true);
    let previous = publication(&old_snapshot, &old_plan);
    let run = run(&old_snapshot, &old_plan, &previous);

    assert!(run.calls.is_empty());
    assert_eq!(run.factories, 0);
    assert_eq!(
        run.drafts
            .iter()
            .map(|draft| draft.content.as_str())
            .collect::<Vec<_>>(),
        vec!["# old page-0\n", "# old page-1\n"]
    );
}

#[test]
fn unchanged_detached_snapshot_reuses_every_page_without_generator_calls() {
    let mut detached_snapshot = snapshot('a', "fn zero() {}\n", Some("fn one() {}\n"), None);
    detached_snapshot.branch.clear();
    let old_plan = plan("en", true);
    let previous = publication(&detached_snapshot, &old_plan);

    assert!(all_pages_reused(&detached_snapshot, &old_plan, &previous));
    let run = run(&detached_snapshot, &old_plan, &previous);

    assert!(run.calls.is_empty());
    assert_eq!(run.factories, 0);
    assert_eq!(
        run.drafts
            .iter()
            .map(|draft| draft.content.as_str())
            .collect::<Vec<_>>(),
        vec!["# old page-0\n", "# old page-1\n"]
    );
}

#[test]
fn changed_git_branch_invalidates_reuse() {
    let old_snapshot = snapshot('a', "fn zero() {}\n", Some("fn one() {}\n"), None);
    let old_plan = plan("en", true);
    let previous = publication(&old_snapshot, &old_plan);
    let mut new_snapshot = old_snapshot.clone();
    new_snapshot.branch = "release".into();

    assert!(!all_pages_reused(&new_snapshot, &old_plan, &previous));
    let run = run(&new_snapshot, &old_plan, &previous);

    assert_eq!(run.calls, vec!["page-0", "page-1"]);
    assert_eq!(run.factories, 1);
}

#[test]
fn one_changed_source_generates_only_the_dependent_page() {
    let old_snapshot = snapshot('a', "fn zero() {}\n", Some("fn one() {}\n"), None);
    let old_plan = plan("en", true);
    let previous = publication(&old_snapshot, &old_plan);
    let new_snapshot = snapshot('b', "fn zero_changed() {}\n", Some("fn one() {}\n"), None);
    let run = run(&new_snapshot, &old_plan, &previous);

    assert_eq!(run.calls, vec!["page-0"]);
    assert_eq!(run.factories, 1);
    assert_eq!(run.drafts[0].content, "# generated page-0\n");
    assert_eq!(run.drafts[1].content, "# old page-1\n");
}

#[test]
fn changed_plan_metadata_invalidates_reuse_for_all_pages() {
    let old_snapshot = snapshot('a', "fn zero() {}\n", Some("fn one() {}\n"), None);
    let old_plan = plan("en", true);
    let previous = publication(&old_snapshot, &old_plan);
    let new_plan = plan("fr", true);
    let run = run(&old_snapshot, &new_plan, &previous);

    assert_eq!(run.calls, vec!["page-0", "page-1"]);
    assert_eq!(run.factories, 1);
}

#[test]
fn changed_steering_without_page_source_membership_invalidates_reuse() {
    let old_snapshot = snapshot(
        'a',
        "fn zero() {}\n",
        Some("fn one() {}\n"),
        Some(r#"{"repo_notes":"old"}"#),
    );
    let old_plan = plan("en", true);
    let previous = publication(&old_snapshot, &old_plan);
    let new_snapshot = snapshot(
        'b',
        "fn zero() {}\n",
        Some("fn one() {}\n"),
        Some(r#"{"repo_notes":"new"}"#),
    );
    let run = run(&new_snapshot, &old_plan, &previous);

    assert_eq!(run.calls, vec!["page-0", "page-1"]);
    assert_eq!(run.factories, 1);
}

#[test]
fn removed_page_is_absent_from_the_next_generation_and_manifest() {
    let old_snapshot = snapshot('a', "fn zero() {}\n", Some("fn one() {}\n"), None);
    let old_plan = plan("en", true);
    let previous = publication(&old_snapshot, &old_plan);
    let new_snapshot = snapshot('b', "fn zero() {}\n", None, None);
    let new_plan = plan("en", false);
    let run = run(&new_snapshot, &new_plan, &previous);

    assert!(run.calls.is_empty());
    assert_eq!(run.factories, 0);
    assert_eq!(run.drafts.len(), 1);
    assert_eq!(run.drafts[0].slug, "page-0");

    let keys = keys();
    let owner = keys.public_key().to_hex();
    let next = build_snapshot(SnapshotBuild {
        owner: &owner,
        repo_d: "repo",
        snapshot: &new_snapshot,
        plan: &new_plan,
        drafts: &run.drafts,
        cadence: "manual",
        snapshot_id: Some("abcdefab-cdef-4abc-8def-abcdefabcdef"),
        expected_revision: None,
        created_at: 11,
        keys: &keys,
    })
    .expect("next publication");
    let manifest: SnapshotManifest =
        serde_json::from_str(&next.manifest.content).expect("manifest");
    assert_eq!(manifest.7.len(), 1);
    assert_eq!(manifest.7[0].0, "page-0");
}

#[test]
fn same_snapshot_with_removed_page_rejects_noop_but_reuses_remaining_body() {
    let old_snapshot = snapshot('a', "fn zero() {}\n", Some("fn one() {}\n"), None);
    let old_plan = plan("en", true);
    let previous = publication(&old_snapshot, &old_plan);
    let new_plan = plan("en", false);

    assert!(!all_pages_reused(&old_snapshot, &new_plan, &previous));
    let run = run(&old_snapshot, &new_plan, &previous);

    assert!(run.calls.is_empty());
    assert_eq!(run.factories, 0);
    assert_eq!(run.drafts.len(), 1);
    assert_eq!(run.drafts[0].content, "# old page-0\n");
}

#[test]
fn same_snapshot_with_reordered_pages_rejects_noop_but_reuses_bodies() {
    let old_snapshot = snapshot('a', "fn zero() {}\n", Some("fn one() {}\n"), None);
    let old_plan = plan("en", true);
    let previous = publication(&old_snapshot, &old_plan);
    let mut reordered_plan = old_plan.clone();
    reordered_plan.sections[0].pages.reverse();

    assert!(!all_pages_reused(&old_snapshot, &reordered_plan, &previous));
    let run = run(&old_snapshot, &reordered_plan, &previous);

    assert!(run.calls.is_empty());
    assert_eq!(run.factories, 0);
    assert_eq!(
        run.drafts
            .iter()
            .map(|draft| draft.content.as_str())
            .collect::<Vec<_>>(),
        vec!["# old page-1\n", "# old page-0\n"]
    );
}
