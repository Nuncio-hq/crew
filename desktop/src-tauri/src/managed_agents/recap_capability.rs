//! Recap-specific admission policy over the existing ACP runtime catalog.
//!
//! Discovery is not certification. In particular, an ACP read-only session
//! does not disable native tools, and a Unix process group does not contain
//! descendants which call setsid. No generation adapter is enabled here until
//! its exact executable, state, tool and process contracts have been proved.

use std::path::{Path, PathBuf};

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
#[derive(Debug, Clone)]
pub(crate) struct RecapSelection {
    pub model: String,
    pub profile: Option<PathBuf>,
    pub auth_available: bool,
}

/// Stable failure vocabulary for the executor and its disabled UI choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecapFailure {
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
