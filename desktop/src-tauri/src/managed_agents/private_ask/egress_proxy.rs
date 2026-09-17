//! The desktop-owned loopback CONNECT proxy that bounds one private Ask run.
//!
//! The Seatbelt policy can deny network egress, but it cannot express "only the
//! model provider": SBPL matches addresses, not hostnames, and a rule broad
//! enough to keep HTTPS working (`remote ip "*:443"`) is broad enough to reach
//! anything. So the boundary moves up one layer. The policy allows exactly one
//! loopback port, this proxy listens on it, and the proxy is the only component
//! that resolves a name or opens a socket off the machine.
//!
//! That inversion is what makes egress *observable*. The child cannot reach the
//! network except through this process, so the set of destinations it asked for
//! is exactly the set this process recorded. `EgressObservation` is that
//! record, and it is the only thing [`super::PrivateAskCapability::from_probe`]
//! will accept as proof of a bounded egress.
//!
//! Properties verified on macOS 25.5 before this was written:
//!
//! * `(allow network-outbound (remote ip "localhost:<port>"))` permits exactly
//!   that loopback port; a second loopback port is refused.
//! * With the mDNSResponder allowance removed, `gethostbyname` inside the
//!   sandbox fails. The child therefore cannot resolve a name even to try.
//!
//! Deliberate non-features:
//!
//! * There is no plain-HTTP forwarding path. The proxy speaks exactly one verb,
//!   `CONNECT`, to exactly one `host:443`. A provider that is not reachable
//!   that way cannot be certified here, and is refused rather than accommodated
//!   by widening the proxy.
//! * Suffix matching is not used for the host. `api.anthropic.com.evil.test` is
//!   a different host, and a suffix rule would accept it.

use super::PrivateAskFailure;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener as StdTcpListener};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Bytes accepted for one CONNECT request head. A client that sends more than
/// this without completing its head is refused; the read is bounded, so it
/// cannot grow this process.
const REQUEST_HEAD_LIMIT: usize = 4 * 1024;
/// Concurrent tunnels one run may hold open. A single one-shot answer needs
/// one; the slack covers a client that opens a second connection for a retry.
const MAX_CONCURRENT_TUNNELS: u32 = 4;
/// Bytes relayed in one direction of one tunnel before it is closed. A private
/// Ask carries a bounded prompt and a bounded answer; anything past this is not
/// the workload.
const TUNNEL_BYTE_LIMIT: u64 = 8 * 1024 * 1024;
/// Wall-clock bound for one tunnel, independent of the attempt's own deadline.
const TUNNEL_TIMEOUT: Duration = Duration::from_secs(120);
/// Recorded targets retained per list. The record is evidence, not a log, and
/// an unbounded vector would be an unbounded resource held by a hostile child.
const RECORDED_TARGET_LIMIT: usize = 64;
/// The only port a private Ask may reach off the machine.
const PROVIDER_PORT: u16 = 443;
/// Deadline for the proxy thread to wind down once the run is over.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// The single destination one private Ask run is allowed to reach.
///
/// Construction is validating: there is no way to hold a provider host that is
/// an IP literal, carries a port, or is not a plausible DNS name. A caller that
/// cannot name a host cannot start a proxy, and therefore cannot run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderHost(String);

