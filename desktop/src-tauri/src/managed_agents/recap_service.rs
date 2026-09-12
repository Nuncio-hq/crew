//! Native recap execution seam.
//!
//! This module is intentionally small and fail-closed. It accepts only a
//! native runtime-ready proof, creates one private disposable run, feeds the
//! prompt through that run's unlinked stdin, and uses the existing bounded
//! process owner for execution. The current staging receipt has no runtime
//! grant, so the command remains unavailable until one is issued.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
#[cfg(test)]
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tauri::Manager;

use super::recap_adapter::{
    bind_hermes_prompt, claude_recap_plan, hermes_recap_plan, RecapLaunchPlan, RecapRunFailure,
};
use super::recap_capability::{
    admit_runtime_ready, same_executable_proof, verify_executable, RecapAdmission, RecapFailure,
    RecapRuntimeContract, RecapRuntimeReadyProof, RecapSelection, RecapSelectionContract,
};
use super::recap_ownership::VerifiedStagingOwnership;
use super::recap_state::{recover_recap_runs, OwnedRecapRun, RecapStateFailure};
use super::{
    bounded_output_with_policy_and_spawn_hook, BoundedFailure, BoundedPolicy, OutputBudget,
};
use crate::app_state::owner_scope::CapturedOwnerScope;

const RECAP_TIMEOUT: Duration = Duration::from_secs(120);

#[cfg(test)]
fn test_scoped_proofs() -> &'static Mutex<Vec<RecapRuntimeReadyProof>> {
    static PROOFS: OnceLock<Mutex<Vec<RecapRuntimeReadyProof>>> = OnceLock::new();
    PROOFS.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(test)]
fn record_test_scoped_proof(proof: &RecapRuntimeReadyProof) {
    test_scoped_proofs()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(proof.clone());
}

#[cfg(test)]
pub(crate) fn clear_test_execution_observers() {
    test_scoped_proofs()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
    test_provider_launches().store(0, std::sync::atomic::Ordering::Release);
}

#[cfg(test)]
pub(crate) fn observed_test_scoped_proofs() -> Vec<RecapRuntimeReadyProof> {
    test_scoped_proofs()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

#[cfg(test)]
fn test_provider_launches() -> &'static std::sync::atomic::AtomicUsize {
    static LAUNCHES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    &LAUNCHES
}

#[cfg(test)]
pub(crate) fn observed_test_provider_launches() -> usize {
    test_provider_launches().load(std::sync::atomic::Ordering::Acquire)
}

#[cfg(test)]
fn record_test_provider_launch() {
    test_provider_launches().fetch_add(1, std::sync::atomic::Ordering::AcqRel);
}

/// Renderer request for one recap. The profile and authentication selection
/// are always read from the native runtime grant and cannot be supplied here.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct RecapRequest {
    pub(crate) runtime_id: String,
    pub(crate) model: String,
    pub(crate) profile_ref: Option<String>,
    pub(crate) input: String,
}

/// Immutable native execution context captured for one recap generation.
/// The retention path and proof belong to the same owner/relay snapshot as
/// the source; the service never resolves the active workspace again.
pub(crate) struct RecapExecutionScope {
    pub(crate) owner: CapturedOwnerScope,
    pub(crate) recap_base: PathBuf,
    pub(crate) retention_db_path: PathBuf,
    pub(crate) proof: RecapRuntimeReadyProof,
    /// Held only through the final child spawn, so workspace replacement
    /// cannot commit between the captured-scope check and process ownership.
    pub(crate) launch_workspace_guard: Option<tokio::sync::OwnedMutexGuard<()>>,
}

