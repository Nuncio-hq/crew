//! Native one-shot adapters for owner-local Thread Recap.
//!
//! The adapter is deliberately closed over two recipes. Claude receives an
//! explicit model and a JSON result. Hermes receives a named, copied staging
//! profile; its model remains profile-owned and is checked through the bounded
//! usage report written below the disposable run root. No caller-authored argv
//! or environment is accepted.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::recap_capability::RecapToolProbeEvidence;

/// Maximum UTF-8 input bytes accepted by the recap executor.
pub(crate) const RECAP_INPUT_LIMIT: usize = 128 * 1024;
/// Maximum captured bytes in each recap output stream.
pub(crate) const RECAP_OUTPUT_LIMIT: usize = 256 * 1024;

const HERMES_PROFILE_FILE_LIMIT: usize = 1024;
const HERMES_PROFILE_ENTRY_LIMIT: usize = 4096;
const HERMES_PROFILE_DEPTH_LIMIT: usize = 32;
const HERMES_PROFILE_BYTES_LIMIT: u64 = 32 * 1024 * 1024;
const HERMES_USAGE_LIMIT: u64 = 64 * 1024;

/// Redacted facts parsed from one native certification-probe result. This is
/// produced by [`RecapLaunchPlan::parse_probe_output`], rather than assembled
/// by a renderer or settings caller. The raw probe envelope is never retained
/// after these bounded fields are extracted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapAdapterObservation {
    output: Vec<u8>,
    effective_model: String,
    one_shot_completed: bool,
    tool_probe: RecapToolProbeEvidence,
}

impl RecapAdapterObservation {
    pub(crate) fn output(&self) -> &[u8] {
        &self.output
    }

    pub(crate) fn effective_model(&self) -> &str {
        &self.effective_model
    }

    pub(crate) fn one_shot_completed(&self) -> bool {
        self.one_shot_completed
    }

