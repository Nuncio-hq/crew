//! Single-use native network boundary for a sandboxed Hermes recap.
//!
//! Hermes receives a random loopback credential. The real provider credential
//! remains in the native keyring and is attached only by this fixed, one-request
//! forwarder. This is intentionally not a general proxy or provider registry.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{fs::OpenOptions, io::Write, path::Path};

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, Response, StatusCode};
use axum::routing::post;
use axum::Router;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use url::Url;

const REQUEST_LIMIT: usize = 512 * 1024;
const RESPONSE_LIMIT: usize = 1024 * 1024;
const GATEWAY_TIMEOUT: Duration = Duration::from_secs(120);
const CODEX_RESPONSES_ENDPOINT: &str = "https://chatgpt.com/backend-api/codex/responses";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HermesGatewayFailure {
    InvalidCredential,
    CredentialUnavailable,
    Bind,
    InvalidRequest,
    Upstream,
    ResponseLimit,
    EffectiveModelMismatch,
    ToolResponse,
    Incomplete,
}

/// Strict value stored under the runtime grant's native keyring reference.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct HermesProviderCredential {
    version: u8,
    endpoint: String,
    bearer_token: String,
    account_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HermesGatewayEvidence {
    pub(crate) effective_model: String,
    pub(crate) request_count: u8,
}

#[derive(Debug, Clone)]
pub(crate) struct HermesGatewayConnection {
    pub(crate) base_url: String,
    pub(crate) token: String,
}

/// Replace a copied profile with Crew's fixed no-tools, no-fallback route.
/// Source-profile bytes are never used as runtime configuration or auth.
pub(crate) fn write_locked_profile(
    destination: &Path,
    connection: &HermesGatewayConnection,
    model: &str,
) -> Result<(), HermesGatewayFailure> {
    if !destination.is_absolute()
        || model.is_empty()
        || model != model.trim()
        || !connection.base_url.starts_with("http://127.0.0.1:")
        || connection.token.len() != 64
        || !connection
            .token
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(HermesGatewayFailure::InvalidCredential);
    }
    if destination.exists() {
        std::fs::remove_dir_all(destination)
            .map_err(|_| HermesGatewayFailure::InvalidCredential)?;
    }
    std::fs::create_dir_all(destination).map_err(|_| HermesGatewayFailure::InvalidCredential)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| HermesGatewayFailure::InvalidCredential)?;
    }
    let config = serde_json::json!({
        "agent": {"api_max_retries": 0, "environment_probe": false, "reasoning_effort": "low"},
        "auxiliary": {
            "background_review": {"enabled": false},
            "title_generation": {"enabled": false}
        },
        "fallback_providers": [],
        "mcp_servers": {},
        "memory": {"memory_enabled": false, "user_profile_enabled": false},
        "model": {
            "api_mode": "codex_responses",
            "base_url": connection.base_url,
            "context_length": 65536,
            "default": model,
            "ollama_num_ctx": 65536,
            "provider": "openai-codex"
        },
        "platform_toolsets": {"cli": []}
    });
    let auth = serde_json::json!({
        "version": 1,
        "active_provider": "openai-codex",
        "credential_pool": {"openai-codex": [{
            "id": "crew-recap-one-shot",
            "label": "Crew recap one-shot gateway",
            "auth_type": "api_key",
            "access_token": connection.token,
            "base_url": connection.base_url,
            "priority": 0,
            "request_count": 0,
            "last_status": "ok",
            "source": "native"
        }]},
        "providers": {}
    });
    write_private_json(&destination.join("config.yaml"), &config)?;
    write_private_json(&destination.join("auth.json"), &auth)
}

fn write_private_json(path: &Path, value: &serde_json::Value) -> Result<(), HermesGatewayFailure> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|_| HermesGatewayFailure::InvalidCredential)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| HermesGatewayFailure::InvalidCredential)?;
    file.write_all(&bytes)
        .map_err(|_| HermesGatewayFailure::InvalidCredential)?;
    file.sync_all()
        .map_err(|_| HermesGatewayFailure::InvalidCredential)
}

