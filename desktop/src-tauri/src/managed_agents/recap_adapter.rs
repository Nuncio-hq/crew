//! Native one-shot adapters for owner-local Thread Recap.
//!
//! The adapter is deliberately closed over two recipes. Claude receives an
//! explicit model and a JSON result. Hermes receives a named, copied staging
//! profile; its model remains profile-owned and is checked through the bounded
//! usage report written below the disposable run root. No caller-authored argv
//! or environment is accepted.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

/// Maximum UTF-8 input bytes accepted by the recap executor.
pub(crate) const RECAP_INPUT_LIMIT: usize = 128 * 1024;
/// Maximum captured bytes in each recap output stream.
pub(crate) const RECAP_OUTPUT_LIMIT: usize = 256 * 1024;

const HERMES_PROFILE_FILE_LIMIT: usize = 1024;
const HERMES_PROFILE_ENTRY_LIMIT: usize = 4096;
const HERMES_PROFILE_DEPTH_LIMIT: usize = 32;
const HERMES_PROFILE_BYTES_LIMIT: u64 = 32 * 1024 * 1024;
const HERMES_USAGE_LIMIT: u64 = 64 * 1024;

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
    ProfileUnavailable,
    ProfileCopyLimit,
    UnsupportedContainment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecapLaunchKind {
    Claude,
    Hermes,
}

impl RecapLaunchKind {
    fn runtime_id(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Hermes => "hermes",
        }
    }
}

/// Immutable native command plan. Input is provided separately through a file
/// and copied profile state is always below `cwd`.
#[derive(Debug)]
pub(crate) struct RecapLaunchPlan {
    kind: RecapLaunchKind,
    executable: PathBuf,
    args: Vec<OsString>,
    env: BTreeMap<OsString, OsString>,
    cwd: PathBuf,
    requested_model: String,
    profile_source: Option<PathBuf>,
    usage_file: Option<PathBuf>,
    /// On macOS this is a fixed process-fork denial policy. The policy is
    /// passed to `/usr/bin/sandbox-exec`; the installed runtime and all of its
    /// state paths remain otherwise unchanged. Other supported platforms use
    /// the existing Job Object/process-group owner.
    sandbox_profile: Option<String>,
}

impl RecapLaunchPlan {
    /// Construct the actual child command with no inherited environment.
    pub(crate) fn command(&self) -> Command {
        let mut command = if let Some(profile) = &self.sandbox_profile {
            let mut command = Command::new("/usr/bin/sandbox-exec");
            command.args([OsString::from("-p"), OsString::from(profile)]);
            command.arg(&self.executable);
            command
        } else {
            Command::new(&self.executable)
        };
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

    /// Ensure a plan still names the exact admitted executable, runtime,
    /// model and profile. This check is repeated at the production seam.
    pub(crate) fn matches_admission(
        &self,
        admission: &super::recap_capability::RecapAdmission,
    ) -> bool {
        self.kind.runtime_id() == admission.runtime_id
            && self.executable == admission.executable.resolved_path
            && self.requested_model == admission.selection.model
            && self.profile_source == admission.selection.profile
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
        match self.kind {
            RecapLaunchKind::Claude => parse_claude_output(&self.requested_model, stdout),
            RecapLaunchKind::Hermes => {
                let result = String::from_utf8(stdout.to_vec())
                    .map_err(|_| RecapRunFailure::InvalidOutput)?;
                if result.trim().is_empty() || result.contains('\0') {
                    return Err(RecapRunFailure::InvalidOutput);
                }
                let usage = self
                    .usage_file
                    .as_deref()
                    .ok_or(RecapRunFailure::ModelRequestedOnly)?;
                let effective = read_usage_model(usage)?;
                if effective != self.requested_model {
                    return Err(RecapRunFailure::ModelRequestedOnly);
                }
                Ok(result)
            }
        }
    }
}

fn parse_claude_output(model: &str, stdout: &[u8]) -> Result<String, RecapRunFailure> {
    let result: ClaudeResult =
        serde_json::from_slice(stdout).map_err(|_| RecapRunFailure::InvalidOutput)?;
    if result.kind != "result"
        || result.subtype != "success"
        || result.is_error
        || result.result.trim().is_empty()
    {
        return Err(RecapRunFailure::InvalidOutput);
    }
    if result.model_usage.len() != 1 || !result.model_usage.contains_key(model) {
        return Err(RecapRunFailure::ModelRequestedOnly);
    }
    Ok(result.result)
}

/// Prepare the fixed native no-tools Claude candidate; never accept arbitrary
/// argv. This recipe is retained for the explicit-model adapter.
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
    let mut env = isolated_env(
        root,
        executable,
        [
            ("CLAUDE_CODE_SAFE_MODE", "1"),
            ("DISABLE_AUTOUPDATER", "1"),
            ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
        ],
    );
    env.insert(
        "CLAUDE_CONFIG_DIR".into(),
        root.join("config").into_os_string(),
    );
    Ok(RecapLaunchPlan {
        kind: RecapLaunchKind::Claude,
        executable: executable.to_owned(),
        args,
        env,
        cwd: root.to_owned(),
        requested_model: model.to_owned(),
        profile_source: None,
        usage_file: None,
        sandbox_profile: None,
    })
}

