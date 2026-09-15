//! Bounded, temporary installed-runtime adapter for Crew Wiki generation.
//!
//! Wiki generation deliberately does not reuse an employee ACP session.  The
//! adapter resolves a known native runtime from the existing catalog, starts a
//! fresh one-shot process in disposable state, and implements the existing
//! `crew-wiki::Generator` seam.  A missing or unrecognised selection is an
//! error; there is no heuristic or HTTP fallback once this adapter is chosen.

use super::discovery::bounded_command::{
    output_with_policy, output_with_policy_and_stdin, BoundedFailure, BoundedPolicy, OutputBudget,
};
use super::recap_adapter::native_containment_profile;
use super::wiki_runtime_output::normalize_generated_page;
#[cfg(test)]
use super::wiki_runtime_output::validate_generated_links;
use super::wiki_runtime_prompt::build_prompt;
use super::wiki_runtime_validation::{
    clear_report, report_path, validate_profile_config, validate_report,
};
use super::{
    hermes_profile::{is_hermes_home_profile, validate_hermes_profile_name},
    hermes_profile_lifecycle::hermes_profile_dir,
    known_acp_runtime_exact, resolve_command,
};
use crew_wiki::{generate::Generator, git_snapshot::RepoSnapshot, types::PlannedPage, WikiError};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Maximum complete prompt bytes supplied to one runtime page request.
pub(crate) const WIKI_RUNTIME_INPUT_LIMIT: usize = 128 * 1024;
/// Maximum complete response bytes retained from one runtime page request.
pub(crate) const WIKI_RUNTIME_OUTPUT_LIMIT: u64 = 2 * 1024 * 1024;
/// Maximum diagnostics retained from one runtime page request.
pub(crate) const WIKI_RUNTIME_STDERR_LIMIT: u64 = 256 * 1024;
/// Maximum wall-clock time for one page request.
pub(crate) const WIKI_RUNTIME_TIMEOUT: Duration = Duration::from_secs(180);
const HERMES_PROFILE_FILE_LIMIT: usize = 1024;
const HERMES_PROFILE_ENTRY_LIMIT: usize = 4096;
const HERMES_PROFILE_DEPTH_LIMIT: usize = 32;
const HERMES_PROFILE_BYTES_LIMIT: u64 = 32 * 1024 * 1024;
const HERMES_PROFILE_DOTENV_BYTES_LIMIT: u64 = 1024 * 1024;
const HERMES_PROFILE_BINDING_KEYS: [&str; 2] = ["HERMES_HOME", "HERMES_MANAGED_DIR"];

/// User-owned Wiki runtime selection.  This is separate from employee agent
/// settings; the selection names a runtime, and never an employee or session.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WikiRuntimeSelection {
    /// Canonical `KnownAcpRuntime::id` (`hermes`, `claude`, or `codex`).
    pub runtime_id: String,
    /// Optional explicit model override for runtimes whose model is not
    /// profile-owned; `None` requests the runtime default.
    #[serde(default)]
    pub model: Option<String>,
    /// Named profile for the profile-owned Hermes runtime.
    #[serde(default)]
    pub profile: Option<String>,
}

impl WikiRuntimeSelection {
    /// Canonicalize the optional model override before it is persisted or
    /// handed to a runtime. Empty UI input is the absence of an override;
    /// non-empty values still pass the closed validation below.
    pub(crate) fn normalized(mut self) -> Result<Self, WikiRuntimeFailure> {
        if self
            .model
            .as_deref()
            .is_some_and(|model| model.trim().is_empty())
        {
            self.model = None;
        }
        self.validate()?;
        Ok(self)
    }

