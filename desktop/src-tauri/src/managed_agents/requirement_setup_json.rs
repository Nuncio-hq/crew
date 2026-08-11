//! Serialize readiness [`Requirement`] values into setup-nudge JSON.

use crate::managed_agents::Requirement;
use serde_json::Value;

/// Map a readiness requirement to the buzz-acp setup-nudge JSON shape.
pub(crate) fn requirement_to_setup_json(requirement: Requirement) -> Value {
    match requirement {
        Requirement::NormalizedField { field } => serde_json::json!({
            "surface": "normalized_field",
            "field": field,
        }),
        Requirement::EnvKey { key } => serde_json::json!({
            "surface": "env_key",
            "key": key,
        }),
        Requirement::CliLogin {
            probe_args,
            setup_copy,
            availability,
        } => serde_json::json!({
            "surface": "cli_login",
            "probe_args": probe_args,
            "setup_copy": setup_copy,
            "availability": availability,
        }),
        Requirement::CliConfigInvalid {
            probe_args,
            setup_copy,
            diagnostic,
        } => serde_json::json!({
            "surface": "cli_config_invalid",
            "probe_args": probe_args,
            "setup_copy": setup_copy,
            "diagnostic": diagnostic,
        }),
        Requirement::GitBash => serde_json::json!({
            "surface": "git_bash",
        }),
        Requirement::MissingBinary { command } => serde_json::json!({
            "surface": "missing_binary",
            "command": command,
        }),
        Requirement::HermesProfileDirectoryMissing { profile } => serde_json::json!({
            "surface": "hermes_profile_directory_missing",
            "profile": profile,
        }),
        Requirement::HermesProfileConfigInvalid {
            profile,
            diagnostic,
        } => serde_json::json!({
            "surface": "hermes_profile_config_invalid",
            "profile": profile,
            "diagnostic": diagnostic,
        }),
    }
}
