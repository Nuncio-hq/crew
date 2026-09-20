//! Gather what this machine can observe about one selected agent, then resolve.
//!
//! This is the half of the resolver that needs an app: the managed-agent store,
//! the live-process registry, the owner scope, one scoped Wiki read, and the
//! native source grant the viewer already chose. It decides nothing — every
//! decision belongs to `selection::resolve_observed_selection`, which is pure
//! and is where the fences are tested.
//!
//! The only relay traffic on this path is a read: the same scoped kind-30623
//! query the Wiki pane already makes, so the snapshot a question is grounded in
//! is verified natively instead of being carried by a renderer. Nothing is
//! published, and the read happens on the command's own thread, outside the
//! attempt thread the no-publish attribution covers.

use super::attempt::AttemptIdentity;
use super::history::{HistoryScope, ScopeKey};
use super::retrieval::{Retrieval, RetrievalManifest};
use super::selection::{resolve_observed_selection, LiveAgentRuntime, ObservedAgent, ResolveStep};
use super::{session_evidence, PriorTurn, PrivateAskFailure, PrivateAskScope};
use crate::app_state::AppState;
use crate::commands::NativeSourceRoot;
use crate::managed_agents::recap_capability::{verify_executable, RecapExecutableIdentity};
use crate::managed_agents::recap_ownership::VerifiedStagingOwnership;
use crate::managed_agents::types::ManagedAgentRecord;
use crate::managed_agents::ManagedAgentRuntimeKey;
use tauri::Manager;

/// How long the whole grounding read may take. Source reads are bounded by the
/// same deadline the renderer-facing read uses.
const GROUNDING_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

/// What one Ask attempt resolved to, with the scope it resolved under.
///
/// The scope travels beside the outcome — including on refusal — because the
/// owner-local record is keyed on it: a refused attempt is still this viewer's
/// attempt about this repository, and it lands in the same scoped history.
pub(crate) struct ResolvedAsk {
    pub(crate) step: ResolveStep,
    /// The history identity of this attempt. Present whenever the scope itself
    /// could be observed — which is earlier than any of the fences that can
    /// refuse.
    pub(crate) scope: HistoryScope,
    /// The snapshot's source revision, when the read got that far.
    pub(crate) source_revision: Option<String>,
    /// What the question retrieved, so a refused selection still reports its
    /// coverage to the record.
    pub(crate) manifest: Option<RetrievalManifest>,
}

/// A resolution that stopped at a fence. `scope` is `None` only when the scope
/// itself could not be observed — an attempt without a scope has no scoped
/// history to be recorded under, which the caller reports honestly.
pub(crate) struct ResolvedRefusal {
    pub(crate) failure: PrivateAskFailure,
    pub(crate) scope: Option<HistoryScope>,
    pub(crate) source_revision: Option<String>,
    pub(crate) manifest: Option<RetrievalManifest>,
}

impl ResolvedRefusal {
    fn bare(failure: PrivateAskFailure) -> Self {
        Self {
            failure,
            scope: None,
            source_revision: None,
            manifest: None,
        }
    }
}

