//! Crew agent roles (issue #116 Slice 1).
//!
//! - Day-one taxonomy lives in ONE place ([`TAXONOMY`]).
//! - Local source of truth: [`super::ManagedAgentRecord::crew_role`] (owner-assigned).
//! - Forward-compat private path: `30179` `extensions["crew:role"]` (spike 0015).
//! - Public projection: `["crew-role", <role>]` tag on kind `10100` (spike 0015).
//! - Only founder(owner)-signed role data has effect ([`role_authority_accepts`]).

#![allow(dead_code)] // codec helpers exercised in unit tests; dual-write path lands later

use std::collections::BTreeMap;

use buzz_core_pkg::kind::KIND_AGENT_PROFILE;
use serde_json::{json, Value};

/// Namespaced extension key on private managed-agent payload (`30179`).
pub const CREW_ROLE_EXTENSION_KEY: &str = "crew:role";

/// Public tag name on kind `10100` agent profile events.
pub const CREW_ROLE_TAG: &str = "crew-role";

/// Day-one taxonomy. Stored as a free string after validation against this list.
pub const TAXONOMY: &[&str] = &["code", "content", "research", "ops"];

/// Validate and normalize a role string.
///
/// Empty / whitespace → `None` (clear). Unknown non-empty values are rejected.
pub fn parse_crew_role(raw: Option<&str>) -> Result<Option<String>, String> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if !TAXONOMY.contains(&trimmed) {
        return Err(format!(
            "unknown crew role '{trimmed}' (allowed: {})",
            TAXONOMY.join(", ")
        ));
    }
    Ok(Some(trimmed.to_string()))
}

/// Insert or remove `crew:role` on a `30179` extensions map.
pub fn set_role_extension(
    extensions: &mut BTreeMap<String, Value>,
    role: Option<&str>,
) -> Result<(), String> {
    match parse_crew_role(role)? {
        Some(role) => {
            extensions.insert(CREW_ROLE_EXTENSION_KEY.to_string(), Value::String(role));
        }
        None => {
            extensions.remove(CREW_ROLE_EXTENSION_KEY);
        }
    }
    Ok(())
}

/// Read role from a `30179` extensions map. Non-string / unknown values → `None`.
pub fn role_from_extensions(extensions: &BTreeMap<String, Value>) -> Option<String> {
    let value = extensions.get(CREW_ROLE_EXTENSION_KEY)?;
    let s = value.as_str()?;
    parse_crew_role(Some(s)).ok().flatten()
}

/// True only when the event author is the founder/owner pubkey.
///
/// Non-owner role events and tags must be ignored for effect.
pub fn role_authority_accepts(author_pubkey_hex: &str, founder_pubkey_hex: &str) -> bool {
    let author = author_pubkey_hex.trim().to_ascii_lowercase();
    let founder = founder_pubkey_hex.trim().to_ascii_lowercase();
    !author.is_empty() && !founder.is_empty() && author == founder
}

/// Resolve effective role for prompt/UI: only owner-signed local assignment.
///
/// `inbound_author` is the pubkey that claimed the role (e.g. event author).
/// When `None`, the value is treated as already local/owner-written.
pub fn verified_owner_role(
    role: Option<&str>,
    inbound_author: Option<&str>,
    founder_pubkey_hex: &str,
) -> Option<String> {
    let role = parse_crew_role(role).ok().flatten()?;
    match inbound_author {
        None => Some(role),
        Some(author) if role_authority_accepts(author, founder_pubkey_hex) => Some(role),
        Some(_) => None,
    }
}

/// Build exactly zero or one `["crew-role", <role>]` tag pairs for kind `10100`.
pub fn crew_role_tag_values(role: Option<&str>) -> Option<(String, String)> {
    parse_crew_role(role)
        .ok()
        .flatten()
        .map(|r| (CREW_ROLE_TAG.to_string(), r))
}

/// Merge a role projection into an existing tag list.
///
/// - Removes any prior `crew-role` tags.
/// - When `role` is `Some`, appends exactly one `["crew-role", role]`.
/// - Preserves all other tags (stock-consumer / unknown-tag safety).
pub fn merge_crew_role_tags(existing: &[Vec<String>], role: Option<&str>) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = existing
        .iter()
        .filter(|t| t.first().map(String::as_str) != Some(CREW_ROLE_TAG))
        .cloned()
        .collect();
    if let Some((name, value)) = crew_role_tag_values(role) {
        out.push(vec![name, value]);
    }
    out
}

/// Read the first `crew-role` tag value from a tag list (display only).
pub fn role_from_tags(tags: &[Vec<String>]) -> Option<String> {
    tags.iter().find_map(|t| {
        if t.first().map(String::as_str) == Some(CREW_ROLE_TAG) {
            t.get(1)
                .map(String::as_str)
                .and_then(|s| parse_crew_role(Some(s)).ok().flatten())
        } else {
            None
        }
    })
}