impl ProviderHost {
    /// Accept one lowercase DNS host name, or refuse.
    pub(crate) fn parse(value: &str) -> Result<Self, PrivateAskFailure> {
        let host = normalize_host(value).ok_or(PrivateAskFailure::EgressBoundUnverified)?;
        // An IP literal would make the host check meaningless: the proxy is the
        // only component that resolves names, so a numeric target is a request
        // to skip the check rather than to pass it.
        if host.parse::<std::net::IpAddr>().is_ok() {
            return Err(PrivateAskFailure::EgressBoundUnverified);
        }
        if !host.contains('.') {
            return Err(PrivateAskFailure::EgressBoundUnverified);
        }
        Ok(Self(host))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Lowercase a host, strip one trailing dot, and reject anything that is not a
/// bounded ASCII DNS name. `API.Anthropic.Com.` and `api.anthropic.com` are the
/// same host; `api.anthropic.com..` and a host with a control byte are not
/// hosts at all.
fn normalize_host(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let trimmed = trimmed.strip_suffix('.').unwrap_or(trimmed);
    if trimmed.is_empty()
        || trimmed.len() > 253
        || !trimmed.is_ascii()
        || trimmed.ends_with('.')
        || trimmed.starts_with('.')
        || trimmed.contains("..")
        || !trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.')
    {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

/// Why one CONNECT attempt was refused. The reason is part of the evidence: a
/// refusal list that cannot distinguish "wrong host" from "malformed" would let
/// a garbage client stand in for a real hostile-target attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefusalReason {
    /// The request was not `CONNECT`, or was not a request at all.
    NotConnect,
    /// The target host is not the configured provider.
    ForeignHost,
    /// The target port is not 443.
    ForeignPort,
    /// The concurrent-tunnel cap was already reached.
    TunnelCap,
}

/// One refused CONNECT, as the proxy saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefusedTarget {
    target: String,
    reason: RefusalReason,
}

impl RefusedTarget {
    pub(crate) fn target(&self) -> &str {
        &self.target
    }

    pub(crate) fn reason(&self) -> RefusalReason {
        self.reason
    }
}

#[derive(Debug, Default)]
struct ProxyRecord {
    accepted: Vec<String>,
    refused: Vec<RefusedTarget>,
    /// Accepted targets the proxy then failed to reach. Counted separately so a
    /// provider outage can never be mistaken for a refusal.
    dial_failures: u32,
    /// Set when either list hit `RECORDED_TARGET_LIMIT`. A truncated record is
    /// not evidence of a complete set, and the producer refuses it.
    truncated: bool,
}

impl ProxyRecord {
    fn accept(&mut self, target: String) {
        if self.accepted.len() >= RECORDED_TARGET_LIMIT {
            self.truncated = true;
            return;
        }
        self.accepted.push(target);
    }

    fn refuse(&mut self, target: String, reason: RefusalReason) {
        if self.refused.len() >= RECORDED_TARGET_LIMIT {
            self.truncated = true;
            return;
        }
        // A refused target is attacker-controlled text that ends up in evidence
        // and in a report. Bound it and strip control bytes here, once.
        let target: String = target
            .chars()
            .filter(|character| !character.is_control())
            .take(128)
            .collect();
        self.refused.push(RefusedTarget { target, reason });
    }
}

/// What the proxy itself observed for one run.
///
/// Fields are private and there is no production constructor that takes them
/// piecemeal: the only way to hold one is to have run a proxy. That is the same
/// discipline `SessionIsolationEvidence` uses, and for the same reason — a
/// capability must not be mintable by a caller that observed nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EgressObservation {
    provider_host: String,
    proxy_port: u16,
    accepted: Vec<String>,
    refused: Vec<RefusedTarget>,
    dial_failures: u32,
    truncated: bool,
    /// Connections the probe's own off-provider listener accepted directly from
    /// the contained child. Anything but zero means the sandbox leaked.
    direct_connections: u32,
}

impl EgressObservation {
    pub(crate) fn provider_host(&self) -> &str {
        &self.provider_host
    }

    pub(crate) fn proxy_port(&self) -> u16 {
        self.proxy_port
    }

    pub(crate) fn accepted(&self) -> &[String] {
        &self.accepted
    }

    pub(crate) fn refused(&self) -> &[RefusedTarget] {
        &self.refused
    }

    pub(crate) fn direct_connections(&self) -> u32 {
        self.direct_connections
    }

    pub(crate) fn dial_failures(&self) -> u32 {
        self.dial_failures
    }

