//! Whole-chain producer evidence for the managed transport status sidechannel.
//!
//! This is deliberately an ignored test.  It is the only test in this module
//! that starts an ACP executable.  The validation recipe must supply the
//! executable and both provenance digests; a missing or mismatched input is a
//! failure, never a reason to silently skip the test.

#![cfg(unix)]

use std::io::ErrorKind;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command as StdCommand, Stdio};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use nostr::{Keys, ToBech32};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::{accept_async, tungstenite::Message};

use buzz_core_pkg::transport_status::{
    TransportAuthClassification, TransportCode, TransportRecordEnvelope, TransportState,
};

use crate::managed_agents::ManagedAgentRuntimeKey;

use super::export::{enabled as native_auth_evidence_export_enabled, ExportCandidate};
use super::monitor::Monitor;
use super::{registered_monitor, Diagnostics};

mod helpers;

use helpers::{
    assert_no_provider_or_user_secrets_in_child_env, create_private_dir, is_lower_hex_64, now_ms,
    read_bounded_file, remaining_until, verify_supplied_binary_and_source, write_poison_provider,
    write_private_file, ChildGuard, FixtureRoot,
};

/// The validation recipe's exact intended test count.  Keep this nonzero so a
/// typo in the test filter cannot turn the whole-chain lane into a green-empty
/// run.  The recipe also checks that this named ignored test is discovered.
pub(crate) const INTENDED_IGNORED_TEST_COUNT: usize = 1;

const ACP_BINARY_ENV: &str = "CREW_ACP_FIXTURE_BINARY";
const ACP_BINARY_SHA256_ENV: &str = "CREW_ACP_FIXTURE_BINARY_SHA256";
const ACP_SOURCE_ROOT_ENV: &str = "CREW_ACP_FIXTURE_SOURCE_ROOT";
const ACP_SOURCE_SHA256_ENV: &str = "CREW_ACP_FIXTURE_SOURCE_SHA256";

const FIXTURE_DEADLINE: Duration = Duration::from_secs(30);
const SOCKET_DEADLINE: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const CAPTURED_STREAM_BYTES: usize = 64 * 1024;
const CAPTURE_DRAIN_DEADLINE: Duration = Duration::from_secs(2);
const CAPTURED_BINARY_BYTES: u64 = 512 * 1024 * 1024;
const CAPTURED_SOURCE_BYTES: u64 = 128 * 1024 * 1024;
const CAPTURED_SOURCE_FILES: usize = 10_000;
const DENIAL: &str = "blocked: you are banned from this community";
const CHALLENGE: &str = "crew-338-auth-evidence-challenge";

/// The recipe must discover this test even though it is intentionally ignored.
#[test]
fn whole_chain_recipe_declares_a_nonzero_test_count() {
    const { assert!(INTENDED_IGNORED_TEST_COUNT > 0) };
}

/// Spawn the supplied ACP executable and prove that its real v2 terminal
/// record survives the secure native reader and the retired-generation fence.
///
/// This remains ignored because it needs a separately built binary with a
/// source binding.  It must be run explicitly by the bounded validation recipe
/// with all four `CREW_ACP_FIXTURE_*` variables set.
#[tokio::test]
#[ignore = "requires a supplied hash-bound ACP binary and the bounded fixture recipe"]
async fn actual_built_acp_writes_atomic_v2_record_and_native_accepts_only_bound_generation() {
    let result = tokio::time::timeout(FIXTURE_DEADLINE, run_whole_chain_fixture()).await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => panic!("whole-chain producer fixture failed: {error}"),
        Err(_) => panic!("whole-chain producer fixture exceeded {FIXTURE_DEADLINE:?}"),
    }
}

