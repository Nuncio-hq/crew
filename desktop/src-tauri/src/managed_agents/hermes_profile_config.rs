//! Read and write the model settings owned by a Hermes profile.

use std::{
    path::{Path, PathBuf},
    process::Output,
};

use serde::{Deserialize, Serialize};

use super::{
    hermes_profile::{crew_may_mutate_hermes_profile, validate_hermes_profile_name},
    hermes_profile_lifecycle::{first_error_line, hermes_profile_dir, run_hermes},
    hermes_profile_readiness::read_profile_yaml,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HermesProfileConfigResult {
    Ok {
        name: String,
        provider: Option<String>,
        model: Option<String>,
    },
    DoesNotExist {
        name: String,
        message: String,
    },
    InvalidName {
        name: String,
        message: String,
    },
    BinaryMissing {
        message: String,
    },
    Rejected {
        name: String,
        message: String,
    },
    Failed {
        name: String,
        message: String,
    },
}

fn profile_dir_for_read(name: &str) -> Result<(String, PathBuf), HermesProfileConfigResult> {
    let trimmed = name.trim();
    if let Err(message) = validate_hermes_profile_name(trimmed) {
        return Err(HermesProfileConfigResult::InvalidName {
            name: trimmed.to_string(),
            message,
        });
    }
    let Some(dir) = hermes_profile_dir(trimmed) else {
        return Err(HermesProfileConfigResult::Failed {
            name: trimmed.to_string(),
            message: "could not resolve Hermes home directory".to_string(),
        });
    };
    Ok((trimmed.to_string(), dir))
}

fn profile_dir(name: &str) -> Result<(String, PathBuf), HermesProfileConfigResult> {
    let (trimmed, dir) = profile_dir_for_read(name)?;
    if !crew_may_mutate_hermes_profile(&trimmed) {
        return Err(HermesProfileConfigResult::InvalidName {
            name: trimmed.clone(),
            message: "hermes profile 'default' cannot be edited by Crew".to_string(),
        });
    }
    Ok((trimmed, dir))
}

fn read_values(name: &str, dir: &Path) -> Result<(Option<String>, Option<String>), String> {
    let Some(document) = read_profile_yaml(dir)? else {
        return Ok((None, None));
    };
    let Some(model) = document
        .as_mapping()
        .and_then(|mapping| mapping.get("model"))
    else {
        return Ok((None, None));
    };
    let Some(model) = model.as_mapping() else {
        return Err(format!("profile '{name}' has a non-mapping model config"));
    };
    let value = |key: &str| -> Result<Option<String>, String> {
        let Some(value) = model.get(key) else {
            return Ok(None);
        };
        value
            .as_str()
            .map(|value| Some(value.to_string()))
            .ok_or_else(|| format!("profile '{name}' has a non-string model.{key} value"))
    };
    Ok((value("provider")?, value("default")?))
}

fn read_from_dir(name: &str, dir: &Path) -> HermesProfileConfigResult {
    if !dir.is_dir() {
        return HermesProfileConfigResult::DoesNotExist {
            name: name.to_string(),
            message: format!("Hermes profile '{name}' does not exist"),
        };
    }
    match read_values(name, dir) {
        Ok((provider, model)) => HermesProfileConfigResult::Ok {
            name: name.to_string(),
            provider,
            model,
        },
        Err(message) => HermesProfileConfigResult::Failed {
            name: name.to_string(),
            message,
        },
    }
}

/// Reads the two model settings exposed by a Hermes profile.
pub fn read_profile_config(name: &str) -> HermesProfileConfigResult {
    match profile_dir_for_read(name) {
        Ok((name, dir)) => read_from_dir(&name, &dir),
        Err(result) => result,
    }
}

fn rejected(name: &str, output: &Output) -> HermesProfileConfigResult {
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    HermesProfileConfigResult::Rejected {
        name: name.to_string(),
        message: first_error_line(&combined).unwrap_or_else(|| {
            format!(
                "Hermes rejected a model configuration change (exit {})",
                output.status.code().unwrap_or(-1)
            )
        }),
    }
}

fn run_set(
    name: &str,
    binary: &Path,
    key: &str,
    value: &str,
) -> Result<(), HermesProfileConfigResult> {
    let args = vec![
        "-p".to_string(),
        name.to_string(),
        "config".to_string(),
        "set".to_string(),
        key.to_string(),
        value.to_string(),
    ];
    let output =
        run_hermes(binary, &args).map_err(|message| HermesProfileConfigResult::Failed {
            name: name.to_string(),
            message,
        })?;
    if output.status.success() {
        return Ok(());
    }
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_lowercase();
    if [
        "invalid",
        "unknown",
        "unsupported",
        "not found",
        "auth",
        "credential",
        "unauthorized",
    ]
    .iter()
    .any(|marker| combined.contains(marker))
    {
        Err(rejected(name, &output))
    } else {
        Err(HermesProfileConfigResult::Failed {
            name: name.to_string(),
            message: first_error_line(&format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
            .unwrap_or_else(|| {
                format!(
                    "Hermes config set failed (exit {})",
                    output.status.code().unwrap_or(-1)
                )
            }),
        })
    }
}

fn write_profile_config_with(
    name: &str,
    provider: Option<&str>,
    model: Option<&str>,
    binary: Option<PathBuf>,
) -> HermesProfileConfigResult {
    let (name, dir) = match profile_dir(name) {
        Ok(value) => value,
        Err(result) => return result,
    };
    if !dir.is_dir() {
        return HermesProfileConfigResult::DoesNotExist {
            name: name.clone(),
            message: format!("Hermes profile '{name}' does not exist"),
        };
    }
    for (kind, value) in [("provider", provider), ("model", model)] {
        if value.is_some_and(|value| value.trim().is_empty()) {
            return HermesProfileConfigResult::Rejected {
                name: name.clone(),
                message: format!("model.{kind} must not be blank"),
            };
        }
    }
    let current = match read_values(&name, &dir) {
        Ok(values) => values,
        Err(message) => {
            return HermesProfileConfigResult::Failed { name, message };
        }
    };
    let provider_changed = provider.is_some_and(|value| current.0.as_deref() != Some(value));
    let model_changed = model.is_some_and(|value| current.1.as_deref() != Some(value));
    if !provider_changed && !model_changed {
        return read_from_dir(&name, &dir);
    }
    let Some(binary) = binary else {
        return HermesProfileConfigResult::BinaryMissing {
            message: "hermes binary not found on PATH".to_string(),
        };
    };
    if provider_changed {
        if let Err(result) = run_set(
            &name,
            &binary,
            "model.provider",
            provider.unwrap_or_default(),
        ) {
            return result;
        }
    }
    if model_changed {
        if let Err(result) = run_set(&name, &binary, "model.default", model.unwrap_or_default()) {
            return result;
        }
    }
    read_from_dir(&name, &dir)
}

/// Applies changed model settings through the Hermes CLI and reads them back.
pub fn write_profile_config(
    name: &str,
    provider: Option<String>,
    model: Option<String>,
) -> HermesProfileConfigResult {
    write_profile_config_with(
        name,
        provider.as_deref(),
        model.as_deref(),
        crate::managed_agents::resolve_command("hermes"),
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

    use tempfile::TempDir;

    use super::*;

    fn setup() -> (
        std::sync::MutexGuard<'static, ()>,
        TempDir,
        PathBuf,
        PathBuf,
    ) {
        let guard = crate::managed_agents::lock_path_mutex();
        let home = tempfile::tempdir().unwrap();
        let profile = home.path().join("profiles/scout");
        fs::create_dir_all(&profile).unwrap();
        let script = home.path().join("hermes");
        fs::write(
            &script,
            r#"#!/bin/sh
set -eu
echo "$@" >> "$HERMES_HOME/commands.log"
profile="$2"
key="$5"
value="$6"
config="$HERMES_HOME/profiles/$profile/config.yaml"
provider="openai"
model="old-model"
if [ -f "$config" ]; then
  provider=$(sed -n 's/^  provider: //p' "$config" | tr -d '"')
  model=$(sed -n 's/^  default: //p' "$config" | tr -d '"')
fi
if [ "$key" = "model.provider" ]; then provider="$value"; fi
if [ "$key" = "model.default" ]; then model="$value"; fi
printf 'model:\n  provider: "%s"\n  default: "%s"\n' "$provider" "$model" > "$config"
"#,
        )
        .unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
        std::env::set_var("HERMES_HOME", home.path());
        (guard, home, profile, script)
    }

    #[test]
    fn reads_model_values_from_real_config() {
        let (_guard, _home, profile, _script) = setup();
        fs::write(
            profile.join("config.yaml"),
            "model:\n  provider: anthropic\n  default: claude-sonnet\nsecret: hidden\n",
        )
        .unwrap();
        assert_eq!(
            read_profile_config("scout"),
            HermesProfileConfigResult::Ok {
                name: "scout".to_string(),
                provider: Some("anthropic".to_string()),
                model: Some("claude-sonnet".to_string()),
            }
        );
    }

    #[test]
    fn absent_config_is_unset_success() {
        let (_guard, _home, _profile, _script) = setup();
        assert_eq!(
            read_profile_config("scout"),
            HermesProfileConfigResult::Ok {
                name: "scout".to_string(),
                provider: None,
                model: None,
            }
        );
    }

    #[test]
    fn missing_profile_is_classified() {
        let (_guard, _home, _profile, _script) = setup();
        assert!(matches!(
            read_profile_config("missing"),
            HermesProfileConfigResult::DoesNotExist { .. }
        ));
    }

    #[test]
    fn invalid_yaml_is_named_failure() {
        let (_guard, _home, profile, _script) = setup();
        fs::write(profile.join("config.yaml"), "model: [").unwrap();
        assert!(matches!(
            read_profile_config("scout"),
            HermesProfileConfigResult::Failed { message, .. } if !message.is_empty()
        ));
    }

    #[test]
    fn successful_write_reads_back_values() {
        let (_guard, home, profile, script) = setup();
        fs::write(
            profile.join("config.yaml"),
            "model:\n  provider: old-provider\n  default: old-model\n",
        )
        .unwrap();
        let result = write_profile_config_with(
            "scout",
            Some("new-provider"),
            Some("new-model"),
            Some(script),
        );
        assert_eq!(
            result,
            HermesProfileConfigResult::Ok {
                name: "scout".to_string(),
                provider: Some("new-provider".to_string()),
                model: Some("new-model".to_string()),
            }
        );
        assert_eq!(
            fs::read_to_string(home.path().join("commands.log"))
                .unwrap()
                .lines()
                .count(),
            2
        );
    }

    #[test]
    fn invalid_cli_value_is_rejected() {
        let (_guard, home, _profile, script) = setup();
        fs::write(&script, "#!/bin/sh\necho 'unknown model' >&2\nexit 1\n").unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();
        assert!(matches!(
            write_profile_config_with("scout", Some("bad"), None, Some(script)),
            HermesProfileConfigResult::Rejected { .. }
        ));
        assert!(!home.path().join("commands.log").exists());
    }

    #[test]
    fn blank_value_is_rejected_before_shelling_out() {
        let (_guard, _home, _profile, script) = setup();
        assert!(matches!(
            write_profile_config_with("scout", Some("  "), None, Some(script)),
            HermesProfileConfigResult::Rejected { .. }
        ));
    }

    #[test]
    fn unchanged_values_do_not_spawn() {
        let (_guard, home, profile, script) = setup();
        fs::write(
            profile.join("config.yaml"),
            "model:\n  provider: openai\n  default: old-model\n",
        )
        .unwrap();
        let result = write_profile_config_with("scout", Some("openai"), None, Some(script));
        assert!(matches!(result, HermesProfileConfigResult::Ok { .. }));
        assert!(!home.path().join("commands.log").exists());
    }

    #[test]
    fn only_changed_key_is_written() {
        let (_guard, home, profile, script) = setup();
        fs::write(
            profile.join("config.yaml"),
            "model:\n  provider: old-provider\n  default: old-model\n",
        )
        .unwrap();
        let _ = write_profile_config_with("scout", Some("new-provider"), None, Some(script));
        let log = fs::read_to_string(home.path().join("commands.log")).unwrap();
        assert_eq!(
            log.lines().collect::<Vec<_>>(),
            vec!["-p scout config set model.provider new-provider"]
        );
    }

    #[test]
    fn home_profile_can_be_read_but_not_written() {
        let (_guard, home, _profile, _script) = setup();
        fs::write(
            home.path().join("config.yaml"),
            "model:\n  provider: anthropic\n  default: claude-sonnet\n",
        )
        .unwrap();
        assert_eq!(
            read_profile_config("default"),
            HermesProfileConfigResult::Ok {
                name: "default".to_string(),
                provider: Some("anthropic".to_string()),
                model: Some("claude-sonnet".to_string()),
            }
        );
        assert!(matches!(
            write_profile_config("default", None, None),
            HermesProfileConfigResult::InvalidName { .. }
        ));
    }

    #[test]
    fn invalid_profile_name_is_rejected() {
        assert!(matches!(
            read_profile_config("Bad Name"),
            HermesProfileConfigResult::InvalidName { .. }
        ));
    }

    #[allow(dead_code)]
    fn _assert_path(_: &Path) {}
}