pub(crate) struct HermesOneShotGateway {
    connection: HermesGatewayConnection,
    shutdown: Option<oneshot::Sender<()>>,
    result: std::sync::mpsc::Receiver<Result<HermesGatewayEvidence, HermesGatewayFailure>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

type GatewayResultSender = Arc<
    Mutex<Option<std::sync::mpsc::Sender<Result<HermesGatewayEvidence, HermesGatewayFailure>>>>,
>;

#[derive(Clone)]
struct GatewayState {
    expected_model: String,
    local_token: String,
    credential: HermesProviderCredential,
    consumed: Arc<AtomicBool>,
    result: GatewayResultSender,
    client: reqwest::Client,
    cancelled: CancellationToken,
}

impl HermesOneShotGateway {
    pub(crate) fn start(
        auth_service: &str,
        auth_reference: &str,
        expected_model: &str,
    ) -> Result<Self, HermesGatewayFailure> {
        let entries = crate::secret_store::SecretStore::keyring(auth_service)
            .load_all_readonly()
            .map_err(|_| HermesGatewayFailure::CredentialUnavailable)?
            .ok_or(HermesGatewayFailure::CredentialUnavailable)?;
        let value = entries
            .get(auth_reference)
            .ok_or(HermesGatewayFailure::CredentialUnavailable)?;
        let credential = parse_credential(value)?;
        Self::start_with_credential(credential, expected_model)
    }

    fn start_with_credential(
        credential: HermesProviderCredential,
        expected_model: &str,
    ) -> Result<Self, HermesGatewayFailure> {
        if expected_model.trim() != expected_model || expected_model.is_empty() {
            return Err(HermesGatewayFailure::InvalidCredential);
        }
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").map_err(|_| HermesGatewayFailure::Bind)?;
        listener
            .set_nonblocking(true)
            .map_err(|_| HermesGatewayFailure::Bind)?;
        let port = listener
            .local_addr()
            .map_err(|_| HermesGatewayFailure::Bind)?
            .port();
        let token = random_token()?;
        let connection = HermesGatewayConnection {
            base_url: format!("http://127.0.0.1:{port}"),
            token: token.clone(),
        };
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(GATEWAY_TIMEOUT)
            .pool_max_idle_per_host(0)
            .build()
            .map_err(|_| HermesGatewayFailure::Bind)?;
        let state = GatewayState {
            expected_model: expected_model.to_string(),
            local_token: token,
            credential,
            consumed: Arc::new(AtomicBool::new(false)),
            result: Arc::new(Mutex::new(Some(result_tx))),
            client,
            cancelled: CancellationToken::new(),
        };
        let thread = std::thread::Builder::new()
            .name("crew-hermes-recap-gateway".into())
            .spawn(move || serve(listener, state, shutdown_rx))
            .map_err(|_| HermesGatewayFailure::Bind)?;
        Ok(Self {
            connection,
            shutdown: Some(shutdown_tx),
            result: result_rx,
            thread: Some(thread),
        })
    }

    pub(crate) fn connection(&self) -> &HermesGatewayConnection {
        &self.connection
    }

    pub(crate) fn finish(mut self) -> Result<HermesGatewayEvidence, HermesGatewayFailure> {
        let result = self
            .result
            .recv_timeout(GATEWAY_TIMEOUT + Duration::from_secs(2))
            .map_err(|_| HermesGatewayFailure::Incomplete)?;
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| HermesGatewayFailure::Incomplete)?;
        }
        result
    }
}

impl Drop for HermesOneShotGateway {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(listener: std::net::TcpListener, state: GatewayState, shutdown: oneshot::Receiver<()>) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        send_result(&state, Err(HermesGatewayFailure::Bind));
        return;
    };
    runtime.block_on(async move {
        let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
            send_result(&state, Err(HermesGatewayFailure::Bind));
            return;
        };
        let app = Router::new()
            .route("/responses", post(forward))
            .layer(DefaultBodyLimit::max(REQUEST_LIMIT))
            .with_state(state.clone());
        let cancelled = state.cancelled.clone();
        let shutdown_cancel = cancelled.clone();
        tokio::spawn(async move {
            let _ = shutdown.await;
            shutdown_cancel.cancel();
        });
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(cancelled.cancelled_owned())
            .await;
    });
}