/// Prepare the Hermes profile-bound native no-tools candidate.
///
/// The selected profile is copied by value into the disposable run. Hermes'
/// `context_engine` toolset is intentionally empty, while `--ignore-rules`
/// prevents AGENTS/SOUL/memory injection. The usage report is required to
/// prove that the profile-owned model actually answered the request.
pub(crate) fn hermes_recap_plan(
    executable: &Path,
    root: &Path,
    model: &str,
    profile: &Path,
    input: &[u8],
) -> Result<RecapLaunchPlan, RecapRunFailure> {
    if input.len() > RECAP_INPUT_LIMIT {
        return Err(RecapRunFailure::InputLimit);
    }
    if !super::recap_capability::valid_recap_model(model)
        || !root.is_absolute()
        || !executable.is_absolute()
        || !profile.is_absolute()
    {
        return Err(RecapRunFailure::InvalidSelection);
    }
    let source = canonical_profile_source(profile)?;
    let profile_name = hermes_profile_ref(&source).ok_or(RecapRunFailure::InvalidSelection)?;
    let destination = if profile_name == super::hermes_profile::HERMES_HOME_PROFILE_NAME {
        root.join("hermes")
    } else {
        root.join("hermes").join("profiles").join(&profile_name)
    };
    let mut budget = ProfileCopyBudget::default();
    copy_profile_tree(&source, &destination, &mut budget, 0)?;

    let usage_file = root.join("usage.json");
    prepare_usage_file(&usage_file)?;
    let args = [
        "-p",
        profile_name.as_str(),
        "--ignore-rules",
        "--no-restore-cwd",
        "--toolsets",
        "context_engine",
        "--in",
        root.to_str().ok_or(RecapRunFailure::StateIsolation)?,
        "--usage-file",
        usage_file.to_str().ok_or(RecapRunFailure::StateIsolation)?,
        "--oneshot",
        // The prompt is deliberately the final value-taking argument; it is
        // bound by `bind_hermes_prompt` after the input limit is checked.
        "__RECAP_PROMPT_PLACEHOLDER__",
    ]
    .map(OsString::from)
    .to_vec();
    let mut env = isolated_env(root, executable, [("HERMES_ACP_SKIP_CONFIGURED_MCP", "1")]);
    // Hermes resolves `-p <name>` relative to the default root. Point that
    // root at the disposable copy before its pre-argparse profile selector
    // runs; the source profile is never mounted or mutated in place.
    env.insert("HERMES_HOME".into(), root.join("hermes").into_os_string());
    let sandbox_profile = macos_containment_profile(executable)?;
    Ok(RecapLaunchPlan {
        kind: RecapLaunchKind::Hermes,
        executable: executable.to_owned(),
        args,
        env,
        cwd: root.to_owned(),
        requested_model: model.to_owned(),
        profile_source: Some(profile.to_owned()),
        usage_file: Some(usage_file),
        sandbox_profile,
    })
}

