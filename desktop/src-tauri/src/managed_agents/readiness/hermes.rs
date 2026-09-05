//! Hermes readiness + profile-aware arg resolution (C-03 / C-12 degraded).

use super::{EffectiveAgentEnv, Requirement};
use crate::managed_agents::custom_harnesses::HarnessDefinition;
use crate::managed_agents::discovery::KnownAcpRuntime;
use crate::managed_agents::normalize_agent_args;
use crate::managed_agents::types::ManagedAgentRecord;
use crate::managed_agents::{hermes_profile_readiness, HermesProfileReadiness};

/// Binary on PATH + bound profile name + profile directory on disk.
/// Auth probing remains deferred (no truthful Hermes probe yet — spike 0010).
pub(super) fn hermes_requirements(effective: &EffectiveAgentEnv) -> Vec<Requirement> {
    let mut missing = Vec::new();
    match effective
        .hermes_profile
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        None => {
            missing.push(Requirement::NormalizedField {
                field: "hermesProfile".to_string(),
            });
            if let Some(HermesProfileReadiness::BinaryMissing { command }) =
                hermes_profile_readiness(&effective.effective_command, None)
            {
                missing.push(Requirement::MissingBinary { command });
            }
        }
        Some(profile) => {
            match hermes_profile_readiness(&effective.effective_command, Some(profile)) {
                Some(HermesProfileReadiness::BinaryMissing { command }) => {
                    missing.push(Requirement::MissingBinary { command });
                }
                Some(HermesProfileReadiness::Missing { profile }) => {
                    missing.push(Requirement::HermesProfileDirectoryMissing { profile });
                }
                Some(HermesProfileReadiness::BrokenConfig {
                    profile,
                    diagnostic,
                }) => missing.push(Requirement::HermesProfileConfigInvalid {
                    profile,
                    diagnostic,
                }),
                Some(HermesProfileReadiness::Ready)
                | Some(HermesProfileReadiness::AuthUnknown { .. })
                | None => {}
            }
        }
    }
    missing
}