    /// Validate the closed runtime/model/profile vocabulary before resolution.
    pub(crate) fn validate(&self) -> Result<(), WikiRuntimeFailure> {
        let runtime_id = self.runtime_id.trim();
        if runtime_id.is_empty()
            || runtime_id != self.runtime_id
            || self.runtime_id.chars().any(char::is_control)
        {
            return Err(WikiRuntimeFailure::UnsupportedRuntime(
                self.runtime_id.clone(),
            ));
        }
        let runtime = known_acp_runtime_exact(runtime_id)
            .ok_or_else(|| WikiRuntimeFailure::UnsupportedRuntime(runtime_id.to_owned()))?;
        if !matches!(runtime.id, "hermes" | "claude" | "codex") {
            return Err(WikiRuntimeFailure::UnsupportedRuntime(
                runtime.id.to_owned(),
            ));
        }

        match runtime.id {
            "hermes" => {
                let Some(profile) = self.profile.as_deref().map(str::trim) else {
                    return Err(WikiRuntimeFailure::MissingProfile);
                };
                if self.profile.as_deref() != Some(profile) {
                    return Err(WikiRuntimeFailure::InvalidProfile);
                }
                validate_hermes_profile_name(profile)
                    .map_err(|_| WikiRuntimeFailure::InvalidProfile)?;
                if self
                    .model
                    .as_deref()
                    .is_some_and(|model| !model.trim().is_empty())
                {
                    // Hermes profiles own provider/model.  Accepting a model
                    // here would make the UI appear to select one while the
                    // runtime silently ignores it.
                    return Err(WikiRuntimeFailure::ProfileOwnsModel);
                }
            }
            "claude" => {
                if self.profile.is_some() {
                    return Err(WikiRuntimeFailure::UnsupportedProfile);
                }
                validate_model(self.model.as_deref())?;
            }
            "codex" => {
                validate_model(self.model.as_deref())?;
                if self.profile.is_some() {
                    // The installed Codex CLI has a profile flag, but this
                    // adapter does not stage a Codex config layer. Refuse it
                    // instead of claiming that an unbound profile is active.
                    return Err(WikiRuntimeFailure::UnsupportedProfile);
                }
            }
            _ => unreachable!("closed above"),
        }
        Ok(())
    }
}

/// Stable, user-safe runtime adapter failure taxonomy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WikiRuntimeFailure {
    /// No executable was found for the selected known runtime.
    MissingExecutable(String),
    /// The runtime is not in the existing catalog or is not certified here.
    UnsupportedRuntime(String),
    /// A runtime-owned profile was not selected.
    MissingProfile,
    /// A profile name/value did not satisfy its runtime contract.
    InvalidProfile,
    /// A runtime does not support the supplied profile field.
    UnsupportedProfile,
    /// A profile-owned runtime must not receive a second model override.
    ProfileOwnsModel,
    /// A model was missing, blank, or could be interpreted as an option.
    InvalidModel,
    /// The process state directory was not an absolute directory.
    InvalidStateDirectory,
    /// The prompt exceeded the complete-input budget.
    InputLimit,
    /// A selected subscription runtime has no usable access credential.
    AuthenticationUnavailable,
    /// A selected subscription runtime credential was malformed.
    InvalidAuthentication,
    /// A selected subscription runtime credential is expired.
    ExpiredAuthentication,
    /// A selected subscription runtime credential exceeded the bounded size.
    AuthenticationLimit,
    /// The immutable repository steering could not be decoded.
    InvalidSteering,
    /// The selected profile could not be copied into disposable state.
    ProfileUnavailable,
    /// The selected profile exceeded its bounded copy budget.
    ProfileCopyLimit,
    /// The staged Hermes profile config is not a mapping or valid YAML.
    InvalidProfileConfig,
    /// The staged Hermes profile config exceeded its bounded parse budget.
    ProfileConfigLimit,
    /// The selected profile attempts to redirect private runtime state.
    ProfileBinding,
    /// The staged Hermes profile did not produce trustworthy run telemetry.
    InvalidRuntimeTelemetry,
    /// Hermes used a provider or model other than the staged profile selection.
    EffectiveRuntimeMismatch,
    /// The platform has no whole-process-tree containment primitive for this
    /// runtime adapter.
    UnsupportedContainment,
    /// The selected process could not be safely owned or completed.
    Process(BoundedFailure),
    /// The process exited unsuccessfully.
    NonzeroExit,
    /// The process returned non-UTF-8 or blank output.
    InvalidOutput,
}

