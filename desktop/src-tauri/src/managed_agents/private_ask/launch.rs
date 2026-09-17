//! The fixed native command recipe for one admitted private Ask attempt.
//!
//! Every argument, environment variable and state directory is derived from the
//! admission and the owned disposable run root. Nothing is inherited from the
//! desktop process: the child has no relay key, no provider key, no employee
//! session and no configured MCP server.

use super::validation::valid_profile;
use super::{PrivateAskAdmission, PrivateAskFailure};
use crate::managed_agents::recap_state::{private_read_file, OwnedRecapRun};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

const USAGE_FILE_LIMIT: u64 = 64 * 1024;

/// A fixed native command recipe bound to one admitted request.
#[derive(Debug)]
pub(super) struct PrivateAskLaunchPlan {
    pub(super) runtime_id: String,
    pub(super) executable: PathBuf,
    pub(super) args: Vec<OsString>,
    pub(super) env: BTreeMap<OsString, OsString>,
    pub(super) cwd: PathBuf,
    pub(super) model: String,
    pub(super) usage_file: Option<PathBuf>,
    pub(super) prompt_on_stdin: bool,
    /// Seatbelt policy text applied by `/usr/bin/sandbox-exec`. It is resolved
    /// once, at plan time, so a platform without an effect-denying boundary
    /// fails admission instead of launching unconstrained.
    pub(super) containment_profile: String,
}

impl PrivateAskLaunchPlan {
    pub(super) fn for_admission(
        admission: &PrivateAskAdmission,
        run: &OwnedRecapRun,
    ) -> Result<Self, PrivateAskFailure> {
        let root = run.path();
        if !root.is_absolute()
            || !admission.capability.executable.resolved_path.is_absolute()
            || !crate::managed_agents::recap_capability::same_executable_proof(
                &admission.capability.executable,
                &admission.state.executable,
            )
        {
            return Err(PrivateAskFailure::InvalidState);
        }
        let containment_profile = super::containment::private_ask_containment_profile(root)?;
        prepare_state_dirs(root)?;
        let mut env = isolated_env(root, &admission.capability.executable.resolved_path);
        let (args, usage_file, prompt_on_stdin) = match admission.state.runtime_id.as_str() {
            "claude" => {
                env.insert(
                    OsString::from("CLAUDE_CONFIG_DIR"),
                    root.join("config").into_os_string(),
                );
                env.insert(OsString::from("CLAUDE_CODE_SAFE_MODE"), OsString::from("1"));
                env.insert(
                    OsString::from("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"),
                    OsString::from("1"),
                );
                (
                    fixed_claude_args(&admission.state.effective_model),
                    None,
                    true,
                )
            }
            "hermes" => {
                let profile = admission
                    .state
                    .profile
                    .as_deref()
                    .ok_or(PrivateAskFailure::MissingProfile)?;
                if !valid_profile(profile)
                    || profile == crate::managed_agents::hermes_profile::HERMES_HOME_PROFILE_NAME
                {
                    return Err(PrivateAskFailure::MissingProfile);
                }
                env.insert(
                    OsString::from("HERMES_HOME"),
                    root.join("hermes").into_os_string(),
                );
                env.insert(
                    OsString::from("HERMES_ACP_SKIP_CONFIGURED_MCP"),
                    OsString::from("1"),
                );
                // Hermes writes its managed directory before the profile is
                // read. Without an explicit private location it defaults to one
                // outside the run root, which the containment policy refuses —
                // a confusing write failure rather than an honest isolation.
                env.insert(
                    OsString::from("HERMES_MANAGED_DIR"),
                    root.join("managed").into_os_string(),
                );
                // Hermes checks this guard before discovering plugins, loading
                // configured MCP servers or registering user hooks; the
                // `--safe-mode` flag alone is applied later.
                env.insert(OsString::from("HERMES_SAFE_MODE"), OsString::from("1"));
                let usage = root.join("usage.json");
                prepare_usage_file(&usage)?;
                (fixed_hermes_args(profile, root, &usage), Some(usage), false)
            }
            _ => return Err(PrivateAskFailure::MissingRuntime),
        };
        Ok(Self {
            runtime_id: admission.state.runtime_id.clone(),
            executable: admission.capability.executable.resolved_path.clone(),
            args,
            env,
            cwd: root.to_owned(),
            model: admission.state.effective_model.clone(),
            usage_file,
            prompt_on_stdin,
            containment_profile,
        })
    }

    /// Build the command actually spawned. The runtime is always wrapped by the
    /// effect-denying Seatbelt policy; removing that wrapper lets a hostile
    /// tool call write outside the run root, reach the network and fork.
    pub(super) fn command(&self) -> Command {
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command.args([
            OsString::from("-p"),
            OsString::from(&self.containment_profile),
        ]);
        command.arg(&self.executable);
        command
            .args(&self.args)
            .env_clear()
            .envs(&self.env)
            .current_dir(&self.cwd);
        command
    }

