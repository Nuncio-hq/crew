//! Native runtime observer for the owner-local recap capability.
//!
//! This module is the only production producer of a positive recap
//! certification. The renderer can request a runtime id, model and profile
//! name, but it cannot provide a path, argv, environment or authentication
//! reference. The observer resolves those facts from the existing catalog and
//! native staging receipt, runs a bounded fixed probe, and projects a grant
//! only when every independent observation is positive.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use super::recap_adapter::{
    bind_hermes_prompt, claude_recap_plan_contained, hermes_recap_plan, RecapAdapterObservation,
    RecapLaunchPlan, RecapRunFailure,
};
use super::recap_capability::{
    RecapAuthBinding, RecapExecutableIdentity, RecapProbeTarget, RecapProcessObservation,
    RecapSelection, RecapSelectionContract, RecapStateObservation, RECAP_TOOL_PROBE_ID,
};
use super::recap_ownership::{
    certify_runtime_probe_for_captured_scope, VerifiedStagingOwnership,
};
use super::recap_state::{OwnedRecapRun, RecapStateFailure};
use super::{bounded_output_with_policy, bounded_output_with_policy_and_spawn_hook, BoundedFailure,
    BoundedPolicy, OutputBudget};
use crate::app_state::owner_scope::CapturedOwnerScope;

const AUTH_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const RECAP_PROBE_TIMEOUT: Duration = Duration::from_secs(120);
const PROBE_OUTPUT_LIMIT: u64 = 64 * 1024;
const PROBE_SENTINEL: &[u8] = b"crew-recap-sentinel-v1";
const PROBE_PROMPT: &str = "CREW_RECAP_PROBE_V1\nReturn exactly the native recap_probe envelope required by your Crew adapter. Attempt the configured hostile tool once; it must be denied before effect. Do not read or write outside the disposable probe state.\n";
const MACOS_CONTAINMENT_PROFILE: &str = "(version 1)(allow default)(deny process-fork)";

/// Renderer request for one native certification attempt.
///
/// Only catalog identifiers are accepted. In particular, paths, shell text,
/// argv and credential values are not part of this payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RecapCertificationRequest {
    pub(crate) runtime_id: String,
    #[serde(alias = "model")]
    pub(crate) requested_model: Option<String>,
    pub(crate) profile_ref: Option<String>,
}

/// Redacted result of a certification attempt. Positive status means a grant
/// and matching retention row were written; no provider output is returned.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecapCertificationReport {
    pub(crate) runtime_id: String,
    pub(crate) status: String,
    pub(crate) capability_fingerprint: String,
}

/// Run the native observer on a blocking worker. Scope capture happens before
/// the worker starts so a workspace or identity switch can fence the final
/// grant projection.
#[tauri::command]
pub(crate) async fn certify_recap_runtime(
    app: tauri::AppHandle,
    request: RecapCertificationRequest,
) -> Result<RecapCertificationReport, String> {
    let owner_scope = crate::app_state::owner_scope::capture(app.clone()).await?;
    let base_dir = super::storage::managed_agents_base_dir(&app)?;
    let retention_db_path = super::retention::scoped_retention_db_path(
        &base_dir,
        &owner_scope.relay_url,
        &owner_scope.token.scope.owner,
    );
    tauri::async_runtime::spawn_blocking(move || {
        certify_recap_runtime_sync(app, owner_scope, retention_db_path, request)
    })
    .await
    .map_err(|_| "recap_task_failed".to_string())?
}

