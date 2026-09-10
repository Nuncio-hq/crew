//! The native guarded-read fences, exercised through production code.
//!
//! `wiki_publication_head_proof_tests` proves the *decision* in
//! `validate_head_retirement_refusal` with scripted reads. That cannot prove
//! the fences, because a scripted `Err` stays an `Err` even if the real guard
//! is deleted. These tests instead build the real `NativeReadContext` over a
//! real captured identity, a real `OwnerOperationTransport` pointed at a
//! bounded loopback relay, and a real SQLite recovery row, then move actual
//! state — identity generation, durable revision, clock past the lease — while
//! a request is in flight.
//!
//! The fake relay only scripts signed protocol responses and release barriers.
//! It never decides whether a guard passes; production does.

use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use nostr::{Event, Keys};
use tauri::Manager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use super::owner_operation_transport::{OperationTransportError, OwnerOperationTransport};
use super::wiki_publication_driver::WikiPublishError;
use super::wiki_publication_native_reads::{NativeClock, NativeJournal, NativeReadContext};
use super::wiki_publication_record::{WikiPublicationLease, WikiPublicationRecord};
use super::wiki_publication_runtime::validate_head_retirement_refusal;
use super::wiki_publication_test_fixture as fixture;
use crate::app_state::owner_scope::{capture, CapturedOwnerScope};
use crate::app_state::{build_app_state, AppState, IdentityStorage};
use crate::owner_operations::{
    CreateResult, Limits, NewOperation, Operation, OperationKind, OperationStore, OperationUpdate,
};

/// Which fence a case moves, and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fence {
    IdentityGeneration,
    Revision,
    Lease,
}

/// When the fence moves relative to the two guarded reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum When {
    /// Before anything is sent: the pre-send guard must refuse and no request
    /// may reach the relay at all.
    BeforeFirstRequest,
    /// While the absence request is at the relay, before its reply is
    /// released: the post-response guard must refuse and the second read must
    /// never be issued.
    DuringFirstRequest,
    /// While the current-head request is held: the proof must not be accepted.
    DuringSecondRequest,
}

/// A barrier owned by ONE request index.
///
/// Only the request whose zero-based index equals `index` announces arrival
/// and waits for release; every other request is served immediately. This is
/// what makes "mutate while the second read is in flight" mean the second
/// read and not the first.
struct HoldAt {
    index: usize,
    arrived: Mutex<Option<oneshot::Sender<()>>>,
    release: Mutex<Option<oneshot::Receiver<()>>>,
}

struct Relay {
    listener: TcpListener,
    /// Signed events returned for the exact-ID absence read, then the head read.
    absence: Vec<Event>,
    head: Vec<Event>,
    requests: Arc<AtomicUsize>,
    hold: Option<Arc<HoldAt>>,
}

/// Abort the fake relay when the test scope ends, so a *prevented* request can
/// never leave a join waiting forever.
struct ServerGuard(tokio::task::JoinHandle<()>);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

const MAX_REQUEST_HEADER_BYTES: usize = 64 * 1024;
const MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;
const MAX_REQUEST_TOTAL_BYTES: usize = 2 * 1024 * 1024;
const SOCKET_IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let headers_end;
    loop {
        let count = tokio::time::timeout(SOCKET_IO_TIMEOUT, socket.read(&mut chunk))
            .await
            .expect("request read timed out")
            .expect("read request");
        assert!(count > 0, "client closed before request headers");
        let next_len = bytes
            .len()
            .checked_add(count)
            .expect("request length overflow");
        assert!(
            next_len <= MAX_REQUEST_TOTAL_BYTES,
            "request total is bounded"
        );
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let header_len = index
                .checked_add(4)
                .expect("request header length overflow");
            assert!(
                header_len <= MAX_REQUEST_HEADER_BYTES,
                "request headers are bounded"
            );
            headers_end = index;
            break;
        }
        assert!(
            next_len <= MAX_REQUEST_HEADER_BYTES,
            "request headers are bounded"
        );
    }
    let headers = String::from_utf8_lossy(&bytes[..headers_end]).to_string();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().expect("content length"))
        })
        .unwrap_or(0);
    assert!(
        content_length <= MAX_REQUEST_BODY_BYTES,
        "request body is bounded"
    );
    let body_start = headers_end
        .checked_add(4)
        .expect("request header length overflow");
    let body_end = body_start
        .checked_add(content_length)
        .expect("request length overflow");
    assert!(
        body_end <= MAX_REQUEST_TOTAL_BYTES,
        "request total is bounded"
    );
    while bytes.len() < body_end {
        let count = tokio::time::timeout(SOCKET_IO_TIMEOUT, socket.read(&mut chunk))
            .await
            .expect("request body read timed out")
            .expect("read body");
        assert!(count > 0, "client closed before request body");
        let next_len = bytes
            .len()
            .checked_add(count)
            .expect("request length overflow");
        assert!(
            next_len <= MAX_REQUEST_TOTAL_BYTES,
            "request total is bounded"
        );
        bytes.extend_from_slice(&chunk[..count]);
    }
    headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default()
        .to_owned()
}