    pub(super) fn parse_output(
        &self,
        exit_success: bool,
        stdout: &[u8],
        stderr: &[u8],
    ) -> Result<String, PrivateAskFailure> {
        if stdout.len() as u64 > super::PRIVATE_ASK_OUTPUT_LIMIT
            || stderr.len() as u64 > super::PRIVATE_ASK_STDERR_LIMIT
        {
            return Err(PrivateAskFailure::InvalidOutput);
        }
        if !exit_success {
            return Err(PrivateAskFailure::NonzeroExit);
        }
        match self.runtime_id.as_str() {
            "claude" => parse_claude_output(&self.model, stdout),
            "hermes" => {
                let text = String::from_utf8(stdout.to_vec())
                    .map_err(|_| PrivateAskFailure::InvalidOutput)?;
                if text.trim().is_empty() || text.contains('\0') {
                    return Err(PrivateAskFailure::InvalidOutput);
                }
                let usage = self
                    .usage_file
                    .as_deref()
                    .ok_or(PrivateAskFailure::ModelMismatch)?;
                if read_usage_model(usage)?.as_deref() != Some(self.model.as_str()) {
                    return Err(PrivateAskFailure::ModelMismatch);
                }
                Ok(text)
            }
            _ => Err(PrivateAskFailure::MissingRuntime),
        }
    }
}

fn fixed_claude_args(model: &str) -> Vec<OsString> {
    [
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
    .into_iter()
    .map(OsString::from)
    .collect()
}

fn fixed_hermes_args(profile: &str, root: &Path, usage: &Path) -> Vec<OsString> {
    [
        OsString::from("-p"),
        OsString::from(profile),
        OsString::from("--ignore-user-config"),
        OsString::from("--ignore-rules"),
        OsString::from("--safe-mode"),
        OsString::from("--no-restore-cwd"),
        OsString::from("--toolsets"),
        // This is a proof input, not a certification.  Admission still
        // requires a retained hostile-tool experiment for this exact binary.
        OsString::from("context_engine"),
        OsString::from("--in"),
        root.as_os_str().to_owned(),
        OsString::from("--usage-file"),
        usage.as_os_str().to_owned(),
        OsString::from("--oneshot"),
    ]
    .into_iter()
    .collect()
}

pub(super) fn isolated_env(root: &Path, executable: &Path) -> BTreeMap<OsString, OsString> {
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
        OsString::from("PATH"),
        format!(
            "{}:/usr/bin:/bin",
            executable
                .parent()
                .unwrap_or_else(|| Path::new("/usr/bin"))
                .display()
        )
        .into(),
    );
    env
}

fn prepare_state_dirs(root: &Path) -> Result<(), PrivateAskFailure> {
    for name in [
        "home", "tmp", "config", "cache", "data", "state", "hermes", "managed",
    ] {
        let path = root.join(name);
        create_private_dir(&path)?;
        crate::managed_agents::recap_state::directory_identity(&path)
            .map_err(PrivateAskFailure::State)?;
    }
    Ok(())
}

fn create_private_dir(path: &Path) -> Result<(), PrivateAskFailure> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(PrivateAskFailure::InvalidState);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = std::fs::DirBuilder::new();
                builder.mode(0o700);
                builder
                    .create(path)
                    .map_err(|_| PrivateAskFailure::InvalidState)?;
            }
            #[cfg(not(unix))]
            {
                std::fs::create_dir(path).map_err(|_| PrivateAskFailure::InvalidState)?;
            }
        }
        Err(_) => return Err(PrivateAskFailure::InvalidState),
    }
    Ok(())
}

fn prepare_usage_file(path: &Path) -> Result<(), PrivateAskFailure> {
    use std::fs::OpenOptions;
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
        .map_err(|_| PrivateAskFailure::InvalidState)
}

fn read_usage_model(path: &Path) -> Result<Option<String>, PrivateAskFailure> {
    let file = private_read_file(path).map_err(PrivateAskFailure::State)?;
    let metadata = file
        .metadata()
        .map_err(|_| PrivateAskFailure::ModelMismatch)?;
    if metadata.len() > USAGE_FILE_LIMIT {
        return Err(PrivateAskFailure::ModelMismatch);
    }
    let mut bytes = Vec::new();
    file.take(USAGE_FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PrivateAskFailure::ModelMismatch)?;
    if bytes.len() as u64 > USAGE_FILE_LIMIT {
        return Err(PrivateAskFailure::ModelMismatch);
    }
    let parsed: HermesUsage =
        serde_json::from_slice(&bytes).map_err(|_| PrivateAskFailure::ModelMismatch)?;
    let model = parsed.model.trim();
    if model.is_empty() || model != parsed.model || model.chars().any(char::is_control) {
        return Err(PrivateAskFailure::ModelMismatch);
    }
    Ok(Some(model.to_owned()))
}

fn parse_claude_output(model: &str, stdout: &[u8]) -> Result<String, PrivateAskFailure> {
    let parsed: ClaudeResult =
        serde_json::from_slice(stdout).map_err(|_| PrivateAskFailure::InvalidOutput)?;
    if parsed.kind != "result"
        || parsed.subtype != "success"
        || parsed.is_error
        || parsed.result.trim().is_empty()
        || parsed.model_usage.len() != 1
        || !parsed.model_usage.contains_key(model)
    {
        return Err(
            if parsed.model_usage.len() != 1 || !parsed.model_usage.contains_key(model) {
                PrivateAskFailure::ModelMismatch
            } else {
                PrivateAskFailure::InvalidOutput
            },
        );
    }
    Ok(parsed.result)
}

#[derive(serde::Deserialize)]
struct ClaudeResult {
    #[serde(rename = "type")]
    kind: String,
    subtype: String,
    is_error: bool,
    result: String,
    #[serde(rename = "modelUsage", default)]
    model_usage: BTreeMap<String, serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct HermesUsage {
    model: String,
}
