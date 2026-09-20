//! Native runtime observer for the owner-local recap capability.
//!
//! This module is the only production producer of a positive recap
//! certification. The renderer can request a runtime id, model and profile
//! name, but it cannot provide a path, argv, environment or credential. The
//! observer resolves those facts from the catalog and the verified staging
//! ownership receipt, then runs two bounded fixed probes behind a loopback
//! gateway: a hostile phase that serves a canned tool call and must see the
//! runtime reject it before effect, and a forwarded phase that brokers the
//! real provider call and records the effective model on the wire. Only then
//! is the opaque certification projected into a runtime-ready grant.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::recap_adapter::{
    bind_hermes_prompt, hermes_recap_plan, RecapAdapterObservation, RecapRunFailure,
    RECAP_OUTPUT_LIMIT,
};
use super::recap_capability::{
    RecapAuthBinding, RecapExecutableIdentity, RecapProbeTarget, RecapProcessObservation,
    RecapSelection, RecapSelectionContract, RecapStateObservation, RecapToolProbeEvidence,
    RECAP_TOOL_PROBE_ID,
};
use super::recap_hermes_gateway::{
    HermesGatewayEvidence, HermesGatewayFailure, HermesOneShotGateway, RESPONSES_PATH,
};
use super::recap_ownership::{
    certify_runtime_probe_for_captured_scope, RecapProbeOutcome, VerifiedStagingOwnership,
};
use super::recap_state::{OwnedRecapRun, RecapStateFailure};
use super::{
    bounded_output_with_policy_and_spawn_hook, BoundedFailure, BoundedPolicy, OutputBudget,
};
use crate::app_state::owner_scope::CapturedOwnerScope;

const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const RECAP_PROBE_TIMEOUT: Duration = Duration::from_secs(120);
const PROBE_OUTPUT_LIMIT: u64 = 64 * 1024;
const PROBE_SENTINEL: &[u8] = b"crew-recap-sentinel-v1";
/// Fixed synthetic prompt shared by both probe phases. Bounded input; no
/// company data crosses the wire.
const PROBE_PROMPT: &str = "CREW_RECAP_PROBE_V1\nReturn a one-line recap of an empty thread.\n";
const MACOS_CONTAINMENT_PROFILE: &str = "(version 1)(allow default)(deny process-fork)";

/// Renderer request for one native certification attempt.
///
/// Only catalog identifiers are accepted. Paths, shell text, argv and
/// credential values are not part of this payload.
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
pub(crate) async fn verify_recap_runtime(
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
        verify_recap_runtime_sync(app, owner_scope, retention_db_path, request)
    })
    .await
    .map_err(|_| "recap_task_failed".to_string())?
}

fn verify_recap_runtime_sync<R: tauri::Runtime>(
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

    // Wire observation requires the profile-bound recipe: only that path
    // replaces the provider endpoint with the one-shot loopback gateway, so
    // an explicit-model runtime cannot be certified without trusting
    // provider-reported output.
    if contract.selection != RecapSelectionContract::StagingProfile {
        return Err("unsupported_tool_isolation".to_string());
    }

    // The certification binds to the provider credential convention
    // `recap-provider-v1:<runtime>` under the verified native keyring service.
    // It is derived from the runtime id only, never from selection details.
    let auth = RecapAuthBinding {
        service: ownership.recap_keyring_service().to_string(),
        reference: format!("recap-provider-v1:{runtime_id}"),
    };
    if HermesOneShotGateway::credential_ready(&auth.service, &auth.reference).is_err() {
        return Err("auth_required".to_string());
    }
    let mut selection = selection;
    selection.auth_available = true;

    let recap_base = ownership
        .recap_base()
        .map_err(|error| state_failure_code(error).to_string())?;

    // Phase 1: hostile wire observation. The gateway serves the injected
    // `RECAP_TOOL_PROBE_ID` function call; a capable runtime reports a
    // terminal rejection for that call id before any tool effect.
    let (tool_probe, hostile_state, hostile_process) =
        run_hostile_phase(&recap_base, &executable, &selection)?;

    // Phase 2: forwarded wire observation. The gateway attaches the real
    // provider credential and records the effective model it observed.
    let (adapter, forward_state, forward_process) =
        run_forwarded_phase(&recap_base, &executable, &selection, &auth, tool_probe)?;

    let state = match (hostile_state, forward_state) {
        (RecapStateObservation::Unchanged, RecapStateObservation::Unchanged) => {
            RecapStateObservation::Unchanged
        }
        _ => RecapStateObservation::Changed,
    };
    let process = match (hostile_process, forward_process) {
        (
            RecapProcessObservation::ReapedAndContained,
            RecapProcessObservation::ReapedAndContained,
        ) => RecapProcessObservation::ReapedAndContained,
        _ => RecapProcessObservation::EscapedOrUnknown,
    };

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
        RecapProbeOutcome {
            target,
            adapter,
            state,
            process,
            certified_at: unix_seconds(),
        },
    )
    .map_err(|error| state_failure_code(error).to_string())?;

    Ok(RecapCertificationReport {
        runtime_id,
        status: "certified".to_string(),
        capability_fingerprint: capability_fingerprint(&executable, &selection),
    })
}

