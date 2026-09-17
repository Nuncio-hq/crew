//! The resolver, bound to its production producers.
//!
//! Each test names the production line whose removal makes it fail. The
//! observation these tests hand in is the same shape `selection_native`
//! assembles; nothing here reaches past `resolve_observed_selection`, which is
//! the function the command's own path calls.

use super::super::tests::{canonical_tempdir, executable, owned_receipt, runtime_installation};
use super::*;
use crate::managed_agents::effective_config::{ConfigSource, EffectiveAgentConfig, ResolvedField};
use crate::managed_agents::global_config::GlobalAgentConfig;
use crate::managed_agents::types::AgentDefinition;
use crew_wiki::snapshot_v1::VerifiedSnapshot;
use crew_wiki::snapshot_v1_build::{build_snapshot, SnapshotBuild, SnapshotPublication};
use nostr::{Keys, SecretKey};
use std::path::Path;

const AGENT_PUBKEY: &str = "b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1b1";
const RELAY_URL: &str = "wss://relay.example/";

fn owner_keys() -> Keys {
    let mut bytes = [0_u8; 32];
    bytes[31] = 9;
    Keys::new(SecretKey::from_slice(&bytes).expect("owner key"))
}

/// One really signed, really verified Wiki snapshot. The resolver decides
/// nothing about a snapshot itself; it hands one to
/// `PrivateAskRequest::from_verified_snapshot`, which is where the repository
/// half of the scope is checked against the signed manifest.
fn publication() -> SnapshotPublication {
    use crew_wiki::git_snapshot::RepoSnapshot;
    use crew_wiki::publish::PageDraft;
    use crew_wiki::types::{PlannedPage, PlannedSection, WikiPlan};
    use std::collections::BTreeMap;

    let keys = owner_keys();
    let owner = keys.public_key().to_hex();
    let commit = "a".repeat(40);
    let snapshot = RepoSnapshot {
        commit: commit.clone(),
        branch: "main".into(),
        files: vec!["src/lib.rs".to_owned()],
        contents: BTreeMap::from([("src/lib.rs".to_owned(), "fn answer() {\n    42\n}\n".into())]),
        source_revision: format!("git:{commit}"),
        omissions: Vec::new(),
    };
    let plan = WikiPlan {
        language: "en".into(),
        sections: vec![PlannedSection {
            id: "overview".into(),
            title: "Overview".into(),
            pages: vec![PlannedPage {
                slug: "lib".into(),
                title: "Lib".into(),
                section: "overview".into(),
                source_files: vec!["src/lib.rs".into()],
            }],
        }],
    };
    let drafts = vec![PageDraft {
        slug: "lib".into(),
        title: "Lib".into(),
        section: "overview".into(),
        source_files: vec!["src/lib.rs".into()],
        commit: commit.clone(),
        language: "en".into(),
        content: "# lib\n".into(),
    }];
    build_snapshot(SnapshotBuild {
        owner: &owner,
        repo_d: "repo-a",
        snapshot: &snapshot,
        plan: &plan,
        drafts: &drafts,
        cadence: "manual",
        snapshot_id: Some("12345678-1234-4234-9234-123456789abc"),
        expected_revision: None,
        created_at: 10,
        keys: &keys,
    })
    .expect("a signed snapshot")
}

fn verified(publication: &SnapshotPublication) -> VerifiedSnapshot {
    let owner = owner_keys().public_key().to_hex();
    let head = serde_json::to_value(&publication.head).expect("head");
    let manifest = serde_json::to_value(&publication.manifest).expect("manifest");
    let pages: Vec<serde_json::Value> = publication
        .pages
        .iter()
        .map(|page| serde_json::to_value(page).expect("page"))
        .collect();
    crew_wiki::snapshot_v1::verify_snapshot(&owner, "repo-a", &head, &manifest, &pages)
        .expect("the fixture snapshot verifies")
}

fn scope() -> PrivateAskScope {
    PrivateAskScope {
        community_id: "community-a".into(),
        relay_url: RELAY_URL.into(),
        viewer_pubkey: "a".repeat(64),
        agent_pubkey: AGENT_PUBKEY.into(),
        project_id: "project-a".into(),
        repo_owner: owner_keys().public_key().to_hex(),
        repo_d: "repo-a".into(),
    }
}

