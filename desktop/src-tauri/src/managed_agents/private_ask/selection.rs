//! Resolve one private Ask from what this machine can observe.
//!
//! The developer surface hands in three things and nothing else: which agent,
//! which scope, and the question. Every value admission checks is produced
//! here, from a native observation:
//!
//! * the selected agent's own effective configuration (persona, model) and its
//!   runtime executable identity;
//! * `acl_fingerprint`, hashed from the agent's effective access projection;
//! * `session_generation`, hashed from the running harness generation's own
//!   start nonce — never the nonce itself, which is a secret shared with that
//!   harness and would otherwise travel into a response and an on-disk record;
//! * the session observation: the harness's own ACP session-ledger directory
//!   plus the PID of the child this process owns a handle to.
//!
//! None of those are parameters of the request. A caller can name an agent and
//! a scope; it cannot name what the agent is configured with, who may address
//! it, which generation is running, or what that generation's ledger says.
//!
//! This module is the pure half — it takes an observation and decides. The
//! observation itself is gathered in `selection_native`, which is the only part
//! that needs an `AppHandle`, a relay read, or a lock.

use super::attempt::AttemptIdentity;
use super::binding::{self, PrivateAskBinding};
use super::retrieval::{Retrieval, RetrievalManifest};
use super::session_evidence::{SessionObservation, SessionSnapshot};
#[cfg(test)]
use super::GroundedSource;
use super::{
    AgentLifecycle, PriorTurn, PrivateAskFailure, PrivateAskRequest, PrivateAskResponse,
    PrivateAskScope, SelectedAgentState,
};
use crate::managed_agents::effective_config::EffectiveConfigResult;
use crate::managed_agents::recap_capability::RecapExecutableIdentity;
use crate::managed_agents::recap_ownership::VerifiedStagingOwnership;
use crate::managed_agents::runtime_types::ManagedAgentRuntimeLifecycle;
use crate::managed_agents::types::{ManagedAgentRecord, RespondTo};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// The live harness generation for the selected agent, read from the handle
/// this process owns rather than from a PID probe.
#[derive(Debug, Clone)]
pub(crate) struct LiveAgentRuntime {
    /// PID of the harness child this process owns.
    pub(crate) acp_pid: u32,
    /// Unpredictable identity of this exact harness generation. It is a
    /// secret: it is hashed, never carried.
    pub(crate) start_nonce: String,
    pub(crate) lifecycle: ManagedAgentRuntimeLifecycle,
}

/// Everything the resolver observed about one selected agent.
///
/// It is assembled by `selection_native` and consumed here, so the deciding
/// half can be exercised on every platform without an app, a relay or a lock.
pub(crate) struct ObservedAgent<'a> {
    pub(crate) record: &'a ManagedAgentRecord,
    pub(crate) config: &'a EffectiveConfigResult,
    /// The build-time owner-only access policy, as every other behavioural
    /// boundary reads it.
    pub(crate) owner_only_access: bool,
    /// The normalized relay url this agent's harness is keyed on.
    pub(crate) relay_url: &'a str,
    pub(crate) live: Option<LiveAgentRuntime>,
    /// The harness's own ACP session-ledger directory for this (relay, agent)
    /// pair, derived by `session_evidence::session_ledger_dir`. It travels as
    /// data so the deciding half can be tested without this machine's home,
    /// and it is never reachable from a request.
    pub(crate) ledger_dir: PathBuf,
    pub(crate) runtime_id: String,
    pub(crate) executable: RecapExecutableIdentity,
    pub(crate) effective_model: String,
    pub(crate) profile: Option<String>,
    /// The approved staging profile directory for a Hermes selection.
    pub(crate) hermes_profile: Option<PathBuf>,
}

/// One resolved private Ask, ready to be answered.
///
/// There is no constructor other than [`resolve_observed_selection`], so a
/// selection cannot be described — only observed.
/// `Debug` carries nothing from the selection itself: a selection holds the
/// question, the grounding and the persona, and the tests that print one only
/// need to know that resolution succeeded.
pub(crate) struct PrivateAskSelection {
    binding: PrivateAskBinding,
}

impl std::fmt::Debug for PrivateAskSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrivateAskSelection(..)")
    }
}

/// What resolution produced: a selection ready to be answered, or a question
/// the verified snapshot cannot cover.
///
/// `Insufficient` is an outcome, not a refusal: the question was asked, the
/// retrieval ran, and the coverage record travels back in `RetrievalManifest`
/// so the viewer and the history see *what* was not covered rather than a
/// bare no.
pub(crate) enum ResolveStep {
    // Boxed: a ready selection carries the binding and grounding, which dwarfs
    // the insufficient arm's manifest.
    Ready(Box<PrivateAskSelection>),
    Insufficient(RetrievalManifest),
}