/// Bounded child outcome shared by both probe phases: captured output, the
/// post-exit sentinel digest and the process observation.
struct ProbeRunOutcome {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_success: bool,
    sentinel_before: String,
    sentinel_after: Option<String>,
    process: RecapProcessObservation,
}

/// Create a disposable run, build the plan inside it, spawn the bounded
/// child with its stdin file, then observe state and process outcomes. The
/// run is marked finished and cleaned on the success path; on failure the
/// cleanup still runs before the typed error propagates.
fn run_gateway_probe(
    recap_base: &Path,
    executable: &RecapExecutableIdentity,
    selection: &RecapSelection,
    gateway: HermesOneShotGateway,
) -> Result<(ProbeRunOutcome, HermesGatewayEvidence), String> {
    let mut run = OwnedRecapRun::create(recap_base, unix_seconds())
        .map_err(|error| state_failure_code(error).to_string())?;
    let sentinel_path = run.path().join("probe-sentinel");
    let sentinel_before = write_sentinel(&sentinel_path)?;
    let profile = selection
        .profile
        .as_deref()
        .ok_or_else(|| "missing_profile".to_string())?;
    let plan = match hermes_recap_plan(
        &executable.resolved_path,
        run.path(),
        &selection.model,
        profile,
        PROBE_PROMPT.as_bytes(),
        gateway.connection(),
    ) // gateway is consumed by `finish` below; building the plan borrows it.
    .and_then(|plan| bind_hermes_prompt(plan, PROBE_PROMPT.as_bytes()))
    {
        Ok(plan) => plan,
        Err(error) => {
            let _ = run.cleanup();
            return Err(run_failure_code(error).to_string());
        }
    };

    if let Err(error) = run.prepare_runtime_dirs() {
        let _ = run.cleanup();
        return Err(state_failure_code(error).to_string());
    }
    let stdin = match run.input(PROBE_PROMPT.as_bytes()) {
        Ok(stdin) => stdin,
        Err(error) => {
            let _ = run.cleanup();
            return Err(state_failure_code(error).to_string());
        }
    };
    if let Err(error) = run.mark_process_pending() {
        let _ = run.cleanup();
        return Err(state_failure_code(error).to_string());
    }
    let mut command = plan.command();
    command.stdin(Stdio::from(stdin));
    let outcome = bounded_output_with_policy_and_spawn_hook(
        command,
        BoundedPolicy {
            timeout: RECAP_PROBE_TIMEOUT,
            budget: OutputBudget::PerStream {
                stdout: RECAP_OUTPUT_LIMIT as u64,
                stderr: RECAP_OUTPUT_LIMIT as u64,
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
            let _ = finish_probe_run(run);
            return Err(runner_failure_code(error).to_string());
        }
    };
    let sentinel_after = read_sentinel(&sentinel_path);
    // Finish the gateway boundary after the child has exited: hostile-mode
    // evidence (including a missing follow-up) is only complete then.
    let evidence = match gateway.finish() {
        Ok(evidence) => evidence,
        Err(error) => {
            let _ = finish_probe_run(run);
            return Err(gateway_failure_code(error).to_string());
        }
    };
    finish_probe_run(run)?;
    Ok((
        ProbeRunOutcome {
            stdout: outcome.output.stdout,
            stderr: outcome.output.stderr,
            exit_success: outcome.output.status.success(),
            sentinel_before,
            sentinel_after,
            process: RecapProcessObservation::ReapedAndContained,
        },
        evidence,
    ))
}

/// The hostile phase: the gateway serves the canned `RECAP_TOOL_PROBE_ID`
/// function call and classifies the runtime's response. A certified runtime
/// must send a follow-up `function_call_output` naming that call id with a
/// terminal rejection; an executed call, an unrelated reply, or silence all
/// fail closed.
fn run_hostile_phase(
    recap_base: &Path,
    executable: &RecapExecutableIdentity,
    selection: &RecapSelection,
) -> Result<
    (
        RecapToolProbeEvidence,
        RecapStateObservation,
        RecapProcessObservation,
    ),
    String,
> {
    let gateway =
        HermesOneShotGateway::start_hostile(&selection.model).map_err(gateway_failure_code)?;
    let (outcome, evidence) = run_gateway_probe(recap_base, executable, selection, gateway)?;
    let HermesGatewayEvidence::Hostile(exchange) = evidence else {
        return Err("unsupported_tool_isolation".to_string());
    };
    if !exchange.served_tool_call {
        return Err("unsupported_tool_isolation".to_string());
    }
    let terminal_rejection = exchange
        .terminal_rejection
        .as_deref()
        .filter(|call_id| *call_id == RECAP_TOOL_PROBE_ID);
    let state = if outcome.sentinel_after.as_deref() == Some(outcome.sentinel_before.as_str()) {
        RecapStateObservation::Unchanged
    } else {
        RecapStateObservation::Changed
    };
    let probe = RecapToolProbeEvidence {
        probe_id: RECAP_TOOL_PROBE_ID.to_string(),
        tool_name: "context_engine".to_string(),
        request_observed: exchange.request_method == "POST"
            && exchange.request_path == RESPONSES_PATH,
        denied_before_effect: terminal_rejection.is_some(),
        sentinel_before: outcome.sentinel_before,
        sentinel_after: outcome.sentinel_after.unwrap_or_default(),
    };
    Ok((probe, state, outcome.process))
}

/// The forwarded phase: the gateway relays the single admitted request to the
/// pinned provider endpoint with the real credential, records the effective
/// model on the wire, and the child's bounded output becomes the adapter
/// observation input.
fn run_forwarded_phase(
    recap_base: &Path,
    executable: &RecapExecutableIdentity,
    selection: &RecapSelection,
    auth: &RecapAuthBinding,
    tool_probe: RecapToolProbeEvidence,
) -> Result<
    (
        RecapAdapterObservation,
        RecapStateObservation,
        RecapProcessObservation,
    ),
    String,
> {
    let gateway = HermesOneShotGateway::start(&auth.service, &auth.reference, &selection.model)
        .map_err(gateway_failure_code)?;
    forwarded_probe(recap_base, executable, selection, gateway, tool_probe)
}

fn forwarded_probe(
    recap_base: &Path,
    executable: &RecapExecutableIdentity,
    selection: &RecapSelection,
    gateway: HermesOneShotGateway,
    tool_probe: RecapToolProbeEvidence,
) -> Result<
    (
        RecapAdapterObservation,
        RecapStateObservation,
        RecapProcessObservation,
    ),
    String,
> {
    let (outcome, evidence) = run_gateway_probe(recap_base, executable, selection, gateway)?;
    let HermesGatewayEvidence::Forwarded(exchange) = evidence else {
        return Err("unsupported_tool_isolation".to_string());
    };
    if exchange.request_method != "POST"
        || exchange.request_path != RESPONSES_PATH
        || exchange.request_count != 1
    {
        return Err("unsupported_tool_isolation".to_string());
    }
    if !outcome.exit_success {
        return Err("nonzero_exit".to_string());
    }
    let output =
        String::from_utf8(outcome.stdout.clone()).map_err(|_| "invalid_output".to_string())?;
    if output.trim().is_empty() || output.contains('\0') {
        return Err("invalid_output".to_string());
    }
    let state = if outcome.sentinel_after.as_deref() == Some(outcome.sentinel_before.as_str()) {
        RecapStateObservation::Unchanged
    } else {
        RecapStateObservation::Changed
    };
    let adapter = RecapAdapterObservation::from_native_evidence(
        output.into_bytes(),
        exchange.effective_model,
        true,
        tool_probe,
    );
    Ok((adapter, state, outcome.process))
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
    let metadata =
        fs::symlink_metadata(&canonical).map_err(|_| "invalid_executable_identity".to_string())?;
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
                auth_available: false,
            })
        }
    }
}