impl std::fmt::Display for WikiRuntimeFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingExecutable(runtime) => {
                write!(f, "Wiki runtime '{runtime}' is not installed.")
            }
            Self::UnsupportedRuntime(runtime) => {
                write!(f, "Wiki runtime '{runtime}' is unsupported.")
            }
            Self::MissingProfile => {
                f.write_str("Wiki generation requires a named runtime profile.")
            }
            Self::InvalidProfile => f.write_str("Wiki runtime profile is invalid."),
            Self::UnsupportedProfile => {
                f.write_str("The selected Wiki runtime does not support profiles.")
            }
            Self::ProfileOwnsModel => f.write_str(
                "The selected runtime profile owns its model; remove the model override.",
            ),
            Self::InvalidModel => f.write_str("Wiki runtime model selection is invalid."),
            Self::InvalidStateDirectory => f.write_str("Wiki runtime state directory is invalid."),
            Self::InputLimit => f.write_str("Wiki runtime input exceeds the per-page limit."),
            Self::AuthenticationUnavailable => {
                f.write_str("Wiki runtime subscription authentication is unavailable.")
            }
            Self::InvalidAuthentication => {
                f.write_str("Wiki runtime subscription authentication is invalid.")
            }
            Self::ExpiredAuthentication => {
                f.write_str("Wiki runtime subscription authentication has expired.")
            }
            Self::AuthenticationLimit => {
                f.write_str("Wiki runtime subscription authentication exceeds its size limit.")
            }
            Self::InvalidSteering => f.write_str("Captured Wiki repository steering is invalid."),
            Self::ProfileUnavailable => {
                f.write_str("The selected Wiki runtime profile is unavailable.")
            }
            Self::ProfileCopyLimit => {
                f.write_str("The selected Wiki runtime profile exceeds its copy limit.")
            }
            Self::InvalidProfileConfig => {
                f.write_str("The selected Wiki runtime profile config is invalid.")
            }
            Self::ProfileConfigLimit => {
                f.write_str("The selected Wiki runtime profile config exceeds its size limit.")
            }
            Self::ProfileBinding => {
                f.write_str("The selected Wiki runtime profile cannot be bound to isolated state.")
            }
            Self::InvalidRuntimeTelemetry => f.write_str(
                "Wiki runtime did not report valid effective provider and model telemetry.",
            ),
            Self::EffectiveRuntimeMismatch => f.write_str(
                "Wiki runtime effective provider or model did not match the selected profile.",
            ),
            Self::UnsupportedContainment => {
                f.write_str("Wiki runtime process containment is unavailable on this platform.")
            }
            Self::Process(failure) => {
                write!(f, "Wiki runtime process was not bounded ({failure:?}).")
            }
            Self::NonzeroExit => f.write_str("Wiki runtime exited without a generated page."),
            Self::InvalidOutput => f.write_str("Wiki runtime returned invalid page output."),
        }
    }
}

impl From<WikiRuntimeFailure> for WikiError {
    fn from(error: WikiRuntimeFailure) -> Self {
        Self::Generate(error.to_string())
    }
}

/// A runtime adapter that owns one disposable state directory and one cancel
/// flag.  Each `generate` call starts a fresh one-shot child process.
pub(crate) struct WikiRuntimeGenerator {
    selection: WikiRuntimeSelection,
    executable: PathBuf,
    state_dir: PathBuf,
    /// macOS receives the fixed no-fork Seatbelt profile. Windows leaves this
    /// empty because the bounded runner owns the child with a Job Object.
    /// Test-only fake runtimes use the private constructor with no wrapper.
    sandbox_profile: Option<String>,
    /// Keeps the installed-runtime state disposable; test callers may provide
    /// their own directory and leave this as `None`.
    temp_state: Option<tempfile::TempDir>,
    require_hermes_usage_report: bool,
    cancel: Arc<AtomicBool>,
    /// Claude receives the existing subscription access token through its
    /// child environment; it is never persisted in the disposable state.
    claude_oauth_token: Option<String>,
}