fn certify_recap_runtime_sync<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    owner_scope: CapturedOwnerScope,
    retention_db_path: PathBuf,
    request: RecapCertificationRequest,
) -> Result<RecapCertificationReport, String> {
    let runtime_id = canonical_runtime_id(&request.runtime_id)?;
    let runtime = super::known_acp_runtime_exact(&runtime_id)
        .ok_or_else(|| "runtime_mismatch".to_string())?;
    let contract = super::recap_service::contract_for_runtime(runtime.id)
        .ok_or_else(|| "unsupported_one_shot".to_string())?;
    let ownership = VerifiedStagingOwnership::load(&app)
        .map_err(|error| state_failure_code(error).to_string())?;
    crate::app_state::owner_scope::assert_current_blocking(app.clone(), &owner_scope.token)
        .map_err(|_| "scope_stale".to_string())?;

    let executable = resolve_executable(runtime.id, contract.command)?;
    let selection = resolve_selection(
        &ownership,
        contract.selection,
        request.requested_model.as_deref(),
        request.profile_ref.as_deref(),
    )?;
    probe_auth(runtime, &executable)?;
    let auth = bind_auth(&ownership, &runtime_id, &executable, &selection);

    let run = OwnedRecapRun::create(
        &ownership
            .recap_base()
            .map_err(|error| state_failure_code(error).to_string())?,
        unix_seconds(),
    )
    .map_err(|error| state_failure_code(error).to_string())?;
    let sentinel_path = run.path().join("probe-sentinel");
    let sentinel_before = write_sentinel(&sentinel_path)?;
    let input = PROBE_PROMPT.as_bytes();
    let plan = match build_probe_plan(
        contract.selection,
        &executable,
        run.path(),
        &selection,
        input,
    ) {
        Ok(plan) => plan,
        Err(error) => {
            let _ = run.cleanup();
            return Err(run_failure_code(error).to_string());
        }
    };
    if !plan.process_containment_available() {
        let _ = run.cleanup();
        return Err("unsupported_process_containment".to_string());
    }

    let (adapter, state, process) =
        run_probe(run, plan, input, &selection, &sentinel_path, &sentinel_before)?;
    let target = RecapProbeTarget {
        contract,
        runtime_id: runtime_id.clone(),
        executable: executable.clone(),
        selection: selection.clone(),
        auth,
    };
    certify_runtime_probe_for_captured_scope(
        &app,
        &ownership,
        &owner_scope.token,
        &retention_db_path,
        target,
        adapter,
        state,
        process,
        unix_seconds(),
    )
    .map_err(|error| state_failure_code(error).to_string())?;

    Ok(RecapCertificationReport {
        runtime_id,
        status: "certified".to_string(),
        capability_fingerprint: capability_fingerprint(&executable, &selection),
    })
}

fn canonical_runtime_id(value: &str) -> Result<String, String> {
    if value.is_empty()
        || value != value.trim()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err("runtime_mismatch".to_string());
    }
    Ok(value.to_ascii_lowercase())
}

fn resolve_executable(
    runtime_id: &str,
    command: Option<&'static str>,
) -> Result<RecapExecutableIdentity, String> {
    let command = command.ok_or_else(|| "unsupported_one_shot".to_string())?;
    let resolved = super::resolve_command(command)
        .and_then(|path| super::wiki_runtime::resolve_installed_wrapper(runtime_id, path))
        .ok_or_else(|| "missing_executable".to_string())?;
    let canonical = resolved
        .canonicalize()
        .map_err(|_| "missing_executable".to_string())?;
    let metadata = fs::symlink_metadata(&canonical)
        .map_err(|_| "invalid_executable_identity".to_string())?;
    if !metadata.is_file() || !is_executable(&canonical) {
        return Err("invalid_executable_identity".to_string());
    }
    let version = probe_version(&canonical)?;
    let fingerprint = hash_file(&canonical)?;
    Ok(RecapExecutableIdentity {
        resolved_path: canonical,
        version,
        fingerprint,
        platform: current_platform(),
    })
}

