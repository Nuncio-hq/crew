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
//! Session isolation is deliberately NOT one of the things a receipt has to
//! carry, because it is not a property of this machine that can be cached: it
//! is a statement about one contained run beside one live session. It is
//! therefore observed around the ANSWERING run itself — the ledger digest and
//! owning PID are read immediately before and immediately after — rather than
//! around the probe. That is what lets a fresh, valid receipt restore the
//! containment dimensions while independence stays observed live on every Ask,
//! and it is why a second Ask on the same selection no longer spawns a probe
//! child.

use super::attempt::AttemptIdentity;
use super::session_evidence::{SessionIsolationEvidence, SessionObservation};
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
    // A trace that does not project onto THIS selection is a miss, not a
    // refusal. It describes a selection that no longer exists — most ordinarily
    // a previous harness generation, since `session_generation` changes every
    // time the agent restarts — and the honest response is to capture a fresh
    // probe rather than to tell the viewer their configuration changed.
    let capability = retained.and_then(|probe| {
        PrivateAskCapability::from_probe(&binding.state, probe, binding.now).ok()
    });
    let capability = match capability {
        // A retained trace that still projects onto this selection is the whole
        // capability: the dimension it cannot carry — independence — is not
        // restored from it at all, it is observed around the answering run
        // below.
        Some(capability) => capability,
        None => {
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
    // The bracket goes around the ANSWER, not around a probe: what a viewer
    // needs to know is that THIS run left the employee's live session alone.
    // A failure to read the session before the run is a refusal — a session
    // that cannot be observed has not been shown to be untouched.
    let before = binding
        .session
        .as_ref()
        .map(SessionObservation::capture)
        .transpose()?;
    let response = attempt.run();
    if let (Some(session), Some(before)) = (binding.session.as_ref(), before) {
        let after = session.capture()?;
        // The runtime child is spawned by THIS process, so its parent is this
        // process by construction rather than by observation.
        //
        // NAMED LIMIT: that makes the lineage clause of `is_verified` a
        // tautology on this path, unlike the probe path where the child reports
        // its own `getppid`. What is genuinely observed here is the session's
        // own ledger and owning PID either side of the run, which is the part
        // a cached trace could never speak to.
        let desktop_pid = std::process::id();
        let evidence = SessionIsolationEvidence::observe(before, after, desktop_pid, desktop_pid)?;
        // Checked before the run's own result is returned: a containment breach
        // is the stronger news, and an answer produced beside a session this
        // run disturbed is not an answer this feature may hand back.
        if !evidence.is_verified() {
            return Err(PrivateAskFailure::IndependentInvocationUnverified);
        }
    }
    response
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
