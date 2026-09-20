//! The `private_ask_*` command surface, tested at its own boundary.
//!
//! Each test names the production line whose removal makes it fail. The app is
//! a real Tauri `MockRuntime` app with the production `AppState`; identity is
//! committed through `commit_imported_identity`, and the staging-ownership
//! seam is the crate's own test override (`set_test_recap_base`), which
//! `VerifiedStagingOwnership::load` honors — so scope capture, history writes
//! and the command bodies all run the shipped code. What is never stubbed is
//! the thing being tested: argument shapes, the register/cancel fence, scope
//! capture, and the owner-local record.

use super::*;
use crate::app_state::{build_app_state, AppState, IdentityStorage};
use crate::managed_agents::private_ask::history::{
    self, HistoryScope, PrivateAskHistoryEntry, ScopeKey,
};
use crate::managed_agents::private_ask::{PrivateAskFailure, PrivateAskResponse};
use tauri::Manager;
/// A unique app-data root per test app — the mock app's own identifier keeps
/// every run's store separate, the way two installs are separate.
struct AskTestApp {
    app: tauri::App<tauri::test::MockRuntime>,
    app_data_dir: std::path::PathBuf,
    identity_dir: tempfile::TempDir,
    history_base: tempfile::TempDir,
}

/// Tests that touch the process-wide test hooks serialize on this lock.
fn hooks_mutex() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    &LOCK
}

impl AskTestApp {
    fn new() -> Self {
        let identifier = format!(
            "xyz.nuncio.crew.test.private-ask-{}",
            uuid::Uuid::new_v4().simple()
        );
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().identifier = identifier;
        let app = tauri::test::mock_builder()
            .manage(build_app_state())
            .manage(crate::managed_agents::private_ask::PrivateAskAttempts::default())
            .build(context)
            .expect("fixture app");
        let app_data_dir = app.path().app_data_dir().expect("app data dir");
        Self {
            app,
            app_data_dir,
            identity_dir: tempfile::tempdir().expect("identity dir"),
            history_base: tempfile::tempdir().expect("history base"),
        }
    }

    /// Commit a fresh owner identity and point the staging-ownership seam at
    /// this test's base — the state a dev-gated developer machine is in.
    fn identified(&self, keys: nostr::Keys, relay_url: &str) -> nostr::Keys {
        let state = self.app.state::<AppState>();
        *state.relay_url_override.lock().unwrap() = Some(relay_url.to_string());
        let guard = state.identity_mutation.lock().unwrap();
        crate::commands::commit_imported_identity(
            &state,
            &guard,
            self.identity_dir.path(),
            keys.clone(),
            |_| Ok(IdentityStorage::LocalFile),
        )
        .expect("commit fixture identity");
        crate::managed_agents::recap_ownership::set_test_recap_base(Some(
            self.history_base.path().to_path_buf(),
        ));
        keys
    }

    fn ownership(&self) -> crate::managed_agents::recap_ownership::VerifiedStagingOwnership {
        crate::managed_agents::recap_ownership::VerifiedStagingOwnership::for_test_recap_base(
            self.history_base.path().to_path_buf(),
        )
    }
}

impl Drop for AskTestApp {
    fn drop(&mut self) {
        crate::managed_agents::recap_ownership::set_test_recap_base(None);
        if self.app_data_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.app_data_dir);
        }
    }
}

impl std::ops::Deref for AskTestApp {
    type Target = tauri::App<tauri::test::MockRuntime>;
    fn deref(&self) -> &Self::Target {
        &self.app
    }
}

fn owner_keys() -> nostr::Keys {
    let mut bytes = [0_u8; 32];
    bytes[31] = 7;
    nostr::Keys::new(nostr::SecretKey::from_slice(&bytes).expect("owner key"))
}

fn coordinate() -> String {
    format!("{}:repo-a", owner_keys().public_key().to_hex())
}

fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The scope key `ask_scope_key` produces for the committed identity and this
/// coordinate: the community is the relay origin, the viewer the committed
/// pubkey — never anything a caller could name. It is captured through the
/// same call the commands make, not recomputed.
async fn scope_key(
    app: &tauri::App<tauri::test::MockRuntime>,
    coordinate: &str,
) -> ScopeKey {
    crate::managed_agents::private_ask::ask_scope_key(&app.handle(), coordinate)
        .await
        .expect("the fixture identity captures a scope")
}

fn scope_for(key: &ScopeKey, agent: &str) -> HistoryScope {
    HistoryScope {
        community_id: key.community_id.clone(),
        relay_url: "wss://relay.example/".into(),
        viewer_pubkey: key.viewer_pubkey.clone(),
        agent_pubkey: agent.to_owned(),
        project_id: format!("{}:{}", key.repo_owner, key.repo_d),
        repo_owner: key.repo_owner.clone(),
        repo_d: key.repo_d.clone(),
    }
}