fn record() -> ManagedAgentRecord {
    serde_json::from_value(serde_json::json!({
        "pubkey": AGENT_PUBKEY,
        "name": "scout",
        "private_key_nsec": "nsec1fake",
        "relay_url": RELAY_URL,
        "acp_command": "buzz-acp",
        "agent_command": "claude-agent-acp",
        "agent_args": [],
        "mcp_command": "",
        "turn_timeout_seconds": 320,
        "created_at": "",
        "updated_at": ""
    }))
    .expect("record fixture")
}

fn resolved_config() -> EffectiveConfigResult {
    EffectiveConfigResult::Resolved(EffectiveAgentConfig {
        model: ResolvedField {
            value: Some("claude-fable-5-1".into()),
            source: ConfigSource::Definition,
        },
        provider: ResolvedField {
            value: Some("anthropic".into()),
            source: ConfigSource::Definition,
        },
        system_prompt: ResolvedField {
            value: Some("You are Scout, the repository archaeologist.".into()),
            source: ConfigSource::Definition,
        },
    })
}

/// A ledger directory with one entry, as a harness that has run writes.
fn ledger(directory: &Path, entries: usize) -> std::path::PathBuf {
    let path = directory.join("ledger");
    std::fs::create_dir_all(&path).expect("ledger directory");
    for index in 0..entries {
        std::fs::write(path.join(format!("{index}.json")), b"{\"turn\":\"idle\"}")
            .expect("ledger entry");
    }
    path
}

fn live(lifecycle: ManagedAgentRuntimeLifecycle) -> LiveAgentRuntime {
    LiveAgentRuntime {
        acp_pid: 4321,
        start_nonce: "harness-generation-nonce".into(),
        lifecycle,
    }
}

struct Observation {
    record: ManagedAgentRecord,
    config: EffectiveConfigResult,
    runtime: std::path::PathBuf,
    ledger_dir: std::path::PathBuf,
    live: Option<LiveAgentRuntime>,
}

impl Observation {
    fn new(directory: &Path) -> Self {
        let runtime = runtime_installation(directory).join("claude");
        std::fs::write(&runtime, b"#!/usr/bin/perl\nprint 1;\n").expect("runtime");
        Self {
            record: record(),
            config: resolved_config(),
            runtime,
            ledger_dir: ledger(directory, 1),
            live: Some(live(ManagedAgentRuntimeLifecycle::Ready)),
        }
    }

    fn agent(&self) -> ObservedAgent<'_> {
        ObservedAgent {
            record: &self.record,
            config: &self.config,
            owner_only_access: true,
            relay_url: RELAY_URL,
            live: self.live.clone(),
            ledger_dir: self.ledger_dir.clone(),
            runtime_id: "claude".into(),
            executable: executable(&self.runtime),
            effective_model: "claude-fable-5-1".into(),
            profile: None,
            hermes_profile: None,
        }
    }
}

fn resolve(
    observation: &Observation,
    scope: PrivateAskScope,
    fixture: &tempfile::TempDir,
    snapshot: &VerifiedSnapshot,
) -> Result<PrivateAskSelection, PrivateAskFailure> {
    resolve_observed_selection(
        scope,
        "What does answer do?".into(),
        observation.agent(),
        snapshot,
        Vec::new(),
        owned_receipt(fixture),
        1_000,
        AttemptIdentity::fresh(),
    )
}

#[test]
fn a_bound_agent_resolves_to_a_selection_built_from_the_observation() {
    let fixture = canonical_tempdir();
    let publication = publication();
    let snapshot = verified(&publication);
    let observation = Observation::new(fixture.path());

    let selection =
        resolve(&observation, scope(), &fixture, &snapshot).expect("a bound agent resolves");

    // Production line: the `session_generation(&live.start_nonce)` argument of
    // the `SelectedAgentState::from_effective_config` call in
    // `resolve_observed_selection`. Passing the nonce itself, or a value from
    // the request, fails here.
    assert_eq!(
        selection.session_generation(),
        &session_generation("harness-generation-nonce")
    );
    assert_ne!(selection.session_generation(), "harness-generation-nonce");
    // Production line: the `acl_fingerprint(&scope, agent.record, ...)`
    // argument of the same call.
    assert_eq!(
        selection.acl_fingerprint(),
        &acl_fingerprint(&scope(), &observation.record, true)
    );
    assert_eq!(selection.lifecycle(), AgentLifecycle::Idle);
}

