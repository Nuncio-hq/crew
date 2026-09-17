//! The only production producer of a positive private Ask capability.
//!
//! Discovery is not certification and a flag is not a denial. Every positive
//! dimension here is derived from a named field of a retained probe trace; no
//! dimension is inferred from an empty tool list, `--safe-mode`, a prompt, or
//! the fact that a process exited zero. Evidence that fails a check produces
//! [`ProofStatus::Unverified`] rather than an error, so the specific refusal the
//! viewer sees still comes from [`super::admit_private_ask`] — a probe can never
//! mint a capability the admission fences did not also accept.

use super::egress_proxy::EgressObservation;
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
    /// The fixture attempted to read a controlled file outside its run root.
    /// A read that was never attempted proves nothing about read isolation.
    pub(crate) read_outside_requested: bool,
    /// That read returned no bytes.
    pub(crate) read_outside_denied: bool,
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
    service: String,
    reference: String,
    auth_available: bool,
}

impl PrivateAskAuthEvidence {
    /// Record one observed authentication outcome.
    ///
    /// The fields are private and this is the only way in, so a caller cannot
    /// assert `auth_available` beside a reference describing something else.
    /// The production observation is
    /// [`super::credential::observe_auth_evidence`], which drives the real
    /// staging path — already gated on the attempt's proxy serving — and
    /// records its result. This dimension is deliberately independent of
    /// containment: folding it in made a broken sandbox report itself as an
    /// authentication failure.
    pub(crate) fn observed(service: String, reference: String, auth_available: bool) -> Self {
        Self {
            service,
            reference,
            auth_available,
        }
    }

    pub(crate) fn auth_available(&self) -> bool {
        self.auth_available
    }

    pub(crate) fn service(&self) -> &str {
        &self.service
    }

    pub(crate) fn reference(&self) -> &str {
        &self.reference
    }
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
    /// What the attempt's own loopback proxy observed. This is the ONLY input
    /// that can make `egress_bounded` positive.
    pub(crate) egress: EgressObservation,
    /// Unix seconds at which this trace was captured, and the per-run nonce it
    /// was captured under. Together they stop one retained trace from
    /// certifying forever, or from being replayed onto a different run.
    pub(crate) captured_at: u64,
    pub(crate) run_nonce: String,
    /// The verified managed-agent staging base the probe's run root sat under.
    ///
    /// LIMIT: this is carried by the trace rather than re-derived here, because
    /// a capability does not retain its probe and this producer has no handle
    /// on the process's `VerifiedStagingOwnership`. It rejects a probe captured
    /// in an unrelated directory; it does not by itself prove the base was the
    /// owned one. The launch path re-derives the real base independently.
    pub(crate) staging_base: PathBuf,
    /// Digest of the working checkout and external fixture state, bracketing
    /// the whole run. This is the no-side-effect observation.
    pub(crate) external_state_before: String,
    pub(crate) external_state_after: String,
    pub(crate) session_isolation: Option<SessionIsolationEvidence>,
    /// Digest of the probe program that actually ran.
    ///
    /// The probe runs a program this build ships rather than the selected
    /// runtime, because every dimension below is a property of the envelope —
    /// the Seatbelt text plus the loopback proxy — and the kernel denies an
    /// effect regardless of who attempts it. Recording what ran keeps that
    /// honest: `executable` is the runtime the policy was DERIVED from and is
    /// what this projection keys on, while this field says what was EXECUTED.
    /// A trace produced by a different probe program — one that attempted
    /// fewer things, or attempted them differently — no longer describes the
    /// evidence this build requires, and is refused rather than reinterpreted.
    pub(crate) probe_program_digest: String,
}

