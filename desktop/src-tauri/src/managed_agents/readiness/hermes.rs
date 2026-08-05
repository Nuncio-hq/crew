//! Hermes readiness + profile-aware arg resolution (C-03 / C-12 degraded).

use super::{EffectiveAgentEnv, Requirement};
use crate::managed_agents::custom_harnesses::HarnessDefinition;
use crate::managed_agents::discovery::KnownAcpRuntime;
use crate::managed_agents::normalize_agent_args;
use crate::managed_agents::types::ManagedAgentRecord;

/// Binary on PATH + bound profile name. Profile-directory existence and auth
/// probing are deferred (no truthful Hermes probe yet — spike 0010).
pub(super) fn hermes_requirements(effective: &EffectiveAgentEnv) -> Vec<Requirement> {
    let mut missing = Vec::new();
    if crate::managed_agents::resolve_command(&effective.effective_command).is_none() {
        missing.push(Requirement::MissingBinary {
            command: effective.effective_command.clone(),
        });
    }
    if effective
        .hermes_profile
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .is_none()
    {
        missing.push(Requirement::NormalizedField {
            field: "hermesProfile".to_string(),
        });
    }
    missing
}

/// Instance args win; else definition args; then inject `-p <profile>` when set.
pub(super) fn resolve_agent_args_with_profile(
    effective_command: &str,
    record: &ManagedAgentRecord,
    harness_def: Option<&HarnessDefinition>,
    runtime_meta: Option<&'static KnownAcpRuntime>,
) -> Vec<String> {
    let record_args = record.agent_args.clone();
    let instance_has_args = record_args.iter().any(|a| !a.trim().is_empty());
    let base = if instance_has_args {
        normalize_agent_args(effective_command, record_args)
    } else if let Some(def) = harness_def {
        normalize_agent_args(effective_command, def.args.clone())
    } else {
        normalize_agent_args(effective_command, record_args)
    };
    crate::managed_agents::hermes_profile::inject_profile_binding_args(
        runtime_meta,
        record.hermes_profile.as_deref(),
        base,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_agents::readiness::agent_readiness;
    use std::collections::BTreeMap;

    #[test]
    fn hermes_without_profile_binding_requires_hermes_profile_field() {
        let env = EffectiveAgentEnv {
            env: BTreeMap::new(),
            config_file_path: None,
            effective_command: "hermes".to_string(),
            hermes_profile: None,
        };
        let readiness = agent_readiness(&env);
        assert!(
            !readiness.is_ready(),
            "unbound Hermes agent must be NotReady"
        );
        assert!(
            readiness
                .requirements()
                .contains(&Requirement::NormalizedField {
                    field: "hermesProfile".to_string(),
                }),
            "expected hermesProfile requirement; got {:?}",
            readiness.requirements()
        );
    }

    #[test]
    fn hermes_with_profile_binding_does_not_require_hermes_profile_field() {
        let env = EffectiveAgentEnv {
            env: BTreeMap::new(),
            config_file_path: None,
            effective_command: "hermes".to_string(),
            hermes_profile: Some("scout".to_string()),
        };
        let readiness = agent_readiness(&env);
        let reqs = readiness.requirements();
        assert!(
            !reqs.contains(&Requirement::NormalizedField {
                field: "hermesProfile".to_string(),
            }),
            "bound profile must clear hermesProfile requirement; got {reqs:?}"
        );
    }
}
