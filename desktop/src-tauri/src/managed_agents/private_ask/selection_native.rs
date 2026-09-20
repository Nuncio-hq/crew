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
use super::selection::{
    resolve_observed_selection, LiveAgentRuntime, ObservedAgent, PrivateAskSelection,
};
use super::{session_evidence, GroundedSource, PrivateAskFailure, PrivateAskScope};
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
    now: u64,
    attempt: AttemptIdentity,
) -> Result<PrivateAskSelection, PrivateAskFailure> {
    let ownership =
        VerifiedStagingOwnership::load(app).map_err(|_| PrivateAskFailure::InvalidState)?;
    // A scope that cannot be captured is this machine's own lock or identity
    // state, not a revocation: saying "access was revoked" would send the
    // reader to the wrong problem.
    let captured = crate::app_state::owner_scope::capture(app.clone())
        .await
        .map_err(|_| PrivateAskFailure::InvalidState)?;

    let record = agent_record(app, agent_id)?;
    let relay_url = bound_relay_url(app, &record)?;
    let (owner, repo_d) = coordinate_parts(coordinate)?;
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

    let snapshot_read =
        crate::commands::read_wiki_snapshot(app.clone(), captured.token.clone(), coordinate.into())
            .await
            .map_err(|_| PrivateAskFailure::AccessRevoked)?;
    let snapshot = verified_snapshot(&owner, &repo_d, &snapshot_read.value)?;

    let source_root = NativeSourceRoot::current(
        &app.state::<crate::commands::SourceState>(),
        &captured.token,
        coordinate,
    );
    let grounding = grounding(&snapshot, source_root.as_ref());

    let (runtime_id, executable, effective_model, profile) = runtime_selection(app, &record)?;
    let hermes_profile = profile
        .as_deref()
        .map(|profile| {
            ownership
                .hermes_profile_source()
                .map(|root| root.join(profile))
                .map_err(PrivateAskFailure::State)
        })
        .transpose()?;
    let live = live_runtime(app, &record, &relay_url);
    let ledger_dir = session_evidence::session_ledger_dir(&relay_url, &record.pubkey)
        .ok_or(PrivateAskFailure::SessionObservationUnavailable)?;
    let config = crate::managed_agents::effective_config::resolve_effective_config(
        &record,
        &agent_definitions(app),
        &global_config(app),
    );

    resolve_observed_selection(
        scope,
        question,
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
        grounding,
        ownership,
        now,
        attempt,
    )
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

/// Read the snapshot's own source references through the viewer's grant.
///
/// The decision of what may be grounded belongs to `selection::collect_grounding`;
/// this only supplies the reader.
fn grounding(
    snapshot: &crew_wiki::snapshot_v1::VerifiedSnapshot,
    source_root: Option<&NativeSourceRoot>,
) -> Vec<GroundedSource> {
    let Some(source_root) = source_root else {
        return Vec::new();
    };
    let deadline = std::time::Instant::now() + GROUNDING_DEADLINE;
    super::selection::collect_grounding(
        snapshot,
        source_root.workspace_mode(),
        |revision, reference| source_root.read_verified_reference(revision, reference, deadline),
    )
}

/// Run one private Ask from the developer surface.
///
/// This is the production entry point the dev command calls. It reaches a real
/// selection or refuses with the typed reason it could not: every safety
/// decision belongs to `resolve_observed_selection`, `admit_private_ask` and
/// `PrivateAskAttempt::run`, and nothing here invents a result or a reason.
///
/// The answer itself runs on a blocking worker, on ONE thread — which is what
/// the thread-scoped no-publish attribution relies on. The resolution above it
/// is async because it reads the Wiki snapshot natively.
pub(crate) async fn dev_run<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    agent_id: &str,
    coordinate: &str,
    question: &str,
    attempt: &AttemptIdentity,
    now: u64,
) -> Result<super::PrivateAskResponse, PrivateAskFailure> {
    super::attempt::check_question(question, attempt)?;
    let selection = resolve_selection(
        app,
        agent_id,
        coordinate,
        question.to_owned(),
        now,
        attempt.clone(),
    )
    .await?;
    tokio::task::spawn_blocking(move || selection.answer())
        .await
        .map_err(|_| PrivateAskFailure::InvalidState)?
}

/// The agents on this machine that currently have a live harness generation.
///
/// A private Ask can only be addressed to one of these — an agent with no
/// running session has no ledger to be isolated from — so the developer surface
/// offers exactly this set rather than every stored record.
pub(crate) fn live_agents<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Vec<(String, String)> {
    let Ok(records) = crate::managed_agents::storage::load_managed_agents(app) else {
        return Vec::new();
    };
    records
        .into_iter()
        .filter(|record| !record.pubkey.is_empty())
        .filter_map(|record| {
            let relay_url = bound_relay_url(app, &record).ok()?;
            live_runtime(app, &record, &relay_url)?;
            Some((record.pubkey.clone(), record.name.clone()))
        })
        .collect()
}