/// Structural reasons a probe cannot describe this selection at all. These are
/// distinct from an unverified dimension: the trace is about something else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrivateAskProbeRejection {
    /// The trace is older than [`PROBE_MAX_AGE`], carries no per-run nonce, or
    /// ran somewhere that is not under the verified staging base.
    StaleOrMisplacedProbe,
    /// The trace was produced by a probe program this build does not ship, so
    /// it does not describe the experiment this build's dimensions assume.
    ForeignProbeProgram,
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
        if !fresh_and_placed(&probe) {
            return Err(PrivateAskProbeRejection::StaleOrMisplacedProbe);
        }
        if probe.probe_program_digest != super::probe_program::probe_program_digest() {
            return Err(PrivateAskProbeRejection::ForeignProbeProgram);
        }

        // The probe's own proxy port is rebuilt into the expected policy. A
        // trace captured under a different port — or under the old any-host
        // rule — no longer reproduces this policy text and cannot certify it.
        let expected_profile = super::launch::runtime_directory(&probe.executable.resolved_path)
            .and_then(|directory| {
                super::containment::private_ask_containment_profile(
                    &probe.probe_run_root,
                    &directory,
                    &super::launch::extra_read_roots(&probe.executable.resolved_path),
                    probe.egress.proxy_port(),
                )
            })
            .ok();
        let containment_verified = expected_profile
            .is_some_and(|expected| expected == probe.containment_profile)
            && probe.tool_probe.surviving_descendants == 0;
        // Egress is projected from the proxy's own record and from nothing
        // else: no flag, no exit code, no absence of evidence.
        let egress_verified = containment_verified
            && probe.egress.bounds_egress()
            && probe.tool_probe.network_connections_observed == 0;
        let tool_isolation_verified =
            containment_verified && valid_tool_probe_evidence(&probe.tool_probe);
        // Read isolation is its own dimension: a policy can deny every write
        // and still let the child read the employee's worktree.
        let read_bounded_verified = containment_verified
            && probe.tool_probe.read_outside_requested
            && probe.tool_probe.read_outside_denied;
        let side_effect_free_verified = tool_isolation_verified
            && is_sha256(&probe.external_state_before)
            && probe.external_state_before == probe.external_state_after;
        // Authentication stays an independent dimension here so a refusal names
        // the thing that actually failed: folding egress into it would report a
        // broken containment as an authentication problem. The gate that ties a
        // credential to a bounded egress lives where the credential is created
        // — `credential::stage_private_ask_credential` refuses unless the
        // attempt's proxy is serving — which is the moment that matters.
        let authentication_verified = probe.auth.auth_available()
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
            egress_bounded: status(egress_verified),
            read_bounded: status(read_bounded_verified),
            process_containment: status(containment_verified),
            side_effect_free: status(side_effect_free_verified),
            independent_invocation: status(independent_verified),
        })
    }
}

/// How long one retained trace may certify a selection.
///
/// A capability is a statement about a machine, and a machine changes: a
/// runtime is upgraded, a credential expires, a proxy is no longer listening. A
/// trace with no expiry certifies forever, which is the failure this bound
/// exists to stop. It is generous rather than tight because re-probing is
/// expensive; the executable re-hash immediately before launch covers the fast-
/// moving part.
pub(crate) const PROBE_MAX_AGE: u64 = 24 * 60 * 60;

/// The trace is recent, carries a per-run nonce, and ran under the staging base.
///
/// A clock that cannot be read at all fails closed: without a `now` there is no
/// age, and an ageless trace is exactly the thing being refused. A trace stamped
/// in the future is refused for the same reason — it is not a reading of this
/// machine's clock.
fn fresh_and_placed(probe: &PrivateAskProbe) -> bool {
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    let now = now.as_secs();
    if probe.captured_at == 0 || probe.captured_at > now || now - probe.captured_at > PROBE_MAX_AGE
    {
        return false;
    }
    if !valid_scope_value(&probe.run_nonce) {
        return false;
    }
    probe.staging_base.is_absolute()
        && probe.probe_run_root.is_absolute()
        && probe.probe_run_root != probe.staging_base
        && probe.probe_run_root.starts_with(&probe.staging_base)
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
