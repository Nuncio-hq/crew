//! Headless acceptance of the durable Wiki task dispatch (#367).
//!
//! These tests bind the production commands (`*_at_path` with a real SQLite
//! `OperationStore`) against a loopback relay that serves the kind:39002
//! membership authority and stores published events deduped by id. The relay
//! is only a transport fixture; signing, operation identity, CAS, and
//! reconciliation are all the shipping code.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nostr::{Event, EventBuilder, JsonUtil, Keys, Kind, Tag, Timestamp};
use tauri::Manager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::wiki_task_dispatch::{
    wiki_task_dispatch_abandon_at_path, wiki_task_dispatch_prepare_at_path,
    wiki_task_dispatch_reconcile_at_path, wiki_task_dispatch_submit_at_path, WikiTaskDispatchInput,
    WikiTaskReferenceRecord,
};
use crate::app_state::owner_scope::capture;
use crate::app_state::{build_app_state, AppState};
use crate::owner_operations::{Limits, OperationStatus, OperationStore};

const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 256;

#[derive(Default)]
struct RelayState {
    /// Events the relay has accepted, deduped by id — the ground truth for
    /// "how many kickoffs actually exist".
    events: Vec<Event>,
    /// Every publish request body, deduplicated or not — the double-submit
    /// lens the mutation check relies on.
    publish_bodies: Vec<Event>,
    /// Store the event but answer 503 — the ambiguous accepted/lost-ack case.
    store_then_fail: usize,
    /// Relay-authored kind:39002 snapshot for the channel under test.
    membership: Option<Event>,
    relay_pubkey: String,
}