async fn run_whole_chain_fixture() -> Result<(), String> {
    let deadline = Instant::now() + FIXTURE_DEADLINE;
    let binary = verify_supplied_binary_and_source(deadline)?;
    let fixture = FixtureRoot::new()?;
    let child_keys = Keys::generate();
    let owner = child_keys.public_key().to_hex().to_ascii_lowercase();
    let relay_listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|error| format!("bind loopback relay fixture: {error}"))?;
    let relay_addr = relay_listener
        .local_addr()
        .map_err(|error| format!("read loopback relay address: {error}"))?;
    let relay_url = format!("ws://127.0.0.1:{}", relay_addr.port());
    let (auth_event_tx, mut auth_event_rx) = oneshot::channel::<String>();
    let (release_tx, release_rx) = oneshot::channel::<()>();
    let mut server = ServerGuard::spawn(tokio::spawn(run_loopback_relay(
        relay_listener,
        child_keys.public_key().to_hex(),
        auth_event_tx,
        release_rx,
        deadline,
    )));

    let nonce = uuid::Uuid::new_v4().to_string();
    let spawn_started_at_ms = now_ms()?.saturating_sub(1).max(1);
    let log_path = fixture.logs.join("managed-agent.log");
    let status_path = super::status_path(&log_path, &nonce);
    let status_dir = status_path
        .parent()
        .ok_or_else(|| "status path has no parent".to_string())?;
    create_private_dir(status_dir)?;
    let poison_marker = fixture.data.join("poison-provider.invoked");
    let poison_provider = fixture.bin.join("poison-provider.sh");
    write_poison_provider(&poison_provider, &poison_marker)?;

    let key_nsec = child_keys
        .secret_key()
        .to_bech32()
        .map_err(|error| format!("encode fixture key: {error}"))?;
    let config_path = fixture.config.join("buzz-acp.toml");
    write_private_file(&config_path, b"# whole-chain fixture config\n")?;

    let mut command = StdCommand::new(&binary);
    command
        .env_clear()
        .current_dir(&fixture.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("--relay-url")
        .arg(&relay_url)
        .arg("--private-key")
        .arg(&key_nsec)
        .arg("--agent-command")
        .arg(&poison_provider)
        .arg("--agent-args")
        .arg("acp")
        .arg("--config")
        .arg(&config_path)
        .arg("--lazy-pool")
        .arg("--no-memory")
        .arg("--no-base-prompt")
        .arg("--no-presence")
        .arg("--no-typing")
        .arg("--no-user-input")
        .arg("--respond-to")
        .arg("anyone")
        .arg("--subscribe")
        .arg("all")
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &fixture.home)
        .env("XDG_CONFIG_HOME", &fixture.config)
        .env("XDG_DATA_HOME", &fixture.data)
        .env("XDG_CACHE_HOME", &fixture.cache)
        .env("TMPDIR", &fixture.tmp)
        .env("TERM", "dumb")
        .env("LANG", "C")
        .env("RUST_LOG", "error")
        .env("CREW_ACP_TRANSPORT_STATUS_PATH", &status_path)
        .env("CREW_ACP_TRANSPORT_START_NONCE", &nonce)
        .env("CREW_ACP_TRANSPORT_STATUS_VERSION", "2")
        .env(
            "CREW_ACP_TRANSPORT_SPAWN_STARTED_AT_MS",
            spawn_started_at_ms.to_string(),
        );
    command.process_group(0);
    assert_no_provider_or_user_secrets_in_child_env(&command);

    let mut child = ChildGuard::spawn(command, deadline)?;
    let child_pid = child.pid()?;
    let key = ManagedAgentRuntimeKey::new(owner.clone(), &relay_url)?;
    let diagnostics = std::sync::Arc::new(std::sync::Mutex::new(Diagnostics::default()));
    // Use the production private constructor so the ticket is made from the
    // actual child PID, nonce, spawn stamp, owner, and derived status path.
    // `bind_registered` additionally needs an AppHandle and the desktop-wide
    // process map; this fixture exercises that constructor directly and then
    // drives the same Monitor/Diagnostics methods used by the poller.
    let mut monitor = registered_monitor(
        &key,
        &log_path,
        &nonce,
        child_pid,
        spawn_started_at_ms,
        Some(&owner),
        false,
        &diagnostics,
    )
    .ok_or_else(|| {
        "production native monitor registration refused fixture generation".to_string()
    })?;
    let ticket = monitor
        .snapshot()
        .ok_or_else(|| "registered production monitor did not expose a read ticket".to_string())?;
    if ticket.path != status_path
        || ticket.nonce != nonce
        || ticket.process_id != child_pid
        || ticket.spawn_started_at_ms != spawn_started_at_ms
        || ticket.owner != owner
    {
        return Err("native monitor ticket did not bind the spawned generation".into());
    }

    // The relay sends a challenge, receives the actual signed AUTH event, and
    // waits here.  This keeps the child alive while the production reader sees
    // the first v2 connecting snapshot and lets the test exercise Monitor.apply
    // against a real atomic write before the terminal denial is released.
    let auth_event_id = wait_for_auth_event(&mut auth_event_rx, deadline).await?;
    let initial = wait_for_snapshot(&status_path, deadline, |record| {
        record
            .v2()
            .is_some_and(|record| record.connection_attempt.is_some())
    })
    .await?;
    if let TransportRecordEnvelope::V2(record) = &initial {
        if record.transport.state != TransportState::Connecting
            || record.received_auth.is_some()
            || record.process_id != child_pid
            || record.spawn_started_at_ms != spawn_started_at_ms
        {
            return Err("initial live v2 snapshot was not the bound connecting state".into());
        }
    } else {
        return Err("actual ACP producer wrote a v1 record despite v2 negotiation".into());
    }
    let initial_wall = now_ms()?;
    monitor.apply(
        &ticket,
        Ok(initial.clone()),
        true,
        Instant::now(),
        initial_wall,
    );
    if monitor.status().state != TransportState::Connecting {
        return Err("production Monitor did not accept the live connecting snapshot".into());
    }
    release_tx
        .send(())
        .map_err(|_| "loopback relay fixture dropped before sending the denial".to_string())?;

    let exit = child.wait_until_exit(deadline).await?;
    let output = child.finish(exit).await?;
    if output.stdout.truncated || output.stderr.truncated {
        return Err("ACP fixture output exceeded the capture cap".into());
    }
    if output.stdout.bytes.len() > CAPTURED_STREAM_BYTES
        || output.stderr.bytes.len() > CAPTURED_STREAM_BYTES
    {
        return Err("ACP fixture output capture exceeded its byte bound".into());
    }
    if poison_marker.exists() {
        return Err("lazy-pool ACP fixture invoked the poison provider".into());
    }

    let final_envelope = wait_for_snapshot(&status_path, deadline, |record| {
        record.v2().is_some_and(|record| {
            record.terminal
                && record.transport.state == TransportState::AuthRejected
                && record.transport.code == TransportCode::AuthDenied
                && record.received_auth.is_some()
        })
    })
    .await?;
    let raw = read_bounded_file(&status_path)?;
    if raw
        .windows(DENIAL.len())
        .any(|window| window == DENIAL.as_bytes())
    {
        return Err("raw relay denial text reached the atomic status file".into());
    }
    let final_record = final_envelope
        .v2()
        .ok_or_else(|| "final status envelope was not v2".to_string())?;
    let received_auth = final_record
        .received_auth
        .as_ref()
        .ok_or_else(|| "final status record omitted correlated AUTH evidence".to_string())?;
    let attempt = final_record
        .connection_attempt
        .as_ref()
        .ok_or_else(|| "final status record omitted its connection attempt".to_string())?;
    if received_auth.auth_event_id != auth_event_id
        || received_auth.accepted
        || received_auth.attempt_id != attempt.id
        || received_auth.attempt_sequence != attempt.sequence
        || received_auth.classification != TransportAuthClassification::CommunityBanned
    {
        return Err("final AUTH evidence was not correlated to the server-observed event".into());
    }

    // The child has exited; native cleanup retains the exact generation for
    // one bounded final read.  This is the production retired path, using the
    // same secure reader snapshot rather than decoding JSON in the fixture.
    monitor.retire(true, Instant::now());
    {
        let pending = diagnostics
            .lock()
            .map_err(|_| "diagnostics lock poisoned before final read".to_string())?
            .pending(Instant::now());
        if !pending.iter().any(|candidate| candidate == &ticket) {
            return Err("retired generation did not enter the bounded final-read queue".into());
        }
    }
    let final_wall = now_ms()?;
    let applied = diagnostics
        .lock()
        .map_err(|_| "diagnostics lock poisoned during final read".to_string())?
        .apply(
            &ticket,
            final_envelope.clone(),
            false,
            Instant::now(),
            final_wall,
        );
    if !applied {
        return Err("production Diagnostics rejected the bound terminal final read".into());
    }
    let projection = diagnostics
        .lock()
        .map_err(|_| "diagnostics lock poisoned reading projection".to_string())?
        .projection(&key, &owner, Instant::now())
        .ok_or_else(|| "retired final-read projection was missing".to_string())?;
    let evidence = super::retired_auth_evidence(&projection)
        .ok_or_else(|| "retired final-read projection omitted AUTH evidence".to_string())?;
    if !evidence.retired
        || !evidence.failed_exit
        || evidence
            .record
            .received_auth
            .as_ref()
            .map(|auth| &auth.auth_event_id)
            != Some(&auth_event_id)
        || evidence.native_binding.process_id != child_pid
        || evidence.native_binding.start_nonce != nonce
    {
        return Err("retired evidence lost the native generation binding".into());
    }

    // Falsify the native fence with the exact bytes just accepted above.  The
    // wrong PID must remain unknown and must never queue an AUTH export marker.
    let mut wrong_ticket = ticket.clone();
    wrong_ticket.process_id = wrong_ticket.process_id.saturating_add(1).max(1);
    let mut wrong_monitor = Monitor::new(wrong_ticket.clone(), &diagnostics);
    wrong_monitor.apply(
        &wrong_ticket,
        Ok(final_envelope.clone()),
        true,
        Instant::now(),
        now_ms()?,
    );
    if wrong_monitor.status().state != TransportState::Unknown
        || super::live_auth_evidence(&wrong_monitor).is_some()
    {
        return Err("wrong native process binding exposed the producer's AUTH evidence".into());
    }
    if native_auth_evidence_export_enabled() {
        let candidate = wrong_monitor.take_export_candidate(Instant::now());
        if matches!(candidate, Some(ExportCandidate::Auth(_))) {
            return Err("wrong native process binding queued an AUTH export marker".into());
        }
    }
    let diagnostic_apply = diagnostics
        .lock()
        .map_err(|_| "diagnostics lock poisoned during wrong-binding check".to_string())?
        .apply(
            &wrong_ticket,
            final_envelope,
            false,
            Instant::now(),
            now_ms()?,
        );
    if diagnostic_apply {
        return Err("Diagnostics accepted a final read under the wrong native binding".into());
    }

    server.finish(deadline).await?;
    Ok(())
}

