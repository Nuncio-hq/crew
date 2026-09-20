//! The owner-local private Ask record.
//!
//! Each test names the production line whose removal makes it fail. The
//! ownership fixture is the same owned-root object the command resolves; the
//! file under it is the real file, so persistence, corruption and deletion
//! are exercised against the same bytes the app writes.

use super::super::tests::{bounded_egress, canonical_tempdir, owned_receipt};
use super::*;

fn scope() -> HistoryScope {
    HistoryScope {
        community_id: "community-a".into(),
        relay_url: "wss://relay.example/".into(),
        viewer_pubkey: "a".repeat(64),
        agent_pubkey: "b".repeat(64),
        project_id: "project-a".into(),
        repo_owner: "c".repeat(64),
        repo_d: "repo-a".into(),
    }
}

fn other_scope() -> HistoryScope {
    HistoryScope {
        viewer_pubkey: "d".repeat(64),
        repo_d: "repo-b".into(),
        ..scope()
    }
}

fn attempt_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn response(attempt_id: &str, markdown: &str) -> super::super::attempt::PrivateAskResponse {
    super::super::attempt::PrivateAskResponse {
        attempt_id: attempt_id.to_owned(),
        session_generation: "generation".into(),
        markdown: markdown.to_owned(),
        citations: Vec::new(),
        source_revision: "git:rev".into(),
        manifest: super::super::retrieval::RetrievalManifest::default(),
        egress: bounded_egress(),
    }
}

fn answered_entry(
    scope: &HistoryScope,
    question_id: &str,
    follow_up_of: Option<String>,
    question: &str,
    attempt_id: &str,
    markdown: &str,
    asked_at: u64,
) -> PrivateAskHistoryEntry {
    PrivateAskHistoryEntry::answered(
        scope,
        question_id,
        follow_up_of,
        question,
        &response(attempt_id, markdown),
        asked_at,
    )
}

fn dead(_: &str) -> bool {
    false
}

fn live(_: &str) -> bool {
    true
}

/// Production line: `visible_status` in `load_scoped`. A stored `running` is
/// only ever a claim about the past — what the registry says *now* decides
/// whether it is a live attempt or a crashed one. Remove the is_live check and
/// the interrupted arm of this test fails.
#[test]
fn a_vanished_attempt_reads_interrupted_never_running() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let scope = scope();
    let attempt = attempt_id();

    upsert(
        &ownership,
        PrivateAskHistoryEntry::pending(&scope, &attempt, None, "what?", &attempt, None, 10),
        10,
    )
    .expect("write");

    let key = scope.key();
    let entries = load_scoped(&ownership, &key, &live);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].status, HistoryStatus::Running);

    let entries = load_scoped(&ownership, &key, &dead);
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].status,
        HistoryStatus::Interrupted,
        "a pending record whose process is gone is interrupted, not running"
    );
}

/// Production line: `entries.retain(|existing| existing.attempt_id != entry.attempt_id)`
/// in `upsert`. The terminal write replaces the pending one — remove it and
/// the attempt exists twice, once still "running" forever.
#[test]
fn the_terminal_write_replaces_the_pending_record() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let scope = scope();
    let attempt = attempt_id();
    let question_id = attempt.clone();

    upsert(
        &ownership,
        PrivateAskHistoryEntry::pending(&scope, &question_id, None, "what?", &attempt, None, 10),
        10,
    )
    .expect("pending write");
    upsert(
        &ownership,
        answered_entry(&scope, &question_id, None, "what?", &attempt, "it is 42", 20),
        20,
    )
    .expect("terminal write");

    let entries = load_scoped(&ownership, &scope.key(), &dead);
    assert_eq!(
        entries.len(),
        1,
        "one attempt, one record — the terminal write fences the pending one"
    );
    assert_eq!(entries[0].status, HistoryStatus::Answered);
    assert_eq!(entries[0].markdown.as_deref(), Some("it is 42"));
    assert_eq!(entries[0].source_revision.as_deref(), Some("git:rev"));
}

/// Production line: the `entry.scope.key() == *key` filter in `load_scoped`
/// and `load_attempt`. History is scoped to community+viewer+repository — a
/// record written under one scope is absent from another, and deleting the
/// key check serves every viewer every record.
#[test]
fn another_viewer_or_repository_never_sees_the_record() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let mine = scope();
    let foreign = other_scope();
    let attempt = attempt_id();

    upsert(
        &ownership,
        answered_entry(&mine, &attempt, None, "secret?", &attempt, "answer", 10),
        10,
    )
    .expect("write");

    // Own scope sees it; the foreign key does not — same file, same viewer
    // root, different identity.
    assert_eq!(load_scoped(&ownership, &mine.key(), &dead).len(), 1);
    assert!(
        load_scoped(&ownership, &foreign.key(), &dead).is_empty(),
        "scope keying is the privacy boundary"
    );
    assert!(
        load_attempt(&ownership, &foreign.key(), &attempt, &dead, 0).is_none(),
        "the attempt read is scoped too"
    );

    // A community switch is a different scope as well.
    let mut other_community = mine.clone();
    other_community.community_id = "community-b".into();
    assert!(load_scoped(&ownership, &other_community.key(), &dead).is_empty());
}