fn json_response(value: &impl serde::Serialize) -> Vec<u8> {
    let body = serde_json::to_vec(value).expect("response JSON");
    let mut response = format!(
        "HTTP/1.1 200 Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(&body);
    response
}

/// Serve the two scoped reads in order. Only the held index announces arrival
/// and waits; the loop itself is bounded by the caller aborting the task.
async fn serve(relay: Relay) {
    // The proof path issues at most one NIP-11 probe plus two scoped reads per
    // validation; bound the accept loop well above that so a runaway client
    // cannot spin here.
    const MAX_CONNECTIONS: usize = 16;
    for _ in 0..MAX_CONNECTIONS {
        let Ok((mut socket, _)) = relay.listener.accept().await else {
            return;
        };
        let path = read_request(&mut socket).await;
        let response = if path == "/" {
            json_response(&serde_json::json!({}))
        } else {
            let index = relay.requests.fetch_add(1, Ordering::SeqCst);
            if let Some(hold) = relay.hold.as_ref().filter(|hold| hold.index == index) {
                if let Some(sender) = hold.arrived.lock().expect("arrived").take() {
                    let _ = sender.send(());
                }
                let release = hold.release.lock().expect("release").take();
                if let Some(release) = release {
                    let _ = release.await;
                }
            }
            let events = if index == 0 {
                relay.absence.clone()
            } else {
                relay.head.clone()
            };
            json_response(&events)
        };
        let _ = tokio::time::timeout(SOCKET_IO_TIMEOUT, socket.write_all(&response))
            .await
            .expect("response write timed out");
    }
}

struct Harness {
    app: tauri::App<tauri::test::MockRuntime>,
    captured: CapturedOwnerScope,
    transport: OwnerOperationTransport,
    _dir: tempfile::TempDir,
    path: std::path::PathBuf,
    clock: Arc<AtomicI64>,
    record: WikiPublicationRecord,
    operation: Operation,
    repo_d: String,
}

/// A real captured scope, a real transport onto the loopback relay, and a real
/// SQLite journal row holding a live lease.
/// The same harness with a real non-absent conditional precondition, signed by
/// the same owner at the same coordinate through the production builder.
async fn harness_with_expected(address: std::net::SocketAddr, repo_d: &str) -> Harness {
    let mut state = harness_inner(address, repo_d, true).await;
    assert_ne!(state.record.expected_revision, "absent");
    state.repo_d = repo_d.to_owned();
    state
}

async fn harness(address: std::net::SocketAddr, repo_d: &str) -> Harness {
    harness_inner(address, repo_d, false).await
}

async fn harness_inner(
    address: std::net::SocketAddr,
    repo_d: &str,
    with_expected: bool,
) -> Harness {
    let app = tauri::test::mock_builder()
        .manage(build_app_state())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    *app.state::<AppState>()
        .relay_url_override
        .lock()
        .expect("relay override lock") = Some(format!("ws://{address}"));
    let captured = capture(app.handle().clone()).await.expect("native scope");

    let keys = captured.keys.clone();
    let coordinate = fixture::coordinate(&keys, repo_d);
    // The precondition E is a real head signed by the SAME owner at the SAME
    // coordinate, written into the record through the production builder.
    let expected =
        with_expected.then(|| fixture::publication(&keys, repo_d, None).head.id.to_hex());
    let mut record = fixture::record(
        fixture::publication_with_expected(&keys, repo_d, None, expected.as_deref()),
        &coordinate,
        &keys,
    );
    // A settled-enough shape with a live lease: the guard decodes exactly this.
    record.head_attempted = true;
    record.progress = super::wiki_publication_record::WikiPublicationProgress::Head;
    record.lease = Some(WikiPublicationLease {
        worker_id: uuid::Uuid::new_v4().to_string(),
        expires_at: 160,
    });

    let dir = tempfile::tempdir().expect("journal directory");
    let path = dir
        .path()
        .canonicalize()
        .expect("canonical journal path")
        .join("owner-operations/recovery.db");
    let mut store = OperationStore::open(&path, Limits::default()).expect("journal");
    let operation = match store
        .create(
            &captured.token.scope,
            NewOperation {
                id: uuid::Uuid::new_v4().to_string(),
                kind: OperationKind::WikiPublication,
                resource_key: coordinate.clone(),
                payload: serde_json::to_value(&record).expect("record JSON"),
            },
            100,
        )
        .expect("reserve the publication")
    {
        CreateResult::Created(operation) | CreateResult::Existing(operation) => operation,
    };
    drop(store);

    let transport = OwnerOperationTransport::captured(
        &app.state::<AppState>(),
        captured.token.scope.community.clone(),
        keys,
        None,
    )
    .expect("captured transport");

    Harness {
        app,
        captured,
        transport,
        _dir: dir,
        path,
        clock: Arc::new(AtomicI64::new(100)),
        record,
        operation,
        repo_d: repo_d.to_owned(),
    }
}

impl Harness {
    fn context<'a>(
        &'a self,
        journal: &'a NativeJournal,
        clock: &'a NativeClock,
    ) -> NativeReadContext<'a, tauri::test::MockRuntime> {
        NativeReadContext {
            app: self.app.handle(),
            expected: &self.captured.token,
            owner: self.captured.keys.public_key(),
            repo_d: &self.repo_d,
            transport: &self.transport,
            journal,
            clock,
        }
    }

    /// Real identity A -> B -> A through the production mutation path, which
    /// advances the actual generation counter.
    fn rotate_identity(&self) {
        let directory = tempfile::tempdir().expect("identity directory");
        let state = self.app.state::<AppState>();
        let mutation = state.identity_mutation.lock().expect("identity lock");
        for keys in [Keys::generate(), self.captured.keys.clone()] {
            crate::commands::commit_imported_identity(
                &state,
                &mutation,
                directory.path(),
                keys,
                |_| Ok(IdentityStorage::LocalFile),
            )
            .expect("identity replacement");
        }
    }

    /// A real concurrent CAS on the same durable row.
    fn advance_revision(&self) {
        let current = OperationStore::open(&self.path, Limits::default())
            .expect("reopen journal")
            .load(&self.captured.token.scope, &self.operation.id)
            .expect("durable row");
        OperationStore::open(&self.path, Limits::default())
            .expect("reopen journal")
            .compare_and_swap(
                &self.captured.token.scope,
                &current.id,
                current.revision,
                OperationUpdate {
                    status: current.status,
                    reconciled: false,
                    payload: current.payload.clone(),
                },
                self.clock.load(Ordering::SeqCst),
            )
            .expect("concurrent revision advance");
    }

    fn apply(&self, fence: Fence) {
        match fence {
            Fence::IdentityGeneration => self.rotate_identity(),
            Fence::Revision => self.advance_revision(),
            // Past the record's unchanged lease (expires_at = 160).
            Fence::Lease => self.clock.store(1_000, Ordering::SeqCst),
        }
    }

    fn durable(&self) -> Operation {
        OperationStore::open(&self.path, Limits::default())
            .expect("reopen journal")
            .load(&self.captured.token.scope, &self.operation.id)
            .expect("durable row")
    }
}