    /// The provider was actually reached, every accepted target was that
    /// provider, something else was actually attempted and refused, no direct
    /// connection was observed, and the record is complete.
    ///
    /// Two of these clauses are the load-bearing ones and they are opposites.
    /// Without the non-empty *refusal* requirement, a proxy that accepts
    /// everything and a proxy that was never tested produce identical evidence.
    /// Without the non-empty *accepted* requirement, a policy that blocks every
    /// destination — including the provider — looks exactly like one that is
    /// correctly scoped, because both leave the accepted list empty.
    pub(crate) fn bounds_egress(&self) -> bool {
        !self.truncated
            && self.proxy_port != 0
            && !self.provider_host.is_empty()
            && !self.accepted.is_empty()
            && !self.refused.is_empty()
            && self.direct_connections == 0
            && self
                .accepted
                .iter()
                .all(|target| target == &self.provider_host)
    }

    /// Attach the count of connections a probe's own off-provider listener
    /// accepted directly from the contained child.
    ///
    /// A production run has no such listener, so its observation carries zero
    /// by construction and that clause certifies nothing on its own. It becomes
    /// evidence only in a probe capture, where a listener really was watching.
    pub(crate) fn with_observed_direct_connections(mut self, count: u32) -> Self {
        self.direct_connections = count;
        self
    }

    /// Assemble an observation without running a proxy. Test-only on purpose.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_parts(
        provider_host: String,
        proxy_port: u16,
        accepted: Vec<String>,
        refused: Vec<(String, RefusalReason)>,
        dial_failures: u32,
        truncated: bool,
        direct_connections: u32,
    ) -> Self {
        Self {
            provider_host,
            proxy_port,
            accepted,
            refused: refused
                .into_iter()
                .map(|(target, reason)| RefusedTarget { target, reason })
                .collect(),
            dial_failures,
            truncated,
            direct_connections,
        }
    }
}

struct ProxyState {
    provider: ProviderHost,
    record: Mutex<ProxyRecord>,
    tunnels: AtomicU32,
    shutdown: Arc<AtomicBool>,
}

/// A running proxy, owned by one attempt.
///
/// Dropping the handle stops the listener and joins the thread. Every return
/// path out of `PrivateAskAttempt::run` therefore stops it, including the ones
/// that fail before spawn — there is no path that leaves an open loopback port
/// behind a finished run.
pub(crate) struct EgressProxy {
    port: u16,
    state: Arc<ProxyState>,
    thread: Option<JoinHandle<()>>,
}

impl EgressProxy {
    /// Bind a loopback listener and start serving it on a dedicated thread.
    ///
    /// The listener is bound synchronously so the port is known before the
    /// thread exists: the port goes into the Seatbelt policy and the child's
    /// environment, and both must be settled before anything is spawned.
    pub(crate) fn start(provider: ProviderHost) -> Result<Self, PrivateAskFailure> {
        let listener = StdTcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .map_err(|_| PrivateAskFailure::EgressBoundUnverified)?;
        let port = listener
            .local_addr()
            .map_err(|_| PrivateAskFailure::EgressBoundUnverified)?
            .port();
        if port == 0 {
            return Err(PrivateAskFailure::EgressBoundUnverified);
        }
        listener
            .set_nonblocking(true)
            .map_err(|_| PrivateAskFailure::EgressBoundUnverified)?;
        let state = Arc::new(ProxyState {
            provider,
            record: Mutex::new(ProxyRecord::default()),
            tunnels: AtomicU32::new(0),
            shutdown: Arc::new(AtomicBool::new(false)),
        });
        let thread_state = Arc::clone(&state);
        let thread = std::thread::Builder::new()
            .name("private-ask-egress".to_owned())
            .spawn(move || serve(listener, thread_state))
            .map_err(|_| PrivateAskFailure::EgressBoundUnverified)?;
        Ok(Self {
            port,
            state,
            thread: Some(thread),
        })
    }