/// Resolve one private Ask for the named agent and repository.
///
/// `agent_id` is the selected agent's pubkey and `coordinate` the repository it
/// is being asked about. Everything else — the viewer, the community, the
/// relay, the runtime, the persona, the model, the access projection, the
/// harness generation and the snapshot — is observed here.
pub(crate) async fn resolve_selection<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    agent_id: &str,
    coordinate: &str,
    question: String,
    prior: Vec<PriorTurn>,
    now: u64,
    attempt: AttemptIdentity,
) -> Result<ResolvedAsk, ResolvedRefusal> {
    let ownership = VerifiedStagingOwnership::load(app)
        .map_err(|_| ResolvedRefusal::bare(PrivateAskFailure::InvalidState))?;
    // A scope that cannot be captured is this machine's own lock or identity
    // state, not a revocation: saying "access was revoked" would send the
    // reader to the wrong problem.
    let captured = crate::app_state::owner_scope::capture(app.clone())
        .await
        .map_err(|_| ResolvedRefusal::bare(PrivateAskFailure::InvalidState))?;

    let record = agent_record(app, agent_id).map_err(ResolvedRefusal::bare)?;
    let relay_url = bound_relay_url(app, &record).map_err(ResolvedRefusal::bare)?;
    let (owner, repo_d) = coordinate_parts(coordinate).map_err(ResolvedRefusal::bare)?;
    let scope = PrivateAskScope {
        community_id: captured.token.scope.community.clone(),
        relay_url: relay_url.clone(),
        viewer_pubkey: captured.token.scope.owner.clone(),
        agent_pubkey: record.pubkey.clone(),
        // There is no native project registry to resolve a project id from, so
        // the repository coordinate is the project identity this Ask is scoped
        // to. It is derived here rather than accepted, so a caller still
        // cannot name one.
        project_id: coordinate.to_owned(),
        repo_owner: owner.clone(),
        repo_d: repo_d.clone(),
    };
    let history_scope = HistoryScope::from_scope(&scope);
    let mut source_revision = None;
    let mut manifest = None;
    // From here on every refusal carries the scope: the attempt is scoped, it
    // just could not run.
    macro_rules! scoped {
        ($failure:expr) => {
            ResolvedRefusal {
                failure: $failure,
                scope: Some(history_scope.clone()),
                source_revision: source_revision.clone(),
                manifest: manifest.clone(),
            }
        };
    }

    let snapshot_read =
        crate::commands::read_wiki_snapshot(app.clone(), captured.token.clone(), coordinate.into())
            .await
            .map_err(|_| scoped!(PrivateAskFailure::AccessRevoked))?;
    let snapshot =
        verified_snapshot(&owner, &repo_d, &snapshot_read.value).map_err(|f| scoped!(f))?;
    source_revision = Some(snapshot.index().source_revision().to_owned());

    let source_root = NativeSourceRoot::current(
        &app.state::<crate::commands::SourceState>(),
        &captured.token,
        coordinate,
    );
    let deadline = std::time::Instant::now() + GROUNDING_DEADLINE;
    let retrieval = super::retrieval::retrieve(
        &question,
        &snapshot,
        source_root.as_ref().map(NativeSourceRoot::workspace_mode),
        deadline,
        |revision, reference| -> Result<_, String> {
            match source_root.as_ref() {
                Some(root) => root.read_verified_reference(revision, reference, deadline),
                None => Err("no source grant".to_string()),
            }
        },
    );
    manifest = Some(retrieval_manifest(&retrieval));

    let (runtime_id, executable, effective_model, profile) =
        runtime_selection(app, &record).map_err(|f| scoped!(f))?;
    let hermes_profile = profile
        .as_deref()
        .map(|profile| {
            ownership
                .hermes_profile_source()
                .map(|root| root.join(profile))
                .map_err(PrivateAskFailure::State)
        })
        .transpose()
        .map_err(|f| scoped!(f))?;
    let live = live_runtime(app, &record, &relay_url);
    let ledger_dir = session_evidence::session_ledger_dir(&relay_url, &record.pubkey)
        .ok_or_else(|| scoped!(PrivateAskFailure::SessionObservationUnavailable))?;
    let config = crate::managed_agents::effective_config::resolve_effective_config(
        &record,
        &agent_definitions(app),
        &global_config(app),
    );

    let step = resolve_observed_selection(
        scope,
        question,
        prior,
        ObservedAgent {
            record: &record,
            config: &config,
            owner_only_access: crate::managed_agents::owner_only(),
            relay_url: &relay_url,
            live,
            ledger_dir,
            runtime_id,
            executable,
            effective_model,
            profile,
            hermes_profile,
        },
        &snapshot,
        retrieval,
        ownership,
        now,
        attempt,
    )
    .map_err(|failure| scoped!(failure))?;

    Ok(ResolvedAsk {
        step,
        scope: history_scope,
        source_revision,
        manifest,
    })
}

/// Take the manifest out of a retrieval about to be consumed.
///
/// `resolve_observed_selection` may reconcile it further (the prompt bound's
/// own trim), which is why the Ready selection carries the request's manifest
/// rather than this one — this copy is the fallback for the paths that never
/// reached a request.
fn retrieval_manifest(retrieval: &Retrieval) -> RetrievalManifest {
    retrieval.manifest.clone()
}