fn answered_entry(key: &ScopeKey, agent: &str, attempt: &str, markdown: &str) -> PrivateAskHistoryEntry {
    // A record stamped at real `now`: the command-layer paths prune on their
    // own clock, so a fixture timestamp of `10` would be decades stale.
    let now = now_seconds();
    PrivateAskHistoryEntry::answered(
        &scope_for(key, agent),
        attempt,
        None,
        "What does answer do?",
        &PrivateAskResponse {
            attempt_id: attempt.to_owned(),
            session_generation: "generation".into(),
            markdown: markdown.to_owned(),
            citations: Vec::new(),
            source_revision: "git:rev".into(),
            manifest: Default::default(),
            egress: crate::managed_agents::private_ask::tests::bounded_egress(),
        },
        now,
    )
}

/// `private_ask_run` checks the caller-minted ids before anything else — an
/// attempt id that was never minted by this surface is refused, never reaches
/// the registry, and never touches history.
///
/// Production line: the `valid_uuid_v4` fences at the top of `private_ask_run`.
#[tokio::test]
async fn a_run_with_an_id_this_surface_did_not_mint_is_refused() {
    let _hooks = hooks_mutex().lock().unwrap_or_else(|p| p.into_inner());
    let app = AskTestApp::new();
    let agent = "b".repeat(64);

    let error = private_ask_run(
        app.handle().clone(),
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        "not-an-attempt".into(),
        uuid(),
        agent.clone(),
        coordinate(),
        "what?".into(),
        None,
    )
    .await
    .expect_err("a foreign id shape is refused");
    assert!(error.contains("attempt"), "the reason names what failed");

    let error = private_ask_run(
        app.handle().clone(),
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        uuid(),
        "not-a-question".into(),
        agent.clone(),
        coordinate(),
        "what?".into(),
        None,
    )
    .await
    .expect_err("the question id is checked too");
    assert!(error.contains("attempt"), "both ids share the fence");

    let error = private_ask_run(
        app.handle().clone(),
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        uuid(),
        uuid(),
        agent.clone(),
        coordinate(),
        "what?".into(),
        Some("not-an-attempt".into()),
    )
    .await
    .expect_err("a foreign parent id is refused");
    assert!(error.contains("follow-up"), "the reason names the parent");
}

/// The only two things a caller may name — the agent and the repository —
/// are refused in the shape this surface mints, before either reaches a store
/// read or the resolver.
///
/// Production line: the `valid_agent_id`/`valid_coordinate`/question fences.
#[tokio::test]
async fn a_run_without_a_real_agent_coordinate_or_question_is_refused() {
    let _hooks = hooks_mutex().lock().unwrap_or_else(|p| p.into_inner());
    let app = AskTestApp::new();


    let error = private_ask_run(
        app.handle().clone(),
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        uuid(),
        uuid(),
        "not-an-agent".into(),
        coordinate(),
        "what?".into(),
        None,
    )
    .await
    .expect_err("an agent id must be a pubkey");
    assert!(error.contains("agent"));

    let error = private_ask_run(
        app.handle().clone(),
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        uuid(),
        uuid(),
        "b".repeat(64),
        "not-a-coordinate".into(),
        "what?".into(),
        None,
    )
    .await
    .expect_err("a coordinate must be owner:repo");
    assert!(error.contains("repository"));

    let error = private_ask_run(
        app.handle().clone(),
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        uuid(),
        uuid(),
        "b".repeat(64),
        coordinate(),
        "   ".into(),
        None,
    )
    .await
    .expect_err("an empty question is refused");
    assert!(error.contains("question"));
}

/// The attempt fence is the registry, not a flag the UI keeps: an id that is
/// already in flight is refused by `register`, and the message says *this*
/// attempt is running, not that the machine is busy.
///
/// Production line: `attempts.register(&attempt_id)` in `private_ask_run`.
#[tokio::test]
async fn a_second_run_on_an_in_flight_attempt_id_is_refused() {
    let _hooks = hooks_mutex().lock().unwrap_or_else(|p| p.into_inner());
    let app = AskTestApp::new();
    let attempt = uuid();
    let _held = app
        .state::<crate::managed_agents::private_ask::PrivateAskAttempts>()
        .register(&attempt)
        .expect("register the id in flight");

    let error = private_ask_run(
        app.handle().clone(),
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        attempt,
        uuid(),
        "b".repeat(64),
        coordinate(),
        "what?".into(),
        None,
    )
    .await
    .expect_err("a duplicate attempt id is refused");
    assert!(error.contains("already running"), "{error}");
}