/// Typed failures kept free of provider output and paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[cfg(test)]
pub(crate) fn execute_admitted_recap(
    admission: RecapAdmission,
    run: OwnedRecapRun,
    plan: RecapLaunchPlan,
    input: &[u8],
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<String, RecapServiceFailure> {
    execute_admitted_recap_with_release(admission, run, plan, input, timeout, cancelled, || {})
}

/// Execute a plan while releasing a scope-launch lease after the child has
/// been secured. The lease is also dropped on every pre-spawn failure, so a
/// failed admission or spawn cannot strand the owner/workspace mutation locks.
fn execute_admitted_recap_with_release(
    admission: RecapAdmission,
    mut run: OwnedRecapRun,
    plan: RecapLaunchPlan,
    input: &[u8],
    timeout: Duration,
    cancelled: &AtomicBool,
    on_spawn_release: impl FnOnce(),
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
    if !plan.profile_matches_admission(&admission) {
        return abort_after_pending_before_spawn(
            run,
            RecapServiceFailure::Admission(RecapFailure::ProfileMismatch),
        );
    }

    let mut command = plan.command();
    command.stdin(Stdio::from(stdin));
    #[cfg(test)]
    record_test_provider_launch();
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
            let result = run
                .mark_process_started(pid)
                .map_err(|_| BoundedFailure::Cleanup);
            // The process owner has now secured the child. Release the
            // captured scope lease before any potentially long output wait.
            on_spawn_release();
            result
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

fn abort_before_start<T>(
    run: OwnedRecapRun,
    error: RecapServiceFailure,
) -> Result<T, RecapServiceFailure> {
    match run.cleanup() {
        Ok(()) => Err(error),
        Err(cleanup) => Err(RecapServiceFailure::Cleanup(cleanup)),
    }
}

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

pub(crate) fn contract_for_runtime(runtime_id: &str) -> Option<RecapRuntimeContract> {
    let runtime = super::known_acp_runtime_exact(runtime_id)?;
    let contract = runtime.recap_contract();
    match (runtime.id, contract.command, contract.selection) {
        ("claude", Some("claude"), RecapSelectionContract::ExplicitModel)
        | ("hermes", Some("hermes"), RecapSelectionContract::StagingProfile) => Some(contract),
        _ => None,
    }
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
        RecapServiceFailure::Admission(RecapFailure::InvalidAuthBinding) => "invalid_auth_binding",
        RecapServiceFailure::Admission(RecapFailure::EmptyProbeOutput) => "empty_probe_output",
        RecapServiceFailure::Admission(RecapFailure::ProbeOutputLimit) => "probe_output_limit",
        RecapServiceFailure::Admission(RecapFailure::InvalidProbeOutput) => "invalid_probe_output",
        RecapServiceFailure::Admission(RecapFailure::EffectiveModelMismatch) => {
            "effective_model_mismatch"
        }
        RecapServiceFailure::Admission(RecapFailure::InvalidToolProbeEvidence) => {
            "invalid_tool_probe_evidence"
        }
        RecapServiceFailure::Admission(RecapFailure::ProbeStateChanged) => "probe_state_changed",
        RecapServiceFailure::Admission(RecapFailure::ProbeProcessNotReaped) => {
            "probe_process_not_reaped"
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
        RecapServiceFailure::Adapter(RecapRunFailure::ProfileUnavailable) => "profile_unavailable",
        RecapServiceFailure::Adapter(RecapRunFailure::ProfileCopyLimit) => "profile_copy_limit",
        RecapServiceFailure::Adapter(RecapRunFailure::UnsupportedContainment) => {
            "unsupported_containment"
        }
        RecapServiceFailure::PlanMismatch => "plan_mismatch",
        RecapServiceFailure::Cleanup(_) => "cleanup_required",
    }
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

pub(crate) fn run_recap_sync_with_cancel<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: RecapExecutionScope,
    request: RecapRequest,
    cancelled: Arc<AtomicBool>,
) -> Result<String, String> {
    let mut launch_workspace_guard = scope.launch_workspace_guard;
    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
        return Err("cancelled".to_string());
    }
    if scope.owner.token.scope.owner != scope.owner.keys.public_key().to_hex() {
        return Err(error_code(RecapServiceFailure::State(
            RecapStateFailure::RuntimeNotReady,
        ))
        .to_string());
    }
    let ownership = VerifiedStagingOwnership::load(&app)
        .map_err(|error| error_code(RecapServiceFailure::State(error)).to_string())?;
    let captured_base = ownership
        .recap_base()
        .map_err(|error| error_code(RecapServiceFailure::State(error)).to_string())?;
    if captured_base != scope.recap_base {
        return Err(error_code(RecapServiceFailure::State(
            RecapStateFailure::RuntimeNotReady,
        ))
        .to_string());
    }
    let scoped_proof = super::recap_ownership::runtime_ready_proof_for_captured_scope(
        &ownership,
        &scope.retention_db_path,
    )
    .map_err(|error| error_code(RecapServiceFailure::State(error)).to_string())?;
    #[cfg(test)]
    record_test_scoped_proof(&scoped_proof);
    if scoped_proof != scope.proof {
        return Err(error_code(RecapServiceFailure::State(
            RecapStateFailure::RuntimeNotReady,
        ))
        .to_string());
    }
    let proof = scoped_proof;
    let contract = contract_for_runtime(&request.runtime_id).ok_or_else(|| {
        error_code(RecapServiceFailure::Admission(
            RecapFailure::RuntimeMismatch,
        ))
        .to_string()
    })?;
    let native_selection = proof.selection_for_service();
    let requested_profile = match contract.selection {
        RecapSelectionContract::ExplicitModel => {
            if request.profile_ref.is_some() {
                return Err(error_code(RecapServiceFailure::Admission(
                    RecapFailure::ProfileMismatch,
                ))
                .to_string());
            }
            None
        }
        RecapSelectionContract::StagingProfile => {
            let Some(requested_name) = request.profile_ref.as_deref() else {
                return Err(error_code(RecapServiceFailure::Admission(
                    RecapFailure::MissingProfile,
                ))
                .to_string());
            };
            if super::hermes_profile::validate_hermes_profile_name(requested_name).is_err() {
                return Err(error_code(RecapServiceFailure::Admission(
                    RecapFailure::ProfileMismatch,
                ))
                .to_string());
            }
            let Some(native_profile) = native_selection.profile.as_deref() else {
                return Err(error_code(RecapServiceFailure::Admission(
                    RecapFailure::MissingProfile,
                ))
                .to_string());
            };
            let native_name =
                super::recap_adapter::hermes_profile_ref(native_profile).ok_or_else(|| {
                    error_code(RecapServiceFailure::Admission(RecapFailure::MissingProfile))
                        .to_string()
                })?;
            if native_name != requested_name {
                return Err(error_code(RecapServiceFailure::Admission(
                    RecapFailure::ProfileMismatch,
                ))
                .to_string());
            }
            Some(native_profile.to_owned())
        }
    };
    let requested = RecapSelection {
        model: request.model,
        profile: requested_profile,
        ..native_selection
    };
    let mut admission = admit_runtime_ready(&request.runtime_id, contract, &proof, &requested)
        .map_err(|error| error_code(RecapServiceFailure::Admission(error)).to_string())?;
    let executable = verify_executable(&admission.executable)
        .map_err(|error| error_code(RecapServiceFailure::Admission(error)).to_string())?;
    if !same_executable_proof(&executable, &admission.executable) {
        return Err(error_code(RecapServiceFailure::Admission(
            RecapFailure::InvalidExecutableIdentity,
        ))
        .to_string());
    }
    admission.executable = executable;

    let run = OwnedRecapRun::create(&scope.recap_base, now_seconds())
        .map_err(|error| error_code(RecapServiceFailure::State(error)).to_string())?;
    let input = request.input.into_bytes();
    let plan = match match contract.selection {
        RecapSelectionContract::ExplicitModel => claude_recap_plan(
            &admission.executable.resolved_path,
            run.path(),
            &admission.selection.model,
            &input,
        ),
        RecapSelectionContract::StagingProfile => {
            let Some(profile) = admission.selection.profile.as_deref() else {
                return abort_before_start(
                    run,
                    RecapServiceFailure::Admission(RecapFailure::MissingProfile),
                )
                .map_err(|error| error_code(error).to_string());
            };
            hermes_recap_plan(
                &admission.executable.resolved_path,
                run.path(),
                &admission.selection.model,
                profile,
                &input,
            )
            .and_then(|plan| bind_hermes_prompt(plan, &input))
        }
    } {
        Ok(plan) => plan,
        Err(error) => {
            return abort_before_start(run, RecapServiceFailure::Adapter(error))
                .map_err(|error| error_code(error).to_string())
        }
    };
    let state = app.state::<crate::app_state::AppState>();
    // Identity imports use this mutex for every key replacement. Hold it
    // together with the workspace guard through the synchronous final scope
    // check and child spawn, then release both from the runner's on-spawn hook.
    let identity_guard = match state.identity_mutation.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return abort_before_start(
                run,
                RecapServiceFailure::State(RecapStateFailure::RuntimeNotReady),
            )
            .map_err(|error| error_code(error).to_string())
        }
    };
    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
        return abort_before_start(run, RecapServiceFailure::Runner(BoundedFailure::Cancelled))
            .map_err(|error| error_code(error).to_string());
    }
    if crate::app_state::owner_scope::assert_current_blocking(app.clone(), &scope.owner.token)
        .is_err()
    {
        drop(identity_guard);
        return abort_before_start(
            run,
            RecapServiceFailure::State(RecapStateFailure::RuntimeNotReady),
        )
        .map_err(|error| error_code(error).to_string());
    }
    let release_launch_lease = move || {
        drop(identity_guard);
        drop(launch_workspace_guard.take());
    };
    execute_admitted_recap_with_release(
        admission,
        run,
        plan,
        &input,
        RECAP_TIMEOUT,
        cancelled.as_ref(),
        release_launch_lease,
    )
    .map_err(|error| error_code(error).to_string())
}

#[cfg(test)]
#[path = "recap_service/tests.rs"]
mod tests;