    /// The loopback port the Seatbelt policy and the child environment name.
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    pub(crate) fn provider_host(&self) -> &ProviderHost {
        &self.state.provider
    }

    /// Whether the proxy is still serving. Credential injection is gated on
    /// this: a token must never be handed to a child whose egress is not
    /// already bounded by a live proxy.
    pub(crate) fn is_listening(&self) -> bool {
        !self.state.shutdown.load(Ordering::Acquire)
            && self
                .thread
                .as_ref()
                .is_some_and(|thread| !thread.is_finished())
    }

    /// Environment entries directing a child's HTTP client at this proxy.
    ///
    /// A numeric loopback URL, never a name: the child has no resolver. Both
    /// casings are set because the runtimes' clients disagree about which they
    /// read, and `ALL_PROXY` covers the ones that read neither.
    pub(crate) fn child_env(&self) -> Vec<(String, String)> {
        let url = format!("http://127.0.0.1:{}", self.port);
        [
            "HTTPS_PROXY",
            "https_proxy",
            "HTTP_PROXY",
            "http_proxy",
            "ALL_PROXY",
            "all_proxy",
        ]
        .into_iter()
        .map(|name| (name.to_owned(), url.clone()))
        // NO_PROXY is set empty rather than left unset: a client that inherits
        // a bypass list would route around the only egress this run has.
        .chain([
            ("NO_PROXY".to_owned(), String::new()),
            ("no_proxy".to_owned(), String::new()),
        ])
        .collect()
    }

    /// Stop the proxy and take its record, together with the count of direct
    /// connections the probe's own listener saw.
    ///
    /// `direct_connections` is supplied by the caller because the proxy cannot
    /// observe a connection that bypassed it; it is the same class of evidence
    /// as the hostile-tool probe's own listener count.
    pub(crate) fn observe(mut self, direct_connections: u32) -> EgressObservation {
        self.stop();
        let record = match self.state.record.lock() {
            Ok(record) => ProxyRecord {
                accepted: record.accepted.clone(),
                refused: record.refused.clone(),
                dial_failures: record.dial_failures,
                truncated: record.truncated,
            },
            // A poisoned record is not evidence. Report it as truncated, which
            // `bounds_egress` refuses, rather than as an empty clean run.
            Err(_) => ProxyRecord {
                truncated: true,
                ..ProxyRecord::default()
            },
        };
        EgressObservation {
            provider_host: self.state.provider.as_str().to_owned(),
            proxy_port: self.port,
            accepted: record.accepted,
            refused: record.refused,
            dial_failures: record.dial_failures,
            truncated: record.truncated,
            direct_connections,
        }
    }

    /// Stop the proxy through its production teardown without consuming the
    /// handle, so a test can observe the state a stopped proxy leaves behind.
    #[cfg(test)]
    pub(super) fn force_stop_for_test(&self) {
        self.state.shutdown.store(true, Ordering::Release);
    }

    fn stop(&mut self) {
        self.state.shutdown.store(true, Ordering::Release);
        // Unblock the accept loop, which polls the flag between accepts.
        let _ = std::net::TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)),
            Duration::from_millis(200),
        );
        if let Some(thread) = self.thread.take() {
            let deadline = std::time::Instant::now() + SHUTDOWN_TIMEOUT;
            while !thread.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            // A thread that ignored the flag is left detached rather than
            // joined forever; it holds no run-root resource and its listener is
            // already refusing work.
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for EgressProxy {
    fn drop(&mut self) {
        self.stop();
    }
}

fn serve(listener: StdTcpListener, state: Arc<ProxyState>) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    // A `LocalSet` keeps every tunnel on this one thread. `tokio::spawn` would
    // need a multi-thread runtime and would put the proxy's work on the shared
    // pool, where it would no longer be the single named thread the zero-relay
    // -publish attribution accounts for.
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, async move {
        let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
            return;
        };
        accept_loop(listener, state).await;
    });
}

