//! Default-off private Wiki Ask seam; it adds no command, event, UI store or
//! registry. A reviewed capability receipt is required before process launch.

use super::discovery::bounded_command::BoundedFailure;
use super::recap_capability::{same_executable_proof, RecapExecutableIdentity};
use super::recap_state::RecapStateFailure;
use std::time::Duration;

mod attempt;
mod binding;
mod cancel_registry;
mod capability;
mod citations;
mod containment;
mod credential;
pub(crate) mod dev_gate;
mod egress_proxy;
pub(crate) mod history;
mod launch;
mod probe_program;
mod probe_receipt;
#[cfg(target_os = "macos")]
pub(crate) mod probe_run;
mod profile;
mod prompt;
mod provider;
mod recovery;
pub(crate) mod retrieval;
mod runtime_paths;
pub(crate) mod selection;
mod selection_native;
mod session_evidence;
mod validation;
#[allow(unused_imports)]
pub(crate) use attempt::{
    AskEvent, AskReporter, AttemptIdentity, PrivateAskAttempt, PrivateAskResponse,
};
pub(crate) use cancel_registry::{PrivateAskAttempts, RegisterFailure};
use launch::PrivateAskLaunchPlan;
#[cfg(test)]
use prompt::build_prompt_with_nonce;
use prompt::{build_prompt, config_fingerprint};
#[cfg(test)]
use recovery::finish_after_process_with;
use recovery::{
    finish_after_process, finish_before_spawn, leave_process_pending, leave_process_pending_state,
};
pub(crate) use selection_native::{
    ask_scope_key, dev_run, observed_agents, DevAskMeta, DevOutcome,
};
use validation::{is_hex64, valid_model, valid_profile, valid_scope_value, valid_source_revision};
// The test modules below are children of this module and share its vocabulary;
// these re-imports keep them compiling after the attempt half moved out.
#[cfg(all(test, target_os = "macos"))]
use super::discovery::bounded_command::{BoundedPolicy, OutputBudget};
#[cfg(test)]
use super::recap_ownership::VerifiedStagingOwnership;
#[cfg(test)]
use super::recap_state::OwnedRecapRun;
#[cfg(test)]
use std::path::Path;

/// Maximum complete question plus grounded source bytes supplied to one run.
pub(crate) const PRIVATE_ASK_INPUT_LIMIT: usize = 128 * 1024;
/// Maximum bytes retained from one answer stream.
pub(crate) const PRIVATE_ASK_OUTPUT_LIMIT: u64 = 256 * 1024;
/// Maximum bytes retained from diagnostics.  Diagnostics never enter a result.
pub(crate) const PRIVATE_ASK_STDERR_LIMIT: u64 = 256 * 1024;
/// Hard wall-clock bound for one private Ask attempt.
pub(crate) const PRIVATE_ASK_TIMEOUT: Duration = Duration::from_secs(120);

/// The fixed reason a "not enough source" outcome carries. It is an outcome,
/// not a failure — the attempt ran, the question was asked, and the verified
/// snapshot is what says it cannot answer. The manifest shows the coverage.
pub(crate) const PRIVATE_ASK_INSUFFICIENT: &str = "the verified Wiki does not cover this question";

/// A capability state is intentionally explicit.  `Unverified` is the safe
/// default for every discovered runtime and cannot be coerced into `Verified`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProofStatus {
    /// The exact runtime/flag/containment experiment was retained and matched.
    Verified,
    /// No retained experiment proves this property for the selected binary.
    Unverified,
}

/// Stable scope for one viewer asking one existing project agent about one repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrivateAskScope {
    community_id: String,
    relay_url: String,
    viewer_pubkey: String,
    agent_pubkey: String,
    project_id: String,
    repo_owner: String,
    repo_d: String,
}

