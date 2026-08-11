use buzz_core::crew_role::{
    compose_role_section, parse_canvas_assignments, resolve_assignment, RoleAssignment,
};

#[test]
fn assignment_parse_and_resolve_is_agent_scoped_and_owner_signed() {
    let canvas = r#"
introductory founder prose
```crew
assignments:
  AGENT-ONE: "  Code Review  "
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

    let assignment = resolve_assignment(canvas, "owner", "OWNER", "agent-one")
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
    let canvas = "```crew\nassignments:\n  agent: code\ndefinitions:\n  code: ignored\n```";
    assert!(resolve_assignment(canvas, "stranger", "owner", "agent")
        .expect("non-owner canvas should fail closed")
        .is_none());
    assert!(
        resolve_assignment("ordinary canvas prose", "owner", "owner", "agent")
            .expect("canvas without crew block is valid")
            .is_none()
    );
}

#[test]
fn malformed_or_partial_block_fails_closed() {
    let malformed = "```crew\nassignments:\n  agent: [unterminated\n```";
    assert!(parse_canvas_assignments(malformed).is_err());
    assert!(resolve_assignment(malformed, "owner", "owner", "agent").is_err());
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