async fn accept_loop(listener: tokio::net::TcpListener, state: Arc<ProxyState>) {
    let mut tasks = Vec::new();
    while !state.shutdown.load(Ordering::Acquire) {
        let accepted = tokio::time::timeout(Duration::from_millis(100), listener.accept()).await;
        let stream = match accepted {
            Ok(Ok((stream, _))) => stream,
            // A transient accept error must not end the loop; a repeated one is
            // bounded by the run's own deadline.
            Ok(Err(_)) => continue,
            Err(_) => continue,
        };
        if state.shutdown.load(Ordering::Acquire) {
            break;
        }
        let task_state = Arc::clone(&state);
        tasks.retain(|task: &tokio::task::JoinHandle<()>| !task.is_finished());
        tasks.push(tokio::task::spawn_local(async move {
            handle_client(stream, task_state).await;
        }));
    }
    // Drop the listener before draining so nothing new is accepted, then let
    // in-flight tunnels finish inside their own timeout.
    drop(listener);
    for task in tasks {
        let _ = tokio::time::timeout(TUNNEL_TIMEOUT, task).await;
    }
}

async fn handle_client(mut stream: tokio::net::TcpStream, state: Arc<ProxyState>) {
    use tokio::io::AsyncWriteExt;

    let head =
        match tokio::time::timeout(Duration::from_secs(10), read_request_head(&mut stream)).await {
            Ok(Ok(head)) => head,
            // A client that never completes a request head is simply dropped: it
            // asked for nothing, so there is nothing to record.
            _ => return,
        };
    let target = match parse_connect_target(&head) {
        Ok(target) => target,
        Err(reason) => {
            record_refusal(&state, request_summary(&head), reason);
            let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await;
            return;
        }
    };
    if target.port != PROVIDER_PORT {
        record_refusal(&state, target.to_string(), RefusalReason::ForeignPort);
        let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await;
        return;
    }
    if target.host != state.provider.as_str() {
        record_refusal(&state, target.to_string(), RefusalReason::ForeignHost);
        let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\n\r\n").await;
        return;
    }
    // Reserve a tunnel slot before dialling so the cap bounds sockets, not
    // just completed tunnels.
    let previous = state.tunnels.fetch_add(1, Ordering::AcqRel);
    if previous >= MAX_CONCURRENT_TUNNELS {
        state.tunnels.fetch_sub(1, Ordering::AcqRel);
        record_refusal(&state, target.to_string(), RefusalReason::TunnelCap);
        let _ = stream
            .write_all(b"HTTP/1.1 429 Too Many Requests\r\n\r\n")
            .await;
        return;
    }
    let _slot = TunnelSlot {
        state: Arc::clone(&state),
    };
    // Recorded before the dial: the target was allowed, and a provider outage
    // must not be able to present itself as a refusal.
    if let Ok(mut record) = state.record.lock() {
        record.accept(target.host.clone());
    }
    let upstream = tokio::net::TcpStream::connect((target.host.as_str(), target.port)).await;
    let upstream = match upstream {
        Ok(upstream) => upstream,
        Err(_) => {
            if let Ok(mut record) = state.record.lock() {
                record.dial_failures = record.dial_failures.saturating_add(1);
            }
            let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await;
            return;
        }
    };
    if stream
        .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
        .await
        .is_err()
    {
        return;
    }
    let _ = tokio::time::timeout(TUNNEL_TIMEOUT, pump(stream, upstream)).await;
}

/// Releases one tunnel slot on every exit path, including an early return.
struct TunnelSlot {
    state: Arc<ProxyState>,
}