impl PrivateAskScope {
    fn validate(&self) -> Result<(), PrivateAskFailure> {
        for value in [
            (&self.community_id, "community"),
            (&self.project_id, "project"),
            (&self.repo_d, "repository"),
        ] {
            valid_scope_value(value.0)
                .then_some(())
                .ok_or(PrivateAskFailure::InvalidScope(value.1))?;
        }
        buzz_core_pkg::relay::normalize_relay_url(&self.relay_url)
            .map_err(|_| PrivateAskFailure::InvalidScope("relay"))?;
        for value in [&self.viewer_pubkey, &self.agent_pubkey, &self.repo_owner] {
            if !is_hex64(value) {
                return Err(PrivateAskFailure::InvalidScope("pubkey"));
            }
        }
        Ok(())
    }
}

/// One complete source excerpt authenticated by the #364 source revision.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GroundedSource {
    path: String,
    start_line: u64,
    end_line: u64,
    source_hash: String,
    content: String,
    snapshot_head_event_id: String,
}

impl GroundedSource {
    /// Where this source lives inside the verified snapshot. Read-only
    /// accessors, so a caller outside this module can render a citation without
    /// being able to describe one.
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn start_line(&self) -> u64 {
        self.start_line
    }

    pub(crate) fn end_line(&self) -> u64 {
        self.end_line
    }

    /// Byte length of the retained content, for read-budget accounting.
    pub(crate) fn content_len(&self) -> usize {
        self.content.len()
    }

    fn validate(&self) -> Result<(), PrivateAskFailure> {
        if !crew_wiki::source_snapshot::valid_source_path(&self.path)
            || self.start_line == 0
            || self.end_line < self.start_line
            || self.content.is_empty()
            || self.content.contains('\0')
            || self.source_hash.len() != 64
            || !self
                .source_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || !is_hex64(&self.snapshot_head_event_id)
            || crew_wiki::source_snapshot::source_hash(self.content.as_bytes()) != self.source_hash
        {
            return Err(PrivateAskFailure::InvalidGrounding);
        }
        let line_count = self.content.split_terminator('\n').count() as u64;
        if self.end_line > line_count {
            return Err(PrivateAskFailure::InvalidGrounding);
        }
        Ok(())
    }

    /// Build grounding only from a page and source reference authenticated by
    /// one complete immutable Wiki snapshot.  The native source reader supplies
    /// `file`; this method verifies that its bytes still match the signed
    /// reference before allowing them into a private prompt.
    pub(crate) fn from_verified_snapshot(
        snapshot: &crew_wiki::snapshot_v1::VerifiedSnapshot,
        page: &crew_wiki::snapshot_v1::VerifiedSnapshotPage,
        reference: &crew_wiki::source_snapshot::SourceReference,
        file: &crew_wiki::source_access::VerifiedSourceFile,
    ) -> Result<Self, PrivateAskFailure> {
        let page_is_member = snapshot
            .pages()
            .iter()
            .any(|candidate| candidate.event_id() == page.event_id());
        let reference_is_member = page
            .source_references()
            .iter()
            .any(|candidate| candidate == reference);
        let (path, source_hash, size, start_line, end_line) = reference;
        if !page_is_member
            || !reference_is_member
            || file.start_line != *start_line
            || file.end_line != *end_line
            || file.content.len() as u64 != *size
            || crew_wiki::source_snapshot::source_hash(file.content.as_bytes()) != *source_hash
            || !valid_source_revision(snapshot.index().source_revision())
        {
            return Err(PrivateAskFailure::InvalidGrounding);
        }
        let grounded = Self {
            path: path.clone(),
            start_line: *start_line,
            end_line: *end_line,
            source_hash: source_hash.clone(),
            content: file.content.clone(),
            snapshot_head_event_id: snapshot.index().head_event_id().to_owned(),
        };
        grounded.validate()?;
        Ok(grounded)
    }
}

/// One earlier turn of the same question thread, carried into the prompt as
/// context for a follow-up.
///
/// Both fields come from this viewer's own owner-local history — an entry the
/// question itself already produced. The bounds are applied where the history
/// is read; here they are data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PriorTurn {
    pub(crate) question: String,
    pub(crate) markdown: String,
}