/// The stored record for this agent id, or `AgentUnbound`.
///
/// This is the one refusal that means "there is no such agent": every other
/// missing piece has its own reason.
fn agent_record<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    agent_id: &str,
) -> Result<ManagedAgentRecord, PrivateAskFailure> {
    crate::managed_agents::storage::load_managed_agents(app)
        .map_err(|_| PrivateAskFailure::AgentUnbound)?
        .into_iter()
        .find(|record| record.pubkey.eq_ignore_ascii_case(agent_id))
        .ok_or(PrivateAskFailure::AgentUnbound)
}

fn agent_definitions<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Vec<crate::managed_agents::types::AgentDefinition> {
    crate::managed_agents::personas::load_personas(app).unwrap_or_default()
}

fn global_config<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> crate::managed_agents::global_config::GlobalAgentConfig {
    crate::managed_agents::global_config::load_global_agent_config(app).unwrap_or_default()
}

/// The normalized relay url this agent's harness is keyed on.
fn bound_relay_url<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    record: &ManagedAgentRecord,
) -> Result<String, PrivateAskFailure> {
    let state = app.state::<AppState>();
    let workspace = crate::relay::relay_ws_url_with_override(&state);
    let relay_url = crate::relay::effective_agent_relay_url(&record.relay_url, &workspace);
    ManagedAgentRuntimeKey::new(record.pubkey.clone(), &relay_url)
        .map(|key| key.relay_url)
        .map_err(|_| PrivateAskFailure::InvalidScope("relay"))
}

/// The live harness generation for this agent, read from the handle this
/// process owns. The lock is released before anything else happens.
fn live_runtime<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    record: &ManagedAgentRecord,
    relay_url: &str,
) -> Option<LiveAgentRuntime> {
    let key = ManagedAgentRuntimeKey::new(record.pubkey.clone(), relay_url).ok()?;
    let state = app.state::<AppState>();
    let runtimes = state.managed_agent_processes.lock().ok()?;
    let runtime = runtimes.get(&key)?;
    Some(LiveAgentRuntime {
        acp_pid: runtime.process.child.id(),
        start_nonce: runtime.start_nonce.clone(),
        lifecycle: runtime.lifecycle.clone(),
    })
}

/// Which runtime this agent is configured with, and the identity of the exact
/// binary a private Ask would run.
///
/// A private Ask runs the runtime's own CLI one-shot, not the ACP adapter, so
/// the executable resolved here is the underlying CLI.
fn runtime_selection<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    record: &ManagedAgentRecord,
) -> Result<(String, RecapExecutableIdentity, String, Option<String>), PrivateAskFailure> {
    let command = crate::managed_agents::discovery::effective_agent_command(
        record.persona_id.as_deref(),
        &agent_definitions(app),
        record.agent_command_override.as_deref(),
    );
    let runtime = crate::managed_agents::discovery::known_acp_runtime(&command)
        .ok_or(PrivateAskFailure::MissingRuntime)?;
    let runtime_id = match runtime.id {
        id @ ("claude" | "hermes") => id.to_owned(),
        // Only these two runtimes have a contained one-shot a private Ask can
        // build a launch plan for; anything else is refused rather than
        // launched under a plan that does not describe it.
        _ => return Err(PrivateAskFailure::MissingRuntime),
    };
    let cli = runtime.underlying_cli.unwrap_or(runtime.id);
    let executable = executable_identity(cli)?;
    let profile = match runtime_id.as_str() {
        "hermes" => Some(
            record
                .hermes_profile
                .clone()
                .ok_or(PrivateAskFailure::MissingProfile)?,
        ),
        _ => None,
    };
    let effective_model = effective_model(app, record, profile.as_deref())?;
    Ok((runtime_id, executable, effective_model, profile))
}

/// The effective model for this selection.
///
/// For Hermes the profile owns the model, exactly as the harness resolves it;
/// for Claude it is the agent's own effective configuration.
fn effective_model<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    record: &ManagedAgentRecord,
    profile: Option<&str>,
) -> Result<String, PrivateAskFailure> {
    if let Some(profile) = profile {
        // Hermes owns the model inside its profile; that is the model the
        // one-shot will actually report, so it is the one admission binds to.
        if let crate::managed_agents::hermes_profile_config::HermesProfileConfigResult::Ok {
            model: Some(model),
            ..
        } = crate::managed_agents::hermes_profile_config::read_profile_config(profile)
        {
            if !model.trim().is_empty() {
                return Ok(model);
            }
        }
        return Err(PrivateAskFailure::InvalidModel);
    }
    match crate::managed_agents::effective_config::resolve_effective_config(
        record,
        &agent_definitions(app),
        &global_config(app),
    ) {
        crate::managed_agents::effective_config::EffectiveConfigResult::Resolved(config) => config
            .model
            .value
            .filter(|model| !model.trim().is_empty())
            .ok_or(PrivateAskFailure::InvalidModel),
        crate::managed_agents::effective_config::EffectiveConfigResult::OrphanedInstance {
            ..
        } => Err(PrivateAskFailure::AgentUnbound),
    }
}

