//! Crew role prompt injection (issue #116 Slice 1).
//!
//! Soft enforcement: when a verified owner-signed role is present, compose a
//! role section into the system prompt delivered on each fresh ACP session
//! (same "next session" model as `!rotate` / Hermes model changes).
//!
//! Section format matches spike 0016 fixtures, strengthened with few-shot
//! examples for the Hermes short-accept declaration gap.

use std::path::{Path, PathBuf};

/// Env: inline role value (spawn-time snapshot).
pub const CREW_ROLE_ENV: &str = "BUZZ_ACP_CREW_ROLE";

/// Env: path to a role file re-read on every fresh session (no respawn).
pub const CREW_ROLE_FILE_ENV: &str = "BUZZ_ACP_CREW_ROLE_FILE";

/// Day-one taxonomy (must match desktop `managed_agents::crew_role::TAXONOMY`).
pub const TAXONOMY: &[&str] = &["code", "content", "research", "ops"];

/// Validate / normalize a role string. Empty → None. Unknown → None (fail closed
/// for injection — do not invent a section for garbage input).
pub fn parse_role(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    TAXONOMY
        .iter()
        .find(|r| **r == trimmed)
        .map(|r| (*r).to_string())
}

/// Read role for the next session: prefer live file contents, else env snapshot.
pub fn resolve_role_for_session(
    role_env: Option<&str>,
    role_file: Option<&Path>,
) -> Option<String> {
    if let Some(path) = role_file {
        if let Ok(contents) = std::fs::read_to_string(path) {
            return parse_role(Some(contents.as_str()));
        }
        // Missing/unreadable file with an explicit path means cleared/unknown.
        return None;
    }
    parse_role(role_env)
}

/// Build the role section markdown for a taxonomy role. `None` if no role.
pub fn role_section(role: &str) -> Option<String> {
    let role = parse_role(Some(role))?;
    Some(match role.as_str() {
        "code" => role_section_code(),
        "content" => role_section_content(),
        "research" => role_section_research(),
        "ops" => role_section_ops(),
        _ => unreachable!("parse_role filters taxonomy"),
    })
}

/// Append role section to an existing system prompt body, or return role-only.
///
/// When `role` is `None`, returns `system_prompt` unchanged (byte-identical for
/// the no-role path).
pub fn compose_system_prompt_with_role(
    system_prompt: Option<&str>,
    role: Option<&str>,
) -> Option<String> {
    let section = role.and_then(role_section);
    match (
        system_prompt.map(str::trim).filter(|s| !s.is_empty()),
        section,
    ) {
        (Some(sp), Some(role_sec)) => Some(format!("{sp}\n\n{role_sec}")),
        (None, Some(role_sec)) => Some(role_sec),
        (Some(sp), None) => Some(sp.to_string()),
        (None, None) => None,
    }
}

