//! Validation of staged Hermes configuration and native one-shot usage reports.

use super::hermes_profile::is_hermes_home_profile;
use super::wiki_runtime::WikiRuntimeFailure;
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};

const REPORT_BYTES_LIMIT: u64 = 16 * 1024;
const PROFILE_CONFIG_BYTES_LIMIT: u64 = 1024 * 1024;

#[derive(Deserialize)]
struct HermesUsageReport {
    model: Option<String>,
    provider: Option<String>,
    api_calls: u64,
    completed: bool,
    failed: bool,
}

pub(super) fn report_path(state_dir: &Path) -> PathBuf {
    state_dir.join("hermes-usage.json")
}

pub(super) fn clear_report(state_dir: &Path) -> Result<(), WikiRuntimeFailure> {
    match std::fs::remove_file(report_path(state_dir)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(WikiRuntimeFailure::InvalidRuntimeTelemetry),
    }
}

/// Validate the copied Hermes config before an installed provider process can start.
///
/// Hermes treats a missing or empty config as a valid first-run state, while a
/// non-mapping root or malformed YAML is not a usable user config for a
/// non-interactive launch. Keep this check bounded and return a fixed failure so
/// config contents never cross the runtime error boundary.
pub(super) fn validate_profile_config(profile_dir: &Path) -> Result<(), WikiRuntimeFailure> {
    let config = profile_dir.join("config.yaml");
    let metadata = match std::fs::symlink_metadata(&config) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(WikiRuntimeFailure::InvalidProfileConfig),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(WikiRuntimeFailure::InvalidProfileConfig);
    }
    if metadata.len() > PROFILE_CONFIG_BYTES_LIMIT {
        return Err(WikiRuntimeFailure::ProfileConfigLimit);
    }
    let file =
        std::fs::File::open(&config).map_err(|_| WikiRuntimeFailure::InvalidProfileConfig)?;
    let mut contents = Vec::new();
    file.take(PROFILE_CONFIG_BYTES_LIMIT + 1)
        .read_to_end(&mut contents)
        .map_err(|_| WikiRuntimeFailure::InvalidProfileConfig)?;
    if contents.len() as u64 > PROFILE_CONFIG_BYTES_LIMIT {
        return Err(WikiRuntimeFailure::ProfileConfigLimit);
    }
    let contents =
        String::from_utf8(contents).map_err(|_| WikiRuntimeFailure::InvalidProfileConfig)?;
    let value = serde_yaml::from_str::<serde_yaml::Value>(&contents)
        .map_err(|_| WikiRuntimeFailure::InvalidProfileConfig)?;
    if !matches!(
        value,
        serde_yaml::Value::Null | serde_yaml::Value::Mapping(_)
    ) {
        return Err(WikiRuntimeFailure::InvalidProfileConfig);
    }
    reject_enabled_secret_sources(&value)
}

pub(super) fn reject_enabled_secret_sources(
    config: &serde_yaml::Value,
) -> Result<(), WikiRuntimeFailure> {
    if yaml_contains_merge_key(config) {
        return Err(WikiRuntimeFailure::ProfileBinding);
    }
    let serde_yaml::Value::Mapping(config) = config else {
        return Ok(());
    };
    let Some(secrets) = config.get(serde_yaml::Value::String("secrets".into())) else {
        return Ok(());
    };
    let serde_yaml::Value::Mapping(secrets) = secrets else {
        return match secrets {
            serde_yaml::Value::Null => Ok(()),
            _ => Err(WikiRuntimeFailure::ProfileBinding),
        };
    };
    if secrets.values().any(|source| {
        let serde_yaml::Value::Mapping(source) = source else {
            return false;
        };
        match source.get(serde_yaml::Value::String("enabled".into())) {
            None | Some(serde_yaml::Value::Null) | Some(serde_yaml::Value::Bool(false)) => false,
            Some(_) => true,
        }
    }) {
        return Err(WikiRuntimeFailure::ProfileBinding);
    }
    Ok(())
}