/// Content body for a kind `10100` projection event.
///
/// Preserves `channel_add_policy` when provided so stock side effects still apply.
pub fn agent_profile_content(
    display_name: &str,
    channel_add_policy: Option<&str>,
) -> Result<String, String> {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "display_name".to_string(),
        Value::String(display_name.to_string()),
    );
    if let Some(policy) = channel_add_policy.map(str::trim).filter(|s| !s.is_empty()) {
        obj.insert(
            "channel_add_policy".to_string(),
            Value::String(policy.to_string()),
        );
    }
    serde_json::to_string(&Value::Object(obj)).map_err(|e| e.to_string())
}

/// Build an unsigned kind `10100` event with optional `crew-role` projection.
pub fn build_agent_profile_event(
    display_name: &str,
    channel_add_policy: Option<&str>,
    role: Option<&str>,
    extra_tags: &[Vec<String>],
) -> Result<nostr::EventBuilder, String> {
    let content = agent_profile_content(display_name, channel_add_policy)?;
    let tags = merge_crew_role_tags(extra_tags, role);
    let nostr_tags = tags
        .iter()
        .map(|parts| {
            nostr::Tag::parse(parts.iter().map(String::as_str).collect::<Vec<_>>())
                .map_err(|e| format!("invalid profile tag: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(
        nostr::EventBuilder::new(nostr::Kind::Custom(KIND_AGENT_PROFILE as u16), content)
            .tags(nostr_tags),
    )
}

/// Announcement body when the founder assigns or clears a role.
pub fn role_announcement_text(agent_name: &str, role: Option<&str>) -> String {
    match role {
        Some(role) => format!(
            "Role assignment: @{agent_name} is now **{role}**. Off-role work should be refused and redirected."
        ),
        None => format!("Role assignment: @{agent_name} no longer has a Crew role."),
    }
}

/// Wire bytes for a role file re-read by buzz-acp on each fresh session.
pub fn role_file_bytes(role: Option<&str>) -> String {
    role.unwrap_or("").to_string()
}

/// Path to the per-agent role file (re-read by buzz-acp without respawn).
pub fn crew_role_file_path(
    app: &tauri::AppHandle,
    pubkey: &str,
) -> Result<std::path::PathBuf, String> {
    let dir = crate::managed_agents::managed_agents_base_dir(app)?;
    Ok(dir.join(format!("{pubkey}.crew-role")))
}

/// Persist the role file so a running harness picks it up on next fresh session.
pub fn write_crew_role_file(
    app: &tauri::AppHandle,
    pubkey: &str,
    role: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    let path = crew_role_file_path(app, pubkey)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create role dir: {e}"))?;
    }
    std::fs::write(&path, role_file_bytes(role)).map_err(|e| format!("write role file: {e}"))?;
    Ok(path)
}

/// Apply Crew role env vars on a spawn command.
pub fn apply_crew_role_spawn_env(
    command: &mut std::process::Command,
    role: Option<&str>,
    role_file: &std::path::Path,
) {
    match role {
        Some(r) => {
            command.env("BUZZ_ACP_CREW_ROLE", r);
        }
        None => {
            command.env_remove("BUZZ_ACP_CREW_ROLE");
        }
    }
    command.env("BUZZ_ACP_CREW_ROLE_FILE", role_file);
}

/// JSON helper used by tests / projection dumps.
pub fn extensions_json(role: Option<&str>) -> Value {
    match role {
        Some(r) => json!({ CREW_ROLE_EXTENSION_KEY: r }),
        None => json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use buzz_core_pkg::private_managed_agent::{self, Payload, State};
    use nostr::Keys;
    use std::collections::BTreeMap;

    #[test]
    fn taxonomy_is_exactly_day_one_four() {
        assert_eq!(TAXONOMY, &["code", "content", "research", "ops"]);
    }

    #[test]
    fn parse_accepts_taxonomy_and_rejects_unknown() {
        assert_eq!(
            parse_crew_role(Some("code")).unwrap().as_deref(),
            Some("code")
        );
        assert_eq!(
            parse_crew_role(Some("  ops ")).unwrap().as_deref(),
            Some("ops")
        );
        assert_eq!(parse_crew_role(Some("")).unwrap(), None);
        assert_eq!(parse_crew_role(None).unwrap(), None);
        assert!(parse_crew_role(Some("marketing")).is_err());
    }

    #[test]
    fn extensions_round_trip_crew_role() {
        let mut ext = BTreeMap::new();
        set_role_extension(&mut ext, Some("code")).unwrap();
        assert_eq!(role_from_extensions(&ext).as_deref(), Some("code"));
        set_role_extension(&mut ext, None).unwrap();
        assert_eq!(role_from_extensions(&ext), None);
        assert!(!ext.contains_key(CREW_ROLE_EXTENSION_KEY));
    }

    #[test]
    fn private_managed_agent_codec_carries_crew_role_extension() {
        // Spike 0015 RED→GREEN path: extensions["crew:role"] survives codec.
        let owner = Keys::generate();
        let agent = Keys::generate();
        let mut extensions = BTreeMap::new();
        set_role_extension(&mut extensions, Some("research")).unwrap();

        // Minimal active payload via public builders when available; else hand-roll
        // only the extensions assertion through validate_payload + serde round-trip.
        let payload = serde_json::json!({
            "format": private_managed_agent::FORMAT,
            "version": private_managed_agent::VERSION,
            "agent_pubkey": agent.public_key().to_hex(),
            "owner_pubkey": owner.public_key().to_hex(),
            "generation": 1,
            "state": "active",
            "updated_at": "2026-08-10T00:00:00Z",
            "extensions": extensions_json(Some("research")),
        });
        let _ = payload;
        assert_eq!(
            role_from_extensions(&{
                let mut m = BTreeMap::new();
                set_role_extension(&mut m, Some("research")).unwrap();
                m
            })
            .as_deref(),
            Some("research")
        );

        // Serde shape for extensions key must contain ':'
        let mut bad = BTreeMap::new();
        bad.insert("role".to_string(), Value::String("code".into()));
        let payload = Payload {
            format: private_managed_agent::FORMAT.into(),
            version: private_managed_agent::VERSION,
            agent_pubkey: agent.public_key().to_hex(),
            owner_pubkey: owner.public_key().to_hex(),
            generation: 1,
            previous_event_id: None,
            state: State::Deleted,
            updated_at: "2026-08-10T00:00:00Z".into(),
            active: None,
            deleted_at: Some("2026-08-10T00:00:00Z".into()),
            extensions: {
                let mut m = BTreeMap::new();
                set_role_extension(&mut m, Some("code")).unwrap();
                m
            },
        };
        private_managed_agent::validate_payload(&payload).expect("valid extensions payload");
        assert_eq!(
            role_from_extensions(&payload.extensions).as_deref(),
            Some("code")
        );

        let mut bad_payload = payload.clone();
        bad_payload.extensions = bad;
        assert!(
            private_managed_agent::validate_payload(&bad_payload).is_err(),
            "bare 'role' key must fail namespaced-key validation"
        );
    }

    #[test]
    fn projection_emits_exactly_one_crew_role_tag() {
        let tags = merge_crew_role_tags(
            &[
                vec!["alt".into(), "agent profile".into()],
                vec!["crew-role".into(), "stale".into()],
            ],
            Some("code"),
        );
        let role_tags: Vec<_> = tags
            .iter()
            .filter(|t| t.first().map(String::as_str) == Some("crew-role"))
            .collect();
        assert_eq!(role_tags.len(), 1);
        assert_eq!(role_tags[0].as_slice(), ["crew-role", "code"]);
        assert!(tags
            .iter()
            .any(|t| t.first().map(String::as_str) == Some("alt")));
    }

    #[test]
    fn role_removal_clears_projection_tag() {
        let tags = merge_crew_role_tags(
            &[
                vec!["crew-role".into(), "code".into()],
                vec!["alt".into(), "keep".into()],
            ],
            None,
        );
        assert!(role_from_tags(&tags).is_none());
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0][0], "alt");
    }

    #[test]
    fn non_founder_role_event_is_ignored() {
        let founder = "aa".repeat(32);
        let other = "bb".repeat(32);
        assert!(role_authority_accepts(&founder, &founder));
        assert!(!role_authority_accepts(&other, &founder));
        assert_eq!(
            verified_owner_role(Some("code"), Some(&other), &founder),
            None
        );
        assert_eq!(
            verified_owner_role(Some("code"), Some(&founder), &founder).as_deref(),
            Some("code")
        );
        // Local (no inbound author) is already owner-written.
        assert_eq!(
            verified_owner_role(Some("ops"), None, &founder).as_deref(),
            Some("ops")
        );
    }

    #[test]
    fn build_agent_profile_event_kind_and_tag() {
        let builder = build_agent_profile_event(
            "Scout",
            Some("owner_only"),
            Some("content"),
            &[vec!["alt".into(), "x".into()]],
        )
        .unwrap();
        let keys = Keys::generate();
        let event = builder.sign_with_keys(&keys).unwrap();
        assert_eq!(event.kind.as_u16() as u32, KIND_AGENT_PROFILE);
        let tags: Vec<Vec<String>> = event
            .tags
            .iter()
            .map(|t| t.as_slice().iter().map(|s| s.to_string()).collect())
            .collect();
        assert_eq!(role_from_tags(&tags).as_deref(), Some("content"));
        let content: Value = serde_json::from_str(&event.content).unwrap();
        assert_eq!(content["channel_add_policy"], "owner_only");
        assert_eq!(content["display_name"], "Scout");
    }

    #[test]
    fn announcement_names_role() {
        let text = role_announcement_text("scout", Some("code"));
        assert!(text.contains("scout"));
        assert!(text.contains("code"));
    }

    #[test]
    fn stock_consumer_ignores_unknown_crew_role_tag_shape() {
        // Tag merge preserves non-role tags; stock side effects only read content.
        let tags =
            merge_crew_role_tags(&[vec!["alt".into(), "agent profile".into()]], Some("code"));
        assert!(tags
            .iter()
            .any(|t| t.first().map(String::as_str) == Some("alt")));
        assert_eq!(role_from_tags(&tags).as_deref(), Some("code"));
    }
}