struct ServerGuard(tokio::task::JoinHandle<()>);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn request(socket: &mut tokio::net::TcpStream) -> Result<(String, Vec<u8>), String> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let headers_end = loop {
        let count = tokio::time::timeout(Duration::from_secs(10), socket.read(&mut chunk))
            .await
            .map_err(|_| "request header timeout".to_string())?
            .map_err(|_| "request header read failed".to_string())?;
        if count == 0 {
            return Err("client closed before request headers".into());
        }
        if bytes.len().saturating_add(count) > MAX_REQUEST_BYTES {
            return Err("request exceeds test bound".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break index;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..headers_end]).into_owned();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let body_start = headers_end + 4;
    let body_end = body_start
        .checked_add(content_length)
        .ok_or_else(|| "request length overflow".to_string())?;
    if body_end > MAX_REQUEST_BYTES {
        return Err("request exceeds test bound".into());
    }
    while bytes.len() < body_end {
        let count = tokio::time::timeout(Duration::from_secs(10), socket.read(&mut chunk))
            .await
            .map_err(|_| "request body timeout".to_string())?
            .map_err(|_| "request body read failed".to_string())?;
        if count == 0 {
            return Err("client closed before request body".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err("request exceeds test bound".into());
        }
    }
    Ok((headers, bytes[body_start..body_end].to_vec()))
}

fn request_path(headers: &str) -> String {
    headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn response(status: u16, value: &impl serde::Serialize) -> Vec<u8> {
    let body = serde_json::to_vec(value).expect("test response JSON");
    let reason = if status == 200 {
        "OK"
    } else {
        "Service Unavailable"
    };
    let mut bytes = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(&body);
    bytes
}

fn tag_value(event: &Event, name: &str) -> Option<String> {
    event
        .tags
        .iter()
        .find(|tag| tag.as_slice().first().is_some_and(|n| n == name))
        .and_then(|tag| tag.as_slice().get(1).map(ToString::to_string))
}

fn matches_filter(event: &Event, filter: &serde_json::Value) -> bool {
    let kind = event.kind.as_u16() as u64;
    let kinds_match = filter
        .get("kinds")
        .and_then(serde_json::Value::as_array)
        .is_none_or(|values| values.iter().any(|value| value.as_u64() == Some(kind)));
    let author_match = filter
        .get("authors")
        .and_then(serde_json::Value::as_array)
        .is_none_or(|values| {
            let owner = event.pubkey.to_hex();
            values
                .iter()
                .any(|value| value.as_str() == Some(owner.as_str()))
        });
    let d_match = filter
        .get("#d")
        .and_then(serde_json::Value::as_array)
        .is_none_or(|values| {
            values
                .iter()
                .any(|value| value.as_str() == tag_value(event, "d").as_deref())
        });
    let id_match = filter
        .get("ids")
        .and_then(serde_json::Value::as_array)
        .is_none_or(|values| {
            let id = event.id.to_hex();
            values
                .iter()
                .any(|value| value.as_str() == Some(id.as_str()))
        });
    kinds_match && author_match && d_match && id_match
}

async fn relay(listener: TcpListener, state: Arc<Mutex<RelayState>>) {
    for _ in 0..MAX_CONNECTIONS {
        let Ok(Ok((mut socket, _))) =
            tokio::time::timeout(Duration::from_secs(20), listener.accept()).await
        else {
            return;
        };
        let Ok((headers, body)) = request(&mut socket).await else {
            return;
        };
        let payload = match request_path(&headers).as_str() {
            "/" => {
                let pubkey = state.lock().expect("relay state").relay_pubkey.clone();
                response(200, &serde_json::json!({ "self": pubkey }))
            }
            "/query" => {
                let filters: Vec<serde_json::Value> =
                    serde_json::from_slice(&body).expect("query JSON");
                let filter = filters.first().expect("one query filter");
                let events: Vec<Event> = {
                    let state = state.lock().expect("relay state");
                    let mut pool = state.events.clone();
                    if let Some(membership) = state.membership.clone() {
                        pool.push(membership);
                    }
                    pool.into_iter()
                        .filter(|event| matches_filter(event, filter))
                        .collect()
                };
                response(200, &events)
            }
            "/events" => {
                let event: Event = serde_json::from_slice(&body).expect("event JSON");
                let mut state = state.lock().expect("relay state");
                let ambiguous = state.store_then_fail > 0;
                if ambiguous {
                    state.store_then_fail -= 1;
                }
                if !state.events.iter().any(|existing| existing.id == event.id) {
                    state.events.push(event.clone());
                }
                state.publish_bodies.push(event.clone());
                if ambiguous {
                    // The kickoff landed; only the acknowledgement was lost.
                    response(
                        503,
                        &serde_json::json!({ "error": "temporary test outage" }),
                    )
                } else {
                    response(
                        200,
                        &serde_json::json!({
                            "event_id": event.id.to_hex(),
                            "accepted": true,
                            "message": "stored"
                        }),
                    )
                }
            }
            _ => response(
                400,
                &serde_json::json!({ "error": "unsupported test path" }),
            ),
        };
        let _ = tokio::time::timeout(Duration::from_secs(10), socket.write_all(&payload)).await;
    }
}

fn membership_event(relay_keys: &Keys, channel_id: &str, members: &[(&str, &str)]) -> Event {
    let mut tags = vec![Tag::parse(["d", channel_id]).expect("d tag")];
    for (pubkey, role) in members {
        tags.push(Tag::parse(["p", pubkey, "", role]).expect("p tag"));
    }
    EventBuilder::new(Kind::Custom(39002), "")
        .tags(tags)
        .custom_created_at(Timestamp::from(42_u64))
        .sign_with_keys(relay_keys)
        .expect("membership event")
}

struct Fixture {
    app: tauri::App<tauri::test::MockRuntime>,
    state: Arc<Mutex<RelayState>>,
    _server: ServerGuard,
}

async fn fixture() -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("relay address");
    let relay_keys = Keys::generate();
    let state = Arc::new(Mutex::new(RelayState {
        relay_pubkey: relay_keys.public_key().to_hex(),
        ..Default::default()
    }));
    let server = ServerGuard(tokio::spawn(relay(listener, state.clone())));
    let app = tauri::test::mock_builder()
        .manage(build_app_state())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    *app.state::<AppState>()
        .relay_url_override
        .lock()
        .expect("relay override lock") = Some(format!("ws://{address}"));
    Fixture {
        app,
        state,
        _server: server,
    }
}

fn dispatch_input(channel_id: &str, agent: &str) -> WikiTaskDispatchInput {
    WikiTaskDispatchInput {
        dispatch_id: uuid::Uuid::new_v4().to_string(),
        draft_key: format!(
            "wiki:task:proj-1:{}:crew:{}",
            "a".repeat(64),
            uuid::Uuid::new_v4()
        ),
        question_id: uuid::Uuid::new_v4().to_string(),
        attempt_id: uuid::Uuid::new_v4().to_string(),
        origin_coordinate: format!("{}:crew", "a".repeat(64)),
        source_revision: Some("git:abc123".into()),
        title: "Review the dispatch seam".into(),
        prompt: "Turn the grounded answer into a thread task.".into(),
        channel_id: channel_id.to_string(),
        agent_pubkey: agent.to_string(),
        references: vec![
            WikiTaskReferenceRecord {
                path: "src/relay.rs".into(),
                start_line: 10,
                end_line: 20,
            },
            WikiTaskReferenceRecord {
                path: "src/private/history.rs".into(),
                start_line: 1,
                end_line: 5,
            },
        ],
    }
}

fn journal() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("journal directory");
    let path = dir
        .path()
        .canonicalize()
        .expect("canonical journal path")
        .join("owner-operations/recovery.db");
    (dir, path)
}