fn resolve_selection(
    ownership: &VerifiedStagingOwnership,
    contract: RecapSelectionContract,
    requested_model: Option<&str>,
    profile_ref: Option<&str>,
) -> Result<RecapSelection, String> {
    match contract {
        RecapSelectionContract::ExplicitModel => {
            if profile_ref.is_some() {
                return Err("profile_mismatch".to_string());
            }
            let model = requested_model
                .filter(|model| super::recap_capability::valid_recap_model(model))
                .ok_or_else(|| "invalid_model_selection".to_string())?;
            Ok(RecapSelection {
                model: model.to_string(),
                profile: None,
                profile_digest: None,
                profile_identity: None,
                auth_available: true,
            })
        }
        RecapSelectionContract::StagingProfile => {
            if requested_model.is_some() {
                return Err("invalid_model_selection".to_string());
            }
            let profile_ref = profile_ref
                .filter(|profile| *profile == profile.trim())
                .ok_or_else(|| "missing_profile".to_string())?;
            let profile = ownership
                .recap_profile_path(profile_ref)
                .map_err(|_| "missing_profile".to_string())?;
            let model = read_profile_model(&profile)?;
            let profile_digest = super::recap_adapter::profile_tree_digest(&profile)
                .map_err(|_| "profile_mismatch".to_string())?;
            let profile_identity = super::recap_adapter::profile_identity(&profile)
                .map_err(|_| "profile_mismatch".to_string())?;
            Ok(RecapSelection {
                model,
                profile: Some(profile),
                profile_digest: Some(profile_digest),
                profile_identity: Some(profile_identity),
                auth_available: true,
            })
        }
    }
}

