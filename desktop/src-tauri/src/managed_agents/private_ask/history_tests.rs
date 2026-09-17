//! The owner-local history: both bounds, the read-back, and the refusal to
//! turn a damaged file into a partial record.
//!
//! The production lines these bind to are `retained`'s count and age bounds and
//! `load`'s "anything unusable is empty" ladder.

use super::super::tests::{canonical_tempdir, owned_receipt};
use super::{
    load, record, PrivateAskHistoryEntry, HISTORY_LIMIT, HISTORY_LIMIT_BYTES, HISTORY_MAX_AGE,
    HISTORY_QUARANTINE_FILE, HISTORY_QUESTION_BYTES, HISTORY_TRUNCATION_MARKER,
};
use crate::managed_agents::recap_ownership::VerifiedStagingOwnership;

fn entry(id: &str, asked_at: u64) -> PrivateAskHistoryEntry {
    PrivateAskHistoryEntry {
        attempt_id: id.to_owned(),
        question: format!("question {id}"),
        markdown: Some("answer".into()),
        refusal: None,
        citations: Vec::new(),
        asked_at,
    }
}

fn history_file(ownership: &VerifiedStagingOwnership) -> std::path::PathBuf {
    ownership
        .private_ask_history_base()
        .expect("history base")
        .join("attempts.json")
}

#[test]
fn an_attempt_is_read_back_after_a_restart() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    record(&ownership, entry("first", 1_000), 1_000).expect("record");

    // A fresh ownership handle is what a restart looks like from here: nothing
    // is carried in memory, the bytes on disk are the whole record.
    let restarted = owned_receipt(&fixture);
    let entries = load(&restarted, 1_100);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].attempt_id, "first");
    assert_eq!(entries[0].question, "question first");
}

#[test]
fn the_history_is_newest_first_and_bounded_by_count() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let now = 100_000;
    for index in 0..(HISTORY_LIMIT + 5) {
        record(
            &ownership,
            entry(
                &format!("attempt-{index}"),
                now - (HISTORY_LIMIT + 5 - index) as u64,
            ),
            now,
        )
        .expect("record");
    }

    let entries = load(&ownership, now);
    assert_eq!(entries.len(), HISTORY_LIMIT, "the count bound holds");
    // The newest survives and the oldest is the one dropped.
    assert_eq!(
        entries[0].attempt_id,
        format!("attempt-{}", HISTORY_LIMIT + 4)
    );
    assert!(
        !entries.iter().any(|entry| entry.attempt_id == "attempt-0"),
        "the oldest attempt is the one the count bound drops"
    );
}

#[test]
fn an_attempt_past_the_age_bound_is_dropped() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let now = 10 * HISTORY_MAX_AGE;
    record(&ownership, entry("old", now - HISTORY_MAX_AGE - 1), now).expect("record old");
    record(&ownership, entry("recent", now - 10), now).expect("record recent");

    let entries = load(&ownership, now);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].attempt_id, "recent");
}

#[test]
fn an_attempt_stamped_in_the_future_is_not_kept() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    // Not a reading of this machine's clock, so it is not retained — otherwise
    // it would sit at the head of the list forever.
    record(&ownership, entry("ahead", 5_000), 1_000).expect("record");

    assert!(load(&ownership, 1_000).is_empty());
}

#[test]
fn a_refusal_is_recorded_as_history_too() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    let refused = PrivateAskHistoryEntry::refused(
        "why is this refused?",
        "attempt-refused",
        &super::super::PrivateAskFailure::AgentBusy,
        2_000,
    );
    record(&ownership, refused, 2_000).expect("record");

    let entries = load(&ownership, 2_000);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].markdown, None);
    assert_eq!(
        entries[0].refusal.as_deref(),
        Some("selected agent is busy"),
        "the screen's reason is the one kept"
    );
}

#[test]
fn a_reused_attempt_id_replaces_rather_than_duplicates() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    record(&ownership, entry("same", 1_000), 1_000).expect("first");
    let mut second = entry("same", 1_200);
    second.question = "the second question".into();
    record(&ownership, second, 1_200).expect("second");

    let entries = load(&ownership, 1_300);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].question, "the second question");
}

#[test]
fn a_damaged_history_reads_as_empty_rather_than_partial() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    record(&ownership, entry("first", 1_000), 1_000).expect("record");

    std::fs::write(history_file(&ownership), b"{not json at all").expect("damage");
    assert!(
        load(&ownership, 1_100).is_empty(),
        "unparseable bytes are not a partial record"
    );

    // And a damaged file does not block the next attempt from being recorded.
    record(&ownership, entry("after", 1_200), 1_200).expect("record after damage");
    assert_eq!(load(&ownership, 1_200).len(), 1);
}

#[test]
fn a_history_from_another_schema_is_not_read() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    record(&ownership, entry("first", 1_000), 1_000).expect("record");
    let path = history_file(&ownership);
    let bytes = std::fs::read(&path).expect("read");
    let document = String::from_utf8(bytes)
        .expect("utf8")
        .replace("crew-private-ask-history", "some-other-log");
    std::fs::write(&path, document).expect("rewrite");

    assert!(load(&ownership, 1_100).is_empty());
}

#[test]
fn a_missing_history_is_an_empty_one() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);

    assert!(load(&ownership, 1_000).is_empty());
}