#[test]
fn a_request_cannot_name_the_acl_projection_or_the_session_generation() {
    // THE case: both values are recomputed from the observation, so two
    // different observations must resolve to different values even though the
    // request is byte-identical, and one observation must resolve to the same
    // value however the request is written.
    let fixture = canonical_tempdir();
    let publication = publication();
    let snapshot = verified(&publication);
    let mut observation = Observation::new(fixture.path());

    let first = resolve(&observation, scope(), &fixture, &snapshot).expect("first");
    let generation = first.session_generation().to_owned();
    let acl = first.acl_fingerprint().to_owned();

    // A restarted harness generation.
    observation.live = Some(LiveAgentRuntime {
        start_nonce: "a-different-generation".into(),
        ..live(ManagedAgentRuntimeLifecycle::Ready)
    });
    let restarted = resolve(&observation, scope(), &fixture, &snapshot).expect("restarted");
    assert_ne!(restarted.session_generation(), generation);

    // A widened access projection, with the request unchanged.
    observation.record.respond_to = crate::managed_agents::types::RespondTo::Anyone;
    observation.record.respond_to_allowlist = vec!["c".repeat(64)];
    let widened = ObservedAgent {
        owner_only_access: false,
        ..{
            let agent = observation.agent();
            agent
        }
    };
    let widened = resolve_observed_selection(
        scope(),
        "What does answer do?".into(),
        widened,
        &snapshot,
        Vec::new(),
        owned_receipt(&fixture),
        1_000,
        AttemptIdentity::fresh(),
    )
    .expect("widened");
    assert_ne!(widened.acl_fingerprint(), acl);
}

#[test]
fn an_agent_with_no_ledger_entry_is_refused_before_anything_launches() {
    let fixture = canonical_tempdir();
    let publication = publication();
    let snapshot = verified(&publication);
    let mut observation = Observation::new(fixture.path());
    // An empty ledger directory: the harness is running but has written
    // nothing this attempt could be isolated from. Production line: the
    // `observation.capture()?` inside `observed_session`.
    observation.ledger_dir = ledger(&fixture.path().join("empty-session"), 0);

    assert_eq!(
        resolve(&observation, scope(), &fixture, &snapshot).unwrap_err(),
        PrivateAskFailure::SessionObservationUnavailable
    );

    // And so is an agent with no live harness generation at all.
    observation.ledger_dir = ledger(fixture.path(), 1);
    observation.live = None;
    assert_eq!(
        resolve(&observation, scope(), &fixture, &snapshot).unwrap_err(),
        PrivateAskFailure::SessionObservationUnavailable
    );
}

#[test]
fn an_orphaned_instance_is_unbound() {
    // `AgentUnbound` is reserved for an agent that has no effective
    // configuration to speak with. Production line: the
    // `EffectiveConfigResult::Resolved` binding inside
    // `SelectedAgentState::from_effective_config`.
    let fixture = canonical_tempdir();
    let publication = publication();
    let snapshot = verified(&publication);
    let mut observation = Observation::new(fixture.path());
    observation.config = EffectiveConfigResult::OrphanedInstance {
        record_pubkey: AGENT_PUBKEY.into(),
        missing_persona_id: "gone".into(),
    };

    assert_eq!(
        resolve(&observation, scope(), &fixture, &snapshot).unwrap_err(),
        PrivateAskFailure::AgentUnbound
    );
}

#[test]
fn a_scope_that_names_another_agent_or_repository_is_refused() {
    let fixture = canonical_tempdir();
    let publication = publication();
    let snapshot = verified(&publication);
    let observation = Observation::new(fixture.path());

    // Production line: the `scope.agent_pubkey` comparison against
    // `agent.record.pubkey` in `resolve_observed_selection`.
    let mut foreign_agent = scope();
    foreign_agent.agent_pubkey = "e".repeat(64);
    assert_eq!(
        resolve(&observation, foreign_agent, &fixture, &snapshot).unwrap_err(),
        PrivateAskFailure::ScopeMismatch
    );

    // Production line: the manifest comparison in
    // `PrivateAskRequest::from_verified_snapshot`, reached because the
    // resolver hands it the natively verified snapshot rather than a
    // caller-named repository.
    let mut foreign_repo = scope();
    foreign_repo.repo_d = "repo-b".into();
    assert_eq!(
        resolve(&observation, foreign_repo, &fixture, &snapshot).unwrap_err(),
        PrivateAskFailure::ScopeMismatch
    );

    // Production line: the `scope.relay_url` comparison against the bound
    // relay the harness is keyed on.
    let mut foreign_relay = scope();
    foreign_relay.relay_url = "wss://elsewhere.example/".into();
    assert_eq!(
        resolve(&observation, foreign_relay, &fixture, &snapshot).unwrap_err(),
        PrivateAskFailure::ScopeMismatch
    );
}

