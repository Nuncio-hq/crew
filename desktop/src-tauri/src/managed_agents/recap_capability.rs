//! Recap-specific admission policy over the existing ACP runtime catalog.
//!
//! Discovery is not certification. In particular, an ACP read-only session
//! does not disable native tools, and a Unix process group does not contain
//! descendants which call setsid. No generation adapter is enabled here until
//! its exact executable, state, tool and process contracts have been proved.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::recap_adapter::RecapAdapterObservation;
#[cfg(test)]
use super::recap_adapter::RECAP_OUTPUT_LIMIT;

/// Runtime-owned selection contract, separate from employee session settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecapSelectionContract {
    ExplicitModel,
    StagingProfile,
}

/// Inventory candidate attached to KnownAcpRuntime; never a support claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RecapRuntimeContract {
    pub command: Option<&'static str>,
    pub selection: RecapSelectionContract,
}

/// Identity of the executable actually tested, not its display label or shim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapExecutableIdentity {
    pub resolved_path: PathBuf,
    pub version: String,
    pub fingerprint: String,
    pub platform: String,
}

/// Native identity of the profile directory bound by a runtime proof.
/// Content hashing detects edits; this identity also detects replacing the
/// directory with another same-content inode under the retained path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct RecapProfileIdentity {
    pub(crate) device: u64,
    pub(crate) inode: u64,
    pub(crate) owner: u32,
}

/// Explicit selection supplied by staging; an empty model cannot mean auto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapSelection {
    pub model: String,
    pub profile: Option<PathBuf>,
    /// Bounded digest of the profile tree observed by the native probe.
    ///
    /// This is separate from the path because a Hermes profile is mutable
    /// state. A retained path without its content binding could authorize a
    /// later replacement under the same name.
    pub profile_digest: Option<String>,
    /// Device/inode identity of the canonical profile directory.
    pub profile_identity: Option<RecapProfileIdentity>,
    pub auth_available: bool,
}

/// Native keyring binding carried by a probe. The reference is opaque and
/// never contains the credential itself; the native owner checks the service
/// name before issuing a runtime grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapAuthBinding {
    pub(crate) service: String,
    pub(crate) reference: String,
}

/// Evidence that a hostile native-tool attempt was observed and denied.
///
/// The adapter must populate this from its actual bounded probe trace. The
/// certification path does not infer tool isolation from an empty tool list,
/// a prompt, `--safe-mode`, or an ACP read-only flag. The sentinel digests are
/// the controlled before/after state around the attempted tool effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapToolProbeEvidence {
    pub(crate) probe_id: String,
    pub(crate) tool_name: String,
    pub(crate) request_observed: bool,
    pub(crate) denied_before_effect: bool,
    pub(crate) sentinel_before: String,
    pub(crate) sentinel_after: String,
}

/// Evidence returned by one bounded native adapter probe.
///
/// This is deliberately not a capability. A caller must pass it through
/// [`RecapRuntimeCertification::from_probe`] before it can be persisted or
/// used for admission. Raw output is consumed and reduced to a digest by the
/// certification constructor.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapProbeAttestation {
    runtime_id: String,
    executable: RecapExecutableIdentity,
    selection: RecapSelection,
    auth: RecapAuthBinding,
    adapter: RecapAdapterObservation,
    state: RecapStateObservation,
    process: RecapProcessObservation,
}

/// State observation produced by the adapter's before/after sentinel check.
/// The producer accepts only the positive observation; a changed or missing
/// snapshot is a typed negative result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecapStateObservation {
    Unchanged,
    Changed,
}

/// Process observation produced after the bounded owner has completed cleanup.
/// Unix group escape remains an explicit negative outcome until a stronger
/// containment boundary is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecapProcessObservation {
    ReapedAndContained,
    EscapedOrUnknown,
}

/// Capabilities that must be attested by the separately reviewed runtime
/// acceptance. A catalog entry or a successful discovery probe cannot create
/// this value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RecapGuarantees {
    pub one_shot: bool,
    pub tool_isolation: bool,
    pub state_isolation: bool,
    pub process_containment: bool,
}