/// Identify the runtime CLI on this machine.
///
/// The fingerprint is the file's own digest; `verify_executable` recomputes it
/// and the launch path recomputes it again immediately before spawning, so a
/// binary replaced between here and there is refused rather than run under an
/// identity that no longer describes it.
fn executable_identity(cli: &str) -> Result<RecapExecutableIdentity, PrivateAskFailure> {
    let resolved = crate::managed_agents::discovery::resolve_command(cli)
        .ok_or(PrivateAskFailure::MissingRuntime)?;
    let resolved_path = resolved
        .canonicalize()
        .map_err(|_| PrivateAskFailure::MissingRuntime)?;
    let candidate = RecapExecutableIdentity {
        resolved_path,
        // The runtime's own version string is not read here: a private Ask
        // binds to the executable's BYTES, and a version probe would have to
        // run the uncontained binary to learn a label the fences never use.
        // The identity is labelled with what it is, so it can never be
        // mistaken for a reported version.
        version: format!("content-addressed-{cli}"),
        fingerprint: "0".repeat(64),
        platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
    };
    // Hash the file through the same reader the launch path re-checks with.
    let observed = hashed(candidate)?;
    verify_executable(&observed).map_err(|_| PrivateAskFailure::MissingRuntime)
}

fn hashed(
    mut identity: RecapExecutableIdentity,
) -> Result<RecapExecutableIdentity, PrivateAskFailure> {
    use sha2::{Digest, Sha256};
    let bytes =
        std::fs::read(&identity.resolved_path).map_err(|_| PrivateAskFailure::MissingRuntime)?;
    identity.fingerprint = hex::encode(Sha256::digest(&bytes));
    Ok(identity)
}

fn coordinate_parts(coordinate: &str) -> Result<(String, String), PrivateAskFailure> {
    let (owner, repo_d) = coordinate
        .split_once(':')
        .ok_or(PrivateAskFailure::InvalidScope("repository"))?;
    if owner.len() != 64 || !owner.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(PrivateAskFailure::InvalidScope("pubkey"));
    }
    Ok((owner.to_ascii_lowercase(), repo_d.to_owned()))
}

/// Verify the natively read Wiki graph. A read that is not a complete v1
/// snapshot cannot ground anything, so it is refused rather than answered from
/// a partial projection.
fn verified_snapshot(
    owner: &str,
    repo_d: &str,
    read: &crate::commands::WikiSnapshotRead,
) -> Result<crew_wiki::snapshot_v1::VerifiedSnapshot, PrivateAskFailure> {
    let (Some(head), Some(manifest)) = (read.head.as_ref(), read.manifest.as_ref()) else {
        return Err(PrivateAskFailure::InvalidGrounding);
    };
    let head = serde_json::to_value(head).map_err(|_| PrivateAskFailure::InvalidGrounding)?;
    let manifest =
        serde_json::to_value(manifest).map_err(|_| PrivateAskFailure::InvalidGrounding)?;
    let pages = read
        .pages
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PrivateAskFailure::InvalidGrounding)?;
    crew_wiki::snapshot_v1::verify_snapshot(owner, repo_d, &head, &manifest, &pages)
        .map_err(|_| PrivateAskFailure::InvalidGrounding)
}

/// The scope key the owner-local history is filtered by, observed natively.
///
/// This is the same capture `resolve_selection` performs, minus the agent:
/// history reads and deletion are scoped to the community, the viewer and the
/// repository coordinate, never to anything a caller supplies.
pub(crate) async fn ask_scope_key<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    coordinate: &str,
) -> Result<ScopeKey, PrivateAskFailure> {
    let captured = crate::app_state::owner_scope::capture(app.clone())
        .await
        .map_err(|_| PrivateAskFailure::InvalidState)?;
    let (repo_owner, repo_d) = coordinate_parts(coordinate)?;
    Ok(ScopeKey {
        community_id: captured.token.scope.community.clone(),
        viewer_pubkey: captured.token.scope.owner.clone(),
        repo_owner,
        repo_d,
    })
}