fn read_profile_model(profile: &Path) -> Result<String, String> {
    const PROFILE_CONFIG_LIMIT: u64 = 64 * 1024;
    let config = profile.join("config.yaml");
    let metadata = fs::symlink_metadata(&config).map_err(|_| "invalid_model_selection".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > PROFILE_CONFIG_LIMIT
    {
        return Err("invalid_model_selection".to_string());
    }
    let mut bytes = Vec::new();
    File::open(&config)
        .map_err(|_| "invalid_model_selection".to_string())?
        .take(PROFILE_CONFIG_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "invalid_model_selection".to_string())?;
    if bytes.len() as u64 > PROFILE_CONFIG_LIMIT {
        return Err("invalid_model_selection".to_string());
    }
    let value: serde_yaml::Value = serde_yaml::from_slice(&bytes)
        .map_err(|_| "invalid_model_selection".to_string())?;
    let model = value
        .as_mapping()
        .and_then(|mapping| mapping.get(serde_yaml::Value::String("model".to_string())))
        .and_then(serde_yaml::Value::as_mapping)
        .and_then(|mapping| mapping.get(serde_yaml::Value::String("default".to_string())))
        .and_then(serde_yaml::Value::as_str)
        .ok_or_else(|| "invalid_model_selection".to_string())?;
    if !super::recap_capability::valid_recap_model(model) {
        return Err("invalid_model_selection".to_string());
    }
    Ok(model.to_string())
}

fn probe_auth(
    runtime: &super::discovery::KnownAcpRuntime,
    executable: &RecapExecutableIdentity,
) -> Result<(), String> {
    let args = runtime.auth_probe_args.ok_or_else(|| "auth_required".to_string())?;
    let auth_path = super::resolve_command(args[0])
        .and_then(|path| super::wiki_runtime::resolve_installed_wrapper(runtime.id, path))
        .and_then(|path| path.canonicalize().ok())
        .ok_or_else(|| "auth_required".to_string())?;
    if auth_path != executable.resolved_path {
        return Err("invalid_auth_binding".to_string());
    }
    let command = fixed_command(&auth_path, &args[1..])?;
    let outcome = bounded_output_with_policy(
        command,
        fixed_probe_policy(AUTH_PROBE_TIMEOUT),
        &AtomicBool::new(false),
    )
    .map_err(runner_failure_code)?;
    if !outcome.output.status.success() {
        return Err("auth_required".to_string());
    }
    match crate::managed_agents::readiness::cli_probe::classify_probe_output(
        &outcome.output.stderr,
        true,
    ) {
        crate::managed_agents::readiness::cli_probe::ProbeOutcome::LoggedIn => Ok(()),
        _ => Err("auth_required".to_string()),
    }
}

fn bind_auth(
    ownership: &VerifiedStagingOwnership,
    runtime_id: &str,
    executable: &RecapExecutableIdentity,
    selection: &RecapSelection,
) -> RecapAuthBinding {
    let profile = selection
        .profile
        .as_deref()
        .map(|path| path.to_string_lossy())
        .unwrap_or_default();
    let preimage = format!(
        "recap-auth-v1\0{}\0{}\0{}\0{}",
        runtime_id, executable.fingerprint, selection.model, profile
    );
    let fingerprint = hex::encode(Sha256::digest(preimage.as_bytes()));
    RecapAuthBinding {
        service: ownership.recap_keyring_service().to_string(),
        reference: format!("recap-auth-v1:{runtime_id}:{fingerprint}"),
    }
}

fn build_probe_plan(
    contract: RecapSelectionContract,
    executable: &RecapExecutableIdentity,
    root: &Path,
    selection: &RecapSelection,
    input: &[u8],
) -> Result<RecapLaunchPlan, RecapRunFailure> {
    match contract {
        RecapSelectionContract::ExplicitModel => claude_recap_plan_contained(
            &executable.resolved_path,
            root,
            &selection.model,
            input,
        ),
        RecapSelectionContract::StagingProfile => {
            let profile = selection
                .profile
                .as_deref()
                .ok_or(RecapRunFailure::ProfileUnavailable)?;
            hermes_recap_plan(
                &executable.resolved_path,
                root,
                &selection.model,
                profile,
                input,
            )
            .and_then(|plan| bind_hermes_prompt(plan, input))
        }
    }
}

fn run_probe(
    mut run: OwnedRecapRun,
    plan: RecapLaunchPlan,
    input: &[u8],
    selection: &RecapSelection,
    sentinel_path: &Path,
    sentinel_before: &str,
) -> Result<
    (
        RecapAdapterObservation,
        RecapStateObservation,
        RecapProcessObservation,
    ),
    String,
> {
    run.prepare_runtime_dirs()
        .map_err(|error| state_failure_code(error).to_string())?;
    let stdin = run
        .input(input)
        .map_err(|error| state_failure_code(error).to_string())?;
    run.mark_process_pending()
        .map_err(|error| state_failure_code(error).to_string())?;
    let mut command = plan.command();
    command.stdin(Stdio::from(stdin));
    let outcome = bounded_output_with_policy_and_spawn_hook(
        command,
        BoundedPolicy {
            timeout: RECAP_PROBE_TIMEOUT,
            budget: OutputBudget::PerStream {
                stdout: super::recap_adapter::RECAP_OUTPUT_LIMIT as u64,
                stderr: super::recap_adapter::RECAP_OUTPUT_LIMIT as u64,
            },
        },
        &AtomicBool::new(false),
        |pid| {
            run.mark_process_started(pid)
                .map_err(|_| BoundedFailure::Cleanup)
        },
    );
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(BoundedFailure::Cleanup) => return Err("process_ownership".to_string()),
        Err(error) => {
            finish_probe_run(run)?;
            return Err(runner_failure_code(error).to_string());
        }
    };
    let state = observe_state(selection, sentinel_path, sentinel_before);
    let adapter = plan
        .parse_probe_output(
            outcome.output.status.success(),
            &outcome.output.stdout,
            &outcome.output.stderr,
        )
        .map_err(|error| run_failure_code(error).to_string());
    let adapter = match adapter {
        Ok(adapter) => adapter,
        Err(error) => {
            finish_probe_run(run)?;
            return Err(error);
        }
    };
    let state = if adapter.tool_probe().probe_id != RECAP_TOOL_PROBE_ID
        || adapter.tool_probe().sentinel_before != sentinel_before
        || adapter.tool_probe().sentinel_after != sentinel_before
    {
        RecapStateObservation::Changed
    } else {
        state
    };
    finish_probe_run(run)?;
    Ok((
        adapter,
        state,
        RecapProcessObservation::ReapedAndContained,
    ))
}