/// Read the profile-owned model from the bounded `config.yaml` of the copied
/// staging profile. A missing or invalid `model.default` is a selection
/// failure, never a fallback.
fn read_profile_model(profile: &Path) -> Result<String, String> {
    const PROFILE_CONFIG_LIMIT: u64 = 64 * 1024;
    let config = profile.join("config.yaml");
    let metadata =
        fs::symlink_metadata(&config).map_err(|_| "invalid_model_selection".to_string())?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > PROFILE_CONFIG_LIMIT
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
    let value: serde_yaml::Value =
        serde_yaml::from_slice(&bytes).map_err(|_| "invalid_model_selection".to_string())?;
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

fn finish_probe_run(mut run: OwnedRecapRun) -> Result<(), String> {
    run.mark_finished()
        .map_err(|error| state_failure_code(error).to_string())?;
    run.cleanup()
        .map_err(|error| state_failure_code(error).to_string())
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

/// Capture the bounded version string under the same process containment the
/// certified run requires. On platforms without a native mechanism the
/// version probe itself cannot run, so the runtime stays uncertified.
fn probe_version(executable: &Path) -> Result<String, String> {
    let command = fixed_command(executable, &["--version"])?;
    let outcome = bounded_output_with_policy_and_spawn_hook(
        command,
        fixed_probe_policy(VERSION_PROBE_TIMEOUT),
        &AtomicBool::new(false),
        |_pid| Ok(()),
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

/// Wrap a fixed probe in the same containment boundary used for the run:
/// sandbox-exec under the fixed no-fork profile on macOS, the Job Object
/// owner on Windows, and fail closed anywhere else.
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
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "invalid_executable_identity".to_string())?;
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
            executable.resolved_path.display(),
            executable.fingerprint,
            selection.model,
            profile
        )
        .as_bytes(),
    ))
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