/// Native proof loaded from the staging runtime-ready grant.
///
/// The fields are intentionally private. Callers can only obtain a proof from
/// the native ownership loader (or the test-only fixture constructor), then
/// must pass it through [`admit_runtime_ready`] before creating a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapRuntimeReadyProof {
    runtime_id: String,
    executable: RecapExecutableIdentity,
    selection: RecapSelection,
    auth_reference: String,
    guarantees: RecapGuarantees,
    auth_service: String,
}

/// Opaque positive certification created only from a completed bounded probe.
///
/// The type intentionally has no public field access and contains no provider
/// output. It is the only value accepted by the native grant producer; catalog
/// discovery, profile config reads, or an ownership receipt cannot construct a
/// positive result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapRuntimeCertification {
    runtime_id: String,
    executable: RecapExecutableIdentity,
    selection: RecapSelection,
    auth: RecapAuthBinding,
    guarantees: RecapGuarantees,
    effective_model: String,
    output_digest: String,
    tool_probe_digest: String,
}

/// Fields projected into the existing native grant and retention row after a
/// certification has passed all checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapCertificationParts {
    pub(crate) runtime_id: String,
    pub(crate) executable: RecapExecutableIdentity,
    pub(crate) selection: RecapSelection,
    pub(crate) auth: RecapAuthBinding,
    pub(crate) guarantees: RecapGuarantees,
    pub(crate) effective_model: String,
    pub(crate) output_digest: String,
    pub(crate) tool_probe_digest: String,
}

/// Native identity and selection supplied alongside one adapter observation.
/// This is request context, not a capability or a positive result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapProbeTarget {
    pub(crate) contract: RecapRuntimeContract,
    pub(crate) runtime_id: String,
    pub(crate) executable: RecapExecutableIdentity,
    pub(crate) selection: RecapSelection,
    pub(crate) auth: RecapAuthBinding,
}

/// The exact runtime and selection admitted for one recap invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapAdmission {
    pub runtime_id: String,
    pub executable: RecapExecutableIdentity,
    pub selection: RecapSelection,
}

/// Stable failure vocabulary for the executor and its disabled UI choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecapFailure {
    RuntimeMismatch,
    ProfileMismatch,
    MissingExecutable,
    UnknownExecutableVersion,
    InvalidExecutableIdentity,
    MissingProfile,
    InvalidModelSelection,
    AuthRequired,
    UnsupportedOneShot,
    UnsupportedToolIsolation,
    UnsupportedStateIsolation,
    UnsupportedProcessContainment,
    UnverifiedCapability,
    InvalidAuthBinding,
    EmptyProbeOutput,
    ProbeOutputLimit,
    InvalidProbeOutput,
    EffectiveModelMismatch,
    InvalidToolProbeEvidence,
    ProbeStateChanged,
    ProbeProcessNotReaped,
}

/// Bounded admission result. There is deliberately no synthetic supported row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapCapability {
    pub runtime_id: String,
    pub executable: Option<RecapExecutableIdentity>,
    pub failure: RecapFailure,
}

/// Classify a candidate without spawning a process or touching a profile.
pub(crate) fn classify_recap(
    runtime_id: &str,
    contract: RecapRuntimeContract,
    executable: Option<RecapExecutableIdentity>,
    selection: &RecapSelection,
) -> RecapCapability {
    let failure = admission_failure(contract, executable.as_ref(), selection);
    RecapCapability {
        runtime_id: runtime_id.to_owned(),
        executable,
        failure,
    }
}

impl RecapRuntimeReadyProof {
    /// Build a proof from a native runtime acceptance document.
    pub(super) fn from_grant(
        runtime_id: String,
        executable: RecapExecutableIdentity,
        selection: RecapSelection,
        auth_service: String,
        auth_reference: String,
        guarantees: RecapGuarantees,
    ) -> Result<Self, RecapFailure> {
        if runtime_id.trim().is_empty() || runtime_id != runtime_id.trim() {
            return Err(RecapFailure::RuntimeMismatch);
        }
        if !valid_identity(&executable) {
            return Err(RecapFailure::InvalidExecutableIdentity);
        }
        if executable.platform != current_platform() {
            return Err(RecapFailure::InvalidExecutableIdentity);
        }
        if !valid_recap_model(&selection.model) {
            return Err(RecapFailure::InvalidModelSelection);
        }
        if selection.profile.is_some()
            && !selection.profile_digest.as_deref().is_some_and(is_sha256)
        {
            return Err(RecapFailure::ProfileMismatch);
        }
        if selection.profile.is_none() && selection.profile_digest.is_some() {
            return Err(RecapFailure::ProfileMismatch);
        }
        if selection.profile.is_none() && selection.profile_identity.is_some() {
            return Err(RecapFailure::ProfileMismatch);
        }
        if selection.profile.is_some()
            && (selection.profile_digest.is_none() || selection.profile_identity.is_none())
        {
            return Err(RecapFailure::ProfileMismatch);
        }
        if !valid_auth_reference(&auth_reference) {
            return Err(RecapFailure::AuthRequired);
        }
        if !valid_auth_service(&auth_service) {
            return Err(RecapFailure::InvalidAuthBinding);
        }
        if selection.auth_available {
            Ok(Self {
                runtime_id,
                executable,
                selection,
                auth_reference,
                guarantees,
                auth_service,
            })
        } else {
            Err(RecapFailure::AuthRequired)
        }
    }