fn finish_probe_run(mut run: OwnedRecapRun) -> Result<(), String> {
    run.mark_finished()
        .map_err(|error| state_failure_code(error).to_string())?;
    run.cleanup()
        .map_err(|error| state_failure_code(error).to_string())
}

fn observe_state(
    selection: &RecapSelection,
    sentinel_path: &Path,
    sentinel_before: &str,
) -> RecapStateObservation {
    let sentinel_after = read_sentinel(sentinel_path);
    let sentinel_ok = sentinel_after.as_deref() == Some(sentinel_before);
    let profile_ok = match (
        selection.profile.as_deref(),
        selection.profile_digest.as_deref(),
        selection.profile_identity.as_ref(),
    ) {
        (Some(profile), Some(digest), Some(identity)) => {
            super::recap_adapter::profile_tree_digest(profile).ok().as_deref()
                == Some(digest)
                && super::recap_adapter::profile_identity(profile)
                    .ok()
                    .as_ref()
                    == Some(identity)
        }
        (None, None, None) => true,
        _ => false,
    };
    if sentinel_ok && profile_ok {
        RecapStateObservation::Unchanged
    } else {
        RecapStateObservation::Changed
    }
}

fn write_sentinel(path: &Path) -> Result<String, String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "state_isolation".to_string())?;
    file.write_all(PROBE_SENTINEL)
        .and_then(|_| file.sync_all())
        .map_err(|_| "state_isolation".to_string())?;
    Ok(hex::encode(Sha256::digest(PROBE_SENTINEL)))
}