impl Drop for TunnelSlot {
    fn drop(&mut self) {
        self.state.tunnels.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Relay both directions under a per-direction byte cap.
///
/// The cap closes the tunnel rather than truncating silently in place: a
/// stream that outgrew the bound is not a completed answer, and the run's own
/// output parsing refuses a truncated result.
async fn pump(client: tokio::net::TcpStream, upstream: tokio::net::TcpStream) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (client_read, mut client_write) = client.into_split();
    let (upstream_read, mut upstream_write) = upstream.into_split();
    let mut client_read = client_read.take(TUNNEL_BYTE_LIMIT);
    let mut upstream_read = upstream_read.take(TUNNEL_BYTE_LIMIT);
    let outbound = async {
        let _ = tokio::io::copy(&mut client_read, &mut upstream_write).await;
        let _ = upstream_write.shutdown().await;
    };
    let inbound = async {
        let _ = tokio::io::copy(&mut upstream_read, &mut client_write).await;
        let _ = client_write.shutdown().await;
    };
    tokio::join!(outbound, inbound);
}

fn record_refusal(state: &ProxyState, target: String, reason: RefusalReason) {
    if let Ok(mut record) = state.record.lock() {
        record.refuse(target, reason);
    }
}

async fn read_request_head(stream: &mut tokio::net::TcpStream) -> io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;

    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while head.len() < REQUEST_HEAD_LIMIT {
        let read = stream.read(&mut byte).await?;
        if read == 0 {
            break;
        }
        head.push(byte[0]);
        if head.ends_with(b"\r\n\r\n") || head.ends_with(b"\n\n") {
            return Ok(head);
        }
    }
    Ok(head)
}

/// A parsed CONNECT target.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ConnectTarget {
    pub(super) host: String,
    pub(super) port: u16,
}

impl std::fmt::Display for ConnectTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}:{}", self.host, self.port)
    }
}

/// A short, control-free summary of a request that was not a CONNECT at all,
/// for the refusal record.
fn request_summary(head: &[u8]) -> String {
    head.iter()
        .copied()
        .take_while(|byte| *byte != b'\r' && *byte != b'\n')
        .map(|byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                byte as char
            } else {
                '?'
            }
        })
        .take(128)
        .collect()
}

/// Parse `CONNECT host:port HTTP/1.1` and nothing else.
///
/// Byte-oriented and ASCII-only on purpose: a non-ASCII target is refused
/// before any UTF-8 conversion, so a hostile client cannot choose which
/// decoding error the proxy takes.
pub(super) fn parse_connect_target(head: &[u8]) -> Result<ConnectTarget, RefusalReason> {
    let line = head
        .split(|byte| *byte == b'\n')
        .next()
        .ok_or(RefusalReason::NotConnect)?;
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.is_empty() || !line.is_ascii() {
        return Err(RefusalReason::NotConnect);
    }
    let line = std::str::from_utf8(line).map_err(|_| RefusalReason::NotConnect)?;
    let mut fields = line.split(' ');
    let method = fields.next().ok_or(RefusalReason::NotConnect)?;
    let target = fields.next().ok_or(RefusalReason::NotConnect)?;
    let version = fields.next().ok_or(RefusalReason::NotConnect)?;
    // Case-sensitive: HTTP methods are case-sensitive, and a client sending
    // `connect` is not speaking the protocol this proxy implements.
    if method != "CONNECT"
        || fields.next().is_some()
        || !version.starts_with("HTTP/1.")
        || target.is_empty()
    {
        return Err(RefusalReason::NotConnect);
    }
    // Authority-form only: exactly one colon, host then port. `host`,
    // `host:443:443` and `[::1]:443` all fail here.
    let (host, port) = target.rsplit_once(':').ok_or(RefusalReason::ForeignPort)?;
    if host.contains(':') {
        return Err(RefusalReason::ForeignHost);
    }
    let port: u16 = port.parse().map_err(|_| RefusalReason::ForeignPort)?;
    let host = normalize_host(host).ok_or(RefusalReason::ForeignHost)?;
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Err(RefusalReason::ForeignHost);
    }
    Ok(ConnectTarget { host, port })
}

#[cfg(test)]
#[path = "egress_proxy_tests.rs"]
mod tests;
