//! Native invocation candidate for Claude Code 2.1.266.
//!
//! Preparing or testing this recipe does not certify its process containment,
//! auth, or effective model. Admission remains in `recap_capability`.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

/// Maximum UTF-8 input bytes accepted by the recap executor.
pub(crate) const RECAP_INPUT_LIMIT: usize = 128 * 1024;
/// Maximum captured bytes in each recap output stream.
pub(crate) const RECAP_OUTPUT_LIMIT: usize = 256 * 1024;

/// Failures expose fixed codes, never a provider's potentially secret stderr.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecapRunFailure {
    InvalidSelection,
    InputLimit,
    OutputLimit,
    NonzeroExit,
    InvalidOutput,
    ModelRequestedOnly,
    StateIsolation,
}

/// Immutable native command plan. Input is provided separately through a file.
#[derive(Debug)]
pub(crate) struct RecapLaunchPlan {
    executable: PathBuf,
    args: Vec<OsString>,
    env: BTreeMap<OsString, OsString>,
    cwd: PathBuf,
    requested_model: String,
}

impl RecapLaunchPlan {
    /// Construct the actual child command with no inherited environment.
    pub(crate) fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .args(&self.args)
            .env_clear()
            .envs(&self.env)
            .current_dir(&self.cwd);
        command
    }

    /// Return the exact executable path captured in this immutable plan.
    pub(crate) fn executable_path(&self) -> &Path {
        &self.executable
    }

    /// Ensure a plan still names the exact admitted executable and model.
    /// Plans are built by the native service; this check keeps a future caller
    /// from pairing a valid proof with a different command recipe.
    pub(crate) fn matches_admission(
        &self,
        admission: &super::recap_capability::RecapAdmission,
    ) -> bool {
        self.executable == admission.executable.resolved_path
            && self.requested_model == admission.selection.model
    }

    /// Validate a native final result without retaining raw output on error.
    pub(crate) fn parse_output(
        &self,
        exit_success: bool,
        stdout: &[u8],
        stderr: &[u8],
    ) -> Result<String, RecapRunFailure> {
        if stdout.len() > RECAP_OUTPUT_LIMIT || stderr.len() > RECAP_OUTPUT_LIMIT {
            return Err(RecapRunFailure::OutputLimit);
        }
        if !exit_success {
            return Err(RecapRunFailure::NonzeroExit);
        }
        let result: ClaudeResult =
            serde_json::from_slice(stdout).map_err(|_| RecapRunFailure::InvalidOutput)?;
        if result.kind != "result"
            || result.subtype != "success"
            || result.is_error
            || result.result.trim().is_empty()
        {
            return Err(RecapRunFailure::InvalidOutput);
        }
        if result.model_usage.len() != 1 || !result.model_usage.contains_key(&self.requested_model)
        {
            return Err(RecapRunFailure::ModelRequestedOnly);
        }
        Ok(result.result)
    }
}

/// Prepare the fixed native no-tools candidate; never accept arbitrary argv.
pub(crate) fn claude_recap_plan(
    executable: &Path,
    root: &Path,
    model: &str,
    input: &[u8],
) -> Result<RecapLaunchPlan, RecapRunFailure> {
    if input.len() > RECAP_INPUT_LIMIT {
        return Err(RecapRunFailure::InputLimit);
    }
    if !super::recap_capability::valid_recap_model(model) {
        return Err(RecapRunFailure::InvalidSelection);
    }
    if !root.is_absolute() || !executable.is_absolute() {
        return Err(RecapRunFailure::StateIsolation);
    }
    let args = [
        "--print",
        "--safe-mode",
        "--tools",
        "",
        "--strict-mcp-config",
        "--mcp-config",
        r#"{"mcpServers":{}}"#,
        "--setting-sources",
        "",
        "--disable-slash-commands",
        "--no-session-persistence",
        "--output-format",
        "json",
        "--max-turns",
        "1",
        "--model",
        model,
    ]
    .map(OsString::from)
    .to_vec();
    let mut env = BTreeMap::new();
    for (name, directory) in [
        ("HOME", "home"),
        ("CLAUDE_CONFIG_DIR", "config"),
        ("TMPDIR", "tmp"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
    ] {
        env.insert(name.into(), root.join(directory).into_os_string());
    }
    for (name, value) in [
        ("PATH", "/usr/bin:/bin"),
        ("CLAUDE_CODE_SAFE_MODE", "1"),
        ("DISABLE_AUTOUPDATER", "1"),
        ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
    ] {
        env.insert(name.into(), value.into());
    }
    Ok(RecapLaunchPlan {
        executable: executable.to_owned(),
        args,
        env,
        cwd: root.to_owned(),
        requested_model: model.to_owned(),
    })
}

#[derive(Deserialize)]
struct ClaudeResult {
    #[serde(rename = "type")]
    kind: String,
    subtype: String,
    is_error: bool,
    result: String,
    #[serde(rename = "modelUsage", default)]
    model_usage: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
#[path = "recap_adapter/tests.rs"]
mod tests;