impl PrivateAskSelection {
    /// Answer this selection on the production path.
    pub(crate) fn answer(self) -> Result<PrivateAskResponse, PrivateAskFailure> {
        binding::answer(self.binding)
    }

    /// The history scope this selection resolved under — the identity its
    /// owner-local record is keyed on.
    pub(crate) fn history_scope(&self) -> super::history::HistoryScope {
        super::history::HistoryScope::from_scope(&self.binding.request.scope)
    }

    /// The source revision this selection's question is bound to.
    pub(crate) fn source_revision(&self) -> &str {
        &self.binding.request.source_revision
    }

    /// The owned staging tree this selection was resolved under. Shared with
    /// the binding, so the pending record and the run that follows it write
    /// under the same ownership.
    pub(crate) fn ownership(&self) -> &VerifiedStagingOwnership {
        &self.binding.ownership
    }

    /// The ACL projection this selection was resolved under.
    #[cfg(test)]
    pub(crate) fn acl_fingerprint(&self) -> &str {
        &self.binding.state.acl_fingerprint
    }

    /// The retrieval manifest this selection carries, after the prompt bound's
    /// own trim reconciled it.
    #[cfg(test)]
    pub(crate) fn manifest(&self) -> &RetrievalManifest {
        &self.binding.request.manifest
    }

    /// The prior turns carried into this selection's prompt.
    #[cfg(test)]
    pub(crate) fn prior(&self) -> &[PriorTurn] {
        &self.binding.request.prior
    }

    /// The page slugs carried into this selection's prompt.
    #[cfg(test)]
    pub(crate) fn page_slugs(&self) -> Vec<&str> {
        self.binding.request.page_slugs()
    }

    /// The harness generation this selection was resolved under.
    #[cfg(test)]
    pub(crate) fn session_generation(&self) -> &str {
        &self.binding.state.session_generation
    }

    /// The grounding this selection will actually prompt with, after the
    /// prompt budget trimmed it.
    #[cfg(test)]
    pub(crate) fn grounding(&self) -> &[GroundedSource] {
        &self.binding.request.grounding
    }

    /// The assembled prompt this selection would send, under its own persona.
    #[cfg(test)]
    pub(crate) fn prompt(&self) -> Result<String, PrivateAskFailure> {
        super::prompt::build_prompt(&self.binding.request, &self.binding.state.persona)
    }

    #[cfg(test)]
    pub(crate) fn lifecycle(&self) -> AgentLifecycle {
        self.binding.state.lifecycle
    }
}

/// Resolve one selection from a native observation.
///
/// Every refusal is typed. The order matters: the fences that cost nothing come
/// first, and the session observation — which reads the harness's own ledger —
/// happens before any child of ours could be started.
// The observation and the ask are deliberately separate arguments rather than
// one bag: `SelectedAgentState::from_effective_config` next door is the same
// shape, and merging them would hide which values are observed and which are
// asked for — the distinction this whole module exists to keep.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_observed_selection(
    scope: PrivateAskScope,
    question: String,
    prior: Vec<PriorTurn>,
    agent: ObservedAgent<'_>,
    snapshot: &crew_wiki::snapshot_v1::VerifiedSnapshot,
    retrieval: Retrieval,
    ownership: VerifiedStagingOwnership,
    now: u64,
    attempt: AttemptIdentity,
) -> Result<ResolveStep, PrivateAskFailure> {
    scope.validate()?;
    // The caller named an agent and a scope; the scope's claims about that
    // agent are checked against the record the id actually resolved to, so a
    // question cannot be addressed to one agent under another's identity.
    if !scope
        .agent_pubkey
        .eq_ignore_ascii_case(&agent.record.pubkey)
    {
        return Err(PrivateAskFailure::ScopeMismatch);
    }
    if scope.relay_url != agent.relay_url {
        return Err(PrivateAskFailure::ScopeMismatch);
    }
    // The repository half of the scope is checked against the snapshot's own
    // signed manifest by `PrivateAskRequest::from_verified_snapshot` below.

    let Some(live) = agent.live else {
        // No live harness generation means no owned PID and no session to
        // observe. It is not `AgentUnbound` — the agent exists and is bound;
        // there is simply nothing to certify an independent invocation
        // against, and admission requires that dimension.
        return Err(PrivateAskFailure::SessionObservationUnavailable);
    };
    // A generation that has failed or stopped is a registry entry, not a
    // running session: there is nothing live to be isolated from, so it is
    // refused here rather than carried to admission as a lifecycle.
    let lifecycle =
        lifecycle_of(&live.lifecycle).ok_or(PrivateAskFailure::SessionObservationUnavailable)?;

    // A question the snapshot cannot cover is an outcome, not a fence
    // violation — but it is decided here, after the busy/lifecycle fences, so
    // an agent that could not have answered anyway still reports its own
    // reason rather than a coverage reason that would mislead.
    if retrieval.insufficient() {
        return Ok(ResolveStep::Insufficient(retrieval.manifest));
    }

    let session = observed_session(agent.ledger_dir, &live)?;

    let state = SelectedAgentState::from_effective_config(
        scope.clone(),
        agent.runtime_id,
        agent.executable,
        agent.effective_model,
        agent.profile,
        agent.config,
        acl_fingerprint(&scope, agent.record, agent.owner_only_access),
        session_generation(&live.start_nonce),
        lifecycle,
    )?;
    let request = PrivateAskRequest::from_verified_snapshot(
        scope,
        question,
        prior,
        snapshot,
        retrieval,
        &state.persona,
    )?;

    Ok(ResolveStep::Ready(Box::new(PrivateAskSelection {
        binding: PrivateAskBinding {
            ownership,
            state,
            request,
            session: Some(session),
            hermes_profile: agent.hermes_profile,
            now,
            attempt,
        },
    })))
}