/// Production line: `follow_up_thread`'s chain walk. A follow-up sees the
/// turns on its own `follow_up_of` chain — the parent and its ancestors —
/// never a sibling branch and never an unanswered turn.
#[test]
fn a_follow_up_carries_only_its_own_chain_of_answered_turns() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let scope = scope();
    let key = scope.key();
    let root_q = "root-question".to_string();
    let root = attempt_id();
    let first = attempt_id();
    let sibling = attempt_id();
    let grandchild = attempt_id();

    // root → first → grandchild is one chain; sibling follows the root too
    // and is a different branch of the same thread.
    upsert(&ownership, answered_entry(&scope, &root_q, None, "root q", &root, "root answer", 10), 10)
        .expect("root");
    upsert(
        &ownership,
        answered_entry(&scope, &root_q, Some(root.clone()), "follow q", &first, "first answer", 20),
        20,
    )
    .expect("first follow-up");
    upsert(
        &ownership,
        answered_entry(&scope, &root_q, Some(root.clone()), "sibling q", &sibling, "sibling answer", 30),
        30,
    )
    .expect("sibling follow-up");
    upsert(
        &ownership,
        answered_entry(&scope, &root_q, Some(first.clone()), "deep q", &grandchild, "deep answer", 40),
        40,
    )
    .expect("grandchild");

    // A follow-up to `deep q` names its parent — the turn it builds on — and
    // the thread it receives is that chain's answered turns, parent last.
    let (question_id, prior) =
        follow_up_thread(&ownership, &key, &first, &dead, 0).expect("a chain");
    assert_eq!(question_id, root_q);
    let questions: Vec<&str> = prior.iter().map(|turn| turn.question.as_str()).collect();
    assert_eq!(
        questions,
        vec!["root q", "follow q"],
        "the chain is root then parent — the sibling branch never enters"
    );
    assert_eq!(prior[1].markdown, "first answer");
}

/// A follow-up into a record that is not an answered turn cannot be grounded:
/// missing, refused and interrupted parents all refuse, because inventing
/// context would be worse than saying so.
#[test]
fn a_follow_up_without_an_answered_parent_is_unavailable() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let scope = scope();
    let key = scope.key();
    let parent = attempt_id();

    // Missing entirely.
    assert!(follow_up_thread(&ownership, &key, &parent, &dead, 0).is_none());

    // Present but refused.
    upsert(
        &ownership,
        PrivateAskHistoryEntry::finished_refusal(
            &scope,
            &parent,
            None,
            "q",
            &parent,
            None,
            &super::super::PrivateAskFailure::AgentUnbound,
            10,
        ),
        10,
    )
    .expect("refusal write");
    assert!(follow_up_thread(&ownership, &key, &parent, &dead, 0).is_none());

    // Present but crashed — a `running` record reads interrupted.
    let crashed = attempt_id();
    upsert(
        &ownership,
        PrivateAskHistoryEntry::pending(&scope, &crashed, None, "q", &crashed, None, 20),
        20,
    )
    .expect("pending write");
    assert!(follow_up_thread(&ownership, &key, &crashed, &dead, 0).is_none());
}

/// Production line: the `HISTORY_LIMIT_PER_SCOPE` pass in `prune`. The oldest
/// entries retire first; the bound is per scope so a second repository's
/// questions are unaffected.
#[test]
fn the_per_scope_entry_bound_retires_the_oldest_first() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let scope = scope();
    let key = scope.key();

    for index in 0..(HISTORY_LIMIT_PER_SCOPE + 5) {
        let attempt = attempt_id();
        upsert(
            &ownership,
            answered_entry(
                &scope,
                &attempt,
                None,
                &format!("question {index}"),
                &attempt,
                &format!("answer {index}"),
                // Distinct, ordered timestamps so "oldest" is unambiguous.
                100 + index as u64,
            ),
            100 + index as u64,
        )
        .expect("write");
    }

    let entries = load_scoped(&ownership, &key, &dead);
    assert_eq!(entries.len(), HISTORY_LIMIT_PER_SCOPE);
    assert!(
        entries.iter().all(|entry| entry.question != "question 0"),
        "the oldest question retired"
    );
    // Newest first to the reader.
    assert_eq!(entries[0].question, format!("question {}", HISTORY_LIMIT_PER_SCOPE + 4));
}