/// A run that resolves to a refusal is still the viewer's record: `dev_run`
/// wrote the pending entry before resolution and the terminal entry after it,
/// so the refusal is readable back through the history command — the point of
/// the record. Nothing was launched.
///
/// Production lines: the pending upsert and the `finished_refusal` upsert in
/// `dev_run`, plus `ask_scope_key`'s observed scope in `private_ask_history`.
#[tokio::test]
async fn a_refused_attempt_is_recorded_and_reads_back_through_the_commands() {
    let _hooks = hooks_mutex().lock().unwrap_or_else(|p| p.into_inner());
    let app = AskTestApp::new();
    app.identified(owner_keys(), "wss://relay.example/");
    let coordinate = coordinate();
    let attempt = uuid();
    let question = uuid();

    let result = private_ask_run(
        app.handle().clone(),
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        attempt.clone(),
        question.clone(),
        "b".repeat(64),
        coordinate.clone(),
        "What does answer do?".into(),
        None,
    )
    .await
    .expect("a refusal is a result, not a command error");

    assert_eq!(result.attempt_id, attempt);
    assert_eq!(result.question_id, question);
    assert_eq!(result.status, "refused");
    // No such agent exists on this machine — the typed refusal, verbatim.
    let refusal = result.refusal.clone().expect("a refusal carries its reason");
    assert_eq!(
        refusal,
        PrivateAskFailure::AgentUnbound.to_string(),
        "the reason is the resolver's, not a paraphrase"
    );
    assert!(
        result.history_recorded,
        "the owner-local record kept the refusal"
    );

    // And the record reads back through the same command a reopening UI calls.
    let items = private_ask_history(
        app.handle().clone(),
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        coordinate.clone(),
    )
    .await
    .expect("history reads");
    assert_eq!(items.len(), 1, "the refusal is the viewer's one record");
    assert_eq!(items[0].attempt_id, attempt);
    assert_eq!(items[0].status, "refused");
    assert_eq!(items[0].question, "What does answer do?");
    // The registry no longer holds the id — the read resolves the stored
    // `running`-vs-terminal state correctly.
    assert!(
        !app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>()
            .is_registered(&attempt),
        "a finished attempt is released from the registry"
    );
}

/// History commands read under the *observed* key, never a caller-supplied
/// scope: a record stored under a different viewer is simply absent.
///
/// Production line: `ask_scope_key` fed into `history::load_scoped`.
#[tokio::test]
async fn another_viewers_record_is_absent_from_the_history_read() {
    let _hooks = hooks_mutex().lock().unwrap_or_else(|p| p.into_inner());
    let app = AskTestApp::new();
    app.identified(owner_keys(), "wss://relay.example/");
    let coordinate = coordinate();
    let key = scope_key(&app, &coordinate).await;

    // A foreign-viewer record written directly into the same file.
    let mut foreign = scope_for(&key, &"b".repeat(64));
    foreign.viewer_pubkey = "f".repeat(64);
    let mut foreign_entry = answered_entry(&key, &"b".repeat(64), &uuid(), "foreign answer");
    foreign_entry.scope = foreign;
    history::upsert(&app.ownership(), foreign_entry, 10).expect("foreign write");

    let items = private_ask_history(
        app.handle().clone(),
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        coordinate.clone(),
    )
    .await
    .expect("history reads");
    assert!(
        items.is_empty(),
        "the read is scoped to the observed viewer, not the file"
    );
}

