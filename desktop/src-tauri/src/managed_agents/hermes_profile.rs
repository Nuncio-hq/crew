//! Hermes profile binding helpers (Crew feature 0001 / D-019).
//!
//! Owns profile-name validation, spawn-arg injection for `profile_arg`,
//! the last-write `BUZZ_ACP_MODEL` strip (spike 0013), and duplicate-binding
//! detection. Capability facts still live on [`KnownAcpRuntime`]; this module
//! only applies them.

use crate::managed_agents::discovery::KnownAcpRuntime;
use crate::managed_agents::types::{BackendKind, ManagedAgentRecord, RespondTo};

/// Hermes rejects the reserved name `default` for Crew bindings (D-019 P-7):
/// the manager's personal `~/.hermes` profile must never be bound.
pub const HERMES_FORBIDDEN_PROFILE_NAME: &str = "default";

/// Validate a Hermes profile name for Crew binding.
///
/// Rule (spike 0011 / Hermes CLI): `^[a-z0-9][a-z0-9_-]{0,63}$`.
/// The reserved name [`HERMES_FORBIDDEN_PROFILE_NAME`] is always rejected.
pub fn validate_hermes_profile_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("hermes profile name must not be empty".to_string());
    }
    if trimmed == HERMES_FORBIDDEN_PROFILE_NAME {
        return Err(
            "hermes profile 'default' cannot be bound to a Crew agent (manager personal profile)"
                .to_string(),
        );
    }
    let valid = trimmed.len() <= 64
        && trimmed
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && trimmed
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if !valid {
        return Err(format!(
            "invalid hermes profile name '{trimmed}': must match [a-z0-9][a-z0-9_-]{{0,63}}"
        ));
    }
    Ok(())
}

/// Validate D-024's trusted-autonomy boundary for a prospective managed-agent
/// record. Callers invoke this on create/update before persisting the changed
/// record. It deliberately does not run from the generic storage path: legacy
/// records must remain readable and stoppable while the owner repairs them.
pub(crate) fn validate_profile_bound_agent_invariants(
    record: &ManagedAgentRecord,
) -> Result<(), String> {
    let Some(profile) = record
        .hermes_profile
        .as_deref()
        .map(str::trim)
        .filter(|profile| !profile.is_empty())
    else {
        return Ok(());
    };
    if record.respond_to != RespondTo::OwnerOnly {
        return Err(format!(
            "Hermes profile-bound agent '{}' (profile '{profile}') must use respond-to 'owner-only'; choose Only me to continue",
            record.name
        ));
    }
    if record.backend != BackendKind::Local {
        return Err(format!(
            "Hermes profile-bound agent '{}' (profile '{profile}') must run locally; create it on This computer, or delete and recreate an existing remote agent, before continuing",
            record.name
        ));
    }
    Ok(())
}

/// Whether the resolved runtime owns its model via a Hermes profile
/// (`KnownAcpRuntime::id == "hermes"` after command normalization).
pub fn is_hermes_runtime(effective_command: &str) -> bool {
    crate::managed_agents::known_acp_runtime(effective_command).is_some_and(|r| r.id == "hermes")
}

/// Last-write guard (spike 0013): strip `BUZZ_ACP_MODEL` from the spawn
/// command when the effective runtime is Hermes, so field-resolution and
/// user env maps cannot override the profile's model.
pub fn strip_model_env_for_profile_locked_runtime(
    command: &mut std::process::Command,
    effective_command: &str,
) {
    if is_hermes_runtime(effective_command) {
        command.env_remove("BUZZ_ACP_MODEL");
    }
}

