//! Native recap execution seam.
//!
//! This module is intentionally small and fail-closed. It accepts only a
//! native runtime-ready proof, creates one private disposable run, feeds the
//! prompt through that run's unlinked stdin, and uses the existing bounded
//! process owner for execution. The current staging receipt has no runtime
//! grant, so the command remains unavailable until one is issued.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::recap_adapter::{RecapLaunchPlan, RecapRunFailure};
use super::recap_capability::{
    admit_runtime_ready, same_executable_proof, verify_executable, RecapAdmission, RecapFailure,
    RecapRuntimeContract, RecapSelection, RecapSelectionContract,
};
use super::recap_ownership::VerifiedStagingOwnership;
use super::recap_state::{recover_recap_runs, OwnedRecapRun, RecapStateFailure};
use super::{
    bounded_output_with_policy_and_spawn_hook, BoundedFailure, BoundedPolicy, OutputBudget,
};

const RECAP_RUNTIME_ID: &str = "claude";
const RECAP_NATIVE_COMMAND: &str = "claude";
const RECAP_TIMEOUT: Duration = Duration::from_secs(120);
const RECAP_SETTINGS_FILENAME: &str = "recap-settings-v1.json";
const RECAP_CACHE_FILENAME: &str = "recap-cache-v1.json";
const RECAP_SETTINGS_VERSION: u8 = 1;
const RECAP_CACHE_VERSION: u8 = 1;
const RECAP_INPUT_LIMIT: u64 = 128 * 1024;
const RECAP_OUTPUT_LIMIT: u64 = 256 * 1024;
const RECAP_CLEANUP_GRACE_MS: u64 = 5_000;
const RECAP_CACHE_ENTRY_LIMIT: usize = 100;
const RECAP_CACHE_BYTES_LIMIT: usize = 10 * 1024 * 1024;

/// The renderer-facing recap settings contract. It is intentionally separate
/// from the employee-agent configuration and persists as one snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RecapMode {
    #[default]
    Off,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecapBounds {
    pub max_input_bytes: u64,
    pub max_output_bytes: u64,
    pub max_wall_time_ms: u64,
    pub cleanup_grace_ms: u64,
}