/// The loopback relay proves that the AUTH event ID came from the actual ACP
/// wire.  It never talks to an external relay or persists any user state.
async fn run_loopback_relay(
    listener: TcpListener,
    expected_pubkey: String,
    auth_event_tx: oneshot::Sender<String>,
    release_rx: oneshot::Receiver<()>,
    deadline: Instant,
) -> Result<(), String> {
    let (stream, _) = tokio::time::timeout(remaining_until(deadline)?, listener.accept())
        .await
        .map_err(|_| "loopback relay accept timed out".to_string())?
        .map_err(|error| format!("loopback relay accept failed: {error}"))?;
    let mut socket = tokio::time::timeout(
        remaining_until(deadline)?.min(SOCKET_DEADLINE),
        accept_async(stream),
    )
    .await
    .map_err(|_| "loopback WebSocket handshake timed out".to_string())?
    .map_err(|error| format!("loopback WebSocket handshake failed: {error}"))?;
    socket
        .send(Message::Text(json!(["AUTH", CHALLENGE]).to_string().into()))
        .await
        .map_err(|error| format!("send loopback AUTH challenge: {error}"))?;
    let event_id = receive_auth_event(&mut socket, deadline, &expected_pubkey).await?;
    auth_event_tx
        .send(event_id.clone())
        .map_err(|_| "producer fixture stopped before receiving AUTH event ID".to_string())?;
    tokio::time::timeout(remaining_until(deadline)?, release_rx)
        .await
        .map_err(|_| "loopback relay denial release timed out".to_string())?
        .map_err(|_| "producer fixture dropped denial release".to_string())?;
    socket
        .send(Message::Text(
            json!(["OK", event_id, false, DENIAL]).to_string().into(),
        ))
        .await
        .map_err(|error| format!("send loopback AUTH denial: {error}"))?;
    // Drain until the child closes, with a fixed bound.  Handling Ping keeps
    // this fixture from leaving a socket task alive on a slow teardown.
    while remaining_until(deadline).is_ok() {
        let next = tokio::time::timeout(
            remaining_until(deadline)?.min(SOCKET_DEADLINE),
            socket.next(),
        )
        .await
        .map_err(|_| "loopback socket close timed out".to_string())?;
        match next {
            Some(Ok(Message::Ping(payload))) => {
                socket
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|error| format!("send loopback pong: {error}"))?;
            }
            Some(Ok(Message::Close(_))) | None => break,
            Some(Ok(_)) => {}
            Some(Err(tokio_tungstenite::tungstenite::Error::ConnectionClosed)) => break,
            Some(Err(tokio_tungstenite::tungstenite::Error::Protocol(
                tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
            ))) => break,
            Some(Err(tokio_tungstenite::tungstenite::Error::Io(error)))
                if matches!(
                    error.kind(),
                    ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset | ErrorKind::BrokenPipe
                ) =>
            {
                break
            }
            Some(Err(error)) => return Err(format!("loopback socket read failed: {error}")),
        }
    }
    Ok(())
}