#[test]
fn a_busy_harness_generation_resolves_as_busy() {
    // Production line: `lifecycle_of`. A generation that is not `Ready` must
    // not resolve as idle, because admission's busy fence is the only thing
    // standing between a private Ask and a session mid-turn.
    let fixture = canonical_tempdir();
    let publication = publication();
    let snapshot = verified(&publication);
    let mut observation = Observation::new(fixture.path());
    observation.live = Some(live(ManagedAgentRuntimeLifecycle::Waking));

    let selection = resolve(&observation, scope(), &fixture, &snapshot).expect("busy resolves");
    assert_eq!(selection.lifecycle(), AgentLifecycle::Busy);

    // A stopped generation is a registry entry, not a live session; there is
    // nothing to be isolated from and it is refused rather than answered.
    observation.live = Some(live(ManagedAgentRuntimeLifecycle::Stopped));
    assert_eq!(
        resolve(&observation, scope(), &fixture, &snapshot).unwrap_err(),
        PrivateAskFailure::SessionObservationUnavailable
    );
}

/// A resolved selection answers through the production path: resolve, probe,
/// admit, run the contained one-shot. It needs the real Seatbelt boundary, so
/// it is macOS-only; `fail_closed_tests` covers the refusal elsewhere.
///
/// Production line: `PrivateAskSelection::answer`'s call into
/// `binding::answer`. If the resolver stopped handing the binding a session
/// observation, this fails as `IndependentInvocationUnverified` rather than
/// answering.
#[cfg(all(unix, target_os = "macos"))]
#[test]
fn a_resolved_selection_answers_through_the_production_path() {
    use super::super::tests::fake_runtime;

    let fixture = canonical_tempdir();
    let publication = publication();
    let snapshot = verified(&publication);
    let mut observation = Observation::new(fixture.path());
    // A benign runtime that answers without citing anything: this Ask carries
    // no grounding, and an answer that cited a path would be refused by the
    // citation fence — correctly, since there is nothing to resolve it against.
    observation.runtime = fake_runtime(
        fixture.path(),
        "claude",
        "#!/usr/bin/perl\nlocal $/; my $in = <STDIN>; die \"no prompt\" unless defined $in; print '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"It returns 42.\",\"modelUsage\":{\"claude-fable-5-1\":{}}}';\n",
    );
    // One real live child, so the PID in the observation comes from a handle
    // this fixture owns rather than from a PID probe.
    let mut session = std::process::Command::new("/usr/bin/perl")
        .arg("-e")
        .arg("sleep 120;")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("employee session");
    observation.live = Some(LiveAgentRuntime {
        acp_pid: session.id(),
        start_nonce: "harness-generation-nonce".into(),
        lifecycle: ManagedAgentRuntimeLifecycle::Ready,
    });

    let response = resolve(&observation, scope(), &fixture, &snapshot)
        .expect("a bound agent resolves")
        .answer();
    let _ = session.kill();
    let _ = session.wait();
    let response = response.expect("a resolved selection answers");

    assert!(response.markdown.starts_with("It returns 42."));
    assert!(response.citations.is_empty());
    assert_eq!(
        response.session_generation,
        session_generation("harness-generation-nonce")
    );
}

/// The definitions/global inputs the native shell resolves the effective
/// config from are the managed-agent layer's own; this keeps the unused-import
/// surface honest about which types the resolver depends on.
#[allow(dead_code)]
fn effective_config_inputs() -> (Vec<AgentDefinition>, GlobalAgentConfig) {
    (Vec::new(), GlobalAgentConfig::default())
}
