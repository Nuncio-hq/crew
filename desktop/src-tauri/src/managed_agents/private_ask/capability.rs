//! The only production producer of a positive private Ask capability.
//!
//! Discovery is not certification and a flag is not a denial. Every positive
//! dimension here is derived from a named field of a retained probe trace; no
//! dimension is inferred from an empty tool list, `--safe-mode`, a prompt, or
//! the fact that a process exited zero. Evidence that fails a check produces
//! [`ProofStatus::Unverified`] rather than an error, so the specific refusal the
//! viewer sees still comes from [`super::admit_private_ask`] — a probe can never
//! mint a capability the admission fences did not also accept.

use super::prompt::config_fingerprint;
pub(crate) use super::session_evidence::SessionIsolationEvidence;
use super::validation::{is_hex64, valid_model, valid_profile, valid_scope_value};
use super::{PrivateAskCapability, ProofStatus, SelectedAgentState};
use crate::managed_agents::recap_capability::{same_executable_proof, RecapExecutableIdentity};
use std::path::PathBuf;

/// Stable identifier for the private Ask hostile-tool probe contract.
///
/// It is deliberately distinct from `RECAP_TOOL_PROBE_ID`: a recap probe denies
/// neither file effects nor network egress, so recap evidence must not be
/// replayed as a private Ask tool-isolation proof.
pub(crate) const PRIVATE_ASK_TOOL_PROBE_ID: &str = "crew-private-ask-hostile-tool-v1";

/// Observed outcome of one hostile-tool attempt under the production launch
/// recipe. The sentinel digests bracket the attempted effect; the counters are
/// recorded by the probe's own observer, not by the runtime under test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrivateAskToolProbe {
    pub(crate) probe_id: String,
    /// The hostile capability the fixture attempted, e.g. `write_file`.
    pub(crate) tool_name: String,
    /// The attempt was actually made. Evidence that never tried anything
    /// proves nothing about denial.
    pub(crate) request_observed: bool,
    /// The attempt was refused before it could change anything observable.
    pub(crate) denied_before_effect: bool,
    /// Digest of the controlled state outside the run root, before the attempt.
    pub(crate) sentinel_before: String,
    /// The same digest after the attempt and after teardown.
    pub(crate) sentinel_after: String,
    /// Connections the probe's own listener accepted from the contained child.
    pub(crate) network_connections_observed: u32,
    /// Descendants still alive after the bounded owner completed cleanup.
    /// A process-group escape is counted here, not assumed away.
    pub(crate) surviving_descendants: u32,
}

/// Native credential binding observed for the exact executable under test. The
/// reference is opaque and never carries the credential itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrivateAskAuthEvidence {
    pub(crate) service: String,
    pub(crate) reference: String,
    pub(crate) auth_available: bool,
}

/// One complete retained probe for a private Ask capability decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrivateAskProbe {
    pub(crate) runtime_id: String,
    pub(crate) executable: RecapExecutableIdentity,
    pub(crate) effective_model: String,
    pub(crate) profile: Option<String>,
    pub(crate) persona: String,
    pub(crate) acl_fingerprint: String,
    pub(crate) session_generation: String,
    pub(crate) auth: PrivateAskAuthEvidence,
    pub(crate) tool_probe: PrivateAskToolProbe,
    /// The exact Seatbelt policy text the probe ran under, and the run root it
    /// ran in. The expected policy is rebuilt here from that root by the same
    /// production function the launch plan uses, so a trace captured under a
    /// weaker hand-written profile cannot certify this recipe.
    pub(crate) containment_profile: String,
    pub(crate) probe_run_root: PathBuf,
    /// Digest of the working checkout and external fixture state, bracketing
    /// the whole run. This is the no-side-effect observation.
    pub(crate) external_state_before: String,
    pub(crate) external_state_after: String,
    pub(crate) session_isolation: Option<SessionIsolationEvidence>,
}

/// Structural reasons a probe cannot describe this selection at all. These are
/// distinct from an unverified dimension: the trace is about something else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrivateAskProbeRejection {
    RuntimeMismatch,
    ExecutableMismatch,
    SelectionMismatch,
    InvalidExecutableIdentity,
    InvalidModelSelection,
    MissingProfile,
    InvalidState,
}