/// Input to the private adapter.  Source text is data and is delimited in the
/// prompt; instructions inside it cannot add tools or change the selected scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrivateAskRequest {
    scope: PrivateAskScope,
    source_revision: String,
    question: String,
    /// Earlier turns of this question thread, oldest first. Only present for a
    /// follow-up; a fresh question has none.
    prior: Vec<PriorTurn>,
    /// The pages the question's terms matched, whole or excerpted. These are
    /// answer context — they are NOT citeable; only `grounding` sources are.
    pages: Vec<retrieval::RetrievedPage>,
    grounding: Vec<GroundedSource>,
    /// What the prompt actually consulted and what it omitted. Written by
    /// retrieval and kept truthful by `prompt::fit_request`.
    manifest: retrieval::RetrievalManifest,
}

impl PrivateAskRequest {
    /// Bind one question to the exact repository and revision in a verified
    /// snapshot.  There is no free-form production constructor for a source
    /// revision, a retrieval, or a grounding list.
    ///
    /// `persona` is the selected agent's own effective persona. It is taken
    /// here rather than at admission because the prompt envelope it belongs to
    /// decides how much grounding fits: without it the request would be built
    /// with more source than the prompt can carry and then refused.
    pub(crate) fn from_verified_snapshot(
        scope: PrivateAskScope,
        question: String,
        prior: Vec<PriorTurn>,
        snapshot: &crew_wiki::snapshot_v1::VerifiedSnapshot,
        retrieval: retrieval::Retrieval,
        persona: &str,
    ) -> Result<Self, PrivateAskFailure> {
        let manifest = snapshot.index().manifest();
        if scope.repo_owner != manifest.2 || scope.repo_d != manifest.3 {
            return Err(PrivateAskFailure::ScopeMismatch);
        }
        let snapshot_head_event_id = snapshot.index().head_event_id();
        if retrieval
            .grounding
            .iter()
            .any(|source| source.snapshot_head_event_id != snapshot_head_event_id)
        {
            return Err(PrivateAskFailure::InvalidGrounding);
        }
        let mut request = Self {
            scope,
            source_revision: snapshot.index().source_revision().to_owned(),
            question,
            prior,
            pages: retrieval.pages,
            grounding: retrieval.grounding,
            manifest: retrieval.manifest,
        };
        prompt::fit_request(&mut request, persona)?;
        request.validate()?;
        Ok(request)
    }

    /// The page slugs carried into this request's prompt.
    #[cfg(test)]
    pub(crate) fn page_slugs(&self) -> Vec<&str> {
        self.pages.iter().map(|page| page.slug.as_str()).collect()
    }

    fn validate(&self) -> Result<(), PrivateAskFailure> {
        self.scope.validate()?;
        if !valid_source_revision(&self.source_revision)
            || self.question.trim().is_empty()
            || self.question.contains('\0')
        {
            return Err(PrivateAskFailure::InvalidQuestion);
        }
        for source in &self.grounding {
            source.validate()?;
        }
        // The persona is not known here; admission re-checks the bound with the
        // selected agent's actual persona before anything can launch.
        // `build_prompt` enforces the bound itself; measuring its output again
        // here would be a second copy of the same rule that could drift.
        prompt::check_fits(self, "")
    }
}

/// Refuse a selection whose own session is mid-turn.
///
/// The signal is the desktop's own lifecycle for the live harness generation —
/// what `selection::lifecycle_of` derived from the runtime it holds a handle to
/// — and deliberately NOT the session ledger digest. The digest cannot
/// distinguish "this Ask touched the session" from "the employee finished a
/// turn of its own", and it can only be compared after the answer has been
/// paid for. This fence is the cheap half: an agent that is working is refused
/// before any probe child or runtime child is started. The before/after bracket
/// in `binding::answer` remains the proof of `independent_invocation` for the
/// runs that get past here.
///
/// Called from both `admit_private_ask` and the top of `binding::answer`, from
/// this one predicate, so the two enforcement points cannot drift: admission is
/// where every fence is stated, and the binding is where "before any probe"
/// actually means something.
pub(super) fn refuse_busy_selection(state: &SelectedAgentState) -> Result<(), PrivateAskFailure> {
    if state.lifecycle == AgentLifecycle::Busy {
        return Err(PrivateAskFailure::AgentBusy);
    }
    Ok(())
}

