//! What turns a natively resolved selection into an answer.
//!
//! Everything this module needs has already been decided by a native producer:
//! the selected agent's own effective configuration, its executable identity,
//! the verified snapshot the question is grounded in, and the owned staging
//! tree. Nothing here resolves any of that from a request — its job is the
//! sequencing that used to be missing between "there is a selection" and "there
//! is an answer":
//!
//! 1. take the retained capability probe for this exact selection, or capture a
//!    fresh one and persist it;
//! 2. project it onto the selection with
//!    [`super::capability::PrivateAskCapability::from_probe`];
//! 3. admit, which is where every fence lives;
//! 4. run the attempt, which applies the citation fence to its own answer.
//!
//! A probe is expensive, so a receipt is reused — but only while it is fresh.
//! `probe_receipt::load` returns nothing for a receipt past `PROBE_MAX_AGE`, so
//! staleness is a re-probe rather than a refusal, and an expired trace can
//! never certify anything.
//!
//! NAMED LIMIT, deliberate: a receipt does not carry session-isolation
//! evidence, so a selection answered from a retained receipt leaves
//! `independent_invocation` unverified. A *busy* agent is therefore refused
//! (`AgentBusy`) unless this attempt captured its own probe beside that live
//! session. That is the fail-closed direction.

use super::attempt::AttemptIdentity;
use super::session_evidence::SessionObservation;
use super::{
    admit_private_ask, probe_program, probe_receipt, PrivateAskAttempt, PrivateAskCapability,
    PrivateAskFailure, PrivateAskRequest, PrivateAskResponse, SelectedAgentState,
};
use crate::managed_agents::recap_ownership::VerifiedStagingOwnership;
use std::path::PathBuf;

/// One resolved selection, ready to be answered.
pub(super) struct PrivateAskBinding {
    /// Owned, because the attempt consumes it: the run root belongs to this
    /// install's verified staging tree and to nothing else.
    pub(super) ownership: VerifiedStagingOwnership,
    pub(super) state: SelectedAgentState,
    pub(super) request: PrivateAskRequest,
    /// `None` when the selected agent has no live session to bracket. A probe
    /// captured without one leaves `independent_invocation` unverified, which
    /// only matters for a busy agent.
    pub(super) session: Option<SessionObservation>,
    /// The approved staging profile directory, for a Hermes selection.
    pub(super) hermes_profile: Option<PathBuf>,
    /// This machine's clock, supplied so receipt freshness is testable.
    pub(super) now: u64,
    /// The id and cancel flag this attempt runs under. Both the probe capture
    /// and the answering run poll the same flag, so a withdrawal is observed
    /// whichever child is in flight.
    pub(super) attempt: AttemptIdentity,
}

/// Resolve the capability for one selection and answer with it.
pub(super) fn answer(binding: PrivateAskBinding) -> Result<PrivateAskResponse, PrivateAskFailure> {
    let retained = probe_receipt::load(
        &binding.ownership,
        &binding.state.executable,
        &binding.state.runtime_id,
        &probe_program::probe_program_digest(),
        binding.now,
    );
    // A structurally wrong trace is an error, not weaker evidence: it is not
    // evidence about this selection in either direction.
    let capability = retained
        .map(|probe| PrivateAskCapability::from_probe(&binding.state, probe, binding.now))
        .transpose()
        .map_err(|_| PrivateAskFailure::SelectionChanged)?;
    // A retained trace never carries session-isolation evidence, by design: an
    // independent invocation is a fact about one contained run beside one live
    // session, not a property of this machine that can be cached. So when the
    // selection HAS a live session and the retained trace cannot speak to it,
    // this attempt captures its own probe beside that session rather than
    // answering under a dimension nobody observed.
    //
    // NAMED LIMIT, and the honest consequence: because admission requires that
    // dimension, an Ask for an agent with a live session captures a fresh probe
    // every time. The receipt still bounds staleness and retains the trace; it
    // does not yet save the probe run.
    let needs_own_probe = binding.session.is_some()
        && capability
            .as_ref()
            .is_none_or(|capability| !capability.certifies_independent_invocation());
    let capability = match capability {
        // Reusable only when nothing this attempt can observe is missing from
        // it. Otherwise — and whenever there is no retained trace at all — this
        // attempt captures its own.
        Some(capability) if !needs_own_probe => capability,
        _ => {
            let probe = capture(&binding)?;
            // Persisted before it is used, so the trace this attempt paid for
            // is on disk. A store failure is returned rather than ignored: a
            // receipt that silently never lands leaves no record of what was
            // certified.
            probe_receipt::store(&binding.ownership, &probe)?;
            PrivateAskCapability::from_probe(&binding.state, probe, binding.now)
                .map_err(|_| PrivateAskFailure::SelectionChanged)?
        }
    };
    let admission = admit_private_ask(binding.request, binding.state, capability)?;
    let mut attempt =
        PrivateAskAttempt::create_as(admission, binding.ownership, binding.now, &binding.attempt)?;
    if let Some(profile) = binding.hermes_profile.as_deref() {
        attempt.stage_hermes_profile(profile)?;
    }
    attempt.run()
}

/// Capture one fresh probe for this selection.
///
/// The session bracketing belongs to `probe_run`, which is the only place that
/// sees the probe child's own reported parent; handing it the observation keeps
/// `independent_invocation` a statement about two real readings and a real
/// lineage rather than about this process's opinion of both.
#[cfg(target_os = "macos")]
fn capture(
    binding: &PrivateAskBinding,
) -> Result<super::capability::PrivateAskProbe, PrivateAskFailure> {
    super::probe_run::capture_probe(super::probe_run::ProbeContext {
        state: &binding.state,
        ownership: &binding.ownership,
        session: binding.session.clone(),
        now: binding.now,
        cancel: binding.attempt.cancel_flag(),
    })
}

/// Off macOS there is no boundary to capture a probe under, so a selection with
/// no retained receipt is refused rather than answered.
#[cfg(not(target_os = "macos"))]
fn capture(
    binding: &PrivateAskBinding,
) -> Result<super::capability::PrivateAskProbe, PrivateAskFailure> {
    let _ = binding;
    Err(PrivateAskFailure::ProcessContainmentUnverified)
}

#[cfg(test)]
#[path = "binding_tests.rs"]
mod tests;