impl WikiRuntimeGenerator {
    /// Resolve an installed executable and bind it to a caller-owned cancel
    /// flag. The flag is scoped to one foreground Wiki generation job.
    pub(crate) fn installed_with_cancel(
        selection: WikiRuntimeSelection,
        cancel: Arc<AtomicBool>,
    ) -> Result<Self, WikiRuntimeFailure> {
        let selection = selection.normalized()?;
        if selection.runtime_id == "claude" {
            return Err(WikiRuntimeFailure::UnsupportedRuntime(selection.runtime_id));
        }
        let runtime = known_acp_runtime_exact(selection.runtime_id.trim())
            .ok_or_else(|| WikiRuntimeFailure::UnsupportedRuntime(selection.runtime_id.clone()))?;
        let command = runtime
            .recap_native_command
            .or(runtime.underlying_cli)
            .or_else(|| runtime.commands.first().copied())
            .ok_or_else(|| WikiRuntimeFailure::UnsupportedRuntime(runtime.id.to_owned()))?;
        let executable = resolve_command(command)
            .and_then(|path| resolve_installed_wrapper(runtime.id, path))
            .ok_or_else(|| WikiRuntimeFailure::MissingExecutable(runtime.id.to_owned()))?;
        // Check containment before allocating or copying any disposable
        // runtime state. Process groups alone cannot contain a setsid escape
        // on Unix, so unsupported platforms fail closed here.
        let sandbox_profile = native_containment_profile(&executable)
            .map_err(|_| WikiRuntimeFailure::UnsupportedContainment)?;
        let state = tempfile::tempdir().map_err(|_| WikiRuntimeFailure::InvalidStateDirectory)?;
        let state_dir = state.path().to_path_buf();
        let mut generator = Self::with_executable_and_cancel(
            selection,
            executable,
            state_dir,
            cancel,
            sandbox_profile,
        )?;
        generator.stage_hermes_profile()?;
        generator.claude_oauth_token = super::wiki_runtime_auth::stage_runtime_auth(
            generator.selection.runtime_id.trim(),
            &generator.state_dir,
            &generator.cancel,
        )?;
        generator.require_hermes_usage_report = generator.selection.runtime_id == "hermes";
        generator.temp_state = Some(state);
        Ok(generator)
    }

    /// Construct a generator with a caller-owned disposable state directory.
    /// The directory is used only for the runtime process; it is never the
    /// selected repository root.  Tests use this seam with a fake executable,
    /// while staging can provide a copied profile/configuration tree.
    #[cfg(test)]
    pub(crate) fn with_executable(
        selection: WikiRuntimeSelection,
        executable: PathBuf,
        state_dir: PathBuf,
    ) -> Result<Self, WikiRuntimeFailure> {
        Self::with_executable_and_cancel(
            selection,
            executable,
            state_dir,
            Arc::new(AtomicBool::new(false)),
            None,
        )
    }

    fn with_executable_and_cancel(
        selection: WikiRuntimeSelection,
        executable: PathBuf,
        state_dir: PathBuf,
        cancel: Arc<AtomicBool>,
        sandbox_profile: Option<String>,
    ) -> Result<Self, WikiRuntimeFailure> {
        let selection = selection.normalized()?;
        if !executable.is_absolute() || !state_dir.is_absolute() {
            return Err(WikiRuntimeFailure::InvalidStateDirectory);
        }
        std::fs::create_dir_all(&state_dir)
            .map_err(|_| WikiRuntimeFailure::InvalidStateDirectory)?;
        for name in ["home", "tmp", "config", "cache", "data", "state", "hermes"] {
            std::fs::create_dir_all(state_dir.join(name))
                .map_err(|_| WikiRuntimeFailure::InvalidStateDirectory)?;
        }
        Ok(Self {
            selection,
            executable,
            state_dir,
            sandbox_profile,
            temp_state: None,
            require_hermes_usage_report: false,
            cancel,
            claude_oauth_token: None,
        })
    }

    fn stage_hermes_profile(&self) -> Result<(), WikiRuntimeFailure> {
        if self.selection.runtime_id.trim() != "hermes" {
            return Ok(());
        }
        let profile = self
            .selection
            .profile
            .as_deref()
            .ok_or(WikiRuntimeFailure::MissingProfile)?;
        let source = hermes_profile_dir(profile).ok_or(WikiRuntimeFailure::ProfileUnavailable)?;
        let source = canonical_profile_source(&source)?;
        let destination = if is_hermes_home_profile(profile) {
            self.state_dir.join("hermes")
        } else {
            self.state_dir.join("hermes").join("profiles").join(profile)
        };
        let mut budget = ProfileCopyBudget::default();
        copy_profile_tree(&source, &destination, &mut budget, 0)?;
        validate_profile_config(&destination)?;
        validate_staged_hermes_profile_dotenv(&destination)?;
        ensure_private_hermes_managed_dir(&self.state_dir)
    }

    /// Cancel the currently owned request.  The bounded runner terminates the
    /// process tree and reports a typed failure at its next poll.
    #[allow(dead_code)]
    pub(crate) fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    /// Report the actual selected runtime provenance for diagnostics/status,
    /// not a cost or provider claim inferred from an environment variable.
    #[allow(dead_code)]
    pub(crate) fn diagnostic(&self) -> String {
        let model = match self.selection.runtime_id.trim() {
            "hermes" => "profile-owned",
            _ => self.selection.model.as_deref().unwrap_or("runtime-default"),
        };
        let profile = self.selection.profile.as_deref().unwrap_or("none");
        format!(
            "runtime={} model={} profile={}",
            self.selection.runtime_id.trim(),
            model,
            profile
        )
    }