/// Read the selected agent's session ledger, and refuse if it cannot be read.
///
/// The read happens here, before anything is launched, so an agent whose
/// harness has never written a ledger entry is refused without a contained
/// child ever starting.
fn observed_session(
    ledger_dir: PathBuf,
    live: &LiveAgentRuntime,
) -> Result<SessionObservation, PrivateAskFailure> {
    let observation = SessionObservation {
        ledger_dir,
        acp_pid: Some(live.acp_pid),
    };
    // Reading it once here is the fence: `capture` refuses a directory that is
    // missing, empty, oversized or unreadable.
    let _: SessionSnapshot = observation.capture()?;
    Ok(observation)
}

/// How a live harness generation's lifecycle reads to a private Ask.
///
/// Only a `Ready` generation is idle. Everything else that is still running is
/// treated as busy, which means it must carry a verified independent
/// invocation to be answered — the fail-closed direction. `None` is a
/// generation that has failed or stopped: a registry entry rather than a live
/// session, which the caller refuses as an unobservable session.
fn lifecycle_of(lifecycle: &ManagedAgentRuntimeLifecycle) -> Option<AgentLifecycle> {
    match lifecycle {
        ManagedAgentRuntimeLifecycle::Ready => Some(AgentLifecycle::Idle),
        ManagedAgentRuntimeLifecycle::Starting
        | ManagedAgentRuntimeLifecycle::Listening
        | ManagedAgentRuntimeLifecycle::Waking => Some(AgentLifecycle::Busy),
        ManagedAgentRuntimeLifecycle::Failed | ManagedAgentRuntimeLifecycle::Stopped => None,
    }
}

/// Hash of the effective owner/project/repository access projection.
///
/// It is derived from the agent's own effective access policy — the same
/// projection every other behavioural boundary reads — together with the scope
/// the question is asked in. Admission compares it against the capability's, so
/// an access change between a retained probe and an answer is `AccessRevoked`
/// rather than an answer under yesterday's permissions.
fn acl_fingerprint(
    scope: &PrivateAskScope,
    record: &ManagedAgentRecord,
    owner_only_access: bool,
) -> String {
    let (respond_to, allowlist) =
        crate::managed_agents::projected_access_with_policy(record, owner_only_access);
    let mut allowlist: Vec<String> = allowlist
        .into_iter()
        .map(|pubkey| pubkey.to_ascii_lowercase())
        .collect();
    allowlist.sort();
    allowlist.dedup();
    let mut hasher = Sha256::new();
    // A versioned, field-separated canonical form: every field is length
    // prefixed so no two different projections can serialize to the same bytes.
    for field in [
        "private-ask-acl/v1",
        &scope.viewer_pubkey,
        &scope.agent_pubkey,
        &scope.project_id,
        &scope.repo_owner,
        &scope.repo_d,
        respond_to_key(respond_to),
    ]
    .into_iter()
    .chain(allowlist.iter().map(String::as_str))
    {
        hasher.update(field.len().to_le_bytes());
        hasher.update(field.as_bytes());
        hasher.update([0]);
    }
    hex::encode(hasher.finalize())
}

fn respond_to_key(respond_to: RespondTo) -> &'static str {
    respond_to.as_str()
}

/// Stable identifier for the running harness generation.
///
/// `start_nonce` is shared only with that generation and is used to reject
/// lifecycle frames from a prior process; it is a secret. The generation string
/// travels in the response and into the owner-local history file, so what is
/// published is its digest, which is just as stable and carries nothing.
fn session_generation(start_nonce: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"private-ask-session-generation/v1\0");
    hasher.update(start_nonce.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
#[path = "selection_tests.rs"]
mod tests;