fn yaml_contains_merge_key(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Mapping(mapping) => mapping.iter().any(|(key, value)| {
            matches!(key, serde_yaml::Value::String(key) if key == "<<")
                || yaml_contains_merge_key(key)
                || yaml_contains_merge_key(value)
        }),
        serde_yaml::Value::Sequence(sequence) => sequence.iter().any(yaml_contains_merge_key),
        serde_yaml::Value::Tagged(_) => true,
        _ => false,
    }
}

pub(super) fn validate_report(state_dir: &Path, profile: &str) -> Result<(), WikiRuntimeFailure> {
    let path = report_path(state_dir);
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|_| WikiRuntimeFailure::InvalidRuntimeTelemetry)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > REPORT_BYTES_LIMIT
    {
        return Err(WikiRuntimeFailure::InvalidRuntimeTelemetry);
    }
    let file =
        std::fs::File::open(path).map_err(|_| WikiRuntimeFailure::InvalidRuntimeTelemetry)?;
    let mut bytes = Vec::new();
    file.take(REPORT_BYTES_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| WikiRuntimeFailure::InvalidRuntimeTelemetry)?;
    if bytes.len() as u64 > REPORT_BYTES_LIMIT {
        return Err(WikiRuntimeFailure::InvalidRuntimeTelemetry);
    }
    let report: HermesUsageReport =
        serde_json::from_slice(&bytes).map_err(|_| WikiRuntimeFailure::InvalidRuntimeTelemetry)?;
    let provider = valid_report_value(report.provider.as_deref())?;
    let model = valid_report_value(report.model.as_deref())?;
    if report.api_calls == 0 || !report.completed || report.failed {
        return Err(WikiRuntimeFailure::InvalidRuntimeTelemetry);
    }
    if let Some((expected_provider, expected_model)) = staged_provider_model(state_dir, profile)? {
        if provider != expected_provider || model != expected_model {
            return Err(WikiRuntimeFailure::EffectiveRuntimeMismatch);
        }
    }
    tracing::info!(
        runtime = "hermes",
        profile,
        provider,
        model,
        api_calls = report.api_calls,
        "Wiki runtime generation telemetry verified"
    );
    Ok(())
}

fn staged_provider_model(
    state_dir: &Path,
    profile: &str,
) -> Result<Option<(String, String)>, WikiRuntimeFailure> {
    let profile_dir = if is_hermes_home_profile(profile) {
        state_dir.join("hermes")
    } else {
        state_dir.join("hermes").join("profiles").join(profile)
    };
    let contents = match std::fs::read_to_string(profile_dir.join("config.yaml")) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(WikiRuntimeFailure::InvalidProfileConfig),
    };
    if contents.trim().is_empty() {
        return Ok(None);
    }
    let value = serde_yaml::from_str::<serde_yaml::Value>(&contents)
        .map_err(|_| WikiRuntimeFailure::InvalidProfileConfig)?;
    let Some(model_config) = value.get("model") else {
        return Ok(None);
    };
    let Some(provider) = model_config
        .get("provider")
        .and_then(serde_yaml::Value::as_str)
    else {
        return Ok(None);
    };
    let Some(model) = model_config
        .get("default")
        .and_then(serde_yaml::Value::as_str)
    else {
        return Ok(None);
    };
    Ok(Some((
        valid_report_value(Some(provider))?.to_owned(),
        valid_report_value(Some(model))?.to_owned(),
    )))
}

fn valid_report_value(value: Option<&str>) -> Result<&str, WikiRuntimeFailure> {
    let Some(value) = value else {
        return Err(WikiRuntimeFailure::InvalidRuntimeTelemetry);
    };
    if value.is_empty() || value != value.trim() || value.chars().any(char::is_control) {
        return Err(WikiRuntimeFailure::InvalidRuntimeTelemetry);
    }
    Ok(value)
}