    fn command(&self, prompt: Option<&str>) -> Command {
        let mut command = if let Some(profile) = self.sandbox_profile.as_deref() {
            let mut command = Command::new("/usr/bin/sandbox-exec");
            command.args(["-p", profile]);
            command.arg(&self.executable);
            command
        } else {
            Command::new(&self.executable)
        };
        command
            .env_clear()
            .current_dir(&self.state_dir)
            .stdin(Stdio::null())
            // Keep only a minimal executable search path.  No caller's env,
            // API key, session, MCP or employee context is inherited.
            .env(
                "PATH",
                format!(
                    "{}:/usr/bin:/bin",
                    self.executable
                        .parent()
                        .unwrap_or_else(|| Path::new("/usr/bin"))
                        .display()
                ),
            )
            .env("HOME", self.state_dir.join("home"))
            .env("TMPDIR", self.state_dir.join("tmp"))
            .env("XDG_CONFIG_HOME", self.state_dir.join("config"))
            .env("XDG_CACHE_HOME", self.state_dir.join("cache"))
            .env("XDG_DATA_HOME", self.state_dir.join("data"))
            .env("XDG_STATE_HOME", self.state_dir.join("state"));

        match self.selection.runtime_id.trim() {
            "hermes" => {
                command
                    .env("HERMES_HOME", self.state_dir.join("hermes"))
                    .env("HERMES_MANAGED_DIR", self.state_dir.join("managed"))
                    // Hermes checks this child-process guard before discovering plugins,
                    // loading configured MCP servers, or registering user hooks/webhooks.
                    // Keep the direct env assignment for early child paths, and pass the
                    // native flag so Hermes reapplies the guard after profile dotenv and
                    // managed-env loading. The oneshot path still reads the staged profile
                    // config for its provider/model selection.
                    .env("HERMES_SAFE_MODE", "1")
                    .args([
                        "--safe-mode",
                        "--ignore-rules",
                        "--no-restore-cwd",
                        // `context_engine` is the installed Hermes CLI's
                        // empty native toolset. Supplying it explicitly keeps
                        // terminal/file/browser/MCP tools out of the one-shot
                        // page generator instead of relying on prompt text.
                        "--toolsets",
                        "context_engine",
                        "--usage-file",
                    ]);
                command.arg(report_path(&self.state_dir));
                if let Some(profile) = self.selection.profile.as_deref() {
                    command.args(["-p", profile]);
                }
                command.args([
                    "--in",
                    self.state_dir.to_string_lossy().as_ref(),
                    "--oneshot",
                ]);
                if let Some(prompt) = prompt {
                    command.arg(prompt);
                }
            }
            "claude" => {
                command
                    .env("CLAUDE_CONFIG_DIR", self.state_dir.join("config"))
                    .env("DISABLE_AUTOUPDATER", "1")
                    .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
                    .args([
                        "--safe-mode",
                        "--restricted",
                        "--print",
                        "--output-format",
                        "text",
                        "--no-session-persistence",
                        "--tools",
                        "",
                        "--strict-mcp-config",
                        "--permission-prompts",
                        "none",
                        "--disable-slash-commands",
                    ]);
                if let Some(token) = self.claude_oauth_token.as_deref() {
                    command.env("CLAUDE_CODE_OAUTH_TOKEN", token);
                }
                if let Some(model) = self.selection.model.as_deref() {
                    command.args(["--model", model]);
                }
                if let Some(prompt) = prompt {
                    command.arg(prompt);
                }
            }
            "codex" => {
                command
                    .env("CODEX_HOME", self.state_dir.join("config"))
                    .args([
                        "exec",
                        "--ephemeral",
                        "--ignore-user-config",
                        "--ignore-rules",
                        "--disable",
                        "shell_tool",
                        "--disable",
                        "shell_snapshot",
                        "--disable",
                        "multi_agent",
                        "--disable",
                        "view_image",
                        "--disable",
                        "image_generation",
                        "-c",
                        "web_search=\"disabled\"",
                        "--sandbox",
                        "read-only",
                        "--skip-git-repo-check",
                    ]);
                if let Some(model) = self.selection.model.as_deref() {
                    command.args(["--model", model]);
                }
                if let Some(profile) = self.selection.profile.as_deref() {
                    command.args(["--profile", profile]);
                }
                command.arg("-");
            }
            _ => unreachable!("selection validated before command construction"),
        }
        command
    }