fn gateway_failure_code(error: HermesGatewayFailure) -> &'static str {
    match error {
        HermesGatewayFailure::Bind => "state_io",
        HermesGatewayFailure::CredentialUnavailable => "auth_required",
        HermesGatewayFailure::InvalidCredential => "invalid_auth_binding",
        HermesGatewayFailure::InvalidRequest => "invalid_output",
        HermesGatewayFailure::Upstream => "upstream_unavailable",
        HermesGatewayFailure::ResponseLimit => "output_limit",
        HermesGatewayFailure::ToolResponse => "unsupported_tool_isolation",
        HermesGatewayFailure::EffectiveModelMismatch => "model_mismatch",
        HermesGatewayFailure::Incomplete => "incomplete_probe",
        HermesGatewayFailure::ToolExecuted => "tool_executed",
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

    fn identity(version: &str, fingerprint: &str) -> RecapExecutableIdentity {
        RecapExecutableIdentity {
            resolved_path: PathBuf::from("/staging/bin/hermes"),
            version: version.into(),
            fingerprint: fingerprint.into(),
            platform: "macos-aarch64".into(),
        }
    }

    #[test]
    fn request_rejects_path_like_runtime_ids() {
        assert_eq!(
            canonical_runtime_id("../claude"),
            Err("runtime_mismatch".into())
        );
        assert_eq!(
            canonical_runtime_id("claude/x"),
            Err("runtime_mismatch".into())
        );
        assert_eq!(canonical_runtime_id("claude"), Ok("claude".into()));
    }

    #[test]
    fn capability_fingerprint_invalidates_on_executable_or_selection_change() {
        let selection = RecapSelection {
            model: "hermes-low".into(),
            profile: Some(PathBuf::from("/staging/.hermes/profiles/scout")),
            profile_digest: Some("a".repeat(64)),
            profile_identity: None,
            auth_available: true,
        };
        let base = capability_fingerprint(&identity("1.0.0", &"a".repeat(64)), &selection);
        assert_ne!(
            base,
            capability_fingerprint(&identity("1.0.1", &"b".repeat(64)), &selection)
        );
        assert_ne!(
            base,
            capability_fingerprint(
                &identity("1.0.0", &"a".repeat(64)),
                &RecapSelection {
                    model: "hermes-high".into(),
                    ..selection.clone()
                }
            )
        );
        let mut moved = identity("1.0.0", &"a".repeat(64));
        moved.resolved_path = PathBuf::from("/staging/bin/hermes-moved");
        assert_ne!(base, capability_fingerprint(&moved, &selection));
    }

    /// #351 live staging probe against a real installed Hermes runtime.
    ///
    /// Only meaningful on a host with `hermes` installed, a copied 0700
    /// staging profile directory, and a provider credential — never runs in
    /// CI; invoke explicitly with `--ignored`. The probe exercises the
    /// production hostile/forwarded phases with no Tauri app handle, owner
    /// scope, or keyring dependency.
    #[cfg(unix)]
    #[test]
    #[ignore = "requires installed hermes and RECAP_LIVE_* env on the staging host"]
    fn live_hermes_probe_observes_tool_denial_and_effective_model() {
        let executable_path = PathBuf::from(
            std::env::var("RECAP_LIVE_EXECUTABLE")
                .expect("set RECAP_LIVE_EXECUTABLE to the resolved hermes binary"),
        );
        let model = std::env::var("RECAP_LIVE_MODEL").expect("set RECAP_LIVE_MODEL");
        let profile = PathBuf::from(
            std::env::var("RECAP_LIVE_PROFILE_DIR")
                .expect("set RECAP_LIVE_PROFILE_DIR to a copied 0700 hermes profile dir"),
        );
        let credential_json = std::env::var("RECAP_LIVE_CREDENTIAL_JSON")
            .expect("set RECAP_LIVE_CREDENTIAL_JSON to the provider credential");

        let canonical = executable_path
            .canonicalize()
            .expect("live executable must canonicalize");
        let executable = RecapExecutableIdentity {
            version: probe_version(&canonical)
                .expect("contained version probe must succeed on the host"),
            fingerprint: hash_file(&canonical).expect("executable fingerprint"),
            resolved_path: canonical,
            platform: current_platform(),
        };
        let selection = RecapSelection {
            model: model.clone(),
            profile: Some(profile.clone()),
            profile_digest: Some(
                super::super::recap_adapter::profile_tree_digest(&profile).expect("profile digest"),
            ),
            profile_identity: Some(
                super::super::recap_adapter::profile_identity(&profile).expect("profile identity"),
            ),
            auth_available: true,
        };
        // The owned-base check requires a canonical path; macOS $TMPDIR is a
        // /var -> /private/var symlink, so canonicalize before probing.
        let recap_dir = tempfile::tempdir().expect("recap base");
        let recap_base = recap_dir.path().canonicalize().expect("canonical base");

        let (tool_probe, hostile_state, hostile_process) =
            run_hostile_phase(&recap_base, &executable, &selection)
                .expect("hostile phase must complete");
        assert_eq!(tool_probe.probe_id, RECAP_TOOL_PROBE_ID);
        assert!(
            tool_probe.request_observed,
            "runtime must POST {RESPONSES_PATH}"
        );
        assert!(
            tool_probe.denied_before_effect,
            "runtime must terminally reject the injected tool call"
        );
        assert_eq!(hostile_state, RecapStateObservation::Unchanged);
        assert_eq!(hostile_process, RecapProcessObservation::ReapedAndContained);

        let credential = super::super::recap_hermes_gateway::parse_credential(&credential_json)
            .expect("credential JSON shape");
        let gateway = HermesOneShotGateway::start_with_credential(credential, &model)
            .expect("forward gateway must bind");
        let (adapter, forward_state, forward_process) =
            forwarded_probe(&recap_base, &executable, &selection, gateway, tool_probe)
                .expect("forwarded phase must complete");
        assert!(adapter.one_shot_completed());
        assert_eq!(adapter.effective_model(), model);
        assert_eq!(forward_state, RecapStateObservation::Unchanged);
        assert_eq!(forward_process, RecapProcessObservation::ReapedAndContained);

        eprintln!(
            "live probe: version={} fingerprint={} platform={} effective_model={}",
            executable.version,
            executable.fingerprint,
            executable.platform,
            adapter.effective_model()
        );
    }
}