/// The managed-agent state observed immediately before Ask admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedAgentState {
    scope: PrivateAskScope,
    runtime_id: String,
    executable: RecapExecutableIdentity,
    /// Effective model, including the model read from a Hermes profile.
    effective_model: String,
    /// Hermes profile name, when the selected runtime owns model/provider.
    profile: Option<String>,
    /// The selected agent's effective persona / system prompt, resolved from
    /// its own managed-agent configuration. It is prompt authority, and it is
    /// part of `config_fingerprint`.
    persona: String,
    /// Stable hash of the effective persona/runtime configuration. Admission
    /// recomputes this from the fields above rather than trusting the caller.
    config_fingerprint: String,
    /// Stable hash of the effective owner/project/repository ACL projection.
    acl_fingerprint: String,
    /// Generation of the existing employee session.  Ask never writes to it.
    session_generation: String,
    lifecycle: AgentLifecycle,
}

impl SelectedAgentState {
    /// Build the observed selection from the agent's own effective
    /// configuration, so the persona and fingerprint come from the managed-agent
    /// resolution path rather than from a private Ask caller.
    ///
    /// An orphaned instance — a persona-linked record whose definition is gone —
    /// is `AgentUnbound`: it has no effective persona to speak with, and the
    /// existing spawn boundary already refuses to start it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_effective_config(
        scope: PrivateAskScope,
        runtime_id: impl Into<String>,
        executable: RecapExecutableIdentity,
        effective_model: impl Into<String>,
        profile: Option<String>,
        config: &super::effective_config::EffectiveConfigResult,
        acl_fingerprint: impl Into<String>,
        session_generation: impl Into<String>,
        lifecycle: AgentLifecycle,
    ) -> Result<Self, PrivateAskFailure> {
        let super::effective_config::EffectiveConfigResult::Resolved(config) = config else {
            return Err(PrivateAskFailure::AgentUnbound);
        };
        let persona =
            prompt::normalize_persona(config.system_prompt.value.as_deref().unwrap_or_default());
        let runtime_id = runtime_id.into();
        let effective_model = effective_model.into();
        if !prompt::valid_persona(&persona) {
            return Err(PrivateAskFailure::SelectionChanged);
        }
        let config_fingerprint =
            config_fingerprint(&runtime_id, &effective_model, profile.as_deref(), &persona);
        Ok(Self {
            scope,
            runtime_id,
            executable,
            effective_model,
            profile,
            persona,
            config_fingerprint,
            acl_fingerprint: acl_fingerprint.into(),
            session_generation: session_generation.into(),
            lifecycle,
        })
    }
}

/// Whether the existing selected agent is safe to address for this attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentLifecycle {
    /// No active turn currently owns the selected session.
    Idle,
    /// The employee session is doing work.
    Busy,
    /// The selected principal or binding disappeared.
    Unbound,
    /// The viewer or project no longer has the required access.
    Revoked,
}

/// Retained proof for the exact executable and its capability envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrivateAskCapability {
    runtime_id: String,
    executable: RecapExecutableIdentity,
    effective_model: String,
    profile: Option<String>,
    config_fingerprint: String,
    acl_fingerprint: String,
    session_generation: String,
    authentication: ProofStatus,
    tool_isolation: ProofStatus,
    /// The run could not read outside its own root: no employee worktree, no
    /// `~/.ssh`, no other agent's credentials. Separate from `tool_isolation`,
    /// which is about a hostile *write* or connect.
    read_bounded: ProofStatus,
    /// The run's network egress could reach the model provider and nothing
    /// else. Distinct from `tool_isolation`, which only establishes that the
    /// child failed to reach a listener the probe controlled.
    egress_bounded: ProofStatus,
    process_containment: ProofStatus,
    side_effect_free: ProofStatus,
    independent_invocation: ProofStatus,
}