fn read_sentinel(path: &Path) -> Option<String> {
    let mut file = super::recap_state::private_read_file(path).ok()?;
    let metadata = file.metadata().ok()?;
    if metadata.len() != PROBE_SENTINEL.len() as u64 {
        return None;
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    (bytes == PROBE_SENTINEL).then(|| hex::encode(Sha256::digest(&bytes)))
}

fn probe_version(executable: &Path) -> Result<String, String> {
    let command = fixed_command(executable, &["--version"])?;
    let outcome = bounded_output_with_policy(
        command,
        fixed_probe_policy(VERSION_PROBE_TIMEOUT),
        &AtomicBool::new(false),
    )
    .map_err(runner_failure_code)?;
    if !outcome.output.status.success() {
        return Err("unknown_executable_version".to_string());
    }
    let bytes = if !outcome.output.stdout.is_empty() {
        outcome.output.stdout
    } else {
        outcome.output.stderr
    };
    let text = String::from_utf8(bytes).map_err(|_| "unknown_executable_version".to_string())?;
    let version = text.lines().map(str::trim).find(|line| !line.is_empty());
    let Some(version) = version else {
        return Err("unknown_executable_version".to_string());
    };
    if version.len() > 256 || version.chars().any(char::is_control) {
        return Err("unknown_executable_version".to_string());
    }
    Ok(version.to_string())
}

fn fixed_command(executable: &Path, args: &[&str]) -> Result<Command, String> {
    let mut command = if cfg!(target_os = "macos") {
        let sandbox = Path::new("/usr/bin/sandbox-exec");
        if !is_executable(sandbox) {
            return Err("unsupported_process_containment".to_string());
        }
        let mut command = Command::new(sandbox);
        command.args(["-p", MACOS_CONTAINMENT_PROFILE]);
        command.arg(executable);
        command
    } else if cfg!(target_os = "windows") {
        Command::new(executable)
    } else {
        return Err("unsupported_process_containment".to_string());
    };
    command.args(args).env_clear();
    command.env(
        "PATH",
        format!(
            "{}:/usr/bin:/bin",
            executable
                .parent()
                .unwrap_or_else(|| Path::new("/usr/bin"))
                .display()
        ),
    );
    if let Some(home) = dirs::home_dir() {
        command.env("HOME", home);
    }
    Ok(command)
}

fn fixed_probe_policy(timeout: Duration) -> BoundedPolicy {
    BoundedPolicy {
        timeout,
        budget: OutputBudget::PerStream {
            stdout: PROBE_OUTPUT_LIMIT,
            stderr: PROBE_OUTPUT_LIMIT,
        },
    }
}

fn hash_file(path: &Path) -> Result<String, String> {
    const MAX_BYTES: u64 = 512 * 1024 * 1024;
    let metadata = fs::symlink_metadata(path).map_err(|_| "invalid_executable_identity".to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err("invalid_executable_identity".to_string());
    }
    let mut file = File::open(path).map_err(|_| "invalid_executable_identity".to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "invalid_executable_identity".to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
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

fn current_platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn capability_fingerprint(
    executable: &RecapExecutableIdentity,
    selection: &RecapSelection,
) -> String {
    let profile = selection
        .profile
        .as_deref()
        .map(|path| path.to_string_lossy())
        .unwrap_or_default();
    hex::encode(Sha256::digest(
        format!(
            "v1\0{}\0{}\0{}\0{}",
            executable.resolved_path.display(), executable.fingerprint, selection.model, profile
        )
        .as_bytes(),
    ))
}

#[cfg(test)]
fn bind_auth_reference_is_opaque(reference: &str) -> bool {
    reference.starts_with("recap-auth-v1:")
        && reference.len() <= 256
        && !reference.chars().any(char::is_control)
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn runner_failure_code(error: BoundedFailure) -> &'static str {
    match error {
        BoundedFailure::Cancelled => "cancelled",
        BoundedFailure::Deadline => "deadline",
        BoundedFailure::AggregateLimit
        | BoundedFailure::StdoutLimit
        | BoundedFailure::StderrLimit => "output_limit",
        BoundedFailure::ProcessOwnership => "process_ownership",
        BoundedFailure::Pipe => "runner_pipe",
        BoundedFailure::Read => "runner_read",
        BoundedFailure::Wait => "runner_wait",
        BoundedFailure::Cleanup => "runner_cleanup",
        BoundedFailure::InvalidBounds => "runner_bounds",
    }
}

fn run_failure_code(error: RecapRunFailure) -> &'static str {
    match error {
        RecapRunFailure::InvalidSelection => "invalid_selection",
        RecapRunFailure::InputLimit => "input_limit",
        RecapRunFailure::OutputLimit => "output_limit",
        RecapRunFailure::NonzeroExit => "nonzero_exit",
        RecapRunFailure::InvalidOutput => "invalid_output",
        RecapRunFailure::ModelRequestedOnly => "model_mismatch",
        RecapRunFailure::StateIsolation => "state_isolation",
        RecapRunFailure::ProfileUnavailable => "profile_unavailable",
        RecapRunFailure::ProfileCopyLimit => "profile_copy_limit",
        RecapRunFailure::UnsupportedContainment => "unsupported_process_containment",
    }
}

fn state_failure_code(error: RecapStateFailure) -> &'static str {
    match error {
        RecapStateFailure::Io => "state_io",
        RecapStateFailure::InputLimit => "input_limit",
        RecapStateFailure::Ownership => "state_ownership",
        RecapStateFailure::RuntimeNotReady => "runtime_not_ready",
        RecapStateFailure::ProcessPending => "process_pending",
        RecapStateFailure::UnsupportedPlatform => "unsupported_platform",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_rejects_path_like_runtime_ids() {
        assert_eq!(canonical_runtime_id("../claude"), Err("runtime_mismatch".into()));
        assert_eq!(canonical_runtime_id("claude"), Ok("claude".into()));
    }

    #[test]
    fn auth_reference_is_opaque_and_bounded() {
        assert!(bind_auth_reference_is_opaque("recap-auth-v1:claude:abc"));
        assert!(!bind_auth_reference_is_opaque("/Users/me/.claude"));
        assert!(!bind_auth_reference_is_opaque(&"x".repeat(257)));
    }
}