impl Default for RecapBounds {
    fn default() -> Self {
        Self {
            max_input_bytes: RECAP_INPUT_LIMIT,
            max_output_bytes: RECAP_OUTPUT_LIMIT,
            max_wall_time_ms: RECAP_TIMEOUT.as_millis() as u64,
            cleanup_grace_ms: RECAP_CLEANUP_GRACE_MS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecapSettings {
    pub version: u8,
    #[serde(default)]
    pub mode: RecapMode,
    pub runtime_id: Option<String>,
    pub requested_model: Option<String>,
    pub profile_ref: Option<String>,
    pub capability_fingerprint: Option<String>,
    pub bounds: RecapBounds,
}

impl Default for RecapSettings {
    fn default() -> Self {
        Self {
            version: RECAP_SETTINGS_VERSION,
            mode: RecapMode::Off,
            runtime_id: None,
            requested_model: None,
            profile_ref: None,
            capability_fingerprint: None,
            bounds: RecapBounds::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecapProfileOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecapRuntimeOption {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub availability: String,
    pub reason: Option<String>,
    pub capability_fingerprint: Option<String>,
    pub profiles: Vec<RecapProfileOption>,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecapSettingsSnapshot {
    pub settings: RecapSettings,
    pub runtimes: Vec<RecapRuntimeOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThreadRecapRequest {
    pub channel_id: String,
    pub root_event_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThreadRecapGenerationRequest {
    pub channel_id: String,
    pub root_event_id: String,
    pub generation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThreadRecap {
    pub generation_id: String,
    pub text: String,
    pub generated_at: u64,
    pub runtime_id: String,
    pub requested_model: Option<String>,
    pub effective_model: Option<String>,
    pub profile_ref: Option<String>,
    pub provenance: String,
    pub source_manifest_hash: String,
    pub source_event_ids: Vec<String>,
    pub omitted_message_count: u64,
    pub oldest_included_event_id: Option<String>,
    pub newest_included_event_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThreadRecapLookup {
    pub status: String,
    pub recap: Option<ThreadRecap>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecapCacheEntry {
    channel_id: String,
    root_event_id: String,
    status: String,
    recap: ThreadRecap,
    last_read_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecapCacheFile {
    version: u8,
    entries: Vec<RecapCacheEntry>,
}

type GenerationKey = (String, String, String);

fn active_generations() -> &'static Mutex<BTreeMap<GenerationKey, Arc<AtomicBool>>> {
    static ACTIVE: OnceLock<Mutex<BTreeMap<GenerationKey, Arc<AtomicBool>>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn default_settings() -> RecapSettings {
    RecapSettings::default()
}

fn app_data_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    tauri::Manager::path(app)
        .app_data_dir()
        .map_err(|_| "state_io".to_string())
}

fn settings_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join(RECAP_SETTINGS_FILENAME))
}

fn cache_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join(RECAP_CACHE_FILENAME))
}

fn validate_id(value: &str, field: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed != value
        || value.len() > 256
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(format!("invalid_{field}"));
    }
    Ok(())
}

fn validate_settings(
    settings: &RecapSettings,
    runtimes: &[RecapRuntimeOption],
) -> Result<(), String> {
    if settings.version != RECAP_SETTINGS_VERSION {
        return Err("settings_version_unsupported".to_string());
    }
    let defaults = RecapBounds::default();
    if settings.bounds.max_input_bytes == 0
        || settings.bounds.max_input_bytes > defaults.max_input_bytes
        || settings.bounds.max_output_bytes == 0
        || settings.bounds.max_output_bytes > defaults.max_output_bytes
        || settings.bounds.max_wall_time_ms == 0
        || settings.bounds.max_wall_time_ms > defaults.max_wall_time_ms
        || settings.bounds.cleanup_grace_ms == 0
        || settings.bounds.cleanup_grace_ms > defaults.cleanup_grace_ms
    {
        return Err("bounds_unsupported".to_string());
    }
    if matches!(settings.mode, RecapMode::Off) {
        return Ok(());
    }
    let Some(runtime_id) = settings.runtime_id.as_deref() else {
        return Err("missing_selection".to_string());
    };
    let Some(runtime) = runtimes.iter().find(|runtime| runtime.id == runtime_id) else {
        return Err("unsupported_runtime".to_string());
    };
    if runtime.availability != "supported" {
        return Err("unsupported_runtime".to_string());
    }
    if runtime.kind == "hermes" && settings.profile_ref.is_none() {
        return Err("missing_profile".to_string());
    }
    if runtime.kind == "hermes" {
        if settings
            .profile_ref
            .as_deref()
            .is_some_and(|profile| !runtime.profiles.iter().any(|option| option.id == profile))
        {
            return Err("unsupported_profile".to_string());
        }
    } else {
        if settings.profile_ref.is_some() {
            return Err("unsupported_profile".to_string());
        }
        if !runtime.models.is_empty() {
            let Some(model) = settings.requested_model.as_deref() else {
                return Err("missing_model".to_string());
            };
            if model.trim().is_empty() {
                return Err("missing_model".to_string());
            }
            if !runtime.models.iter().any(|candidate| candidate == model) {
                return Err("unsupported_model".to_string());
            }
        }
    }
    if let Some(fingerprint) = runtime.capability_fingerprint.as_deref() {
        if settings.capability_fingerprint.as_deref() != Some(fingerprint) {
            return Err("capability_changed".to_string());
        }
    }
    Ok(())
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "state_io".to_string())?;
    fs::create_dir_all(parent).map_err(|_| "state_io".to_string())?;
    let bytes = serde_json::to_vec_pretty(value).map_err(|_| "state_io".to_string())?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("recap"),
        std::process::id(),
        now_seconds()
    ));
    fs::write(&temporary, &bytes).map_err(|_| "state_io".to_string())?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("state_io:{error}"));
    }
    Ok(())
}

fn load_settings_from_path(path: &Path) -> Result<RecapSettings, String> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "settings_invalid".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(default_settings()),
        Err(_) => Err("state_io".to_string()),
    }
}

