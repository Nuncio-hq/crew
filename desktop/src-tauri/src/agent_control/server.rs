//! Localhost axum `POST /agent-control` (media-proxy pattern).

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use super::protocol::{ControlRequest, ControlResponse, PROTOCOL_VERSION};
use super::runtime::ControlRuntime;
use super::token::{bearer_matches, generate_token};

#[derive(Clone)]
pub struct AgentControlHandle {
    pub runtime: Arc<ControlRuntime>,
    pub token: String,
    pub port: Arc<AtomicU16>,
    pub url: Arc<Mutex<Option<String>>>,
}

impl AgentControlHandle {
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(ControlRuntime::new()),
            token: generate_token(),
            port: Arc::new(AtomicU16::new(0)),
            url: Arc::new(Mutex::new(None)),
        }
    }

    pub fn control_url(&self) -> Option<String> {
        let port = self.port.load(Ordering::Relaxed);
        if port == 0 {
            None
        } else {
            Some(format!("http://127.0.0.1:{port}/agent-control"))
        }
    }

    /// Bind `127.0.0.1:0` on the calling thread so spawn-time env can read the
    /// port before the axum task starts (agent restore races a fire-and-forget
    /// spawn).
    pub fn bind_now(&self) -> Option<std::net::TcpListener> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
        let _ = listener.set_nonblocking(true);
        let port = listener.local_addr().ok()?.port();
        self.port.store(port, Ordering::Relaxed);
        Some(listener)
    }

    #[cfg(test)]
    pub async fn listen_url(&self) -> Option<String> {
        self.url.lock().await.clone()
    }
}

impl Default for AgentControlHandle {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct HttpState {
    handle: AgentControlHandle,
}

#[cfg(test)]
pub async fn spawn_agent_control(handle: AgentControlHandle) -> u16 {
    spawn_agent_control_on(handle, None).await
}

pub async fn spawn_agent_control_on(
    handle: AgentControlHandle,
    prebound: Option<std::net::TcpListener>,
) -> u16 {
    let state = HttpState {
        handle: handle.clone(),
    };
    let app = Router::new()
        .route("/agent-control", post(agent_control_handler))
        .route("/agent-control/bridge-reply", post(bridge_reply_handler))
        .with_state(state);

    let listener = match prebound {
        Some(std_listener) => match TcpListener::from_std(std_listener) {
            Ok(l) => l,
            Err(error) => {
                eprintln!("buzz-desktop: agent-control from_std failed: {error}");
                return 0;
            }
        },
        None => match TcpListener::bind("127.0.0.1:0").await {
            Ok(l) => l,
            Err(error) => {
                eprintln!("buzz-desktop: agent-control bind failed: {error}");
                return 0;
            }
        },
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    handle.port.store(port, Ordering::Relaxed);
    let url = format!("http://127.0.0.1:{port}/agent-control");
    *handle.url.lock().await = Some(url);
    eprintln!("buzz-desktop: agent-control listening on 127.0.0.1:{port}");

    let ticker = handle.runtime.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            interval.tick().await;
            ticker.tick().await;
        }
    });

    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    port
}

async fn agent_control_handler(
    State(state): State<HttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let origin = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !origin.is_empty() {
        return (
            StatusCode::FORBIDDEN,
            serde_json::to_string(&ControlResponse::err(
                None,
                super::protocol::ControlError::instrument_unreachable(
                    "browser-originated requests are rejected",
                )
                .into_body(),
            ))
            .unwrap_or_else(|_| "{\"v\":1,\"error\":{\"code\":\"instrument_unreachable\",\"message\":\"forbidden\"}}".into()),
        )
            .into_response();
    }

    let auth = headers.get("authorization").and_then(|v| v.to_str().ok());
    if !bearer_matches(auth, &state.handle.token) {
        return (
            StatusCode::UNAUTHORIZED,
            serde_json::to_string(&ControlResponse::err(
                None,
                super::protocol::ControlError::instrument_unreachable(
                    "missing or stale desktop control token (restart the agent after a desktop restart)",
                )
                .into_body(),
            ))
            .unwrap_or_else(|_| "{\"v\":1,\"error\":{\"code\":\"instrument_unreachable\",\"message\":\"unauthorized\"}}".into()),
        )
            .into_response();
    }

    let req: ControlRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                serde_json::to_string(&ControlResponse::err(
                    None,
                    super::protocol::ControlError::instrument_unreachable(format!(
                        "invalid control request: {error}"
                    ))
                    .into_body(),
                ))
                .unwrap_or_else(|_| format!("{{\"v\":{PROTOCOL_VERSION},\"error\":{{\"code\":\"instrument_unreachable\",\"message\":\"bad request\"}}}}")),
            )
                .into_response();
        }
    };

    let response = state.handle.runtime.handle(req).await;
    let body = serde_json::to_string(&response).unwrap_or_else(|_| {
        "{\"v\":1,\"error\":{\"code\":\"instrument_unreachable\",\"message\":\"serialize failed\"}}"
            .into()
    });
    (StatusCode::OK, body).into_response()
}

async fn bridge_reply_handler(State(state): State<HttpState>, body: Bytes) -> impl IntoResponse {
    let parsed: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let nonce = parsed.get("nonce").and_then(|v| v.as_str()).unwrap_or("");
    let id = parsed.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let payload = parsed
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    if nonce.is_empty() || id.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if state
        .handle
        .runtime
        .complete_bridge_reply(nonce, id, payload)
        .await
    {
        StatusCode::NO_CONTENT.into_response()
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}