/// Load role config from process environment (spawn).
pub fn role_config_from_env() -> (Option<String>, Option<PathBuf>) {
    let role_env = std::env::var(CREW_ROLE_ENV)
        .ok()
        .and_then(|s| parse_role(Some(&s)));
    let role_file = std::env::var(CREW_ROLE_FILE_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    (role_env, role_file)
}

fn declaration_block(role: &str) -> String {
    format!(
        r#"MANDATORY declaration: The FIRST line of your first reply message for each turn MUST be exactly:

ROLE-CHECK: role={role} decision=accept|refuse reason=<short>

Never omit ROLE-CHECK — not even for short "no work needed" answers.
Few-shot (copy the shape):
- On-role short accept:
ROLE-CHECK: role={role} decision=accept reason=on-role-no-mutation-needed
Already correct; no file changes.
- Off-role refuse:
ROLE-CHECK: role={role} decision=refuse reason=off-role
This belongs to another role. Please re-assign or mention that role's agent.

Then continue with the accept work or the refuse/redirect body."#
    )
}

fn role_section_code() -> String {
    format!(
        r#"## Role assignment (Crew)

You are assigned role: **code**.

ALLOWED work for this role:
- Repository code changes (source, tests, build config)
- Debugging, refactors, code review notes
- Short code comments / developer-facing API docs that ship with code

NOT ALLOWED (off-role):
- Marketing copy, blog posts, LinkedIn/social posts, brand style guides
- Pure product launch prose or sales landing pages
- Long-form content writing that is not part of shipping code

When a mention is OFF-ROLE:
1. Do NOT silently execute repo-mutating or content work for that request.
2. Refuse with a short explanation.
3. Name the correct role to handle it (usually `content`) and say the founder should re-assign or mention that role's agent.
4. Do not partially do the off-role work "as a favor".

Boundary guidance:
- Fixing a typo inside a **code identifier** or test is ON-ROLE.
- Rewriting README marketing narrative is OFF-ROLE (content).
- Changing a user-visible **dialog string in source** (e.g. TypeScript/Rust UI string) is ON-ROLE code maintenance.
- Writing a standalone blog post file is OFF-ROLE.

{}"#,
        declaration_block("code")
    )
}

fn role_section_content() -> String {
    format!(
        r#"## Role assignment (Crew)

You are assigned role: **content**.

ALLOWED work for this role:
- Marketing copy, blog posts, release notes prose, social posts
- README narrative / product messaging (non-code)
- Brand tone and style guide text

NOT ALLOWED (off-role):
- Repository code changes, refactors, tests, build config
- Debugging production code or changing source identifiers
- Editing TypeScript/Rust/UI source except pure prose docs outside code

When a mention is OFF-ROLE:
1. Do NOT silently execute code or repo-mutating engineering work.
2. Refuse with a short explanation.
3. Name the correct role (`code`) and say the founder should re-assign or mention that role's agent.
4. Do not partially edit code "as a favor".

Boundary guidance:
- Drafting README product story is ON-ROLE.
- Changing `fn greet` or adding unit tests is OFF-ROLE (code).
- Editing dialog copy as a marketing rewrite request without touching code structure may be ON-ROLE if delivered as prose suggestion; do not edit source files for code tasks.

{}"#,
        declaration_block("content")
    )
}

fn role_section_research() -> String {
    format!(
        r#"## Role assignment (Crew)

You are assigned role: **research**.

ALLOWED work for this role:
- Investigation, comparison, literature/web survey, evidence summaries
- Clarifying questions and options analysis
- Non-mutating notes that document findings

NOT ALLOWED (off-role):
- Shipping production code changes or refactors
- Marketing/launch prose as primary deliverable
- Ops/infrastructure changes (deploy, secrets, production config)

When a mention is OFF-ROLE:
1. Do NOT silently execute off-role mutating work.
2. Refuse with a short explanation.
3. Name the correct role (`code`, `content`, or `ops`) and redirect to the founder or that agent.
4. Do not partially do the off-role work "as a favor".

{}"#,
        declaration_block("research")
    )
}

fn role_section_ops() -> String {
    format!(
        r#"## Role assignment (Crew)

You are assigned role: **ops**.

ALLOWED work for this role:
- Deploy, runtime config, CI plumbing, environment and process ops
- Incident response notes and operational runbooks
- Secrets/process hygiene that is not product marketing or app feature code

NOT ALLOWED (off-role):
- Product feature implementation (belongs to `code`)
- Marketing/blog/social content (belongs to `content`)
- Pure research literature reviews without ops outcome (belongs to `research`)

When a mention is OFF-ROLE:
1. Do NOT silently execute off-role work.
2. Refuse with a short explanation.
3. Name the correct role and redirect.
4. Do not partially do the off-role work "as a favor".

{}"#,
        declaration_block("ops")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn no_role_leaves_system_prompt_byte_identical() {
        let sp = "You are helpful.";
        let out = compose_system_prompt_with_role(Some(sp), None).unwrap();
        assert_eq!(out, sp);
        assert!(compose_system_prompt_with_role(None, None).is_none());
    }

    #[test]
    fn role_section_included_iff_verified_role() {
        let out = compose_system_prompt_with_role(Some("base"), Some("code")).unwrap();
        assert!(out.starts_with("base\n\n## Role assignment (Crew)"));
        assert!(out.contains("role: **code**"));
        assert!(out.contains("ROLE-CHECK: role=code"));
        assert!(out.contains("few-shot") || out.contains("Few-shot"));
    }

    #[test]
    fn role_section_content_matches_record_role() {
        let code = role_section("code").unwrap();
        assert!(code.contains("**code**"));
        assert!(!code.contains("**content**") || code.contains("`content`")); // may name redirect
        let content = role_section("content").unwrap();
        assert!(content.contains("**content**"));
        assert!(content.contains("ROLE-CHECK: role=content"));
    }

    #[test]
    fn unknown_role_does_not_inject() {
        assert!(role_section("marketing").is_none());
        let out = compose_system_prompt_with_role(Some("base"), Some("marketing")).unwrap();
        assert_eq!(out, "base");
    }

    #[test]
    fn role_file_reread_enables_fresh_session_semantics() {
        let dir = tempfile_dir();
        let path = dir.join("crew-role.txt");
        std::fs::write(&path, "code").unwrap();
        assert_eq!(
            resolve_role_for_session(Some("ops"), Some(&path)).as_deref(),
            Some("code"),
            "file wins over stale env"
        );
        std::fs::write(&path, "content").unwrap();
        assert_eq!(
            resolve_role_for_session(Some("code"), Some(&path)).as_deref(),
            Some("content"),
            "next session re-reads file without respawn"
        );
        std::fs::write(&path, "").unwrap();
        assert_eq!(
            resolve_role_for_session(Some("code"), Some(&path)),
            None,
            "empty file clears role"
        );
        // env-only path when no file
        assert_eq!(
            resolve_role_for_session(Some("research"), None).as_deref(),
            Some("research")
        );
    }

    #[test]
    fn section_has_allowed_not_allowed_and_declaration() {
        for role in TAXONOMY {
            let s = role_section(role).unwrap();
            assert!(s.contains("ALLOWED"), "{role}");
            assert!(s.contains("NOT ALLOWED"), "{role}");
            assert!(s.contains(&format!("ROLE-CHECK: role={role}")), "{role}");
            assert!(s.contains("decision=accept|refuse"), "{role}");
        }
    }

    fn tempfile_dir() -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "buzz-acp-crew-role-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn write_helper_smoke() {
        // keep Write import used if needed for future
        let dir = tempfile_dir();
        let mut f = std::fs::File::create(dir.join("x")).unwrap();
        f.write_all(b"code").unwrap();
    }
}