impl PrivateAskCapability {
    /// Build an inert capability from discovery.  It is always unverified.
    pub(crate) fn from_inventory(
        runtime_id: impl Into<String>,
        executable: RecapExecutableIdentity,
        effective_model: impl Into<String>,
        profile: Option<String>,
        config_fingerprint: impl Into<String>,
        acl_fingerprint: impl Into<String>,
        session_generation: impl Into<String>,
    ) -> Self {
        Self {
            runtime_id: runtime_id.into(),
            executable,
            effective_model: effective_model.into(),
            profile,
            config_fingerprint: config_fingerprint.into(),
            acl_fingerprint: acl_fingerprint.into(),
            session_generation: session_generation.into(),
            authentication: ProofStatus::Unverified,
            tool_isolation: ProofStatus::Unverified,
            read_bounded: ProofStatus::Unverified,
            egress_bounded: ProofStatus::Unverified,
            process_containment: ProofStatus::Unverified,
            side_effect_free: ProofStatus::Unverified,
            independent_invocation: ProofStatus::Unverified,
        }
    }

    #[cfg(test)]
    fn verified_for_fixture(state: &SelectedAgentState) -> Self {
        Self {
            runtime_id: state.runtime_id.clone(),
            executable: state.executable.clone(),
            effective_model: state.effective_model.clone(),
            profile: state.profile.clone(),
            config_fingerprint: state.config_fingerprint.clone(),
            acl_fingerprint: state.acl_fingerprint.clone(),
            session_generation: state.session_generation.clone(),
            authentication: ProofStatus::Verified,
            tool_isolation: ProofStatus::Verified,
            read_bounded: ProofStatus::Verified,
            egress_bounded: ProofStatus::Verified,
            process_containment: ProofStatus::Verified,
            side_effect_free: ProofStatus::Verified,
            independent_invocation: ProofStatus::Verified,
        }
    }
}

/// Fixed, user-safe failure vocabulary for the adapter and its status rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrivateAskFailure {
    InvalidScope(&'static str),
    InvalidQuestion,
    InvalidGrounding,
    /// The selected agent's own session is mid-turn. Refused up front, before
    /// any probe or child, from the desktop's own lifecycle signal — the
    /// session bracket around an answer can only speak after the model call has
    /// already been paid for.
    AgentBusy,
    InputLimit,
    /// The question and persona alone — with no grounding at all — do not fit
    /// the prompt bound. Distinct from [`Self::InputLimit`], which is the
    /// assembled-prompt invariant: grounding is trimmed to fit, never refused,
    /// so a viewer is only told their input is too large when it actually is.
    QuestionLimit,
    ScopeMismatch,
    AgentUnbound,
    AccessRevoked,
    SelectionChanged,
    MissingRuntime,
    InvalidModel,
    MissingProfile,
    AuthenticationUnverified,
    ToolIsolationUnverified,
    ReadIsolationUnverified,
    EgressBoundUnverified,
    ProcessContainmentUnverified,
    SideEffectProofUnverified,
    IndependentInvocationUnverified,
    /// A running session's own state could not be read, so nothing can be
    /// claimed about whether a private Ask left it alone.
    SessionObservationUnavailable,
    /// The earlier answered attempt a follow-up names is not in this viewer's
    /// scoped history — deleted, expired, another scope's, or never answered.
    /// Refusing is honest where guessing the thread would not be.
    FollowUpUnavailable,
    InvalidState,
    ProfileUnavailable,
    Process(BoundedFailure),
    State(RecapStateFailure),
    NonzeroExit,
    InvalidOutput,
    ModelMismatch,
}