/// Every validation runs under a wall bound: a fence that prevents a request
/// must fail this test quickly rather than hang it.
async fn bounded<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(20), future)
        .await
        .expect("the guarded validation must not hang")
}

fn refusal(head_id: &str) -> OperationTransportError {
    OperationTransportError::RelayResponse {
        status: 400,
        reason: format!("conflict: wiki-head-retired:{head_id}"),
    }
}

/// Positive control: with nothing moved, the same production reads complete
/// and the proof is accepted.
#[tokio::test(flavor = "multi_thread")]
async fn unchanged_state_completes_both_real_reads_and_proves_retirement() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let state = harness(address, "crew.fence.ok").await;
    let head_id = state.record.head.id.to_hex();
    let requests = Arc::new(AtomicUsize::new(0));
    let _server = ServerGuard(tokio::spawn(serve(Relay {
        listener,
        absence: Vec::new(),
        head: Vec::new(),
        requests: requests.clone(),
        hold: None,
    })));

    let journal = NativeJournal::Path(state.path.clone());
    let clock = NativeClock::Fixed(state.clock.clone());
    let proof = bounded(validate_head_retirement_refusal(
        &state.context(&journal, &clock),
        &state.operation,
        &state.record,
        &state.record.head.clone(),
        &refusal(&head_id),
    ))
    .await
    .expect("validated")
    .expect("unchanged state proves retirement");
    assert_eq!(
        proof.retirement,
        super::wiki_publication_record::WikiHeadRetirement::Head { head_id }
    );
    assert_eq!(
        requests.load(Ordering::SeqCst),
        2,
        "both guarded reads really reached the relay"
    );
    assert!(!state.durable().reconciled, "the claim is untouched");
}