#[test]
fn a_completed_write_leaves_no_temporary_file() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    for index in 0..3 {
        record(
            &ownership,
            entry(&format!("attempt-{index}"), 1_000 + index),
            2_000,
        )
        .expect("record");
    }

    let base = ownership.private_ask_history_base().expect("history base");
    let files: Vec<_> = std::fs::read_dir(&base)
        .expect("readable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files, vec!["attempts.json".to_string()]);
}

#[test]
fn two_attempts_finishing_together_both_survive() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);

    // The owned directory is created once, up front. Creating it is not what is
    // under test, and the surrounding suite mutates process-global path state,
    // so racing that creation here would make this test flaky about something
    // else.
    record(&ownership, entry("seed", 900), 1_000).expect("seed");

    // `private_ask_run` is async and a renderer can invoke it twice, so the
    // read-modify-write in `record` really can interleave. Without the lock one
    // of these two entries is silently lost: both threads read the same list
    // and the second rename wins.
    std::thread::scope(|scope| {
        for index in 0..2 {
            let ownership = &ownership;
            scope.spawn(move || {
                record(
                    ownership,
                    entry(&format!("concurrent-{index}"), 1_000),
                    1_000,
                )
                .expect("record");
            });
        }
    });

    let entries = load(&ownership, 1_000);
    let ids: std::collections::BTreeSet<_> = entries
        .iter()
        .map(|entry| entry.attempt_id.as_str())
        .collect();
    assert!(
        ids.contains("concurrent-0") && ids.contains("concurrent-1"),
        "neither attempt is lost to the other's write: {ids:?}"
    );
}

/// The writer must not be able to produce a file its own reader refuses.
///
/// Production line: the `serialized_within_bound` call in `record`. Without it,
/// `HISTORY_LIMIT` entries carrying long questions and answers serialize past
/// `HISTORY_LIMIT_BYTES`, `load` discards the file whole, and the viewer's
/// entire private record disappears on the next read.
#[test]
fn a_history_of_large_attempts_stays_readable_rather_than_vanishing() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);

    // Entries far larger than the per-entry caps would allow, written through
    // the struct directly so this test exercises the write-side prune rather
    // than the constructors' caps.
    for index in 0..HISTORY_LIMIT {
        let mut large = entry(&format!("attempt-{index}"), 1_000 + index as u64);
        large.question = "q".repeat(200 * 1024);
        large.markdown = Some("a".repeat(200 * 1024));
        record(&ownership, large, 2_000).expect("record");
    }

    let size = std::fs::metadata(history_file(&ownership))
        .expect("history file")
        .len();
    assert!(
        size <= HISTORY_LIMIT_BYTES,
        "the writer must keep the file inside the bound its reader enforces: {size}"
    );
    let entries = load(&owned_receipt(&fixture), 2_000);
    assert!(
        !entries.is_empty(),
        "an oversized write must cost the oldest attempts, not all of them"
    );
    assert_eq!(
        entries[0].attempt_id,
        format!("attempt-{}", HISTORY_LIMIT - 1),
        "the newest attempt is the one that must survive"
    );
}

/// A question longer than the per-entry cap is shortened at a character
/// boundary and marked, never sliced through a multi-byte character.
///
/// Production line: the `shortened(question, HISTORY_QUESTION_BYTES)` call in
/// `PrivateAskHistoryEntry::refused`. A naive `&value[..limit]` panics here.
#[test]
fn a_long_multibyte_question_is_shortened_at_a_character_boundary() {
    // Three bytes per character, so the cap lands mid-character.
    let question = "な".repeat(HISTORY_QUESTION_BYTES);
    assert!(!question.is_char_boundary(HISTORY_QUESTION_BYTES));

    let recorded = PrivateAskHistoryEntry::refused(
        &question,
        "attempt",
        &super::super::PrivateAskFailure::AgentBusy,
        1_000,
    );

    assert!(recorded.question.ends_with(HISTORY_TRUNCATION_MARKER));
    assert!(recorded.question.len() <= HISTORY_QUESTION_BYTES + HISTORY_TRUNCATION_MARKER.len());
    assert!(
        recorded.question.starts_with("な"),
        "the retained prefix must still be valid text"
    );
}

/// An unreadable history is moved aside rather than silently replaced.
///
/// Production line: the `quarantine(&path)` calls in `load`. Without them the
/// only copy of what this viewer asked is overwritten by the next attempt, and
/// nothing on the machine says it ever existed.
#[test]
fn an_unusable_history_is_kept_under_a_name_that_says_so() {
    let fixture = canonical_tempdir();
    let ownership = owned_receipt(&fixture);
    record(&ownership, entry("first", 1_000), 1_000).expect("record");
    let path = history_file(&ownership);
    std::fs::write(&path, b"not json at all").expect("damage the history");

    assert!(load(&owned_receipt(&fixture), 1_100).is_empty());

    let quarantined = path
        .parent()
        .expect("history directory")
        .join(HISTORY_QUARANTINE_FILE);
    assert_eq!(
        std::fs::read(&quarantined).expect("quarantined bytes"),
        b"not json at all",
        "the bytes that could not be read must still be on disk"
    );
    assert!(
        !path.exists(),
        "the unusable file must not stay in place to be read again every time"
    );

    // The next attempt writes a fresh history beside the quarantine.
    record(&ownership, entry("second", 1_200), 1_200).expect("record after quarantine");
    let entries = load(&owned_receipt(&fixture), 1_300);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].attempt_id, "second");
}