fn load_cache_from_path(path: &Path) -> Result<Vec<RecapCacheEntry>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("cache_io".to_string()),
    };
    if bytes.len() > RECAP_CACHE_BYTES_LIMIT {
        return Err("cache_limit".to_string());
    }
    let cache: RecapCacheFile =
        serde_json::from_slice(&bytes).map_err(|_| "cache_invalid".to_string())?;
    if cache.version != RECAP_CACHE_VERSION || cache.entries.len() > RECAP_CACHE_ENTRY_LIMIT {
        return Err("cache_version".to_string());
    }
    Ok(cache.entries)
}

fn runtime_options<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Vec<RecapRuntimeOption> {
    let ready = VerifiedStagingOwnership::load(app)
        .ok()
        .and_then(|ownership| ownership.runtime_ready_proof().ok());
    super::KNOWN_ACP_RUNTIMES
        .iter()
        .filter(|runtime| runtime.recap_contract().command.is_some())
        .map(|runtime| {
            let mut option = RecapRuntimeOption {
                id: runtime.id.to_string(),
                label: runtime.label.to_string(),
                kind: if runtime.profile_arg.is_some() {
                    "hermes".to_string()
                } else {
                    "cli".to_string()
                },
                availability: "unsupported".to_string(),
                reason: Some("runtime_not_ready".to_string()),
                capability_fingerprint: None,
                profiles: Vec::new(),
                models: Vec::new(),
            };
            if let Some(proof) = ready
                .as_ref()
                .filter(|proof| proof.runtime_id() == runtime.id)
            {
                let selection = proof.selection_for_service();
                option.availability = "supported".to_string();
                option.reason = None;
                option.capability_fingerprint = Some(proof.executable_fingerprint().to_string());
                option.models = vec![selection.model];
                if let Some(profile) = selection.profile {
                    option.profiles.push(RecapProfileOption {
                        id: profile.to_string_lossy().into_owned(),
                        label: profile.to_string_lossy().into_owned(),
                    });
                }
            }
            option
        })
        .collect()
}

fn settings_snapshot<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    settings: RecapSettings,
) -> Result<RecapSettingsSnapshot, String> {
    let runtimes = runtime_options(app);
    Ok(RecapSettingsSnapshot { settings, runtimes })
}

fn generation_key(request: &ThreadRecapGenerationRequest) -> Result<GenerationKey, String> {
    validate_id(&request.channel_id, "channel_id")?;
    validate_id(&request.root_event_id, "root_event_id")?;
    validate_id(&request.generation_id, "generation_id")?;
    Ok((
        request.channel_id.clone(),
        request.root_event_id.clone(),
        request.generation_id.clone(),
    ))
}

fn current_cache_entry(
    entries: &[RecapCacheEntry],
    request: &ThreadRecapRequest,
) -> Option<RecapCacheEntry> {
    entries
        .iter()
        .find(|entry| {
            entry.channel_id == request.channel_id && entry.root_event_id == request.root_event_id
        })
        .cloned()
}

/// Typed failures kept free of provider output and paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum RecapServiceFailure {
    Admission(RecapFailure),
    State(RecapStateFailure),
    Runner(BoundedFailure),
    Adapter(RecapRunFailure),
    PlanMismatch,
    Cleanup(RecapStateFailure),
}