/// Positive control for the precondition form: E and H are signed by the same
/// owner at the same coordinate, and E is the record's real stored
/// `expected_revision`, so both scoped reads run and the E proof is accepted.
#[tokio::test(flavor = "multi_thread")]
async fn unchanged_state_proves_expected_precondition_retirement() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
    let address = listener.local_addr().expect("address");
    let state = harness_with_expected(address, "crew.fence.expected").await;
    let head_id = state.record.head.id.to_hex();
    let expected = state.record.expected_revision.clone();
    assert_ne!(
        expected, "absent",
        "this control really carries a precondition"
    );
    let requests = Arc::new(AtomicUsize::new(0));
    let _server = ServerGuard(tokio::spawn(serve(Relay {
        listener,
        absence: Vec::new(),
        head: Vec::new(),
        requests: requests.clone(),
        hold: None,
    })));

    let journal = NativeJournal::Path(state.path.clone());
    let clock = NativeClock::Fixed(state.clock.clone());
    let proof = bounded(validate_head_retirement_refusal(
        &state.context(&journal, &clock),
        &state.operation,
        &state.record,
        &state.record.head.clone(),
        &OperationTransportError::RelayResponse {
            status: 400,
            reason: format!("conflict: wiki-expected-head-retired:{head_id}:{expected}"),
        },
    ))
    .await
    .expect("validated")
    .expect("unchanged state proves precondition retirement");
    assert_eq!(
        proof.retirement,
        super::wiki_publication_record::WikiHeadRetirement::ExpectedHead {
            head_id,
            expected_revision: expected
        }
    );
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    assert!(!state.durable().reconciled);
}