/// Replace the fixed prompt slot in a Hermes plan after input limits have been
/// checked by the caller. This does not permit adding or reordering flags.
pub(crate) fn bind_hermes_prompt(
    mut plan: RecapLaunchPlan,
    input: &[u8],
) -> Result<RecapLaunchPlan, RecapRunFailure> {
    if plan.kind != RecapLaunchKind::Hermes {
        return Err(RecapRunFailure::InvalidSelection);
    }
    if input.len() > RECAP_INPUT_LIMIT {
        return Err(RecapRunFailure::InputLimit);
    }
    let prompt =
        String::from_utf8(input.to_vec()).map_err(|_| RecapRunFailure::InvalidSelection)?;
    if prompt.contains('\0') {
        return Err(RecapRunFailure::InvalidSelection);
    }
    let Some(slot) = plan.args.last_mut() else {
        return Err(RecapRunFailure::InvalidSelection);
    };
    if slot != "__RECAP_PROMPT_PLACEHOLDER__" {
        return Err(RecapRunFailure::InvalidSelection);
    }
    *slot = OsString::from(prompt);
    Ok(plan)
}

fn isolated_env<const N: usize>(
    root: &Path,
    executable: &Path,
    extra: [(&str, &str); N],
) -> BTreeMap<OsString, OsString> {
    let mut env = BTreeMap::new();
    for (name, directory) in [
        ("HOME", "home"),
        ("TMPDIR", "tmp"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
    ] {
        env.insert(name.into(), root.join(directory).into_os_string());
    }
    env.insert(
        "PATH".into(),
        format!(
            "{}:/usr/bin:/bin",
            executable
                .parent()
                .unwrap_or_else(|| Path::new("/usr/bin"))
                .display()
        )
        .into(),
    );
    for (name, value) in extra {
        env.insert(name.into(), value.into());
    }
    env
}

fn macos_containment_profile(executable: &Path) -> Result<Option<String>, RecapRunFailure> {
    #[cfg(target_os = "macos")]
    {
        if !is_executable(Path::new("/usr/bin/sandbox-exec")) {
            return Err(RecapRunFailure::UnsupportedContainment);
        }
        // `allow default` preserves the runtime's in-process provider HTTP
        // path and dynamic libraries. The explicit process-fork denial is the
        // containment boundary: MCP/terminal/plugin descendants cannot fork,
        // call setsid, or escape the bounded owner. The runtime has no tools in
        // this recipe, so file access outside its isolated HOME is unreachable
        // from model output.
        let _ = executable;
        Ok(Some(
            "(version 1)(allow default)(deny process-fork)".to_string(),
        ))
    }
    #[cfg(target_os = "windows")]
    {
        let _ = executable;
        Ok(None)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = executable;
        Err(RecapRunFailure::UnsupportedContainment)
    }
}

#[derive(Default)]
struct ProfileCopyBudget {
    entries: usize,
    files: usize,
    bytes: u64,
}

fn canonical_profile_source(source: &Path) -> Result<PathBuf, RecapRunFailure> {
    let metadata =
        std::fs::symlink_metadata(source).map_err(|_| RecapRunFailure::ProfileUnavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RecapRunFailure::ProfileUnavailable);
    }
    let canonical = source
        .canonicalize()
        .map_err(|_| RecapRunFailure::ProfileUnavailable)?;
    let canonical_metadata =
        std::fs::symlink_metadata(&canonical).map_err(|_| RecapRunFailure::ProfileUnavailable)?;
    if canonical_metadata.file_type().is_symlink() || !canonical_metadata.is_dir() {
        return Err(RecapRunFailure::ProfileUnavailable);
    }
    Ok(canonical)
}

/// Derive the stable profile selection name from a native grant path. Hermes'
/// home profile is represented by the `.hermes` directory itself; named
/// profiles must be the final directory component directly under `profiles/`.
pub(crate) fn hermes_profile_ref(source: &Path) -> Option<String> {
    let name = source.file_name()?.to_str()?;
    if name == ".hermes" {
        return Some(super::hermes_profile::HERMES_HOME_PROFILE_NAME.to_string());
    }
    if name == super::hermes_profile::HERMES_HOME_PROFILE_NAME
        || source.parent()?.file_name()?.to_str()? != "profiles"
    {
        return None;
    }
    super::hermes_profile::validate_hermes_profile_name(name).ok()?;
    Some(name.to_string())
}