impl PrivateAskCapability {
    /// Consume one retained probe and project it onto the selected agent.
    ///
    /// The returned capability is only as strong as the evidence: each
    /// dimension is `Verified` when its own named fields hold and `Unverified`
    /// otherwise. A structural mismatch is an error, because such a trace is
    /// not evidence about this selection in either direction.
    pub(crate) fn from_probe(
        state: &SelectedAgentState,
        probe: PrivateAskProbe,
    ) -> Result<Self, PrivateAskProbeRejection> {
        if probe.runtime_id != state.runtime_id
            || !matches!(probe.runtime_id.as_str(), "claude" | "hermes")
        {
            return Err(PrivateAskProbeRejection::RuntimeMismatch);
        }
        if !same_executable_proof(&probe.executable, &state.executable) {
            return Err(PrivateAskProbeRejection::ExecutableMismatch);
        }
        if probe.executable.platform != current_platform() {
            return Err(PrivateAskProbeRejection::InvalidExecutableIdentity);
        }
        if !valid_model(&probe.effective_model) {
            return Err(PrivateAskProbeRejection::InvalidModelSelection);
        }
        if probe.runtime_id == "hermes"
            && probe
                .profile
                .as_deref()
                .is_none_or(|profile| !valid_profile(profile))
        {
            return Err(PrivateAskProbeRejection::MissingProfile);
        }
        if probe.runtime_id == "claude" && probe.profile.is_some() {
            return Err(PrivateAskProbeRejection::SelectionMismatch);
        }
        if probe.effective_model != state.effective_model
            || probe.profile != state.profile
            || probe.persona != state.persona
            || probe.acl_fingerprint != state.acl_fingerprint
            || probe.session_generation != state.session_generation
        {
            return Err(PrivateAskProbeRejection::SelectionMismatch);
        }
        if !is_hex64(&probe.acl_fingerprint) || !valid_scope_value(&probe.session_generation) {
            return Err(PrivateAskProbeRejection::InvalidState);
        }

        let expected_profile =
            super::containment::private_ask_containment_profile(&probe.probe_run_root).ok();
        let containment_verified = expected_profile
            .is_some_and(|expected| expected == probe.containment_profile)
            && probe.tool_probe.surviving_descendants == 0;
        let tool_isolation_verified =
            containment_verified && valid_tool_probe_evidence(&probe.tool_probe);
        let side_effect_free_verified = tool_isolation_verified
            && is_sha256(&probe.external_state_before)
            && probe.external_state_before == probe.external_state_after;
        let authentication_verified = probe.auth.auth_available
            && valid_bounded_label(&probe.auth.service)
            && valid_bounded_label(&probe.auth.reference);
        let independent_verified = probe
            .session_isolation
            .as_ref()
            .is_some_and(SessionIsolationEvidence::is_verified);

        Ok(Self {
            runtime_id: probe.runtime_id,
            executable: probe.executable,
            effective_model: probe.effective_model,
            profile: probe.profile,
            config_fingerprint: config_fingerprint(
                &state.runtime_id,
                &state.effective_model,
                state.profile.as_deref(),
                &probe.persona,
            ),
            acl_fingerprint: probe.acl_fingerprint,
            session_generation: probe.session_generation,
            authentication: status(authentication_verified),
            tool_isolation: status(tool_isolation_verified),
            process_containment: status(containment_verified),
            side_effect_free: status(side_effect_free_verified),
            independent_invocation: status(independent_verified),
        })
    }
}

fn status(verified: bool) -> ProofStatus {
    if verified {
        ProofStatus::Verified
    } else {
        ProofStatus::Unverified
    }
}

/// The hostile attempt must have been made, refused before any effect, left the
/// controlled sentinel byte-identical, and opened no connection the probe's own
/// listener could accept.
fn valid_tool_probe_evidence(evidence: &PrivateAskToolProbe) -> bool {
    evidence.probe_id == PRIVATE_ASK_TOOL_PROBE_ID
        && valid_bounded_label(&evidence.tool_name)
        && evidence.request_observed
        && evidence.denied_before_effect
        && is_sha256(&evidence.sentinel_before)
        && evidence.sentinel_before == evidence.sentinel_after
        && evidence.network_connections_observed == 0
}

fn valid_bounded_label(value: &str) -> bool {
    !value.is_empty()
        && value == value.trim()
        && value.len() <= 256
        && !value.chars().any(char::is_control)
}

fn is_sha256(value: &str) -> bool {
    is_hex64(value)
}

fn current_platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}