/// Each real fence, moved at each observable position.
#[tokio::test(flavor = "multi_thread")]
async fn a_moved_native_fence_refuses_the_read_at_its_own_await_position() {
    for fence in [Fence::IdentityGeneration, Fence::Revision, Fence::Lease] {
        for when in [
            When::BeforeFirstRequest,
            When::DuringFirstRequest,
            When::DuringSecondRequest,
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
            let address = listener.local_addr().expect("address");
            let repo_d = "crew.fence.case";
            let state = harness(address, repo_d).await;
            let head_id = state.record.head.id.to_hex();
            let before = state.durable();

            let requests = Arc::new(AtomicUsize::new(0));
            let (arrived_tx, arrived_rx) = oneshot::channel();
            let (release_tx, release_rx) = oneshot::channel();
            // The barrier belongs to ONE request index, so "during the second
            // read" really holds request 1 and lets request 0 through.
            let hold = match when {
                When::BeforeFirstRequest => None,
                When::DuringFirstRequest => Some(0),
                When::DuringSecondRequest => Some(1),
            }
            .map(|index| {
                Arc::new(HoldAt {
                    index,
                    arrived: Mutex::new(Some(arrived_tx)),
                    release: Mutex::new(Some(release_rx)),
                })
            });
            let _server = ServerGuard(tokio::spawn(serve(Relay {
                listener,
                absence: Vec::new(),
                head: Vec::new(),
                requests: requests.clone(),
                hold,
            })));

            if when == When::BeforeFirstRequest {
                state.apply(fence);
            }

            let journal = NativeJournal::Path(state.path.clone());
            let clock = NativeClock::Fixed(state.clock.clone());
            let head = state.record.head.clone();
            let error = refusal(&head_id);
            let validation = async {
                validate_head_retirement_refusal(
                    &state.context(&journal, &clock),
                    &state.operation,
                    &state.record,
                    &head,
                    &error,
                )
                .await
            };

            let label = format!("{fence:?}/{when:?}");
            let observed_before_release = Arc::new(AtomicUsize::new(usize::MAX));
            let outcome = if when == When::BeforeFirstRequest {
                bounded(validation).await
            } else {
                let seen = observed_before_release.clone();
                let counter = requests.clone();
                let mover = async {
                    // Bounded: if the held request never arrives, this returns
                    // instead of waiting forever, and the count assertions
                    // below report the real position.
                    if tokio::time::timeout(std::time::Duration::from_secs(10), arrived_rx)
                        .await
                        .is_ok()
                    {
                        seen.store(counter.load(Ordering::SeqCst), Ordering::SeqCst);
                        state.apply(fence);
                    }
                    let _ = release_tx.send(());
                };
                let (outcome, ()) = tokio::join!(bounded(validation), mover);
                outcome
            };

            // A moved fence is specifically an Unknown transport outcome, not
            // merely "some error" and never an unproven Ok(None): the guard
            // failure must surface as the production error type.
            match &outcome {
                Err(WikiPublishError::Unknown(_)) => {}
                other => panic!(
                    "{label}: a moved fence must be Unknown, got proof/none: {}",
                    other.is_ok()
                ),
            }

            // Exact request counts at every position.
            let requests_made = requests.load(Ordering::SeqCst);
            match when {
                When::BeforeFirstRequest => assert_eq!(
                    requests_made, 0,
                    "{label}: the pre-send guard must refuse before any request"
                ),
                When::DuringFirstRequest => assert_eq!(
                    requests_made, 1,
                    "{label}: a refused first read must not issue the second"
                ),
                When::DuringSecondRequest => {
                    assert_eq!(
                        requests_made, 2,
                        "{label}: the second read must actually have been issued"
                    );
                    assert_eq!(
                        observed_before_release.load(Ordering::SeqCst),
                        2,
                        "{label}: request 1 must have completed unchanged before request 2 arrived"
                    );
                }
            }

            let after = state.durable();
            assert!(!after.reconciled, "{label}: the claim stays unresolved");
            if fence != Fence::Revision {
                assert_eq!(after.revision, before.revision, "{label}");
            }
            let stored: WikiPublicationRecord =
                serde_json::from_value(after.payload).expect("durable record");
            assert_eq!(stored.head, state.record.head, "{label}: signed head kept");
            assert_eq!(stored.manifest, state.record.manifest, "{label}");
            assert_eq!(stored.pages, state.record.pages, "{label}");
            assert!(
                stored.reconciliation.is_none(),
                "{label}: no proof is stored"
            );
            // `_server` aborts on drop, so a prevented request cannot hang.
        }
    }
}