    fn invoke(&self, prompt: &str) -> Result<String, WikiRuntimeFailure> {
        if prompt.len() > WIKI_RUNTIME_INPUT_LIMIT {
            return Err(WikiRuntimeFailure::InputLimit);
        }
        let is_hermes = self.selection.runtime_id.trim() == "hermes";
        if is_hermes {
            clear_report(&self.state_dir)?;
        }
        let policy = BoundedPolicy {
            timeout: WIKI_RUNTIME_TIMEOUT,
            budget: OutputBudget::PerStream {
                stdout: WIKI_RUNTIME_OUTPUT_LIMIT,
                stderr: WIKI_RUNTIME_STDERR_LIMIT,
            },
        };
        let output = if self.selection.runtime_id.trim() == "codex" {
            output_with_policy_and_stdin(
                self.command(None),
                Some(prompt.as_bytes().to_vec()),
                policy,
                &self.cancel,
            )
        } else {
            output_with_policy(self.command(Some(prompt)), policy, &self.cancel)
        }
        .map_err(WikiRuntimeFailure::Process)?
        .output;
        if !output.status.success() {
            return Err(WikiRuntimeFailure::NonzeroExit);
        }
        if is_hermes && (self.require_hermes_usage_report || report_path(&self.state_dir).exists())
        {
            validate_report(
                &self.state_dir,
                self.selection
                    .profile
                    .as_deref()
                    .ok_or(WikiRuntimeFailure::MissingProfile)?,
            )?;
        }
        let text =
            String::from_utf8(output.stdout).map_err(|_| WikiRuntimeFailure::InvalidOutput)?;
        if text.trim().is_empty() || text.contains('\0') {
            return Err(WikiRuntimeFailure::InvalidOutput);
        }
        Ok(text)
    }
}

/// Inspect copied dotenv files without rewriting accepted credentials. The
/// installed launcher loads the profile `.env` with override semantics, and
/// `.op.env` is another early source, so either file could otherwise redirect
/// `HERMES_HOME` or `HERMES_MANAGED_DIR` before the native safe-mode guard.
fn validate_staged_hermes_profile_dotenv(profile_dir: &Path) -> Result<(), WikiRuntimeFailure> {
    let env_path = profile_dir.join(".env");
    match read_staged_hermes_dotenv(&env_path)? {
        Some(contents) => validate_hermes_dotenv_bytes(&contents)?,
        None => {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&env_path)
                .map_err(|_| WikiRuntimeFailure::ProfileUnavailable)?;
        }
    }
    if let Some(contents) = read_staged_hermes_dotenv(&profile_dir.join(".op.env"))? {
        validate_hermes_dotenv_bytes(&contents)?;
    }
    Ok(())
}

fn read_staged_hermes_dotenv(path: &Path) -> Result<Option<Vec<u8>>, WikiRuntimeFailure> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(WikiRuntimeFailure::ProfileBinding),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(WikiRuntimeFailure::ProfileBinding);
    }
    if metadata.len() > HERMES_PROFILE_DOTENV_BYTES_LIMIT {
        return Err(WikiRuntimeFailure::ProfileConfigLimit);
    }
    let mut contents = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| WikiRuntimeFailure::ProfileBinding)?
        .take(HERMES_PROFILE_DOTENV_BYTES_LIMIT + 1)
        .read_to_end(&mut contents)
        .map_err(|_| WikiRuntimeFailure::ProfileBinding)?;
    if contents.len() as u64 > HERMES_PROFILE_DOTENV_BYTES_LIMIT {
        return Err(WikiRuntimeFailure::ProfileConfigLimit);
    }
    Ok(Some(contents))
}

fn validate_hermes_dotenv_bytes(contents: &[u8]) -> Result<(), WikiRuntimeFailure> {
    if contents.contains(&0)
        || contents
            .windows(3)
            .any(|window| window == [0xef, 0xbb, 0xbf])
    {
        return Err(WikiRuntimeFailure::ProfileBinding);
    }
    let text = std::str::from_utf8(contents).map_err(|_| WikiRuntimeFailure::ProfileBinding)?;
    if text
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(WikiRuntimeFailure::ProfileBinding);
    }
    if text
        .split(['\n', '\r'])
        .any(dotenv_line_has_hermes_binding_assignment)
    {
        return Err(WikiRuntimeFailure::ProfileBinding);
    }
    Ok(())
}