async fn receive_auth_event(
    socket: &mut tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    deadline: Instant,
    expected_pubkey: &str,
) -> Result<String, String> {
    loop {
        let frame = tokio::time::timeout(remaining_until(deadline)?, socket.next())
            .await
            .map_err(|_| "waiting for ACP AUTH event timed out".to_string())?
            .ok_or_else(|| "ACP closed before sending AUTH event".to_string())?
            .map_err(|error| format!("read ACP AUTH event: {error}"))?;
        match frame {
            Message::Text(text) => {
                let value: Value = serde_json::from_str(text.as_ref())
                    .map_err(|error| format!("parse ACP AUTH frame: {error}"))?;
                if value.get(0).and_then(Value::as_str) != Some("AUTH") {
                    continue;
                }
                let event = value
                    .get(1)
                    .ok_or_else(|| "AUTH frame omitted its event".to_string())?;
                if event.get("pubkey").and_then(Value::as_str) != Some(expected_pubkey) {
                    return Err("AUTH event was signed by an unexpected fixture key".into());
                }
                let id = event
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "AUTH event omitted its ID".to_string())?;
                if !is_lower_hex_64(id) {
                    return Err("ACP AUTH event ID was not canonical lower hex".into());
                }
                return Ok(id.to_string());
            }
            Message::Ping(payload) => {
                socket
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|error| format!("send AUTH fixture pong: {error}"))?;
            }
            Message::Close(_) => return Err("ACP closed before sending AUTH event".into()),
            _ => {}
        }
    }
}