/// Execute an already-admitted native plan with durable process state.
///
/// The caller must have built `plan` from the same admission. The check here
/// binds the plan's executable and model again at the production seam, so a
/// future adapter cannot accidentally pair a valid grant with another recipe.
#[allow(dead_code)]
pub(crate) fn execute_admitted_recap(
    admission: RecapAdmission,
    mut run: OwnedRecapRun,
    plan: RecapLaunchPlan,
    input: &[u8],
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<String, RecapServiceFailure> {
    if !plan.matches_admission(&admission) {
        return abort_before_start(run, RecapServiceFailure::PlanMismatch);
    }
    if let Err(error) = run.prepare_runtime_dirs() {
        return abort_before_start(run, RecapServiceFailure::State(error));
    }
    let stdin = match run.input(input) {
        Ok(stdin) => stdin,
        Err(error) => return abort_before_start(run, RecapServiceFailure::State(error)),
    };
    if let Err(error) = run.mark_process_pending() {
        return abort_before_start(run, RecapServiceFailure::State(error));
    }

    // Revalidate at the last production seam before handing the command to the
    // bounded spawner.  The grant is a content identity, so planning and run
    // directory setup cannot create a window in which an upgraded/replaced
    // binary is accepted under the old proof.  If this check fails the pending
    // manifest is retained for recovery and no child is spawned.
    let verified_executable = match verify_executable(&admission.executable) {
        Ok(identity) => identity,
        Err(error) => {
            return abort_after_pending_before_spawn(run, RecapServiceFailure::Admission(error))
        }
    };
    if !same_executable_proof(&verified_executable, &admission.executable)
        || plan.executable_path() != admission.executable.resolved_path
    {
        return abort_after_pending_before_spawn(
            run,
            RecapServiceFailure::Admission(RecapFailure::InvalidExecutableIdentity),
        );
    }

    let mut command = plan.command();
    command.stdin(Stdio::from(stdin));
    let result = bounded_output_with_policy_and_spawn_hook(
        command,
        BoundedPolicy {
            timeout,
            budget: OutputBudget::PerStream {
                stdout: super::recap_adapter::RECAP_OUTPUT_LIMIT as u64,
                stderr: super::recap_adapter::RECAP_OUTPUT_LIMIT as u64,
            },
        },
        cancelled,
        |pid| {
            run.mark_process_started(pid)
                .map_err(|_| BoundedFailure::Cleanup)
        },
    );
    let (result, process_state_is_known) = match result {
        Ok(outcome) => (
            plan.parse_output(
                outcome.output.status.success(),
                &outcome.output.stdout,
                &outcome.output.stderr,
            )
            .map_err(RecapServiceFailure::Adapter),
            true,
        ),
        Err(error) => {
            let known = !matches!(error, BoundedFailure::Cleanup);
            (Err(RecapServiceFailure::Runner(error)), known)
        }
    };

    // A cleanup failure means the runner could not prove ownership teardown;
    // preserve ProcessMayBeRunning for startup recovery instead of marking the
    // run finished and deleting the only retry record.
    if !process_state_is_known {
        return result;
    }
    if let Err(error) = run.mark_finished() {
        return Err(RecapServiceFailure::State(error));
    }
    match run.cleanup() {
        Ok(()) => result,
        Err(error) => Err(RecapServiceFailure::Cleanup(error)),
    }
}

#[allow(dead_code)]
fn abort_before_start<T>(
    run: OwnedRecapRun,
    error: RecapServiceFailure,
) -> Result<T, RecapServiceFailure> {
    match run.cleanup() {
        Ok(()) => Err(error),
        Err(cleanup) => Err(RecapServiceFailure::Cleanup(cleanup)),
    }
}

#[allow(dead_code)]
fn abort_after_pending_before_spawn<T>(
    mut run: OwnedRecapRun,
    error: RecapServiceFailure,
) -> Result<T, RecapServiceFailure> {
    // The pending phase is written before the final executable check so a
    // crash cannot turn an unknown spawn into an apparently clean run. Once
    // this synchronous check fails we know no child was handed to the runner,
    // so it is safe to close that phase and preserve the original typed error.
    if let Err(state) = run.mark_finished() {
        return Err(RecapServiceFailure::State(state));
    }
    abort_before_start(run, error)
}

fn contract_for_runtime(runtime_id: &str) -> Option<RecapRuntimeContract> {
    let runtime = super::known_acp_runtime_exact(runtime_id)?;
    let contract = runtime.recap_contract();
    (runtime.id == RECAP_RUNTIME_ID
        && contract.command == Some(RECAP_NATIVE_COMMAND)
        && contract.selection == RecapSelectionContract::ExplicitModel)
        .then_some(contract)
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn error_code(error: RecapServiceFailure) -> &'static str {
    match error {
        RecapServiceFailure::Admission(RecapFailure::RuntimeMismatch) => "runtime_mismatch",
        RecapServiceFailure::Admission(RecapFailure::ProfileMismatch) => "profile_mismatch",
        RecapServiceFailure::Admission(RecapFailure::MissingExecutable) => "missing_executable",
        RecapServiceFailure::Admission(RecapFailure::UnknownExecutableVersion) => {
            "unknown_executable_version"
        }
        RecapServiceFailure::Admission(RecapFailure::InvalidExecutableIdentity) => {
            "invalid_executable_identity"
        }
        RecapServiceFailure::Admission(RecapFailure::MissingProfile) => "missing_profile",
        RecapServiceFailure::Admission(RecapFailure::InvalidModelSelection) => {
            "invalid_model_selection"
        }
        RecapServiceFailure::Admission(RecapFailure::AuthRequired) => "auth_required",
        RecapServiceFailure::Admission(RecapFailure::UnsupportedOneShot) => "unsupported_one_shot",
        RecapServiceFailure::Admission(RecapFailure::UnsupportedToolIsolation) => {
            "unsupported_tool_isolation"
        }
        RecapServiceFailure::Admission(RecapFailure::UnsupportedStateIsolation) => {
            "unsupported_state_isolation"
        }
        RecapServiceFailure::Admission(RecapFailure::UnsupportedProcessContainment) => {
            "unsupported_process_containment"
        }
        RecapServiceFailure::Admission(RecapFailure::UnverifiedCapability) => {
            "unverified_capability"
        }
        RecapServiceFailure::State(RecapStateFailure::RuntimeNotReady) => "runtime_not_ready",
        RecapServiceFailure::State(RecapStateFailure::Io) => "state_io",
        RecapServiceFailure::State(RecapStateFailure::InputLimit) => "input_limit",
        RecapServiceFailure::State(RecapStateFailure::Ownership) => "state_ownership",
        RecapServiceFailure::State(RecapStateFailure::ProcessPending) => "process_pending",
        RecapServiceFailure::State(RecapStateFailure::UnsupportedPlatform) => {
            "unsupported_platform"
        }
        RecapServiceFailure::Runner(BoundedFailure::Cancelled) => "cancelled",
        RecapServiceFailure::Runner(BoundedFailure::Deadline) => "deadline",
        RecapServiceFailure::Runner(BoundedFailure::AggregateLimit) => "output_limit",
        RecapServiceFailure::Runner(BoundedFailure::StdoutLimit) => "stdout_limit",
        RecapServiceFailure::Runner(BoundedFailure::StderrLimit) => "stderr_limit",
        RecapServiceFailure::Runner(BoundedFailure::ProcessOwnership) => "process_ownership",
        RecapServiceFailure::Runner(BoundedFailure::Pipe) => "runner_pipe",
        RecapServiceFailure::Runner(BoundedFailure::Read) => "runner_read",
        RecapServiceFailure::Runner(BoundedFailure::Wait) => "runner_wait",
        RecapServiceFailure::Runner(BoundedFailure::Cleanup) => "runner_cleanup",
        RecapServiceFailure::Runner(BoundedFailure::InvalidBounds) => "runner_bounds",
        RecapServiceFailure::Adapter(RecapRunFailure::InvalidSelection) => "invalid_selection",
        RecapServiceFailure::Adapter(RecapRunFailure::InputLimit) => "input_limit",
        RecapServiceFailure::Adapter(RecapRunFailure::OutputLimit) => "output_limit",
        RecapServiceFailure::Adapter(RecapRunFailure::NonzeroExit) => "nonzero_exit",
        RecapServiceFailure::Adapter(RecapRunFailure::InvalidOutput) => "invalid_output",
        RecapServiceFailure::Adapter(RecapRunFailure::ModelRequestedOnly) => "model_mismatch",
        RecapServiceFailure::Adapter(RecapRunFailure::StateIsolation) => "state_isolation",
        RecapServiceFailure::PlanMismatch => "plan_mismatch",
        RecapServiceFailure::Cleanup(_) => "cleanup_required",
    }
}

/// Read the owner-local recap settings and backend-discovered runtime options.
#[tauri::command]
pub async fn get_recap_settings(app: tauri::AppHandle) -> Result<RecapSettingsSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = load_settings_from_path(&settings_path(&app)?)?;
        settings_snapshot(&app, settings)
    })
    .await
    .map_err(|_| "recap_task_failed".to_string())?
}