async fn forward(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == format!("Bearer {}", state.local_token));
    if !authorized {
        return fixed_response(StatusCode::FORBIDDEN);
    }
    if state
        .consumed
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return fixed_response(StatusCode::CONFLICT);
    }
    let request: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return fail(&state, HermesGatewayFailure::InvalidRequest),
    };
    let model_matches = request.get("model").and_then(|value| value.as_str())
        == Some(state.expected_model.as_str());
    let tools_absent = request
        .get("tools")
        .is_none_or(|tools| tools.is_null() || tools.as_array().is_some_and(Vec::is_empty));
    if !model_matches
        || !tools_absent
        || request.get("tool_choice").is_some()
        || request.get("parallel_tool_calls").is_some()
        || request.get("stream") != Some(&serde_json::Value::Bool(true))
    {
        return fail(&state, HermesGatewayFailure::InvalidRequest);
    }

    let upstream = state
        .client
        .post(&state.credential.endpoint)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", state.credential.bearer_token),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .header("chatgpt-account-id", &state.credential.account_id)
        .header("originator", "nuncio-crew-recap");
    let response = tokio::select! {
        response = upstream.body(body).send() => match response {
            Ok(response) => response,
            Err(_) => return fail(&state, HermesGatewayFailure::Upstream),
        },
        () = state.cancelled.cancelled() => return fail(&state, HermesGatewayFailure::Incomplete),
    };
    if !response.status().is_success() {
        return fail(&state, HermesGatewayFailure::Upstream);
    }
    let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    loop {
        let chunk = tokio::select! {
            chunk = stream.next() => chunk,
            () = state.cancelled.cancelled() => return fail(&state, HermesGatewayFailure::Incomplete),
        };
        let Some(chunk) = chunk else { break };
        let Ok(chunk) = chunk else {
            return fail(&state, HermesGatewayFailure::Upstream);
        };
        if bytes.len().saturating_add(chunk.len()) > RESPONSE_LIMIT {
            return fail(&state, HermesGatewayFailure::ResponseLimit);
        }
        bytes.extend_from_slice(&chunk);
    }
    let evidence = match inspect_response(&bytes, &state.expected_model) {
        Ok(evidence) => evidence,
        Err(error) => return fail(&state, error),
    };
    send_result(&state, Ok(evidence));
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    if let Some(content_type) = content_type {
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, content_type);
    }
    response
}

fn inspect_response(
    bytes: &[u8],
    expected_model: &str,
) -> Result<HermesGatewayEvidence, HermesGatewayFailure> {
    let mut effective_model = None;
    for line in bytes.split(|byte| *byte == b'\n') {
        let line = line.strip_prefix(b"data: ").unwrap_or(line);
        if line.is_empty() || line == b"[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        if contains_tool_output(&value) {
            return Err(HermesGatewayFailure::ToolResponse);
        }
        let candidate = (value.get("type").and_then(|kind| kind.as_str())
            == Some("response.completed"))
        .then(|| {
            value
                .get("response")
                .and_then(|response| response.get("model"))
                .and_then(|model| model.as_str())
        })
        .flatten();
        if let Some(candidate) = candidate {
            if candidate != expected_model {
                return Err(HermesGatewayFailure::EffectiveModelMismatch);
            }
            effective_model = Some(candidate.to_string());
        }
    }
    let effective_model = effective_model.ok_or(HermesGatewayFailure::EffectiveModelMismatch)?;
    Ok(HermesGatewayEvidence {
        effective_model,
        request_count: 1,
    })
}

fn contains_tool_output(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => map.iter().any(|(key, value)| {
            matches!(key.as_str(), "function_call" | "tool_call" | "tool_calls")
                || (key == "type"
                    && value.as_str().is_some_and(|kind| {
                        kind.contains("function_call") || kind.contains("tool_call")
                    }))
                || contains_tool_output(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(contains_tool_output),
        _ => false,
    }
}

fn parse_credential(value: &str) -> Result<HermesProviderCredential, HermesGatewayFailure> {
    let credential: HermesProviderCredential =
        serde_json::from_str(value).map_err(|_| HermesGatewayFailure::InvalidCredential)?;
    let endpoint =
        Url::parse(&credential.endpoint).map_err(|_| HermesGatewayFailure::InvalidCredential)?;
    if credential.version != 1
        || credential.bearer_token.is_empty()
        || credential.bearer_token.len() > 16 * 1024
        || credential.bearer_token.chars().any(char::is_control)
        || credential.account_id.is_empty()
        || credential.account_id.len() > 256
        || credential.account_id.chars().any(char::is_control)
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
        || credential.endpoint != CODEX_RESPONSES_ENDPOINT
    {
        return Err(HermesGatewayFailure::InvalidCredential);
    }
    Ok(credential)
}

fn random_token() -> Result<String, HermesGatewayFailure> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|_| HermesGatewayFailure::Bind)?;
    Ok(hex::encode(bytes))
}

fn fail(state: &GatewayState, error: HermesGatewayFailure) -> Response<Body> {
    send_result(state, Err(error));
    fixed_response(StatusCode::BAD_GATEWAY)
}

fn send_result(state: &GatewayState, result: Result<HermesGatewayEvidence, HermesGatewayFailure>) {
    if let Ok(mut sender) = state.result.lock() {
        if let Some(sender) = sender.take() {
            let _ = sender.send(result);
        }
    }
}

fn fixed_response(status: StatusCode) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{\"error\":\"recap_gateway_rejected\"}"))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

#[cfg(test)]
#[path = "recap_hermes_gateway/tests.rs"]
mod tests;