    /// Return the native-owned selection for the service to compare with a
    /// request. The service cannot provide a profile path or authentication
    /// state from the renderer.
    pub(super) fn selection_for_service(&self) -> RecapSelection {
        self.selection.clone()
    }

    pub(super) fn auth_binding_for_service(&self) -> RecapAuthBinding {
        RecapAuthBinding {
            service: self.auth_service.clone(),
            reference: self.auth_reference.clone(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        runtime_id: &str,
        executable: RecapExecutableIdentity,
        selection: RecapSelection,
        guarantees: RecapGuarantees,
    ) -> Self {
        Self {
            runtime_id: runtime_id.to_string(),
            executable,
            selection,
            auth_reference: "test-auth-reference".to_string(),
            guarantees,
            auth_service: "test-keyring-service".to_string(),
        }
    }

    #[cfg(test)]
    pub(crate) fn guarantees_for_test(&self) -> RecapGuarantees {
        self.guarantees
    }
}

impl RecapRuntimeCertification {
    /// Refuse provider output until a native observer binds it to the executed
    /// plan and independently records state/process outcomes.
    ///
    /// `parse_probe_output` only validates a bounded provider envelope. It is
    /// not evidence that the declared command, executable, selection, state or
    /// process observations actually occurred, so this seam intentionally
    /// cannot mint a positive certification today.
    pub(crate) fn from_adapter_observation(
        target: RecapProbeTarget,
        adapter: RecapAdapterObservation,
        state: RecapStateObservation,
        process: RecapProcessObservation,
    ) -> Result<Self, RecapFailure> {
        let _ = (target, adapter, state, process);
        Err(RecapFailure::UnverifiedCapability)
    }

    /// Construct a synthetic certificate for module tests only. Production
    /// code must use a future native observer rather than this fixture seam.
    #[cfg(test)]
    pub(crate) fn for_test_from_parts(parts: RecapCertificationParts) -> Self {
        Self {
            runtime_id: parts.runtime_id,
            executable: parts.executable,
            selection: parts.selection,
            auth: parts.auth,
            guarantees: parts.guarantees,
            effective_model: parts.effective_model,
            output_digest: parts.output_digest,
            tool_probe_digest: parts.tool_probe_digest,
        }
    }

    /// Consume one adapter probe and create the sole positive certification
    /// value. Every positive property is supplied as explicit evidence; this
    /// function never infers tool, state, or process guarantees from flags.
    #[cfg(test)]
    fn from_probe(
        contract: RecapRuntimeContract,
        probe: RecapProbeAttestation,
    ) -> Result<Self, RecapFailure> {
        let Some(runtime) = super::known_acp_runtime_exact(&probe.runtime_id) else {
            return Err(RecapFailure::RuntimeMismatch);
        };
        if runtime.recap_contract() != contract || contract.command.is_none() {
            return Err(RecapFailure::UnsupportedOneShot);
        }
        if !valid_identity(&probe.executable) {
            return Err(RecapFailure::InvalidExecutableIdentity);
        }
        if probe.executable.platform != current_platform() {
            return Err(RecapFailure::InvalidExecutableIdentity);
        }
        if !valid_recap_model(&probe.selection.model) {
            return Err(RecapFailure::InvalidModelSelection);
        }
        if probe.selection.profile.is_some()
            && !probe
                .selection
                .profile_digest
                .as_deref()
                .is_some_and(is_sha256)
        {
            return Err(RecapFailure::ProfileMismatch);
        }
        if probe.selection.profile.is_none() && probe.selection.profile_digest.is_some() {
            return Err(RecapFailure::ProfileMismatch);
        }
        if probe.selection.profile.is_none() && probe.selection.profile_identity.is_some() {
            return Err(RecapFailure::ProfileMismatch);
        }
        if !probe.selection.auth_available {
            return Err(RecapFailure::AuthRequired);
        }
        if !valid_auth_service(&probe.auth.service) || !valid_auth_reference(&probe.auth.reference)
        {
            return Err(RecapFailure::InvalidAuthBinding);
        }
        match contract.selection {
            RecapSelectionContract::ExplicitModel if probe.selection.profile.is_some() => {
                return Err(RecapFailure::ProfileMismatch);
            }
            RecapSelectionContract::StagingProfile
                if !probe
                    .selection
                    .profile
                    .as_deref()
                    .is_some_and(Path::is_absolute) =>
            {
                return Err(RecapFailure::MissingProfile);
            }
            RecapSelectionContract::StagingProfile if probe.selection.profile_digest.is_none() => {
                return Err(RecapFailure::ProfileMismatch);
            }
            RecapSelectionContract::StagingProfile
                if probe.selection.profile_identity.is_none() =>
            {
                return Err(RecapFailure::ProfileMismatch);
            }
            _ => {}
        }
        if let (Some(profile), Some(expected_digest)) = (
            probe.selection.profile.as_deref(),
            probe.selection.profile_digest.as_deref(),
        ) {
            let observed_digest = super::recap_adapter::profile_tree_digest(profile)
                .map_err(|_| RecapFailure::ProfileMismatch)?;
            if observed_digest != expected_digest {
                return Err(RecapFailure::ProfileMismatch);
            }
            if super::recap_adapter::profile_identity(profile)
                .ok()
                .as_ref()
                != probe.selection.profile_identity.as_ref()
            {
                return Err(RecapFailure::ProfileMismatch);
            }
        }
        if probe.adapter.output().is_empty() {
            return Err(RecapFailure::EmptyProbeOutput);
        }
        if probe.adapter.output().len() > RECAP_OUTPUT_LIMIT {
            return Err(RecapFailure::ProbeOutputLimit);
        }
        if probe.adapter.output().contains(&0) {
            return Err(RecapFailure::InvalidProbeOutput);
        }
        if !valid_recap_model(probe.adapter.effective_model()) {
            return Err(RecapFailure::EffectiveModelMismatch);
        }
        if probe.adapter.effective_model() != probe.selection.model {
            return Err(RecapFailure::EffectiveModelMismatch);
        }
        if !probe.adapter.one_shot_completed() {
            return Err(RecapFailure::UnsupportedOneShot);
        }
        if !valid_tool_probe_evidence(probe.adapter.tool_probe()) {
            return Err(RecapFailure::InvalidToolProbeEvidence);
        }
        if probe.state != RecapStateObservation::Unchanged {
            return Err(RecapFailure::ProbeStateChanged);
        }
        if probe.process != RecapProcessObservation::ReapedAndContained {
            return Err(RecapFailure::ProbeProcessNotReaped);
        }
        Ok(Self {
            runtime_id: probe.runtime_id,
            executable: probe.executable,
            selection: probe.selection,
            auth: probe.auth,
            guarantees: RecapGuarantees {
                one_shot: true,
                tool_isolation: true,
                state_isolation: true,
                process_containment: true,
            },
            effective_model: probe.adapter.effective_model().to_owned(),
            output_digest: hex::encode(Sha256::digest(probe.adapter.output())),
            tool_probe_digest: tool_probe_digest(probe.adapter.tool_probe()),
        })
    }

    pub(crate) fn parts(&self) -> RecapCertificationParts {
        RecapCertificationParts {
            runtime_id: self.runtime_id.clone(),
            executable: self.executable.clone(),
            selection: self.selection.clone(),
            auth: self.auth.clone(),
            guarantees: self.guarantees,
            effective_model: self.effective_model.clone(),
            output_digest: self.output_digest.clone(),
            tool_probe_digest: self.tool_probe_digest.clone(),
        }
    }
}

/// Admit a runtime only when the native proof and requested selection agree.
///
/// This is the sole positive-admission function. It deliberately does not
/// infer a model, profile, authentication state, or containment property.
pub(crate) fn admit_runtime_ready(
    runtime_id: &str,
    contract: RecapRuntimeContract,
    proof: &RecapRuntimeReadyProof,
    requested: &RecapSelection,
) -> Result<RecapAdmission, RecapFailure> {
    if contract.command.is_none() {
        return Err(RecapFailure::UnsupportedOneShot);
    }
    if runtime_id != proof.runtime_id {
        return Err(RecapFailure::RuntimeMismatch);
    }
    if !proof.guarantees.one_shot {
        return Err(RecapFailure::UnsupportedOneShot);
    }
    if !proof.guarantees.tool_isolation {
        return Err(RecapFailure::UnsupportedToolIsolation);
    }
    if !proof.guarantees.state_isolation {
        return Err(RecapFailure::UnsupportedStateIsolation);
    }
    if !proof.guarantees.process_containment {
        return Err(RecapFailure::UnsupportedProcessContainment);
    }
    if !valid_identity(&proof.executable) {
        return Err(RecapFailure::InvalidExecutableIdentity);
    }
    if !valid_auth_reference(&proof.auth_reference) || !proof.selection.auth_available {
        return Err(RecapFailure::AuthRequired);
    }
    if !valid_recap_model(&proof.selection.model) {
        return Err(RecapFailure::InvalidModelSelection);
    }
    if proof.selection.profile.is_some()
        && !proof
            .selection
            .profile_digest
            .as_deref()
            .is_some_and(is_sha256)
    {
        return Err(RecapFailure::ProfileMismatch);
    }
    if proof.selection.profile.is_none() && proof.selection.profile_digest.is_some() {
        return Err(RecapFailure::ProfileMismatch);
    }
    if proof.selection.profile.is_none() && proof.selection.profile_identity.is_some() {
        return Err(RecapFailure::ProfileMismatch);
    }
    match contract.selection {
        RecapSelectionContract::ExplicitModel if proof.selection.profile.is_some() => {
            // The explicit-model adapter has no profile argument. Accepting
            // a profile-bearing proof would silently ignore native selection.
            return Err(RecapFailure::ProfileMismatch);
        }
        RecapSelectionContract::StagingProfile
            if !proof
                .selection
                .profile
                .as_deref()
                .is_some_and(Path::is_absolute) =>
        {
            return Err(RecapFailure::MissingProfile);
        }
        RecapSelectionContract::StagingProfile if proof.selection.profile_digest.is_none() => {
            return Err(RecapFailure::ProfileMismatch);
        }
        RecapSelectionContract::StagingProfile if proof.selection.profile_identity.is_none() => {
            return Err(RecapFailure::ProfileMismatch);
        }
        _ => {}
    }
    if requested.auth_available != proof.selection.auth_available {
        return Err(RecapFailure::AuthRequired);
    }
    if requested.model != proof.selection.model {
        return Err(RecapFailure::InvalidModelSelection);
    }
    if requested.profile != proof.selection.profile {
        return Err(RecapFailure::ProfileMismatch);
    }
    if requested.profile_digest != proof.selection.profile_digest {
        return Err(RecapFailure::ProfileMismatch);
    }
    if requested.profile_identity != proof.selection.profile_identity {
        return Err(RecapFailure::ProfileMismatch);
    }
    Ok(RecapAdmission {
        runtime_id: proof.runtime_id.clone(),
        executable: proof.executable.clone(),
        selection: proof.selection.clone(),
    })
}

fn admission_failure(
    contract: RecapRuntimeContract,
    executable: Option<&RecapExecutableIdentity>,
    selection: &RecapSelection,
) -> RecapFailure {
    if contract.command.is_none() {
        return RecapFailure::UnsupportedOneShot;
    }
    let Some(executable) = executable else {
        return RecapFailure::MissingExecutable;
    };
    if unknown_version(&executable.version) {
        return RecapFailure::UnknownExecutableVersion;
    }
    if !valid_identity(executable) {
        return RecapFailure::InvalidExecutableIdentity;
    }
    if contract.selection == RecapSelectionContract::StagingProfile
        && !selection.profile.as_deref().is_some_and(Path::is_absolute)
    {
        return RecapFailure::MissingProfile;
    }
    if !valid_recap_model(&selection.model) {
        return RecapFailure::InvalidModelSelection;
    }
    if !selection.auth_available {
        return RecapFailure::AuthRequired;
    }
    RecapFailure::UnverifiedCapability
}

/// One explicit model value, never an option, automatic selection or control text.
pub(crate) fn valid_recap_model(model: &str) -> bool {
    !model.is_empty()
        && model == model.trim()
        && !model.eq_ignore_ascii_case("auto")
        && !model.starts_with('-')
        && !model.chars().any(char::is_control)
}

fn unknown_version(version: &str) -> bool {
    version.trim().is_empty() || version.trim().eq_ignore_ascii_case("unknown")
}

fn valid_identity(identity: &RecapExecutableIdentity) -> bool {
    identity.resolved_path.is_absolute()
        && !unknown_version(&identity.version)
        && identity.fingerprint.len() == 64
        && identity
            .fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        && !identity.platform.trim().is_empty()
}

fn current_platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn valid_auth_reference(reference: &str) -> bool {
    !reference.is_empty()
        && reference == reference.trim()
        && reference.len() <= 256
        && !reference.chars().any(char::is_control)
}

fn valid_auth_service(service: &str) -> bool {
    !service.is_empty()
        && service == service.trim()
        && service.len() <= 256
        && !service.chars().any(char::is_control)
}

/// Stable identifier for the hostile-tool probe evidence contract.
pub(crate) const RECAP_TOOL_PROBE_ID: &str = "crew-recap-hostile-tool-v1";

fn valid_tool_probe_evidence(evidence: &RecapToolProbeEvidence) -> bool {
    evidence.probe_id == RECAP_TOOL_PROBE_ID
        && !evidence.tool_name.is_empty()
        && evidence.tool_name == evidence.tool_name.trim()
        && evidence.tool_name.len() <= 128
        && !evidence.tool_name.chars().any(char::is_control)
        && evidence.request_observed
        && evidence.denied_before_effect
        && is_sha256(&evidence.sentinel_before)
        && evidence.sentinel_before == evidence.sentinel_after
}

fn tool_probe_digest(evidence: &RecapToolProbeEvidence) -> String {
    let preimage = format!(
        "{}\0{}\0{}\0{}\0{}\0{}",
        evidence.probe_id,
        evidence.tool_name,
        evidence.request_observed,
        evidence.denied_before_effect,
        evidence.sentinel_before,
        evidence.sentinel_after,
    );
    hex::encode(Sha256::digest(preimage.as_bytes()))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Recheck the retained executable fingerprint immediately before launch.
///
/// The grant records the version and platform observed during acceptance. The
/// executable bytes are hashed again at use time, and the declared path must
/// already be a canonical regular file. This avoids silently running a shim or
/// an upgraded binary under an old proof.
pub(crate) fn verify_executable(
    identity: &RecapExecutableIdentity,
) -> Result<RecapExecutableIdentity, RecapFailure> {
    const MAX_EXECUTABLE_BYTES: u64 = 512 * 1024 * 1024;

    if !valid_identity(identity) {
        return Err(RecapFailure::InvalidExecutableIdentity);
    }
    let canonical = identity
        .resolved_path
        .canonicalize()
        .map_err(|_| RecapFailure::MissingExecutable)?;
    if canonical != identity.resolved_path {
        return Err(RecapFailure::InvalidExecutableIdentity);
    }
    let metadata =
        std::fs::metadata(&identity.resolved_path).map_err(|_| RecapFailure::MissingExecutable)?;
    if !metadata.is_file() || metadata.len() > MAX_EXECUTABLE_BYTES {
        return Err(RecapFailure::InvalidExecutableIdentity);
    }
    let mut file =
        File::open(&identity.resolved_path).map_err(|_| RecapFailure::MissingExecutable)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| RecapFailure::InvalidExecutableIdentity)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let mut observed = identity.clone();
    observed.fingerprint = hex::encode(hasher.finalize());
    if !same_executable_proof(&observed, identity) {
        return Err(RecapFailure::InvalidExecutableIdentity);
    }
    Ok(observed)
}

/// Exact version/platform/path/content binding required for a retained proof.
pub(crate) fn same_executable_proof(
    observed: &RecapExecutableIdentity,
    retained: &RecapExecutableIdentity,
) -> bool {
    valid_identity(observed) && valid_identity(retained) && observed == retained
}

#[cfg(test)]
#[path = "recap_capability/tests.rs"]
mod tests;