/// Persist one complete owner-local settings snapshot atomically.
#[tauri::command]
pub async fn save_recap_settings(
    app: tauri::AppHandle,
    settings: RecapSettings,
) -> Result<RecapSettingsSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let runtimes = runtime_options(&app);
        validate_settings(&settings, &runtimes)?;
        atomic_write_json(&settings_path(&app)?, &settings)?;
        Ok(RecapSettingsSnapshot { settings, runtimes })
    })
    .await
    .map_err(|_| "recap_task_failed".to_string())?
}

/// Read a cached recap for one exact channel/thread scope. A missing cache is
/// a normal `no_recap` state; malformed cache remains an explicit error.
#[tauri::command]
pub async fn get_thread_recap(
    app: tauri::AppHandle,
    channel_id: String,
    root_event_id: String,
) -> Result<ThreadRecapLookup, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let request = ThreadRecapRequest {
            channel_id,
            root_event_id,
        };
        validate_id(&request.channel_id, "channel_id")?;
        validate_id(&request.root_event_id, "root_event_id")?;
        let entries = load_cache_from_path(&cache_path(&app)?)?;
        match current_cache_entry(&entries, &request) {
            Some(entry) => Ok(ThreadRecapLookup {
                status: entry.status,
                recap: Some(entry.recap),
                reason: None,
            }),
            None => Ok(ThreadRecapLookup {
                status: "no_recap".to_string(),
                recap: None,
                reason: None,
            }),
        }
    })
    .await
    .map_err(|_| "recap_task_failed".to_string())?
}