fn dotenv_line_has_hermes_binding_assignment(line: &str) -> bool {
    let mut candidate = line.trim_start_matches(|character: char| character.is_whitespace());
    if candidate.is_empty() || candidate.starts_with('#') {
        return false;
    }
    if let Some(rest) = candidate.strip_prefix("export") {
        if rest.starts_with(|character: char| character.is_whitespace()) {
            candidate = rest.trim_start_matches(|character: char| character.is_whitespace());
        }
    }
    HERMES_PROFILE_BINDING_KEYS
        .iter()
        .any(|key| dotenv_candidate_matches_key(candidate, key))
}

fn dotenv_candidate_matches_key(candidate: &str, key: &str) -> bool {
    if let Some(rest) = candidate.strip_prefix(key) {
        let Some(first) = rest.chars().next() else {
            return true;
        };
        if first.is_ascii_alphanumeric() || first == '_' {
            return false;
        }
        return true;
    }
    for quote in ['\'', '"'] {
        let Some(quoted) = candidate.strip_prefix(quote) else {
            continue;
        };
        let Some(end) = quoted.find(quote) else {
            return quoted.starts_with(key);
        };
        if &quoted[..end] == key {
            return true;
        }
    }
    false
}

fn ensure_private_hermes_managed_dir(state_dir: &Path) -> Result<(), WikiRuntimeFailure> {
    let managed = state_dir.join("managed");
    match std::fs::symlink_metadata(&managed) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(WikiRuntimeFailure::InvalidStateDirectory);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&managed).map_err(|_| WikiRuntimeFailure::InvalidStateDirectory)?;
        }
        Err(_) => return Err(WikiRuntimeFailure::InvalidStateDirectory),
    }
    let mut entries =
        std::fs::read_dir(&managed).map_err(|_| WikiRuntimeFailure::InvalidStateDirectory)?;
    if entries
        .next()
        .transpose()
        .map_err(|_| WikiRuntimeFailure::InvalidStateDirectory)?
        .is_some()
    {
        return Err(WikiRuntimeFailure::InvalidStateDirectory);
    }
    Ok(())
}

/// Resolve a profile root only after inspecting the path itself.  Checking
/// after `canonicalize` would silently accept a profile whose root is a
/// symlink, defeating the copy routine's symlink boundary at its first node.
fn canonical_profile_source(source: &Path) -> Result<PathBuf, WikiRuntimeFailure> {
    let metadata =
        std::fs::symlink_metadata(source).map_err(|_| WikiRuntimeFailure::ProfileUnavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(WikiRuntimeFailure::ProfileUnavailable);
    }
    let canonical =
        std::fs::canonicalize(source).map_err(|_| WikiRuntimeFailure::ProfileUnavailable)?;
    let canonical_metadata = std::fs::symlink_metadata(&canonical)
        .map_err(|_| WikiRuntimeFailure::ProfileUnavailable)?;
    if canonical_metadata.file_type().is_symlink() || !canonical_metadata.is_dir() {
        return Err(WikiRuntimeFailure::ProfileUnavailable);
    }
    Ok(canonical)
}

/// Resolve the real binary behind the two user-facing macOS launch wrappers
/// used by the current installed runtime layout. The adapter later replaces
/// `HOME` with disposable state, so launching either wrapper directly would
/// make it look as though the runtime was missing: Claude's wrapper searches
/// `$HOME/.local/share/claude/versions` and Hermes' wrapper searches
/// `$HOME/.hermes/hermes-agent`. Direct binaries and unrelated shims are
/// preserved unchanged.
fn resolve_installed_wrapper(runtime_id: &str, resolved: PathBuf) -> Option<PathBuf> {
    let file = std::fs::File::open(&resolved).ok()?;
    let mut source = Vec::new();
    file.take(64 * 1024).read_to_end(&mut source).ok()?;
    let source = std::str::from_utf8(&source).ok();
    match (runtime_id, source) {
        ("claude", Some(source)) if source.contains(".local/share/claude/versions") => {
            let home = dirs::home_dir()?;
            let versions = home.join(".local/share/claude/versions");
            let mut candidates = std::fs::read_dir(versions)
                .ok()?
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let path = entry.path();
                    let metadata = std::fs::metadata(&path).ok()?;
                    if !metadata.is_file() || metadata.len() <= 1_000_000 || !is_executable(&path) {
                        return None;
                    }
                    let modified = metadata.modified().ok()?;
                    Some((modified, path))
                })
                .collect::<Vec<_>>();
            candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
            candidates.into_iter().next().map(|(_, path)| path)
        }
        ("hermes", Some(source)) if source.contains(".hermes/hermes-agent") => {
            let home = dirs::home_dir()?;
            let path = home.join(".hermes/hermes-agent/venv/bin/hermes");
            is_executable(&path).then_some(path)
        }
        _ => Some(resolved),
    }
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

