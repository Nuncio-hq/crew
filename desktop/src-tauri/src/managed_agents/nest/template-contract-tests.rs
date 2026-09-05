use super::*;

#[test]
fn nest_skill_contains_safe_mention_workflow() {
    assert!(BUZZ_CLI_SKILL_MD.contains("--mention <hex-or-npub>"));
    assert!(BUZZ_CLI_SKILL_MD.contains("every presentation-only name that should notify"));
    assert!(BUZZ_CLI_SKILL_MD
        .contains("permits unresolved or ambiguous `@Name` text as presentation-only"));
    assert!(BUZZ_CLI_SKILL_MD.contains("signed event's `mention_pubkeys`"));
    assert!(BUZZ_CLI_SKILL_MD.contains("no follow-up verification command is needed"));
    assert!(BUZZ_CLI_SKILL_MD.contains("Add membership separately only when authorized"));
    assert!(BUZZ_CLI_SKILL_MD.contains("never changes membership automatically"));
}

#[test]
fn nest_agents_template_separates_commit_attribution_claims() {
    assert_eq!(AGENTS_MD.matches("## Git Commit Attribution").count(), 1);
    assert!(AGENTS_MD.contains(
        "Git authorship, co-authorship, DCO sign-off, and cryptographic signing are separate claims"
    ));
    assert!(AGENTS_MD
        .contains("Request, approval, review, or accountability alone is not co-authorship"));
    assert!(AGENTS_MD.contains("A sign-off is not an approval marker"));
    assert!(AGENTS_MD.contains("Never use another person's signing key"));
    assert!(AGENTS_MD.contains("inspect every outgoing commit against the actual upstream or base"));
    assert!(AGENTS_MD.contains("An agent-owned repository may use the agent as author"));
    assert!(!AGENTS_MD.contains("every commit MUST include a `Signed-off-by`"));
}