fn copy_profile_tree(
    source: &Path,
    destination: &Path,
    budget: &mut ProfileCopyBudget,
    depth: usize,
) -> Result<(), RecapRunFailure> {
    if depth > HERMES_PROFILE_DEPTH_LIMIT {
        return Err(RecapRunFailure::ProfileCopyLimit);
    }
    budget.entries = budget
        .entries
        .checked_add(1)
        .ok_or(RecapRunFailure::ProfileCopyLimit)?;
    if budget.entries > HERMES_PROFILE_ENTRY_LIMIT {
        return Err(RecapRunFailure::ProfileCopyLimit);
    }
    let metadata =
        std::fs::symlink_metadata(source).map_err(|_| RecapRunFailure::ProfileUnavailable)?;
    if metadata.file_type().is_symlink() {
        return Err(RecapRunFailure::ProfileUnavailable);
    }
    if metadata.is_dir() {
        std::fs::create_dir_all(destination).map_err(|_| RecapRunFailure::ProfileUnavailable)?;
        let mut entries = std::fs::read_dir(source)
            .map_err(|_| RecapRunFailure::ProfileUnavailable)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| RecapRunFailure::ProfileUnavailable)?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            copy_profile_tree(
                &entry.path(),
                &destination.join(entry.file_name()),
                budget,
                depth + 1,
            )?;
        }
        return Ok(());
    }
    if !metadata.is_file() {
        return Err(RecapRunFailure::ProfileUnavailable);
    }
    budget.files = budget
        .files
        .checked_add(1)
        .ok_or(RecapRunFailure::ProfileCopyLimit)?;
    budget.bytes = budget
        .bytes
        .checked_add(metadata.len())
        .ok_or(RecapRunFailure::ProfileCopyLimit)?;
    if budget.files > HERMES_PROFILE_FILE_LIMIT || budget.bytes > HERMES_PROFILE_BYTES_LIMIT {
        return Err(RecapRunFailure::ProfileCopyLimit);
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|_| RecapRunFailure::ProfileUnavailable)?;
    }
    std::fs::copy(source, destination).map_err(|_| RecapRunFailure::ProfileUnavailable)?;
    Ok(())
}

fn prepare_usage_file(path: &Path) -> Result<(), RecapRunFailure> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map(|_| ())
        .map_err(|_| RecapRunFailure::StateIsolation)
}

fn read_usage_model(path: &Path) -> Result<String, RecapRunFailure> {
    let mut file = super::recap_state::private_read_file(path)
        .map_err(|_| RecapRunFailure::ModelRequestedOnly)?;
    let initial = file
        .metadata()
        .map_err(|_| RecapRunFailure::ModelRequestedOnly)?;
    if initial.len() > HERMES_USAGE_LIMIT {
        return Err(RecapRunFailure::ModelRequestedOnly);
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(HERMES_USAGE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| RecapRunFailure::ModelRequestedOnly)?;
    let final_metadata = file
        .metadata()
        .map_err(|_| RecapRunFailure::ModelRequestedOnly)?;
    if final_metadata.len() != initial.len()
        || bytes.len() as u64 != initial.len()
        || bytes.len() as u64 > HERMES_USAGE_LIMIT
    {
        return Err(RecapRunFailure::ModelRequestedOnly);
    }
    let usage: HermesUsage =
        serde_json::from_slice(&bytes).map_err(|_| RecapRunFailure::ModelRequestedOnly)?;
    let model = usage.model.trim();
    if model.is_empty() || model != usage.model || model.chars().any(char::is_control) {
        return Err(RecapRunFailure::ModelRequestedOnly);
    }
    Ok(model.to_owned())
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[derive(Deserialize)]
struct HermesUsage {
    model: String,
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