async fn wait_for_auth_event(
    receiver: &mut oneshot::Receiver<String>,
    deadline: Instant,
) -> Result<String, String> {
    tokio::time::timeout(remaining_until(deadline)?, receiver)
        .await
        .map_err(|_| "waiting for server-observed AUTH event timed out".to_string())?
        .map_err(|_| "loopback server dropped the AUTH event ID".to_string())
}

async fn wait_for_snapshot<F>(
    path: &Path,
    deadline: Instant,
    predicate: F,
) -> Result<TransportRecordEnvelope, String>
where
    F: Fn(&TransportRecordEnvelope) -> bool,
{
    loop {
        if Instant::now() >= deadline {
            return Err(format!(
                "waiting for status snapshot {} timed out",
                path.display()
            ));
        }
        if let Ok(record) = super::reader::read_owned_envelope(path) {
            if predicate(&record) {
                return Ok(record);
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

struct ServerGuard {
    handle: Option<tokio::task::JoinHandle<Result<(), String>>>,
}

impl ServerGuard {
    fn spawn(handle: tokio::task::JoinHandle<Result<(), String>>) -> Self {
        Self {
            handle: Some(handle),
        }
    }

    async fn finish(&mut self, deadline: Instant) -> Result<(), String> {
        let Some(mut handle) = self.handle.take() else {
            return Ok(());
        };
        let remaining = match remaining_until(deadline) {
            Ok(remaining) => remaining,
            Err(error) => {
                handle.abort();
                let _ = handle.await;
                return Err(error);
            }
        };
        match tokio::time::timeout(remaining, &mut handle).await {
            Ok(Ok(result)) => {
                result.map_err(|error| format!("loopback server task failed: {error}"))
            }
            Ok(Err(error)) => Err(format!("loopback server task failed: {error}")),
            Err(_) => {
                handle.abort();
                let _ = handle.await;
                Err("loopback server cleanup timed out".into())
            }
        }
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}
