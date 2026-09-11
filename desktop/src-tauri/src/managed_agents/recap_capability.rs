//! Recap-specific admission policy over the existing ACP runtime catalog.
//!
//! Discovery is not certification. In particular, an ACP read-only session
//! does not disable native tools, and a Unix process group does not contain
//! descendants which call setsid. No generation adapter is enabled here until
//! its exact executable, state, tool and process contracts have been proved.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

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

/// Explicit selection supplied by staging; an empty model cannot mean auto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecapSelection {
    pub model: String,
    pub profile: Option<PathBuf>,
    pub auth_available: bool,
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
        if !valid_auth_reference(&auth_reference) {
            return Err(RecapFailure::AuthRequired);
        }
        if selection.auth_available {
            Ok(Self {
                runtime_id,
                executable,
                selection,
                auth_reference,
                guarantees,
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

    pub(super) fn runtime_id(&self) -> &str {
        &self.runtime_id
    }

    pub(super) fn executable_fingerprint(&self) -> &str {
        &self.executable.fingerprint
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
        }
    }

    #[cfg(test)]
    pub(crate) fn guarantees_for_test(&self) -> RecapGuarantees {
        self.guarantees
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