    pub(crate) fn tool_probe(&self) -> &RecapToolProbeEvidence {
        &self.tool_probe
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        output: Vec<u8>,
        effective_model: &str,
        one_shot_completed: bool,
        tool_probe: RecapToolProbeEvidence,
    ) -> Self {
        Self {
            output,
            effective_model: effective_model.to_owned(),
            one_shot_completed,
            tool_probe,
        }
    }
}

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
    profile_digest: Option<String>,
    profile_identity: Option<super::recap_capability::RecapProfileIdentity>,
    profile_destination: Option<PathBuf>,
    profile_destination_identity: Option<super::recap_capability::RecapProfileIdentity>,
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

    /// Recheck the source profile immediately before spawning the child.
    pub(crate) fn profile_matches_admission(
        &self,
        admission: &super::recap_capability::RecapAdmission,
    ) -> bool {
        match (&self.profile_source, &self.profile_digest) {
            (None, None) => {
                admission.selection.profile.is_none()
                    && admission.selection.profile_digest.is_none()
                    && admission.selection.profile_identity.is_none()
            }
            (Some(source), Some(expected)) => {
                admission.selection.profile.as_deref() == Some(source.as_path())
                    && admission.selection.profile_digest.as_deref() == Some(expected.as_str())
                    && admission.selection.profile_identity == self.profile_identity
                    && profile_identity(source).ok().as_ref() == self.profile_identity.as_ref()
                    && profile_tree_digest(source).ok().as_deref() == Some(expected.as_str())
                    && self
                        .profile_destination
                        .as_deref()
                        .is_some_and(|destination| {
                            profile_identity(destination).ok().as_ref()
                                == self.profile_destination_identity.as_ref()
                                && profile_tree_digest(destination).ok().as_deref()
                                    == Some(expected.as_str())
                        })
            }
            _ => false,
        }
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
            && self.profile_digest == admission.selection.profile_digest
            && self.profile_identity == admission.selection.profile_identity
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

    /// Parse the fixed certification envelope emitted by a native adapter
    /// probe. A normal recap result is intentionally not enough: the envelope
    /// must carry an observed hostile-tool denial and its unchanged sentinel.
    /// Installed runtimes that cannot emit this evidence remain unsupported.
    pub(crate) fn parse_probe_output(
        &self,
        exit_success: bool,
        stdout: &[u8],
        stderr: &[u8],
    ) -> Result<RecapAdapterObservation, RecapRunFailure> {
        if stdout.len() > RECAP_OUTPUT_LIMIT || stderr.len() > RECAP_OUTPUT_LIMIT {
            return Err(RecapRunFailure::OutputLimit);
        }
        if !exit_success {
            return Err(RecapRunFailure::NonzeroExit);
        }
        let envelope: RecapProbeEnvelope =
            serde_json::from_slice(stdout).map_err(|_| RecapRunFailure::InvalidOutput)?;
        if envelope.kind != "recap_probe"
            || envelope.result.trim().is_empty()
            || envelope.result.contains('\0')
            || envelope.effective_model.trim().is_empty()
            || envelope.effective_model != envelope.effective_model.trim()
            || envelope.effective_model.chars().any(char::is_control)
        {
            return Err(RecapRunFailure::InvalidOutput);
        }
        Ok(RecapAdapterObservation {
            output: envelope.result.into_bytes(),
            effective_model: envelope.effective_model,
            one_shot_completed: envelope.one_shot_completed,
            tool_probe: RecapToolProbeEvidence {
                probe_id: envelope.tool_probe.probe_id,
                tool_name: envelope.tool_probe.tool_name,
                request_observed: envelope.tool_probe.request_observed,
                denied_before_effect: envelope.tool_probe.denied_before_effect,
                sentinel_before: envelope.tool_probe.sentinel_before,
                sentinel_after: envelope.tool_probe.sentinel_after,
            },
        })
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
        profile_digest: None,
        profile_identity: None,
        profile_destination: None,
        profile_destination_identity: None,
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
    let source_identity = profile_identity(&source)?;
    let profile_digest = profile_tree_digest(&source)?;
    let profile_name = hermes_profile_ref(&source).ok_or(RecapRunFailure::InvalidSelection)?;
    let destination = if profile_name == super::hermes_profile::HERMES_HOME_PROFILE_NAME {
        root.join("hermes")
    } else {
        root.join("hermes").join("profiles").join(&profile_name)
    };
    let mut budget = ProfileCopyBudget::default();
    copy_profile_tree(&source, &destination, &mut budget, 0)?;
    if profile_tree_digest(&source)? != profile_digest
        || profile_tree_digest(&destination)? != profile_digest
    {
        return Err(RecapRunFailure::ProfileUnavailable);
    }
    let destination_identity = profile_identity(&destination)?;

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
        profile_digest: Some(profile_digest),
        profile_identity: Some(source_identity),
        profile_destination: Some(destination),
        profile_destination_identity: Some(destination_identity),
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

/// Hash the bounded, canonical Hermes profile tree used by a recap grant.
///
/// The digest includes relative entry names, directory/file markers and file
/// bytes. Symlinks and changes during the walk fail closed, and the same
/// limits as profile copying apply so certification cannot turn into an
/// unbounded filesystem read.
pub(crate) fn profile_tree_digest(source: &Path) -> Result<String, RecapRunFailure> {
    let canonical = canonical_profile_source(source)?;
    let mut budget = ProfileCopyBudget::default();
    let mut hasher = Sha256::new();
    hash_profile_tree(&canonical, &canonical, &mut budget, 0, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

/// Capture the canonical directory identity used alongside the content hash.
pub(crate) fn profile_identity(
    source: &Path,
) -> Result<super::recap_capability::RecapProfileIdentity, RecapRunFailure> {
    let canonical = canonical_profile_source(source)?;
    let metadata =
        std::fs::symlink_metadata(&canonical).map_err(|_| RecapRunFailure::ProfileUnavailable)?;
    if !metadata.is_dir() {
        return Err(RecapRunFailure::ProfileUnavailable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o777 != 0o700 || metadata.uid() != rustix::process::geteuid().as_raw()
        {
            return Err(RecapRunFailure::ProfileUnavailable);
        }
        Ok(super::recap_capability::RecapProfileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
            owner: metadata.uid(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        Err(RecapRunFailure::UnsupportedContainment)
    }
}

fn hash_profile_tree(
    root: &Path,
    current: &Path,
    budget: &mut ProfileCopyBudget,
    depth: usize,
    hasher: &mut Sha256,
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
        std::fs::symlink_metadata(current).map_err(|_| RecapRunFailure::ProfileUnavailable)?;
    if metadata.file_type().is_symlink() {
        return Err(RecapRunFailure::ProfileUnavailable);
    }
    let relative = current
        .strip_prefix(root)
        .map_err(|_| RecapRunFailure::ProfileUnavailable)?;
    hasher.update(if metadata.is_dir() { b"D\0" } else { b"F\0" });
    let relative_bytes = relative
        .to_str()
        .ok_or(RecapRunFailure::ProfileUnavailable)?;
    hasher.update(relative_bytes.as_bytes());
    hasher.update([0]);
    if metadata.is_dir() {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(current).map_err(|_| RecapRunFailure::ProfileUnavailable)? {
            if entries.len() >= HERMES_PROFILE_ENTRY_LIMIT {
                return Err(RecapRunFailure::ProfileCopyLimit);
            }
            entries.push(entry.map_err(|_| RecapRunFailure::ProfileUnavailable)?);
        }
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            hash_profile_tree(root, &entry.path(), budget, depth + 1, hasher)?;
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
    hasher.update(metadata.len().to_le_bytes());
    let mut file = open_profile_file(current)?;
    let opened_metadata = file
        .metadata()
        .map_err(|_| RecapRunFailure::ProfileUnavailable)?;
    if !same_profile_file_identity(&metadata, &opened_metadata) {
        return Err(RecapRunFailure::ProfileUnavailable);
    }
    let mut buffer = [0u8; 64 * 1024];
    let mut remaining = metadata.len();
    while remaining > 0 {
        let chunk = remaining.min(buffer.len() as u64) as usize;
        let read = file
            .read(&mut buffer[..chunk])
            .map_err(|_| RecapRunFailure::ProfileUnavailable)?;
        if read == 0 {
            return Err(RecapRunFailure::ProfileUnavailable);
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    let mut extra = [0u8; 1];
    if file
        .read(&mut extra)
        .map_err(|_| RecapRunFailure::ProfileUnavailable)?
        != 0
    {
        return Err(RecapRunFailure::ProfileUnavailable);
    }
    let final_metadata = file
        .metadata()
        .map_err(|_| RecapRunFailure::ProfileUnavailable)?;
    if !same_profile_file_identity(&metadata, &final_metadata) {
        return Err(RecapRunFailure::ProfileUnavailable);
    }
    Ok(())
}

fn open_profile_file(path: &Path) -> Result<File, RecapRunFailure> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|_| RecapRunFailure::ProfileUnavailable)
    }
    #[cfg(not(unix))]
    {
        File::open(path).map_err(|_| RecapRunFailure::ProfileUnavailable)
    }
}

fn same_profile_file_identity(before: &std::fs::Metadata, after: &std::fs::Metadata) -> bool {
    if !before.is_file() || !after.is_file() || before.len() != after.len() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        before.dev() == after.dev()
            && before.ino() == after.ino()
            && before.mtime() == after.mtime()
            && before.mtime_nsec() == after.mtime_nsec()
            && before.ctime() == after.ctime()
            && before.ctime_nsec() == after.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        true
    }
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
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| RecapRunFailure::ProfileUnavailable)?;
        }
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(source).map_err(|_| RecapRunFailure::ProfileUnavailable)? {
            if entries.len() >= HERMES_PROFILE_ENTRY_LIMIT {
                return Err(RecapRunFailure::ProfileCopyLimit);
            }
            entries.push(entry.map_err(|_| RecapRunFailure::ProfileUnavailable)?);
        }
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
#[serde(deny_unknown_fields)]
struct RecapProbeEnvelope {
    #[serde(rename = "type")]
    kind: String,
    result: String,
    #[serde(rename = "effectiveModel")]
    effective_model: String,
    #[serde(rename = "oneShotCompleted")]
    one_shot_completed: bool,
    #[serde(rename = "toolProbe")]
    tool_probe: RecapToolProbeEnvelope,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecapToolProbeEnvelope {
    #[serde(rename = "probeId")]
    probe_id: String,
    #[serde(rename = "toolName")]
    tool_name: String,
    #[serde(rename = "requestObserved")]
    request_observed: bool,
    #[serde(rename = "deniedBeforeEffect")]
    denied_before_effect: bool,
    #[serde(rename = "sentinelBefore")]
    sentinel_before: String,
    #[serde(rename = "sentinelAfter")]
    sentinel_after: String,
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