/// Prepend `[profile_arg, name]` to normalized agent args when the runtime
/// declares a profile flag and the record binds a profile.
///
/// If the user's explicit args already contain the flag (legacy tier-3 JSON),
/// those args win and injection is skipped. Binding with no `profile_arg` on
/// the runtime is ignored (no error).
pub fn inject_profile_binding_args(
    runtime: Option<&KnownAcpRuntime>,
    hermes_profile: Option<&str>,
    mut args: Vec<String>,
) -> Vec<String> {
    let Some(rt) = runtime else {
        return args;
    };
    let Some(flag) = rt.profile_arg else {
        return args;
    };
    let Some(name) = hermes_profile.map(str::trim).filter(|n| !n.is_empty()) else {
        return args;
    };
    if args.iter().any(|a| a == flag) {
        return args;
    }
    let mut out = Vec::with_capacity(args.len() + 2);
    out.push(flag.to_string());
    out.push(name.to_string());
    out.append(&mut args);
    out
}

/// Strip `BUZZ_ACP_MODEL` from an env map for hash/spawn agreement when the
/// runtime is Hermes (post-guard view).
pub fn env_without_suppressed_model_for_runtime(
    effective_command: &str,
    env: &std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    if !is_hermes_runtime(effective_command) {
        return env.clone();
    }
    env.iter()
        .filter(|(k, _)| k.as_str() != "BUZZ_ACP_MODEL")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Find another managed-agent record on the same relay that already binds
/// `profile`. `exclude_pubkey` skips the record being updated.
pub fn find_duplicate_hermes_profile_binding<'a>(
    records: &'a [ManagedAgentRecord],
    profile: &str,
    relay_url: &str,
    exclude_pubkey: Option<&str>,
) -> Option<&'a ManagedAgentRecord> {
    let profile = profile.trim();
    if profile.is_empty() {
        return None;
    }
    records.iter().find(|r| {
        if exclude_pubkey.is_some_and(|pk| r.pubkey == pk) {
            return false;
        }
        r.hermes_profile.as_deref().is_some_and(|p| p == profile) && r.relay_url == relay_url
    })
}

/// Parse create-time `hermes_profile` input (trim empty → None; validate when Some).
pub fn parse_optional_hermes_profile(raw: Option<&str>) -> Result<Option<String>, String> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => {
            validate_hermes_profile_name(name)?;
            Ok(Some(name.to_string()))
        }
        None => Ok(None),
    }
}

/// Parse + duplicate-check for create-time binding (call under the records lock).
pub fn bind_hermes_profile_on_create(
    raw: Option<&str>,
    records: &[ManagedAgentRecord],
    relay_url: &str,
) -> Result<Option<String>, String> {
    let profile = parse_optional_hermes_profile(raw)?;
    reject_duplicate_hermes_profile_if_set(records, profile.as_deref(), relay_url, None)?;
    Ok(profile)
}

/// Reject when `profile` is already bound on `relay_url` (C-10).
pub fn reject_duplicate_hermes_profile(
    records: &[ManagedAgentRecord],
    profile: &str,
    relay_url: &str,
    exclude_pubkey: Option<&str>,
) -> Result<(), String> {
    if let Some(other) =
        find_duplicate_hermes_profile_binding(records, profile, relay_url, exclude_pubkey)
    {
        return Err(format!(
            "hermes profile '{profile}' is already bound to agent '{}' ({})",
            other.name, other.pubkey
        ));
    }
    Ok(())
}

/// No-op when `profile` is `None`; otherwise [`reject_duplicate_hermes_profile`].
pub fn reject_duplicate_hermes_profile_if_set(
    records: &[ManagedAgentRecord],
    profile: Option<&str>,
    relay_url: &str,
    exclude_pubkey: Option<&str>,
) -> Result<(), String> {
    match profile {
        Some(p) => reject_duplicate_hermes_profile(records, p, relay_url, exclude_pubkey),
        None => Ok(()),
    }
}