fn open_record(
    path: &std::path::Path,
    scope: &crate::owner_operations::OperationScope,
    id: &str,
) -> crate::owner_operations::Operation {
    OperationStore::open(path, Limits::default())
        .expect("reopen journal")
        .load(scope, id)
        .expect("recovered row")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prepare_persists_the_exact_signed_kickoff_without_publishing() {
    let fixture = fixture().await;
    let captured = capture(fixture.app.handle().clone()).await.expect("scope");
    let signer = captured.keys.public_key().to_hex();
    let agent = "b".repeat(64);
    let channel_id = uuid::Uuid::new_v4().to_string();
    // Point NIP-11 and the roster at one relay identity we control.
    let relay_keys = Keys::generate();
    fixture.state.lock().expect("relay state").relay_pubkey = relay_keys.public_key().to_hex();
    fixture.state.lock().expect("relay state").membership = Some(membership_event(
        &relay_keys,
        &channel_id,
        &[(&signer, "member"), (&agent, "bot")],
    ));

    let (_dir, path) = journal();
    let input = dispatch_input(&channel_id, &agent);
    let question_id = input.question_id.clone();
    let attempt_id = input.attempt_id.clone();
    let job = wiki_task_dispatch_prepare_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        input,
    )
    .await
    .expect("prepare")
    .value;
    assert_eq!(job.status, "pending");
    let event_id = job.event_id.clone().expect("event id persisted");
    assert!(
        !fixture
            .state
            .lock()
            .expect("relay state")
            .publish_bodies
            .iter()
            .any(|_| true),
        "prepare must not publish"
    );

    let operation = open_record(&path, &captured.token.scope, &job.dispatch_id);
    let record = &operation.payload;
    assert_eq!(
        record.get("eventId").and_then(|value| value.as_str()),
        Some(event_id.as_str())
    );
    let signed: Event = serde_json::from_value(
        record
            .get("signedEvent")
            .cloned()
            .expect("signed kickoff persisted before publish"),
    )
    .expect("signed event JSON");
    assert_eq!(signed.id.to_hex(), event_id);
    assert_eq!(
        signed.kind.as_u16(),
        9,
        "the kickoff is a channel root message"
    );
    assert_eq!(signed.pubkey.to_hex(), signer);
    assert_eq!(
        tag_value(&signed, "h").as_deref(),
        Some(channel_id.as_str())
    );
    assert_eq!(tag_value(&signed, "p").as_deref(), Some(agent.as_str()));
    let client_markers: Vec<_> = signed
        .tags
        .iter()
        .filter(|tag| tag.as_slice().first().is_some_and(|n| n == "client"))
        .collect();
    assert!(
        client_markers.iter().any(|tag| {
            tag.as_slice().get(1).is_some_and(|m| m == "crew-wiki-task")
                && tag
                    .as_slice()
                    .get(2)
                    .is_some_and(|id| *id == job.dispatch_id)
        }),
        "the durable idempotency marker carries the operation id",
    );
    assert!(client_markers.iter().any(|tag| {
        tag.as_slice()
            .get(1)
            .is_some_and(|m| m == "crew-wiki-task-origin")
            && tag
                .as_slice()
                .get(2)
                .is_some_and(|c| c.as_str().ends_with(":crew"))
    }));
    assert!(signed.content.contains("Review the dispatch seam"));
    assert!(signed.content.contains("Turn the grounded answer"));
    assert!(signed.content.contains("src/relay.rs:10-20"));
    // Only the reviewed material crosses the boundary — the private
    // question/attempt ids never enter the published payload.
    assert!(!signed.content.contains(question_id.as_str()));
    assert!(!signed.content.contains(attempt_id.as_str()));
    assert!(
        !signed.as_json().contains(question_id.as_str()),
        "the whole signed event (tags included) carries no private ids",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn submit_is_idempotent_and_retries_reuse_the_persisted_identity() {
    let fixture = fixture().await;
    let captured = capture(fixture.app.handle().clone()).await.expect("scope");
    let signer = captured.keys.public_key().to_hex();
    let agent = "b".repeat(64);
    let channel_id = uuid::Uuid::new_v4().to_string();
    let relay_keys = Keys::generate();
    fixture.state.lock().expect("relay state").relay_pubkey = relay_keys.public_key().to_hex();
    fixture.state.lock().expect("relay state").membership = Some(membership_event(
        &relay_keys,
        &channel_id,
        &[(&signer, "member"), (&agent, "bot")],
    ));

    let (_dir, path) = journal();
    let input = dispatch_input(&channel_id, &agent);
    let dispatch_id = input.dispatch_id.clone();
    let prepared = wiki_task_dispatch_prepare_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        input,
    )
    .await
    .expect("prepare")
    .value;
    let event_id = prepared.event_id.clone().expect("event id");

    // Ambiguous first attempt: the relay stores the kickoff but loses the ack.
    fixture.state.lock().expect("relay state").store_then_fail = 1;
    let first = wiki_task_dispatch_submit_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        dispatch_id.clone(),
    )
    .await
    .expect("submit returns a failed-but-recorded job")
    .value;
    assert_eq!(first.status, "failed");
    assert!(first.error.is_some());
    assert_eq!(first.event_id.as_deref(), Some(event_id.as_str()));
    // The kickoff really did land — reconcile must settle it, not republish.
    assert_eq!(
        fixture
            .state
            .lock()
            .expect("relay state")
            .events
            .iter()
            .map(|event| event.id.to_hex())
            .collect::<Vec<_>>(),
        vec![event_id.clone()],
        "exactly one kickoff event exists",
    );

    let reconciled = wiki_task_dispatch_reconcile_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        dispatch_id.clone(),
    )
    .await
    .expect("reconcile")
    .value;
    assert!(reconciled.accepted);
    assert_eq!(reconciled.status, "complete");
    let publish_bodies = fixture
        .state
        .lock()
        .expect("relay state")
        .publish_bodies
        .clone();
    assert_eq!(
        publish_bodies.len(),
        1,
        "reconciliation must not republish — the persisted event id settled it",
    );

    // A submit after acceptance is a no-op returning the same kickoff.
    let again = wiki_task_dispatch_submit_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        dispatch_id.clone(),
    )
    .await
    .expect("idempotent submit")
    .value;
    assert!(again.accepted);
    assert_eq!(again.event_id.as_deref(), Some(event_id.as_str()));
    assert_eq!(
        fixture
            .state
            .lock()
            .expect("relay state")
            .publish_bodies
            .len(),
        1
    );

    let operation = open_record(&path, &captured.token.scope, &dispatch_id);
    assert!(operation.reconciled);
    assert_eq!(operation.status, OperationStatus::Complete);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ambiguous_retry_republishes_the_same_signed_event() {
    let fixture = fixture().await;
    let captured = capture(fixture.app.handle().clone()).await.expect("scope");
    let signer = captured.keys.public_key().to_hex();
    let agent = "b".repeat(64);
    let channel_id = uuid::Uuid::new_v4().to_string();
    let relay_keys = Keys::generate();
    fixture.state.lock().expect("relay state").relay_pubkey = relay_keys.public_key().to_hex();
    fixture.state.lock().expect("relay state").membership = Some(membership_event(
        &relay_keys,
        &channel_id,
        &[(&signer, "member"), (&agent, "bot")],
    ));

    let (_dir, path) = journal();
    let input = dispatch_input(&channel_id, &agent);
    let dispatch_id = input.dispatch_id.clone();
    let prepared = wiki_task_dispatch_prepare_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        input,
    )
    .await
    .expect("prepare")
    .value;
    let event_id = prepared.event_id.clone().expect("event id");

    // First attempt: the relay stores the event but the acknowledgement is
    // lost. The user clicks Start again — the retried submission must
    // republish the *same* persisted signed bytes. Mutation target:
    // re-signing at a fresh created_at mints a second event id and this
    // test turns RED.
    fixture.state.lock().expect("relay state").store_then_fail = 1;
    let failed = wiki_task_dispatch_submit_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        dispatch_id.clone(),
    )
    .await
    .expect("submit returns a failed job")
    .value;
    assert_eq!(failed.status, "failed");
    assert!(!failed.accepted);

    // Event identity binds to the operation's creation second; a retry that
    // re-signs at wall-clock only diverges across a second boundary, so the
    // mutation check needs real elapsed time between the two attempts.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let retried = wiki_task_dispatch_submit_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        dispatch_id.clone(),
    )
    .await
    .expect("retry")
    .value;
    assert!(retried.accepted);
    assert_eq!(retried.event_id.as_deref(), Some(event_id.as_str()));
    let state = fixture.state.lock().expect("relay state");
    assert_eq!(
        state
            .events
            .iter()
            .map(|event| event.id.to_hex())
            .collect::<Vec<_>>(),
        vec![event_id.clone()],
        "one kickoff, never two",
    );
    assert_eq!(state.publish_bodies.len(), 2);
    assert!(
        state
            .publish_bodies
            .iter()
            .all(|event| event.id.to_hex() == event_id),
        "every publish request carries the persisted event id",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_membership_recheck_blocks_a_removed_agent() {
    let fixture = fixture().await;
    let captured = capture(fixture.app.handle().clone()).await.expect("scope");
    let signer = captured.keys.public_key().to_hex();
    let agent = "b".repeat(64);
    let channel_id = uuid::Uuid::new_v4().to_string();
    let relay_keys = Keys::generate();
    fixture.state.lock().expect("relay state").relay_pubkey = relay_keys.public_key().to_hex();
    fixture.state.lock().expect("relay state").membership = Some(membership_event(
        &relay_keys,
        &channel_id,
        &[(&signer, "member"), (&agent, "bot")],
    ));

    let (_dir, path) = journal();
    let input = dispatch_input(&channel_id, &agent);
    let dispatch_id = input.dispatch_id.clone();
    wiki_task_dispatch_prepare_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        input,
    )
    .await
    .expect("prepare");

    // The agent is removed between open and Start — stale picker state must
    // never authorize the send.
    fixture.state.lock().expect("relay state").membership = Some(membership_event(
        &relay_keys,
        &channel_id,
        &[(&signer, "member")],
    ));
    let outcome = wiki_task_dispatch_submit_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        dispatch_id.clone(),
    )
    .await
    .expect("submit returns a blocked job")
    .value;
    assert!(!outcome.accepted);
    assert!(
        outcome
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("not a channel member"),
        "expected member-removal error, got {:?}",
        outcome.error
    );
    assert!(fixture
        .state
        .lock()
        .expect("relay state")
        .publish_bodies
        .is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abandon_rules_follow_the_publication_boundary() {
    let fixture = fixture().await;
    let captured = capture(fixture.app.handle().clone()).await.expect("scope");
    let signer = captured.keys.public_key().to_hex();
    let agent = "b".repeat(64);
    let channel_id = uuid::Uuid::new_v4().to_string();
    let relay_keys = Keys::generate();
    fixture.state.lock().expect("relay state").relay_pubkey = relay_keys.public_key().to_hex();
    fixture.state.lock().expect("relay state").membership = Some(membership_event(
        &relay_keys,
        &channel_id,
        &[(&signer, "member"), (&agent, "bot")],
    ));

    let (_dir, path) = journal();
    // Case 1: never submitted — clean cancel.
    let input = dispatch_input(&channel_id, &agent);
    let dispatch_id = input.dispatch_id.clone();
    wiki_task_dispatch_prepare_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        input,
    )
    .await
    .expect("prepare");
    let canceled = wiki_task_dispatch_abandon_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        dispatch_id.clone(),
        false,
    )
    .await
    .expect("clean abandon")
    .value;
    assert_eq!(canceled.status, "canceled");

    // Case 2: accepted — refuses to pretend retraction.
    let input2 = dispatch_input(&channel_id, &agent);
    let dispatch_id2 = input2.dispatch_id.clone();
    wiki_task_dispatch_prepare_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        input2,
    )
    .await
    .expect("prepare 2");
    wiki_task_dispatch_submit_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        dispatch_id2.clone(),
    )
    .await
    .expect("submit 2");
    let refused = wiki_task_dispatch_abandon_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        dispatch_id2.clone(),
        false,
    )
    .await;
    assert!(refused.is_err(), "accepted work cannot be retracted");
    assert!(refused
        .err()
        .unwrap_or_default()
        .contains("cannot be retracted"));

    // Case 3: attempted but not acknowledged — requires force, and the record
    // keeps the honest tombstone.
    let input3 = dispatch_input(&channel_id, &agent);
    let dispatch_id3 = input3.dispatch_id.clone();
    wiki_task_dispatch_prepare_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        input3,
    )
    .await
    .expect("prepare 3");
    fixture.state.lock().expect("relay state").store_then_fail = 1;
    wiki_task_dispatch_submit_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        dispatch_id3.clone(),
    )
    .await
    .expect("ambiguous submit");
    let guarded = wiki_task_dispatch_abandon_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        dispatch_id3.clone(),
        false,
    )
    .await;
    assert!(
        guarded.is_err(),
        "an attempted dispatch must reconcile before abandon",
    );
    let forced = wiki_task_dispatch_abandon_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        dispatch_id3.clone(),
        true,
    )
    .await
    .expect("forced abandon")
    .value;
    assert_eq!(forced.status, "canceled");
    let operation = open_record(&path, &captured.token.scope, &dispatch_id3);
    assert_eq!(
        operation
            .payload
            .get("abandonedWithUnresolvedPublish")
            .and_then(|value| value.as_bool()),
        Some(true),
        "the record keeps the honest unresolved-publish note",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn changed_intent_on_one_draft_conflicts_instead_of_dispatching_twice() {
    let fixture = fixture().await;
    let captured = capture(fixture.app.handle().clone()).await.expect("scope");
    let signer = captured.keys.public_key().to_hex();
    let agent = "b".repeat(64);
    let channel_id = uuid::Uuid::new_v4().to_string();
    let relay_keys = Keys::generate();
    fixture.state.lock().expect("relay state").relay_pubkey = relay_keys.public_key().to_hex();
    fixture.state.lock().expect("relay state").membership = Some(membership_event(
        &relay_keys,
        &channel_id,
        &[(&signer, "member"), (&agent, "bot")],
    ));

    let (_dir, path) = journal();
    let input = dispatch_input(&channel_id, &agent);
    let draft_key = input.draft_key.clone();
    wiki_task_dispatch_prepare_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        input,
    )
    .await
    .expect("prepare");

    // Same draft, different dispatch id and changed title — the unresolved
    // resource claim must conflict rather than mint a sibling kickoff.
    let mut changed = dispatch_input(&channel_id, &agent);
    changed.draft_key = draft_key;
    changed.title = "A different task".into();
    let conflicted = wiki_task_dispatch_prepare_at_path(
        fixture.app.handle().clone(),
        path.clone(),
        captured.token.clone(),
        changed,
    )
    .await;
    assert!(conflicted.is_err(), "changed intent must conflict");
}