/// The draft input is the read-only validated answer #367 consumes: it exists
/// for an answered attempt in this scope and carries the record's own fields —
/// question id, attempt id, material, manifest, provenance — and producing it
/// sends nothing anywhere.
///
/// Production line: the `entry.status == HistoryStatus::Answered` guard and
/// field mapping in `private_ask_draft`.
#[tokio::test]
async fn the_draft_exists_only_for_an_answered_attempt_in_scope() {
    let _hooks = hooks_mutex().lock().unwrap_or_else(|p| p.into_inner());
    let app = AskTestApp::new();
    let keys = app.identified(owner_keys(), "wss://relay.example/");
    let coordinate = coordinate();
    let key = scope_key(&app, &coordinate).await;
    let agent = "b".repeat(64);

    let answered = uuid();
    let refused = uuid();
    let foreign = uuid();
    history::upsert(
        &app.ownership(),
        answered_entry(&key, &agent, &answered, "the answer is 42"),
        10,
    )
    .expect("answered write");
    history::upsert(
        &app.ownership(),
        PrivateAskHistoryEntry::finished_refusal(
            &scope_for(&key, &agent),
            &refused,
            None,
            "What does answer do?",
            &refused,
            Some("git:rev"),
            &PrivateAskFailure::AgentUnbound,
            20,
        ),
        20,
    )
    .expect("refused write");
    // Same attempt id shape, foreign viewer — must not be servable.
    let mut foreign_scope = scope_for(&key, &agent);
    foreign_scope.viewer_pubkey = "f".repeat(64);
    let mut foreign_entry = answered_entry(&key, &agent, &foreign, "foreign");
    foreign_entry.scope = foreign_scope;
    history::upsert(&app.ownership(), foreign_entry, 30).expect("foreign write");

    let draft = private_ask_draft(
        app.handle().clone(),
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        coordinate.clone(),
        answered.clone(),
    )
    .await
    .expect("the draft reads")
    .expect("an answered attempt drafts");
    assert_eq!(draft.attempt_id, answered);
    assert_eq!(draft.question_id, answered);
    assert_eq!(draft.markdown, "the answer is 42");
    assert_eq!(draft.origin_agent, agent);
    assert_eq!(draft.origin_coordinate, coordinate);
    assert_eq!(draft.source_revision.as_deref(), Some("git:rev"));

    // Refusals and foreign-scope records have no validated material.
    assert!(
        private_ask_draft(
            app.handle().clone(),
            app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
            coordinate.clone(),
            refused.clone(),
        )
        .await
        .expect("read")
        .is_none(),
        "a refused attempt has nothing to draft"
    );
    assert!(
        private_ask_draft(
            app.handle().clone(),
            app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
            coordinate.clone(),
            foreign.clone(),
        )
        .await
        .expect("read")
        .is_none(),
        "another viewer's answered attempt is absent, not drafted"
    );

    let _ = keys;
}

/// `private_ask_forget` removes the named attempt's record from the observed
/// scope and nothing else — the only write that takes history away.
///
/// Production line: `history::forget`'s scoped retain.
#[tokio::test]
async fn forgetting_removes_the_named_attempt_from_the_observed_scope() {
    let _hooks = hooks_mutex().lock().unwrap_or_else(|p| p.into_inner());
    let app = AskTestApp::new();
    app.identified(owner_keys(), "wss://relay.example/");
    let coordinate = coordinate();
    let key = scope_key(&app, &coordinate).await;
    let agent = "b".repeat(64);
    let gone = uuid();
    let kept = uuid();

    history::upsert(&app.ownership(), answered_entry(&key, &agent, &gone, "gone"), 10)
        .expect("write");
    history::upsert(&app.ownership(), answered_entry(&key, &agent, &kept, "kept"), 20)
        .expect("write");

    assert!(
        private_ask_forget(app.handle().clone(), coordinate.clone(), gone.clone())
            .await
            .expect("forget"),
        "the named record was removed"
    );
    let items = private_ask_history(
        app.handle().clone(),
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        coordinate.clone(),
    )
    .await
    .expect("history");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].attempt_id, kept);

    // Forgetting again is a no-op, not an error.
    assert!(
        !private_ask_forget(app.handle().clone(), coordinate, gone)
            .await
            .expect("forget")
    );
}

/// Cancelling is keyed by the registry: an unknown id — or one that already
/// finished — is a no-op, and a non-id shape is ignored outright.
///
/// Production line: the `valid_uuid_v4` gate and `attempts.cancel` call.
#[tokio::test]
async fn cancelling_an_unknown_or_finished_attempt_is_a_no_op() {
    let _hooks = hooks_mutex().lock().unwrap_or_else(|p| p.into_inner());
    let app = AskTestApp::new();
    private_ask_cancel(
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        Some(uuid()),
    )
    .await
    .expect("an unknown id is a no-op");
    private_ask_cancel(
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        Some("not-an-id".into()),
    )
    .await
    .expect("a non-id is ignored");
    private_ask_cancel(
        app.state::<crate::managed_agents::private_ask::PrivateAskAttempts>(),
        None,
    )
    .await
    .expect("nothing to cancel");
}

/// The attempt's own events become the two events the UI subscribes to,
/// tagged with the attempt id — a renderer never infers a phase it was not
/// told, and a stale attempt's event can never masquerade as the current one.
///
/// Production line: `progress_event`, the mapping the command's reporter
/// emits verbatim.
#[test]
fn progress_events_carry_the_attempt_id_and_phase() {
    use crate::managed_agents::private_ask::AskEvent;
    let attempt = uuid();

    let (name, payload) = progress_event(&attempt, AskEvent::Retrieving);
    assert_eq!(name, "private-ask:progress");
    assert_eq!(payload["attemptId"], attempt);
    assert_eq!(payload["phase"], "retrieving");

    let (name, payload) = progress_event(&attempt, AskEvent::Running);
    assert_eq!(name, "private-ask:progress");
    assert_eq!(payload["phase"], "running");

    let (name, payload) = progress_event(&attempt, AskEvent::Chunk("bytes".into()));
    assert_eq!(name, "private-ask:chunk");
    assert_eq!(payload["text"], "bytes");
}
