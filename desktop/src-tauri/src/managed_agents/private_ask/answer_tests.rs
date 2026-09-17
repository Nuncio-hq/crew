//! The answer path through the real launch: what a contained runtime prints
//! decides what comes back, and a foreign citation refuses the whole answer.
//!
//! These bind to the production line in `PrivateAskAttempt::run` that calls
//! `citations::resolve` on the parsed markdown. Restoring the old
//! `request.grounding.clone()` makes both of them fail.

#![cfg(all(unix, target_os = "macos"))]

use super::tests::{
    canonical_tempdir, executable, fake_runtime, grounding, owned_receipt, request, state,
};
use super::{
    admit_private_ask, PrivateAskAttempt, PrivateAskCapability, PrivateAskFailure,
    PrivateAskResponse,
};

/// A benign runtime that prints the answer it is told to print.
fn answering_runtime(directory: &std::path::Path, answer: &str) -> std::path::PathBuf {
    let script = format!(
        "#!/usr/bin/perl\nlocal $/; my $in = <STDIN>; die \"no prompt\" unless defined $in; print '{{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"{answer}\",\"modelUsage\":{{\"claude-fable-5-1\":{{}}}}}}';\n"
    );
    fake_runtime(directory, "claude", &script)
}

fn run_with_answer(answer: &str) -> Result<PrivateAskResponse, PrivateAskFailure> {
    let fixture = canonical_tempdir();
    let path = answering_runtime(fixture.path(), answer);
    let mut selected = state(&path, "claude", "claude-fable-5-1", None);
    selected.executable = executable(&path);
    let capability = PrivateAskCapability::verified_for_fixture(&selected);
    let admission = admit_private_ask(request(), selected, capability).expect("admission");
    let ownership = owned_receipt(&fixture);
    let base = ownership.recap_base().expect("recap base");
    let attempt = PrivateAskAttempt::create(admission, ownership, 1).expect("attempt");
    let outcome = attempt.run();
    // Either way the generation is closed out; a refused answer is a finished
    // attempt, not an abandoned run root.
    assert!(
        !base
            .join("recap-runs")
            .read_dir()
            .expect("run roots")
            .any(|entry| entry.is_ok()),
        "the finished generation is cleaned on both outcomes"
    );
    outcome
}

#[test]
fn a_contained_runtime_answers_with_the_citation_it_printed() {
    let response = run_with_answer("It returns 42.\\n\\n[^cite]: src/lib.rs")
        .expect("a grounded answer is returned");

    assert!(response.markdown.starts_with("It returns 42."));
    assert_eq!(response.citations, vec![grounding()]);
}

#[test]
fn an_answer_citing_a_file_outside_the_snapshot_is_refused() {
    // The markdown is otherwise a perfectly ordinary answer. What makes it
    // unacceptable is the path it claims to have used.
    assert_eq!(
        run_with_answer("Here is the key.\\n\\n[^cite]: /Users/employee/.ssh/id_ed25519")
            .unwrap_err(),
        PrivateAskFailure::InvalidOutput
    );
}

#[test]
fn an_answer_that_cites_nothing_still_answers() {
    let response = run_with_answer("The source does not say.").expect("silence is not hostile");

    assert_eq!(response.markdown, "The source does not say.");
    assert!(response.citations.is_empty());
}
