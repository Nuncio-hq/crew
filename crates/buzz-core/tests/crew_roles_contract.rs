use buzz_core::crew_role::{
    compose_role_section, parse_canvas_assignments, resolve_assignment, resolve_canvas_assignments,
    strip_crew_block, RoleAssignment, RoleParseError,
};
use nostr::{Keys, ToBech32};

#[test]
fn assignment_parse_and_resolve_is_agent_scoped_and_owner_signed() {
    let owner = Keys::generate().public_key();
    let agent = Keys::generate().public_key();
    let owner_hex = owner.to_hex();
    let agent_hex = agent.to_hex();
    let agent_npub = agent.to_bech32().expect("npub");
    let canvas = r#"
introductory founder prose
```crew
assignments:
  AGENT-NPUB: "  Code Review  "
definitions:
  code review: |
    allowed: inspect and review code
    not-allowed: change production files
    redirect: ask the implementation agent
future_key:
  ignored: true
```
closing founder prose
"#;
    let canvas = canvas
        .replace("AGENT-NPUB", &agent_npub)
        .replace("owner", &owner_hex)
        .replace("agent-one", &agent_hex);

    let assignment = resolve_assignment(&canvas, &owner_hex, &owner_hex, &agent_hex)
        .expect("valid crew block")
        .expect("assignment exists");

    assert_eq!(assignment.label, "Code Review");
    assert_eq!(
        assignment.definition,
        "allowed: inspect and review code\nnot-allowed: change production files\nredirect: ask the implementation agent\n"
    );
}

#[test]
fn non_owner_canvas_is_ignored_and_no_block_is_unchanged() {
    let owner = "11".repeat(32);
    let agent = "22".repeat(32);
    let stranger = "33".repeat(32);
    let canvas =
        format!("```crew\nassignments:\n  {agent}: code\ndefinitions:\n  code: ignored\n```");
    assert!(resolve_assignment(&canvas, &stranger, &owner, &agent)
        .expect("non-owner canvas should fail closed")
        .is_none());
    assert!(
        resolve_assignment("ordinary canvas prose", &owner, &owner, &agent)
            .expect("canvas without crew block is valid")
            .is_none()
    );
}

#[test]
fn all_owner_assignments_resolve_to_channel_scoped_labels() {
    let owner = Keys::generate().public_key();
    let agent = Keys::generate().public_key();
    let canvas = format!(
        "```crew\nassignments:\n  {}: reviewer\ndefinitions:\n  reviewer: inspect only\n```",
        agent.to_bech32().expect("agent npub")
    );
    let assignments = resolve_canvas_assignments(&canvas, &owner.to_hex(), &owner.to_hex())
        .expect("valid crew block")
        .expect("founder canvas");
    assert_eq!(
        assignments,
        vec![buzz_core::crew_role::CanvasRoleAssignment {
            agent_pubkey: agent.to_hex(),
            role_label: "reviewer".into(),
        }]
    );
}

#[test]
fn malformed_or_partial_block_fails_closed() {
    let malformed = "```crew\nassignments:\n  \"11\": [unterminated\n```";
    assert!(parse_canvas_assignments(malformed).is_err());
    assert!(resolve_assignment(
        malformed,
        &"11".repeat(32),
        &"11".repeat(32),
        &"22".repeat(32)
    )
    .is_err());
}

#[test]
fn missing_definition_is_a_distinct_fail_closed_error() {
    let owner = "11".repeat(32);
    let agent = "22".repeat(32);
    assert_eq!(
        resolve_assignment(
            &format!("```crew\nassignments:\n  {agent}: reviewer\n```"),
            &owner,
            &owner,
            &agent,
        ),
        Err(RoleParseError::MissingDefinition("reviewer".into()))
    );
}

#[test]
fn invalid_pubkey_is_a_fail_closed_error() {
    assert!(matches!(
        resolve_assignment(
            "```crew\nassignments:\n  not-a-key: reviewer\ndefinitions:\n  reviewer: review\n```",
            &"11".repeat(32),
            &"11".repeat(32),
            &"22".repeat(32),
        ),
        Err(RoleParseError::InvalidPubkey(_))
    ));
}

#[test]
fn canvas_rendering_strips_crew_block_and_preserves_prose() {
    let rendered = strip_crew_block(
        "Founder intro.\n\n```crew\nassignments: {}\ndefinitions: {}\n```\n\nClosing prose.",
    )
    .expect("valid fence");
    assert_eq!(rendered, "Founder intro.\n\n\nClosing prose.");
    assert!(!rendered.contains("assignments:"));
}

#[test]
fn definition_text_reaches_prompt_verbatim() {
    let assignment = RoleAssignment {
        label: "arbitrary founder label".into(),
        definition: "allowed: do X\nnot-allowed: do Y\nredirect: ask Z".into(),
    };
    let section = compose_role_section(&assignment);
    assert!(section.contains(&assignment.label));
    assert!(section.contains(&assignment.definition));
    assert!(section.contains("ROLE-CHECK:"));
}