fn generate_thread_recap_sync(
    app: tauri::AppHandle,
    request: ThreadRecapGenerationRequest,
    cancelled: Arc<AtomicBool>,
) -> Result<ThreadRecap, String> {
    let _key = generation_key(&request)?;
    if cancelled.load(Ordering::Acquire) {
        return Err("cancelled".to_string());
    }
    let settings = load_settings_from_path(&settings_path(&app)?)?;
    let snapshot = settings_snapshot(&app, settings.clone())?;
    validate_settings(&snapshot.settings, &snapshot.runtimes)?;
    if matches!(settings.mode, RecapMode::Off) {
        return Err("recap_off".to_string());
    }

    // Source collection is intentionally kept behind this command boundary;
    // until the authenticated relay source adapter is installed, no provider
    // is invoked and no synthetic recap is returned.
    let ownership = VerifiedStagingOwnership::load(&app)
        .map_err(|error| error_code(RecapServiceFailure::State(error)).to_string())?;
    let proof = ownership
        .runtime_ready_proof()
        .map_err(|error| error_code(RecapServiceFailure::State(error)).to_string())?;
    let contract = contract_for_runtime(
        settings
            .runtime_id
            .as_deref()
            .ok_or_else(|| "missing_selection".to_string())?,
    )
    .ok_or_else(|| "unsupported_runtime".to_string())?;
    let native_selection = proof.selection_for_service();
    let requested = RecapSelection {
        model: settings
            .requested_model
            .clone()
            .ok_or_else(|| "missing_selection".to_string())?,
        profile: settings.profile_ref.as_deref().map(PathBuf::from),
        ..native_selection
    };
    let _admission = admit_runtime_ready(
        settings.runtime_id.as_deref().unwrap_or_default(),
        contract,
        &proof,
        &requested,
    )
    .map_err(|error| error_code(RecapServiceFailure::Admission(error)).to_string())?;
    if cancelled.load(Ordering::Acquire) {
        return Err("cancelled".to_string());
    }
    Err("source_unavailable".to_string())
}