/// What one dev-surface Ask produced, with the truth about its record.
///
/// `dev_run` never fails at the boundary: everything an attempt can produce —
/// answer, insufficiency or refusal — is an outcome. What the command still
/// needs to tell the viewer is the scope the outcome belongs to and whether
/// the owner-local record kept it.
pub(crate) struct DevRunResult {
    pub(crate) outcome: DevOutcome,
    /// The scope the attempt resolved under. `None` means the attempt never
    /// reached a scope, so there is no scoped record for it either.
    pub(crate) scope: Option<HistoryScope>,
    /// The question thread the attempt landed on — the caller's own id, or the
    /// parent's when a follow-up inherited the thread.
    pub(crate) question_id: String,
    /// The source revision the attempt bound to, when resolution got that far.
    pub(crate) source_revision: Option<String>,
    /// The coverage record, when retrieval ran.
    pub(crate) manifest: Option<RetrievalManifest>,
    pub(crate) history_recorded: bool,
}

/// The terminal state of one attempt.
pub(crate) enum DevOutcome {
    // Boxed: an answered outcome carries the whole response next to the small
    // manifest/failure arms.
    Answered(Box<super::PrivateAskResponse>),
    /// The verified snapshot does not cover the question; the manifest is the
    /// coverage record.
    Insufficient(RetrievalManifest),
    Refused(PrivateAskFailure),
}

/// What a follow-up is threaded under: the chain id it shares, and the earlier
/// turn it builds on.
pub(crate) struct DevAskMeta {
    /// Identity of the question thread. A fresh question gets a fresh id; a
    /// follow-up inherits the parent's.
    pub(crate) question_id: String,
    /// The attempt this follows up on, if any.
    pub(crate) follow_up_of: Option<String>,
}

