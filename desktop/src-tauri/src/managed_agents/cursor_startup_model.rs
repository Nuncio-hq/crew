//! Cursor CLI Auto / model selection at process start.
//!
//! Cursor's ACP session metadata often returns an empty model catalog, so
//! runtime `session/set_model` cannot apply the owner's choice reliably.
//! Pinning `--model` at process start is the documented Cursor CLI path.
//!
//! Model ids travel through comma-delimited `BUZZ_ACP_AGENT_ARGS`, so commas
//! inside a model value are rejected (same hazard as custom harness args).

use crate::managed_agents::custom_harnesses::HarnessDefinition;
use crate::managed_agents::discovery::KnownAcpRuntime;
use crate::managed_agents::effective_config::{resolve_effective_config, EffectiveConfigResult};
use crate::managed_agents::normalize_command_identity;
use crate::managed_agents::readiness::hermes::resolve_agent_args_with_profile;
use crate::managed_agents::types::{AgentDefinition, ManagedAgentRecord};
use crate::managed_agents::GlobalAgentConfig;

/// Whether this command is the Cursor ACP CLI (`cursor-agent`).
pub(crate) fn is_cursor_agent_command(command: &str) -> bool {
    normalize_command_identity(command) == "cursor-agent"
}

/// True when `BUZZ_ACP_MODEL` must stay unset because model is applied via argv.
pub(crate) fn skip_buzz_acp_model_env(command: &str) -> bool {
    is_cursor_agent_command(command)
}

/// Reject model values that would split across `BUZZ_ACP_AGENT_ARGS` commas.
pub(crate) fn model_safe_for_agent_args(model: &str) -> bool {
    !model.contains(',')
}

/// Prepend `cursor-agent --model <id> …` when a model is configured.
///
/// Existing `--model` flags are left untouched. Models containing `,` are
/// skipped so they cannot inject extra argv tokens through the env transport.
pub(crate) fn inject_cursor_startup_model_arg(
    command: &str,
    args: Vec<String>,
    model: Option<&str>,
) -> Vec<String> {
    if !is_cursor_agent_command(command) {
        return args;
    }
    let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
        return args;
    };
    if !model_safe_for_agent_args(model) {
        tracing::warn!(
            model,
            "skipping Cursor --model injection: model contains a comma \
             (would split BUZZ_ACP_AGENT_ARGS)"
        );
        return args;
    }
    if args.windows(2).any(|window| window[0] == "--model") {
        return args;
    }
    let mut injected = Vec::with_capacity(args.len() + 2);
    injected.push("--model".to_string());
    injected.push(model.to_string());
    injected.extend(args);
    injected
}

/// Hermes profile args, then Cursor `--model` when applicable.
pub(crate) fn resolve_effective_agent_args(
    effective_command: &str,
    record: &ManagedAgentRecord,
    harness_def: Option<&HarnessDefinition>,
    runtime_meta: Option<&'static KnownAcpRuntime>,
    personas: &[AgentDefinition],
    global: &GlobalAgentConfig,
) -> Vec<String> {
    let args =
        resolve_agent_args_with_profile(effective_command, record, harness_def, runtime_meta);
    let effective_model = match resolve_effective_config(record, personas, global) {
        EffectiveConfigResult::Resolved(cfg) => cfg.model.value,
        EffectiveConfigResult::OrphanedInstance { .. } => None,
    };
    inject_cursor_startup_model_arg(effective_command, args, effective_model.as_deref())
}

/// Wire model for `BUZZ_ACP_MODEL`, or `None` when Cursor owns model via argv.
#[cfg(feature = "mesh-llm")]
pub(crate) fn resolve_buzz_acp_model_env(
    effective_command: &str,
    mesh_model_id: &Option<String>,
    effective_model: Option<&str>,
) -> Option<String> {
    if skip_buzz_acp_model_env(effective_command) {
        return None;
    }
    match (mesh_model_id, effective_model) {
        (Some(mesh_model_id), _) => {
            Some(crate::managed_agents::relay_mesh_wire_model(mesh_model_id).to_string())
        }
        (None, model) => model.map(str::to_owned),
    }
}

#[cfg(not(feature = "mesh-llm"))]
pub(crate) fn resolve_buzz_acp_model_env(
    effective_command: &str,
    effective_model: Option<&str>,
) -> Option<String> {
    if skip_buzz_acp_model_env(effective_command) {
        return None;
    }
    effective_model.map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_cursor_agent_command() {
        assert!(is_cursor_agent_command("cursor-agent"));
        assert!(is_cursor_agent_command("/usr/local/bin/cursor-agent"));
        assert!(!is_cursor_agent_command("buzz-agent"));
    }

    #[test]
    fn injects_cursor_startup_model_before_acp() {
        assert_eq!(
            inject_cursor_startup_model_arg("cursor-agent", vec!["acp".into()], Some("auto"),),
            vec!["--model".to_string(), "auto".to_string(), "acp".to_string()]
        );
        assert_eq!(
            inject_cursor_startup_model_arg("cursor-agent", vec!["acp".into()], None),
            vec!["acp".to_string()]
        );
        assert_eq!(
            inject_cursor_startup_model_arg(
                "cursor-agent",
                vec!["--model".into(), "composer-2".into(), "acp".into()],
                Some("auto"),
            ),
            vec![
                "--model".to_string(),
                "composer-2".to_string(),
                "acp".to_string()
            ]
        );
        assert_eq!(
            inject_cursor_startup_model_arg("goose", vec!["acp".into()], Some("auto")),
            vec!["acp".to_string()]
        );
    }

    #[test]
    fn rejects_comma_in_cursor_model_for_argv_injection() {
        assert_eq!(
            inject_cursor_startup_model_arg(
                "cursor-agent",
                vec!["acp".into()],
                Some("composer-2,--evil"),
            ),
            vec!["acp".to_string()]
        );
        assert!(!model_safe_for_agent_args("a,b"));
        assert!(model_safe_for_agent_args("auto"));
    }

    #[test]
    fn skips_buzz_acp_model_env_for_cursor() {
        assert!(skip_buzz_acp_model_env("cursor-agent"));
        assert!(!skip_buzz_acp_model_env("buzz-agent"));
    }
}