/// Generate one exact thread recap. The active generation registry is the
/// cancellation fence shared by the renderer's explicit Cancel action.
#[tauri::command]
pub async fn generate_thread_recap(
    app: tauri::AppHandle,
    channel_id: String,
    root_event_id: String,
    generation_id: String,
) -> Result<ThreadRecap, String> {
    let request = ThreadRecapGenerationRequest {
        channel_id,
        root_event_id,
        generation_id,
    };
    let key = generation_key(&request)?;
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut active = active_generations()
            .lock()
            .map_err(|_| "generation_registry".to_string())?;
        if active.contains_key(&key) {
            return Err("already_running".to_string());
        }
        active.insert(key.clone(), cancelled.clone());
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        generate_thread_recap_sync(app, request, cancelled)
    })
    .await
    .map_err(|_| "recap_task_failed".to_string())?;
    if let Ok(mut active) = active_generations().lock() {
        active.remove(&key);
    }
    result
}

/// Request cancellation of the exact owner-local generation. Cancellation is
/// idempotent so a pane that has already detached can safely retry the command.
#[tauri::command]
pub async fn cancel_thread_recap(
    channel_id: String,
    root_event_id: String,
    generation_id: String,
) -> Result<(), String> {
    let request = ThreadRecapGenerationRequest {
        channel_id,
        root_event_id,
        generation_id,
    };
    let key = generation_key(&request)?;
    if let Ok(active) = active_generations().lock() {
        if let Some(cancelled) = active.get(&key) {
            cancelled.store(true, Ordering::Release);
        }
    }
    Ok(())
}

/// Run the bounded disposable-state recovery sweep from the native startup
/// hook. A staging ownership receipt is optional for ordinary builds, so an
/// absent receipt is a quiet no-op; once present, every typed failure is
/// surfaced to the native log and the durable run records remain intact.
pub(crate) fn recover_recap_runs_at_boot<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use std::io::ErrorKind;
    use tauri::Manager;

    let app_data = match app.path().app_data_dir() {
        Ok(path) => path,
        Err(_) => {
            eprintln!("buzz-desktop: recap recovery skipped: state_io");
            return;
        }
    };
    match std::fs::symlink_metadata(app_data.join(super::recap_ownership::OWNERSHIP_FILENAME)) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => {
            eprintln!("buzz-desktop: recap recovery skipped: state_ownership");
            return;
        }
        Err(error) if error.kind() == ErrorKind::NotFound => return,
        Err(_) => {
            eprintln!("buzz-desktop: recap recovery skipped: state_ownership");
            return;
        }
    }

    let result = (|| {
        let ownership = VerifiedStagingOwnership::load(app)?;
        let base = ownership.recap_base()?;
        recover_recap_runs(&base, now_seconds())
    })();
    match result {
        Ok(report) => {
            if report.removed > 0 || report.pending_process > 0 || report.scan_limited {
                eprintln!(
                    "buzz-desktop: recap recovery removed={} pending={} preserved={} scan_limited={}",
                    report.removed, report.pending_process, report.preserved_unknown, report.scan_limited
                );
            }
        }
        Err(error) => eprintln!("buzz-desktop: recap recovery failed: {error:?}"),
    }
}

#[cfg(test)]
#[path = "recap_service/tests.rs"]
mod tests;
