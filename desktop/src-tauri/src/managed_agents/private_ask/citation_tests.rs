//! The citation fence, exercised directly.
//!
//! The production line each of these binds to is the foreign-path check and the
//! grounding-ordered projection in `citations::resolve`; deleting either makes
//! at least one of these fail.

use super::super::tests::grounding;
use super::{resolve, CITATION_PREFIX};
use super::{GroundedSource, PrivateAskFailure};

/// A second grounded source whose path shares a prefix with the first, so a
/// prefix-matching fence would confuse the two.
fn sibling() -> GroundedSource {
    let content = "fn other() {\n    7\n}\n".to_string();
    GroundedSource {
        path: "src/lib.rs.bak".into(),
        start_line: 1,
        end_line: 3,
        source_hash: crew_wiki::source_snapshot::source_hash(content.as_bytes()),
        content,
        snapshot_head_event_id: "a".repeat(64),
    }
}

#[test]
fn an_answer_citing_its_grounding_keeps_that_citation() {
    let grounding = vec![grounding()];
    let markdown = format!("It returns 42.\n\n{CITATION_PREFIX}src/lib.rs\n");

    let citations = resolve(&markdown, &grounding).expect("a grounded citation resolves");
    assert_eq!(citations, grounding);
}

#[test]
fn an_answer_citing_a_path_it_was_never_given_is_refused() {
    let grounding = vec![grounding()];
    let markdown =
        format!("It reads the key.\n\n{CITATION_PREFIX}/Users/employee/.ssh/id_ed25519\n");

    // The whole answer is refused: an answer that cites a file outside the
    // verified snapshot either invented it or reached it.
    assert_eq!(
        resolve(&markdown, &grounding).unwrap_err(),
        PrivateAskFailure::InvalidOutput
    );
}

#[test]
fn a_citation_that_is_only_a_prefix_of_a_grounded_path_is_foreign() {
    let grounding = vec![sibling()];
    let markdown = format!("Prefix.\n{CITATION_PREFIX}src/lib.rs\n");

    // `src/lib.rs` is a prefix of `src/lib.rs.bak`; only exact equality accounts
    // for a citation.
    assert_eq!(
        resolve(&markdown, &grounding).unwrap_err(),
        PrivateAskFailure::InvalidOutput
    );
}

#[test]
fn an_answer_with_no_citation_line_is_accepted_with_none() {
    let grounding = vec![grounding()];

    let citations =
        resolve("The source does not say.", &grounding).expect("silence is not hostile");
    assert!(citations.is_empty());
}

#[test]
fn the_same_path_cited_twice_yields_one_citation() {
    let grounding = vec![grounding()];
    let markdown = format!("Twice.\n{CITATION_PREFIX}src/lib.rs\n{CITATION_PREFIX}src/lib.rs\n");

    assert_eq!(
        resolve(&markdown, &grounding)
            .expect("duplicates collapse")
            .len(),
        1
    );
}

#[test]
fn citations_follow_the_grounding_order_not_the_answer_order() {
    let grounding = vec![grounding(), sibling()];
    let markdown = format!("Both.\n{CITATION_PREFIX}src/lib.rs.bak\n{CITATION_PREFIX}src/lib.rs\n");

    let citations = resolve(&markdown, &grounding).expect("both are grounded");
    assert_eq!(
        citations
            .iter()
            .map(|source| source.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/lib.rs", "src/lib.rs.bak"]
    );
}

#[test]
fn an_empty_citation_line_is_refused() {
    let grounding = vec![grounding()];
    let markdown = format!("Empty.\n{CITATION_PREFIX}   \n");

    // A citation naming nothing is not "no citation"; it is a malformed answer.
    assert_eq!(
        resolve(&markdown, &grounding).unwrap_err(),
        PrivateAskFailure::InvalidOutput
    );
}

#[test]
fn an_answer_with_no_grounding_cannot_cite_anything() {
    let markdown = format!("Ungrounded.\n{CITATION_PREFIX}src/lib.rs\n");

    assert_eq!(
        resolve(&markdown, &[]).unwrap_err(),
        PrivateAskFailure::InvalidOutput
    );
    assert!(resolve("Ungrounded.", &[])
        .expect("an uncited answer needs no grounding")
        .is_empty());
}

#[test]
fn an_answer_flooded_with_citation_lines_is_refused_rather_than_walked() {
    let grounding = vec![grounding()];
    let mut markdown = String::from("Flood.\n");
    for _ in 0..2000 {
        markdown.push_str(CITATION_PREFIX);
        markdown.push_str("src/lib.rs\n");
    }

    assert_eq!(
        resolve(&markdown, &grounding).unwrap_err(),
        PrivateAskFailure::InvalidOutput
    );
}