/// Run one private Ask from the developer surface.
///
/// This is the production entry point the dev command calls. It reaches a real
/// selection or refuses with the typed reason it could not: every safety
/// decision belongs to `resolve_observed_selection`, `admit_private_ask` and
/// `PrivateAskAttempt::run`, and nothing here invents a result or a reason.
///
/// The owner-local record is written here — the pending entry before anything
/// can launch, and the terminal entry the moment the outcome is known — so a
/// viewer navigating away mid-attempt cannot orphan the record of what they
/// asked. A history write that fails is reported (`history_recorded`), never
/// allowed to block the answer.
///
/// The answer itself runs on a blocking worker, on ONE thread — which is what
/// the thread-scoped no-publish attribution relies on. The resolution above it
/// is async because it reads the Wiki snapshot natively.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dev_run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    agent_id: &str,
    coordinate: &str,
    question: &str,
    meta: DevAskMeta,
    is_live: std::sync::Arc<dyn Fn(&str) -> bool + Send + Sync>,
    attempt: &AttemptIdentity,
    now: u64,
) -> DevRunResult {
    attempt.report(super::attempt::AskEvent::Retrieving);

    // The scope key for the record and the ownership root that holds it are
    // observed up front: the pending entry is written before anything else can
    // run, so a vanishing process always leaves `running` behind to be read
    // back as `interrupted`.
    let scope_key = ask_scope_key(app, coordinate).await.ok();
    let ownership = VerifiedStagingOwnership::load(app).ok();
    // What the scope can say before resolution verified it: the observed key
    // fields plus the caller-supplied names, marked as such (empty relay).
    let provisional_scope = scope_key
        .as_ref()
        .map(|key| history_scope_for(key, agent_id, coordinate));

    if let Err(failure) = super::attempt::check_question(question, attempt) {
        // A withdrawn or malformed question never reached a scope, but the
        // refusal still reports the scope it would have been under.
        return DevRunResult {
            outcome: DevOutcome::Refused(failure),
            scope: provisional_scope,
            question_id: meta.question_id,
            source_revision: None,
            manifest: None,
            history_recorded: false,
        };
    }

    // Write the attempt's identity before any resolution or launch. The scope
    // here is provisional — the terminal write carries the resolved one.
    let mut history_recorded = match (ownership.as_ref(), provisional_scope.as_ref()) {
        (Some(ownership), Some(scope)) => super::history::upsert(
            ownership,
            super::history::PrivateAskHistoryEntry::pending(
                scope,
                &meta.question_id,
                meta.follow_up_of.clone(),
                question,
                attempt.attempt_id(),
                None,
                now,
            ),
            now,
        )
        .is_ok(),
        _ => false,
    };

    // A follow-up names the attempt it builds on. The prior turns are read
    // from this viewer's own scoped history — the record the earlier attempt
    // left — never from caller-supplied text.
    let mut question_id = meta.question_id;
    let mut prior: Vec<PriorTurn> = Vec::new();
    if let Some(parent) = meta.follow_up_of.as_deref() {
        let thread = match (ownership.as_ref(), scope_key.as_ref()) {
            (Some(ownership), Some(key)) => {
                super::history::follow_up_thread(ownership, key, parent, is_live.as_ref(), now)
            }
            _ => None,
        };
        match thread {
            Some((thread_id, turns)) => {
                question_id = thread_id;
                prior = turns;
            }
            None => {
                let failure = PrivateAskFailure::FollowUpUnavailable;
                history_recorded = match (ownership.as_ref(), provisional_scope.as_ref()) {
                    (Some(ownership), Some(scope)) => super::history::upsert(
                        ownership,
                        super::history::PrivateAskHistoryEntry::finished_refusal(
                            scope,
                            &question_id,
                            meta.follow_up_of.clone(),
                            question,
                            attempt.attempt_id(),
                            None,
                            &failure,
                            now,
                        ),
                        now,
                    )
                    .is_ok(),
                    _ => history_recorded,
                };
                return DevRunResult {
                    outcome: DevOutcome::Refused(failure),
                    scope: provisional_scope,
                    question_id,
                    source_revision: None,
                    manifest: None,
                    history_recorded,
                };
            }
        }
    }

    let resolved = resolve_selection(
        app,
        agent_id,
        coordinate,
        question.to_owned(),
        prior,
        now,
        attempt.clone(),
    )
    .await;

    let resolved = match resolved {
        Err(refusal) => {
            // The refusal is terminal even when resolution died before it
            // could verify a scope: the record lands under the provisional
            // scope the pending write used, so a viewer reads `refused` back
            // instead of an entry stuck reporting `interrupted`.
            if let (Some(ownership), Some(scope)) = (
                ownership.as_ref(),
                refusal.scope.as_ref().or(provisional_scope.as_ref()),
            ) {
                history_recorded = super::history::upsert(
                    ownership,
                    super::history::PrivateAskHistoryEntry::finished_refusal(
                        scope,
                        &question_id,
                        meta.follow_up_of.clone(),
                        question,
                        attempt.attempt_id(),
                        refusal.source_revision.as_deref(),
                        &refusal.failure,
                        now,
                    ),
                    now,
                )
                .is_ok();
            }
            return DevRunResult {
                outcome: DevOutcome::Refused(refusal.failure),
                scope: refusal.scope.clone().or(provisional_scope),
                question_id,
                source_revision: refusal.source_revision,
                manifest: refusal.manifest,
                history_recorded,
            };
        }
        Ok(resolved) => resolved,
    };

    match resolved.step {
        ResolveStep::Insufficient(manifest) => {
            if let Some(ownership) = ownership.as_ref() {
                history_recorded = super::history::upsert(
                    ownership,
                    super::history::PrivateAskHistoryEntry::insufficient(
                        &resolved.scope,
                        &question_id,
                        meta.follow_up_of,
                        question,
                        attempt.attempt_id(),
                        resolved.source_revision.as_deref(),
                        manifest.clone(),
                        now,
                    ),
                    now,
                )
                .is_ok();
            }
            DevRunResult {
                outcome: DevOutcome::Insufficient(manifest.clone()),
                scope: Some(resolved.scope),
                question_id,
                source_revision: resolved.source_revision,
                manifest: Some(manifest),
                history_recorded,
            }
        }
        ResolveStep::Ready(selection) => {
            let scope = resolved.scope;
            let outcome = match tokio::task::spawn_blocking(move || (*selection).answer()).await {
                Ok(Ok(response)) => DevOutcome::Answered(Box::new(response)),
                Ok(Err(failure)) => DevOutcome::Refused(failure),
                Err(_) => DevOutcome::Refused(PrivateAskFailure::InvalidState),
            };
            if let Some(ownership) = ownership.as_ref() {
                let entry = match &outcome {
                    DevOutcome::Answered(response) => {
                        Some(super::history::PrivateAskHistoryEntry::answered(
                            &scope,
                            &question_id,
                            meta.follow_up_of.clone(),
                            question,
                            response,
                            now,
                        ))
                    }
                    DevOutcome::Refused(failure) => {
                        Some(super::history::PrivateAskHistoryEntry::finished_refusal(
                            &scope,
                            &question_id,
                            meta.follow_up_of.clone(),
                            question,
                            attempt.attempt_id(),
                            resolved.source_revision.as_deref(),
                            failure,
                            now,
                        ))
                    }
                    // A ready step cannot resolve to insufficient — that
                    // outcome is decided before the selection is built.
                    DevOutcome::Insufficient(_) => None,
                };
                if let Some(entry) = entry {
                    history_recorded = super::history::upsert(ownership, entry, now).is_ok();
                }
            }
            DevRunResult {
                outcome,
                scope: Some(scope),
                question_id,
                source_revision: resolved.source_revision,
                manifest: resolved.manifest,
                history_recorded,
            }
        }
    }
}