/// Production line: the `HISTORY_MAX_AGE_SECS` guard in `prune`. A stale
/// record retires on the next write — it is never silently kept past the
/// retention window.
#[test]
fn records_older_than_the_retention_window_retire_on_write() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let scope = scope();
    let key = scope.key();
    let old = attempt_id();

    upsert(
        &ownership,
        answered_entry(&scope, &old, None, "old q", &old, "old answer", 10),
        10,
    )
    .expect("old write");
    let now = 10 + HISTORY_MAX_AGE_SECS + 1;
    let fresh = attempt_id();
    upsert(
        &ownership,
        answered_entry(&scope, &fresh, None, "fresh q", &fresh, "fresh answer", now),
        now,
    )
    .expect("fresh write");

    let entries = load_scoped(&ownership, &key, &dead);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].question, "fresh q");
}

/// A corrupt file is moved aside — never silently truncated and never served
/// as history — and the next write starts clean.
#[test]
fn a_corrupt_file_is_quarantined_and_reads_empty() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let scope = scope();
    let key = scope.key();
    let path = history_path(&ownership).expect("path");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"{{{{not json").unwrap();

    assert!(load_scoped(&ownership, &key, &dead).is_empty());
    // Quarantined: the bad bytes still exist beside the file for inspection,
    // and the live path is gone.
    assert!(!path.exists());
    assert!(
        std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .any(|entry| entry.unwrap().file_name().to_string_lossy().contains("corrupt")),
        "the corrupt file was moved aside, not deleted"
    );

    // A fresh write works after the quarantine.
    let attempt = attempt_id();
    upsert(
        &ownership,
        answered_entry(&scope, &attempt, None, "q", &attempt, "a", 10),
        10,
    )
    .expect("write after quarantine");
    assert_eq!(load_scoped(&ownership, &key, &dead).len(), 1);
}

/// A v1 file — the unscoped shape from before history had an identity — is
/// readable only to be told apart. Its entries can never be attributed to a
/// scope, so they are never served as this viewer's history.
#[test]
fn a_v1_file_is_quarantined_onto_the_next_write() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let scope = scope();
    let key = scope.key();
    let path = history_path(&ownership).expect("path");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "entries": [{"answer_id": "a", "created_at": 1_u64}]
        }))
        .unwrap(),
    )
    .unwrap();

    // Reads see nothing — unscoped entries are foreign, not history.
    assert!(load_scoped(&ownership, &key, &dead).is_empty());

    // The next write keeps the v1 file out of the served record.
    let attempt = attempt_id();
    upsert(
        &ownership,
        answered_entry(&scope, &attempt, None, "q", &attempt, "a", 10),
        10,
    )
    .expect("write");
    let entries = load_scoped(&ownership, &key, &dead);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].question, "q");
}

/// Production line: `forget`'s `scope.key() == *key && attempt_id ==` filter.
/// Deletion is explicit and scoped — it removes this attempt's record here and
/// touches nothing else, not even a same-named record under a foreign scope.
#[test]
fn forgetting_removes_only_the_named_attempt_in_this_scope() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let mine = scope();
    let foreign = other_scope();
    let kept = attempt_id();
    let gone = attempt_id();
    let foreign_attempt = attempt_id();

    upsert(&ownership, answered_entry(&mine, &kept, None, "keep q", &kept, "keep a", 10), 10)
        .expect("kept write");
    upsert(&ownership, answered_entry(&mine, &gone, None, "drop q", &gone, "drop a", 20), 20)
        .expect("dropped write");
    upsert(
        &ownership,
        answered_entry(&foreign, &foreign_attempt, None, "foreign q", &foreign_attempt, "f", 30),
        30,
    )
    .expect("foreign write");

    assert!(forget(&ownership, &mine.key(), &gone, 40).expect("forget"));
    let entries = load_scoped(&ownership, &mine.key(), &dead);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].attempt_id, kept);
    // A record under a foreign scope is untouched.
    assert_eq!(load_scoped(&ownership, &foreign.key(), &dead).len(), 1);
    // Forgetting what is not there is a no-op, not an error.
    assert!(forget(&ownership, &mine.key(), &gone, 50).map(|removed| !removed).unwrap_or(false));
}

/// A record that arrives oversized — hand-edited, corrupt, or simply long — is
/// clamped at write, so the file stays a bounded record rather than a growth
/// sink.
#[test]
fn oversized_fields_are_clamped_at_the_write_boundary() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let scope = scope();
    let key = scope.key();
    let attempt = attempt_id();
    let huge = "x".repeat(MARKDOWN_LIMIT_BYTES + 4096);

    upsert(
        &ownership,
        answered_entry(&scope, &attempt, None, "q", &attempt, &huge, 10),
        10,
    )
    .expect("write");

    let entry = load_scoped(&ownership, &key, &dead).remove(0);
    let stored = entry.markdown.expect("markdown");
    assert!(stored.len() <= MARKDOWN_LIMIT_BYTES + 8);
    assert!(stored.ends_with('…'), "the cut is visible, not silent");
}