/// Resolve a patch update for `hermes_profile` (`None` = don't touch).
pub fn resolve_hermes_profile_update(
    update: &Option<Option<String>>,
    records: &[ManagedAgentRecord],
    pubkey: &str,
    relay_url_override: Option<&str>,
) -> Result<Option<Option<String>>, String> {
    match update {
        None => Ok(None),
        Some(None) => Ok(Some(None)),
        Some(Some(name)) => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                return Ok(Some(None));
            }
            validate_hermes_profile_name(trimmed)?;
            let relay = records
                .iter()
                .find(|r| r.pubkey == pubkey)
                .map(|r| {
                    relay_url_override
                        .map(str::trim)
                        .unwrap_or(r.relay_url.as_str())
                        .to_string()
                })
                .unwrap_or_else(|| relay_url_override.unwrap_or("").trim().to_string());
            reject_duplicate_hermes_profile(records, trimmed, &relay, Some(pubkey))?;
            Ok(Some(Some(trimmed.to_string())))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_agents::known_acp_runtime;
    use std::collections::BTreeMap;
    use std::ffi::OsStr;

    #[test]
    fn validate_accepts_valid_profile_names() {
        let max = "a".repeat(64);
        for name in ["scout", "a", "builder-1", "x_y", "0start", max.as_str()] {
            validate_hermes_profile_name(name)
                .unwrap_or_else(|e| panic!("expected {name:?} valid: {e}"));
        }
    }

    #[test]
    fn validate_rejects_default_profile() {
        let err = validate_hermes_profile_name("default").unwrap_err();
        assert!(err.contains("default"), "{err}");
    }

    #[test]
    fn validate_rejects_invalid_profile_names() {
        let too_long = "a".repeat(65);
        for name in [
            "Bad",
            "has space",
            "bad!",
            "",
            "-leading",
            "_leading",
            too_long.as_str(),
        ] {
            assert!(
                validate_hermes_profile_name(name).is_err(),
                "expected {name:?} invalid"
            );
        }
    }

    #[test]
    fn inject_prepends_profile_flag_before_acp() {
        let rt = known_acp_runtime("hermes").expect("hermes runtime");
        let args = inject_profile_binding_args(Some(rt), Some("scout"), vec!["acp".into()]);
        assert_eq!(
            args,
            vec!["-p".to_string(), "scout".to_string(), "acp".to_string()]
        );
    }

    #[test]
    fn inject_skips_when_explicit_args_already_have_flag() {
        let rt = known_acp_runtime("hermes").expect("hermes runtime");
        let explicit = vec!["-p".into(), "scout".into(), "acp".into()];
        let args = inject_profile_binding_args(Some(rt), Some("scout"), explicit.clone());
        assert_eq!(args, explicit);
    }

    #[test]
    fn inject_noop_without_profile_or_non_profile_runtime() {
        let hermes = known_acp_runtime("hermes").expect("hermes");
        let goose = known_acp_runtime("goose").expect("goose");
        assert_eq!(
            inject_profile_binding_args(Some(hermes), None, vec!["acp".into()]),
            vec!["acp".to_string()]
        );
        assert_eq!(
            inject_profile_binding_args(Some(goose), Some("scout"), vec!["acp".into()]),
            vec!["acp".to_string()]
        );
    }

    #[test]
    fn strip_model_env_removes_buzz_acp_model_for_hermes() {
        let mut cmd = std::process::Command::new("buzz-acp");
        cmd.env("BUZZ_ACP_MODEL", "should-not-leak");
        cmd.env("OTHER", "keep");
        strip_model_env_for_profile_locked_runtime(&mut cmd, "hermes");
        let envs: BTreeMap<_, _> = cmd
            .get_envs()
            .filter_map(|(k, v)| {
                Some((
                    k.to_string_lossy().into_owned(),
                    v?.to_string_lossy().into_owned(),
                ))
            })
            .collect();
        assert!(!envs.contains_key("BUZZ_ACP_MODEL"));
        assert_eq!(envs.get("OTHER").map(String::as_str), Some("keep"));
    }

    #[test]
    fn strip_model_env_preserves_buzz_acp_model_for_goose() {
        let mut cmd = std::process::Command::new("buzz-acp");
        cmd.env("BUZZ_ACP_MODEL", "goose-model");
        strip_model_env_for_profile_locked_runtime(&mut cmd, "goose");
        assert!(cmd.get_envs().any(
            |(k, v)| k == OsStr::new("BUZZ_ACP_MODEL") && v == Some(OsStr::new("goose-model"))
        ));
    }

    #[test]
    fn duplicate_binding_detects_same_relay_profile() {
        let mut a = minimal_record("aaa", "wss://relay");
        a.hermes_profile = Some("scout".into());
        let mut b = minimal_record("bbb", "wss://relay");
        b.hermes_profile = Some("scout".into());
        let records = vec![a, b];
        let hit =
            find_duplicate_hermes_profile_binding(&records, "scout", "wss://relay", Some("bbb"));
        assert_eq!(hit.map(|r| r.pubkey.as_str()), Some("aaa"));
    }

    #[test]
    fn duplicate_binding_allows_same_profile_on_different_relay() {
        let mut a = minimal_record("aaa", "wss://relay-a");
        a.hermes_profile = Some("scout".into());
        let records = vec![a];
        assert!(
            find_duplicate_hermes_profile_binding(&records, "scout", "wss://relay-b", None)
                .is_none()
        );
    }

    #[test]
    fn profile_bound_agents_require_owner_only_local_boundary() {
        let mut record = minimal_record("aaa", "wss://relay");
        record.hermes_profile = Some("scout".to_string());
        record.respond_to = RespondTo::Anyone;

        let public_error = validate_profile_bound_agent_invariants(&record)
            .expect_err("profile-bound Hermes agents must reject respond-to anyone");
        assert!(public_error.contains("owner-only"), "{public_error}");
        assert!(public_error.contains("Only me"), "{public_error}");

        record.respond_to = RespondTo::OwnerOnly;
        record.backend = BackendKind::Provider {
            id: "remote".to_string(),
            config: serde_json::json!({}),
        };
        let remote_error = validate_profile_bound_agent_invariants(&record)
            .expect_err("profile-bound Hermes agents must reject provider backends");
        assert!(remote_error.contains("local"), "{remote_error}");
        assert!(remote_error.contains("This computer"), "{remote_error}");

        record.backend = BackendKind::Local;
        validate_profile_bound_agent_invariants(&record)
            .expect("owner-only local profile-bound Hermes agent must be accepted");
    }

    fn minimal_record(pubkey: &str, relay: &str) -> ManagedAgentRecord {
        ManagedAgentRecord {
            pubkey: pubkey.into(),
            name: "agent".into(),
            persona_id: None,
            team_id: None,
            private_key_nsec: String::new(),
            auth_tag: None,
            relay_url: relay.into(),
            avatar_url: None,
            acp_command: "buzz-acp".into(),
            agent_command: "hermes".into(),
            agent_command_override: None,
            agent_args: vec![],
            hermes_profile: None,
            mcp_command: String::new(),
            turn_timeout_seconds: 320,
            idle_timeout_seconds: None,
            max_turn_duration_seconds: None,
            parallelism: 1,
            system_prompt: None,
            model: None,
            provider: None,
            persona_source_version: None,
            env_vars: BTreeMap::new(),
            start_on_app_launch: true,
            auto_restart_on_config_change: true,
            runtime_pid: None,
            backend: Default::default(),
            backend_agent_id: None,
            provider_binary_path: None,
            persona_team_dir: None,
            persona_name_in_team: None,
            created_at: String::new(),
            updated_at: String::new(),
            last_started_at: None,
            last_stopped_at: None,
            last_exit_code: None,
            last_error: None,
            last_error_code: None,
            respond_to: Default::default(),
            respond_to_allowlist: vec![],
            display_name: None,
            slug: None,
            runtime: Some("hermes".into()),
            name_pool: vec![],
            is_builtin: false,
            is_active: true,
            shared: false,
            source_team: None,
            source_team_persona_slug: None,
            catalog_source: None,
            definition_respond_to: None,
            definition_respond_to_allowlist: vec![],
            definition_parallelism: None,
            relay_mesh: None,
        }
    }
}