/// Instance args win; else definition args; then inject `-p <profile>` when set.
pub(crate) fn resolve_agent_args_with_profile(
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
    fn unbound_hermes_with_missing_binary_reports_both_requirements() {
        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("tempdir");
        let original_path = std::env::var("PATH").ok();
        std::env::set_var("PATH", temp.path());
        crate::managed_agents::clear_resolve_cache();

        let env = EffectiveAgentEnv {
            env: BTreeMap::new(),
            config_file_path: None,
            effective_command: "hermes".to_string(),
            hermes_profile: None,
        };
        let readiness = agent_readiness(&env);
        let requirements = readiness.requirements();
        assert!(requirements.contains(&Requirement::NormalizedField {
            field: "hermesProfile".to_string(),
        }));
        assert!(requirements.contains(&Requirement::MissingBinary {
            command: "hermes".to_string(),
        }));

        match original_path {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
        crate::managed_agents::clear_resolve_cache();
    }

    #[test]
    fn hermes_with_profile_binding_does_not_require_hermes_profile_field() {
        // Isolate HERMES_HOME so the orphan check sees a present directory.
        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("tempdir");
        let hermes_home = temp.path().join("hermes-home");
        std::fs::create_dir_all(hermes_home.join("profiles/scout")).expect("scout dir");
        let original = std::env::var("HERMES_HOME").ok();
        std::env::set_var("HERMES_HOME", &hermes_home);

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
        assert!(
            !reqs
                .iter()
                .any(|r| matches!(r, Requirement::HermesProfileDirectoryMissing { .. })),
            "present directory must not mark orphan; got {reqs:?}"
        );

        match original {
            Some(h) => std::env::set_var("HERMES_HOME", h),
            None => std::env::remove_var("HERMES_HOME"),
        }
    }

    #[test]
    fn hermes_bound_but_directory_missing_is_orphan_requirement() {
        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("tempdir");
        let hermes_home = temp.path().join("hermes-home");
        std::fs::create_dir_all(hermes_home.join("profiles")).expect("profiles");
        let binary = temp.path().join("hermes");
        std::fs::write(&binary, "#!/bin/sh\nexit 0\n").expect("fake hermes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
        }
        let original = std::env::var("HERMES_HOME").ok();
        let original_path = std::env::var("PATH").ok();
        std::env::set_var("HERMES_HOME", &hermes_home);
        std::env::set_var("PATH", temp.path());
        crate::managed_agents::clear_resolve_cache();

        let env = EffectiveAgentEnv {
            env: BTreeMap::new(),
            config_file_path: None,
            effective_command: "hermes".to_string(),
            hermes_profile: Some("ghost".to_string()),
        };
        let readiness = agent_readiness(&env);
        assert!(!readiness.is_ready());
        assert!(
            readiness
                .requirements()
                .contains(&Requirement::HermesProfileDirectoryMissing {
                    profile: "ghost".to_string(),
                }),
            "expected orphan requirement; got {:?}",
            readiness.requirements()
        );

        match original {
            Some(h) => std::env::set_var("HERMES_HOME", h),
            None => std::env::remove_var("HERMES_HOME"),
        }
        match original_path {
            Some(path) => std::env::set_var("PATH", path),
            None => std::env::remove_var("PATH"),
        }
        crate::managed_agents::clear_resolve_cache();
    }

    #[test]

    fn readiness_contract_names_all_states_and_keeps_auth_unknown_advisory() {
        let states = [
            HermesProfileReadiness::Ready,
            HermesProfileReadiness::Missing {
                profile: "ghost".into(),
            },
            HermesProfileReadiness::BrokenConfig {
                profile: "broken".into(),
                diagnostic: "invalid YAML".into(),
            },
            HermesProfileReadiness::BinaryMissing {
                command: "hermes".into(),
            },
            HermesProfileReadiness::AuthUnknown {
                profile: "scout".into(),
            },
        ];
        assert_eq!(states.len(), 5);
        assert!(matches!(states[0], HermesProfileReadiness::Ready));
        assert!(matches!(states[1], HermesProfileReadiness::Missing { .. }));
        assert!(matches!(
            states[2],
            HermesProfileReadiness::BrokenConfig { .. }
        ));
        assert!(matches!(
            states[3],
            HermesProfileReadiness::BinaryMissing { .. }
        ));
        assert!(matches!(
            states[4],
            HermesProfileReadiness::AuthUnknown { .. }
        ));
    }

    #[test]
    fn healthy_profile_is_auth_unknown_without_a_requirement() {
        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("tempdir");
        let hermes_home = temp.path().join("hermes-home");
        let profile_dir = hermes_home.join("profiles/scout");
        std::fs::create_dir_all(&profile_dir).expect("profile dir");
        let binary = temp.path().join("hermes");
        std::fs::write(&binary, "#!/bin/sh\nexit 0\n").expect("fake hermes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
        }
        let original_home = std::env::var("HERMES_HOME").ok();
        let original_path = std::env::var("PATH").ok();
        std::env::set_var("HERMES_HOME", &hermes_home);
        std::env::set_var("PATH", temp.path());
        crate::managed_agents::clear_resolve_cache();

        let state = hermes_profile_readiness("hermes", Some("scout"))
            .expect("Hermes command should be evaluated");
        assert!(matches!(
            state,
            HermesProfileReadiness::AuthUnknown { ref profile } if profile == "scout"
        ));
        let env = EffectiveAgentEnv {
            env: BTreeMap::new(),
            config_file_path: None,
            effective_command: "hermes".to_string(),
            hermes_profile: Some("scout".to_string()),
        };
        let readiness = agent_readiness(&env);
        assert!(
            readiness.is_ready(),
            "auth-unknown must not enter setup mode"
        );
        assert!(readiness.requirements().is_empty());

        match original_home {
            Some(value) => std::env::set_var("HERMES_HOME", value),
            None => std::env::remove_var("HERMES_HOME"),
        }
        match original_path {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        crate::managed_agents::clear_resolve_cache();
    }

    #[test]
    fn home_profile_default_uses_hermes_home_not_profiles_default() {
        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("tempdir");
        let hermes_home = temp.path().join("hermes-home");
        // Home profile lives at HERMES_HOME itself. No profiles/default dir.
        std::fs::create_dir_all(&hermes_home).expect("hermes home");
        std::fs::create_dir_all(hermes_home.join("profiles")).expect("profiles");
        let binary = temp.path().join("hermes");
        std::fs::write(&binary, "#!/bin/sh\nexit 0\n").expect("fake hermes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
        }
        let original_home = std::env::var("HERMES_HOME").ok();
        let original_path = std::env::var("PATH").ok();
        std::env::set_var("HERMES_HOME", &hermes_home);
        std::env::set_var("PATH", temp.path());
        crate::managed_agents::clear_resolve_cache();

        let state = hermes_profile_readiness("hermes", Some("default"))
            .expect("Hermes command should be evaluated");
        assert!(
            matches!(
                state,
                HermesProfileReadiness::AuthUnknown { ref profile } if profile == "default"
            ),
            "default must resolve to ~/.hermes, not Missing profiles/default; got {state:?}"
        );
        let env = EffectiveAgentEnv {
            env: BTreeMap::new(),
            config_file_path: None,
            effective_command: "hermes".to_string(),
            hermes_profile: Some("default".to_string()),
        };
        let readiness = agent_readiness(&env);
        assert!(
            readiness.is_ready(),
            "bound default home profile must be ready; got {:?}",
            readiness.requirements()
        );
        assert!(
            !readiness
                .requirements()
                .iter()
                .any(|r| matches!(r, Requirement::HermesProfileDirectoryMissing { .. })),
            "must not treat missing profiles/default as orphan; got {:?}",
            readiness.requirements()
        );

        match original_home {
            Some(value) => std::env::set_var("HERMES_HOME", value),
            None => std::env::remove_var("HERMES_HOME"),
        }
        match original_path {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        crate::managed_agents::clear_resolve_cache();
    }

    #[test]
    fn readiness_fixture_maps_missing_broken_and_binary_states() {
        let _path_guard = crate::managed_agents::lock_path_mutex();
        let temp = tempfile::tempdir().expect("tempdir");
        let hermes_home = temp.path().join("hermes-home");
        let profiles = hermes_home.join("profiles");
        std::fs::create_dir_all(profiles.join("broken")).expect("broken profile");
        std::fs::create_dir_all(&profiles).expect("profiles");
        std::fs::write(profiles.join("broken/config.yaml"), "model: [").expect("broken config");
        let binary = temp.path().join("hermes");
        std::fs::write(&binary, "#!/bin/sh\nexit 0\n").expect("fake hermes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
        }
        let original_home = std::env::var("HERMES_HOME").ok();
        let original_path = std::env::var("PATH").ok();
        std::env::set_var("HERMES_HOME", &hermes_home);
        std::env::set_var("PATH", temp.path());
        crate::managed_agents::clear_resolve_cache();

        assert!(matches!(
            hermes_profile_readiness("hermes", Some("ghost")),
            Some(HermesProfileReadiness::Missing { .. })
        ));
        assert!(matches!(
            hermes_profile_readiness("hermes", Some("broken")),
            Some(HermesProfileReadiness::BrokenConfig { .. })
        ));

        std::env::set_var("PATH", temp.path().join("missing"));
        crate::managed_agents::clear_resolve_cache();
        assert!(matches!(
            hermes_profile_readiness("hermes", Some("scout")),
            Some(HermesProfileReadiness::BinaryMissing { .. })
        ));

        match original_home {
            Some(value) => std::env::set_var("HERMES_HOME", value),
            None => std::env::remove_var("HERMES_HOME"),
        }
        match original_path {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        crate::managed_agents::clear_resolve_cache();
    }
}