impl std::fmt::Display for PrivateAskFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidScope(_) => f.write_str("private Ask scope is invalid"),
            Self::InvalidQuestion => f.write_str("private Ask question is invalid"),
            Self::InvalidGrounding => f.write_str("private Ask grounding is invalid"),
            Self::AgentBusy => f.write_str("selected agent is busy"),
            Self::InputLimit => f.write_str("private Ask input exceeds its bound"),
            Self::QuestionLimit => {
                f.write_str("private Ask question exceeds its bound before any source is grounded")
            }
            Self::ScopeMismatch => f.write_str("private Ask scope changed"),
            Self::AgentUnbound => f.write_str("selected agent is unavailable"),
            Self::AccessRevoked => f.write_str("private Ask access was revoked"),
            Self::SelectionChanged => f.write_str("selected agent configuration changed"),
            Self::MissingRuntime => f.write_str("selected runtime is unavailable"),
            Self::InvalidModel => f.write_str("selected runtime model is invalid"),
            Self::MissingProfile => f.write_str("selected runtime profile is unavailable"),
            Self::AuthenticationUnverified => f.write_str("runtime authentication is unverified"),
            Self::ToolIsolationUnverified => f.write_str("runtime tool isolation is unverified"),
            Self::ReadIsolationUnverified => f.write_str("runtime read isolation is unverified"),
            Self::EgressBoundUnverified => {
                f.write_str("runtime network egress is not bounded to the model provider")
            }
            Self::SessionObservationUnavailable => {
                f.write_str("the selected agent's running session could not be observed")
            }
            Self::FollowUpUnavailable => {
                f.write_str("the earlier answer this follows up on is not available")
            }
            Self::ProcessContainmentUnverified => {
                f.write_str("runtime process containment is unverified")
            }
            Self::SideEffectProofUnverified => {
                f.write_str("runtime side-effect proof is unverified")
            }
            Self::IndependentInvocationUnverified => {
                f.write_str("independent invocation is unverified")
            }
            Self::InvalidState => f.write_str("private Ask state is invalid"),
            Self::ProfileUnavailable => f.write_str("selected runtime profile is unavailable"),
            // A withdrawal is not a malfunction, and the viewer is the one who
            // withdrew it: the generic process wording would make their own
            // Cancel button read like a failure, in the answer box and in the
            // owner-local record alike.
            Self::Process(BoundedFailure::Cancelled) => {
                f.write_str("the private Ask was cancelled")
            }
            Self::Process(_) => f.write_str("private Ask process did not complete safely"),
            Self::State(_) => f.write_str("private Ask state could not be retained safely"),
            Self::NonzeroExit => f.write_str("selected runtime returned an error"),
            Self::InvalidOutput => f.write_str("selected runtime returned invalid output"),
            Self::ModelMismatch => f.write_str("selected runtime used a different model"),
        }
    }
}

/// Admission result bound to the exact selected-agent generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PrivateAskAdmission {
    pub request: PrivateAskRequest,
    pub state: SelectedAgentState,
    pub capability: PrivateAskCapability,
}