/// A best-effort scope for a record whose resolution could not produce one.
///
/// The key fields are observed (community, viewer, repository); the agent and
/// relay fields are the names the caller gave, marked honestly — they were
/// never verified because the attempt refused before either was bound.
fn history_scope_for(key: &ScopeKey, agent_id: &str, coordinate: &str) -> HistoryScope {
    HistoryScope {
        community_id: key.community_id.clone(),
        relay_url: String::new(),
        viewer_pubkey: key.viewer_pubkey.clone(),
        agent_pubkey: agent_id.to_ascii_lowercase(),
        project_id: coordinate.to_owned(),
        repo_owner: key.repo_owner.clone(),
        repo_d: key.repo_d.clone(),
    }
}

/// One agent as the picker sees it: named, addressed, and honest about why it
/// can or cannot be asked right now.
///
/// `status` is what the resolver itself would conclude at this moment — a
/// `busy` agent has a live session mid-turn, an `offline` one has no live
/// generation at all. The renderer offers a recovery (start the agent, or pick
/// another), never a substitution.
pub(crate) struct ObservedAskAgent {
    pub(crate) pubkey: String,
    pub(crate) name: String,
    /// The relay the agent's harness is bound to — what the runtime-start
    /// command needs to bring it back.
    pub(crate) relay_url: String,
    pub(crate) status: &'static str,
}

/// All managed agents on this machine with their askability.
///
/// Unlike the old `live_agents` this lists every record, because the picker
/// must show *why* an agent cannot be asked rather than silently dropping it —
/// an agent that vanished from the list between render and resolve would look
/// like a crash.
pub(crate) fn observed_agents<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Vec<ObservedAskAgent> {
    let Ok(records) = crate::managed_agents::storage::load_managed_agents(app) else {
        return Vec::new();
    };
    records
        .into_iter()
        .filter(|record| !record.pubkey.is_empty())
        .filter_map(|record| {
            let relay_url = bound_relay_url(app, &record).ok()?;
            let status = match live_runtime(app, &record, &relay_url) {
                Some(live) => match live.lifecycle {
                    crate::managed_agents::runtime_types::ManagedAgentRuntimeLifecycle::Ready => {
                        "ready"
                    }
                    crate::managed_agents::runtime_types::ManagedAgentRuntimeLifecycle::Starting
                    | crate::managed_agents::runtime_types::ManagedAgentRuntimeLifecycle::Listening
                    | crate::managed_agents::runtime_types::ManagedAgentRuntimeLifecycle::Waking => {
                        "busy"
                    }
                    crate::managed_agents::runtime_types::ManagedAgentRuntimeLifecycle::Failed
                    | crate::managed_agents::runtime_types::ManagedAgentRuntimeLifecycle::Stopped => {
                        "offline"
                    }
                },
                None => "offline",
            };
            Some(ObservedAskAgent {
                pubkey: record.pubkey.clone(),
                name: record.name.clone(),
                relay_url,
                status,
            })
        })
        .collect()
}