impl Generator for WikiRuntimeGenerator {
    fn generate(
        &self,
        page: &PlannedPage,
        snapshot: &RepoSnapshot,
        language: &str,
    ) -> Result<String, WikiError> {
        let prompt = build_prompt(page, snapshot, language).map_err(WikiError::from)?;
        let output = self.invoke(&prompt).map_err(|error| {
            // Keep the selected runtime provenance attached to the actionable
            // failure. A native status/recovery row must never imply that a
            // different provider or model answered the page.
            WikiError::Generate(format!("{}: {error}", self.diagnostic()))
        })?;
        normalize_generated_page(page, snapshot, &output)
            .map_err(|error| WikiError::Generate(format!("{}: {error}", self.diagnostic())))
    }
}

fn validate_model(model: Option<&str>) -> Result<(), WikiRuntimeFailure> {
    let Some(model) = model.filter(|model| !model.trim().is_empty()) else {
        return Ok(());
    };
    validate_value(model).map_err(|_| WikiRuntimeFailure::InvalidModel)
}

#[derive(Default)]
struct ProfileCopyBudget {
    entries: usize,
    files: usize,
    bytes: u64,
}

fn copy_profile_tree(
    source: &Path,
    destination: &Path,
    budget: &mut ProfileCopyBudget,
    depth: usize,
) -> Result<(), WikiRuntimeFailure> {
    if depth > HERMES_PROFILE_DEPTH_LIMIT {
        return Err(WikiRuntimeFailure::ProfileCopyLimit);
    }
    budget.entries = budget
        .entries
        .checked_add(1)
        .ok_or(WikiRuntimeFailure::ProfileCopyLimit)?;
    if budget.entries > HERMES_PROFILE_ENTRY_LIMIT {
        return Err(WikiRuntimeFailure::ProfileCopyLimit);
    }
    let metadata =
        std::fs::symlink_metadata(source).map_err(|_| WikiRuntimeFailure::ProfileUnavailable)?;
    if metadata.file_type().is_symlink() {
        return Err(WikiRuntimeFailure::ProfileUnavailable);
    }
    if metadata.is_dir() {
        std::fs::create_dir_all(destination).map_err(|_| WikiRuntimeFailure::ProfileUnavailable)?;
        let mut entries = std::fs::read_dir(source)
            .map_err(|_| WikiRuntimeFailure::ProfileUnavailable)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| WikiRuntimeFailure::ProfileUnavailable)?;
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
        return Err(WikiRuntimeFailure::ProfileUnavailable);
    }
    budget.files = budget
        .files
        .checked_add(1)
        .ok_or(WikiRuntimeFailure::ProfileCopyLimit)?;
    budget.bytes = budget
        .bytes
        .checked_add(metadata.len())
        .ok_or(WikiRuntimeFailure::ProfileCopyLimit)?;
    if budget.files > HERMES_PROFILE_FILE_LIMIT || budget.bytes > HERMES_PROFILE_BYTES_LIMIT {
        return Err(WikiRuntimeFailure::ProfileCopyLimit);
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|_| WikiRuntimeFailure::ProfileUnavailable)?;
    }
    std::fs::copy(source, destination).map_err(|_| WikiRuntimeFailure::ProfileUnavailable)?;
    Ok(())
}

fn validate_value(value: &str) -> Result<(), ()> {
    const MAX_RUNTIME_VALUE_BYTES: usize = 300;
    if value.is_empty()
        || value.len() > MAX_RUNTIME_VALUE_BYTES
        || value != value.trim()
        || value.starts_with('-')
        || value.chars().any(char::is_control)
    {
        Err(())
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "wiki_runtime_tests.rs"]
mod wiki_runtime_tests;

#[cfg(all(test, not(target_os = "windows")))]
#[path = "wiki_runtime_containment_tests.rs"]
mod wiki_runtime_containment_tests;