/// Check all identity, ACL, busy-session and retained-capability fences.
pub(crate) fn admit_private_ask(
    request: PrivateAskRequest,
    state: SelectedAgentState,
    capability: PrivateAskCapability,
) -> Result<PrivateAskAdmission, PrivateAskFailure> {
    request.validate()?;
    state.scope.validate()?;
    if request.scope != state.scope {
        return Err(PrivateAskFailure::ScopeMismatch);
    }
    match state.lifecycle {
        AgentLifecycle::Unbound => return Err(PrivateAskFailure::AgentUnbound),
        AgentLifecycle::Revoked => return Err(PrivateAskFailure::AccessRevoked),
        AgentLifecycle::Idle | AgentLifecycle::Busy => {}
    }
    if !matches!(state.runtime_id.as_str(), "claude" | "hermes") {
        return Err(PrivateAskFailure::MissingRuntime);
    }
    refuse_busy_selection(&state)?;
    if !is_hex64(&state.config_fingerprint)
        || !is_hex64(&state.acl_fingerprint)
        || !valid_scope_value(&state.session_generation)
        || !is_hex64(&capability.config_fingerprint)
        || !is_hex64(&capability.acl_fingerprint)
        || !valid_scope_value(&capability.session_generation)
    {
        return Err(PrivateAskFailure::InvalidState);
    }
    if capability.runtime_id != state.runtime_id
        || !same_executable_proof(&capability.executable, &state.executable)
        || capability.effective_model != state.effective_model
        || capability.profile != state.profile
        || capability.config_fingerprint != state.config_fingerprint
        || capability.session_generation != state.session_generation
    {
        return Err(PrivateAskFailure::SelectionChanged);
    }
    if capability.acl_fingerprint != state.acl_fingerprint {
        return Err(PrivateAskFailure::AccessRevoked);
    }
    if !valid_model(&state.effective_model) {
        return Err(PrivateAskFailure::InvalidModel);
    }
    if !prompt::valid_persona(&state.persona) {
        return Err(PrivateAskFailure::SelectionChanged);
    }
    // A caller-supplied fingerprint only proves it is well-formed. Recompute it
    // from the observed runtime, model, profile and persona so a stale or
    // hand-set value cannot admit a different effective configuration.
    if config_fingerprint(
        &state.runtime_id,
        &state.effective_model,
        state.profile.as_deref(),
        &state.persona,
    ) != state.config_fingerprint
    {
        return Err(PrivateAskFailure::SelectionChanged);
    }
    // The assembled prompt must fit under the agent's REAL persona, which the
    // request's own validation could not see. Grounding was already trimmed to
    // this persona when the selection was resolved, so this is the invariant
    // check rather than the place a viewer is normally refused.
    prompt::check_fits(&request, &state.persona)?;
    if state.runtime_id == "hermes"
        && state
            .profile
            .as_deref()
            .is_none_or(|profile| !valid_profile(profile))
    {
        return Err(PrivateAskFailure::MissingProfile);
    }
    match capability.authentication {
        ProofStatus::Verified => {}
        ProofStatus::Unverified => return Err(PrivateAskFailure::AuthenticationUnverified),
    }
    if capability.tool_isolation != ProofStatus::Verified {
        return Err(PrivateAskFailure::ToolIsolationUnverified);
    }
    if capability.read_bounded != ProofStatus::Verified {
        return Err(PrivateAskFailure::ReadIsolationUnverified);
    }
    if capability.process_containment != ProofStatus::Verified {
        return Err(PrivateAskFailure::ProcessContainmentUnverified);
    }
    if capability.side_effect_free != ProofStatus::Verified {
        return Err(PrivateAskFailure::SideEffectProofUnverified);
    }
    // Independence is deliberately NOT checked here. It is not a property of a
    // capability at all — it is an observation of the employee's own live
    // session either side of the answering run, which admission happens before.
    // `binding::answer` takes that bracket and refuses the answer if it does
    // not hold.
    // Last, so a probe that failed a dimension it *could* have proved hears
    // about that first. Today no producer can verify this one, so a private Ask
    // is refused here even when every other proof holds — see
    // `PrivateAskCapability::from_probe`.
    if capability.egress_bounded != ProofStatus::Verified {
        return Err(PrivateAskFailure::EgressBoundUnverified);
    }
    Ok(PrivateAskAdmission {
        request,
        state,
        capability,
    })
}

/// `pub(crate)` so the command-layer tests (a sibling module) can reuse its
/// fixtures. Under `cfg(test)` it never exists in a shipped build.
#[cfg(test)]
#[path = "private_ask/tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "private_ask/answer_tests.rs"]
mod answer_tests;

#[cfg(test)]
#[path = "private_ask/containment_tests.rs"]
mod containment_tests;

#[cfg(test)]
#[path = "private_ask/capability_tests.rs"]
mod capability_tests;

#[cfg(test)]
#[path = "private_ask/fail_closed_tests.rs"]
mod fail_closed_tests;

#[cfg(test)]
#[path = "private_ask/isolation_tests.rs"]
mod isolation_tests;

#[cfg(test)]
#[path = "private_ask/privacy_tests.rs"]
mod privacy_tests;

#[cfg(test)]
#[path = "private_ask/prompt_model_tests.rs"]
mod prompt_model_tests;
