//! Production adapters: #196 Resource Governor, webview eval, sim bridge.
//! Contract tests leave `ControlRuntime.live` unset and stay on the fakes.

use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::{oneshot, Mutex};

use crate::resource_governor::{
    backend, bridge_describe_ui_args, bridge_tap_args, discover_sim_bridge, window_label,
    BridgeAvailability, BrowserBackend, DeviceLifecycle, ResourceGovernorHandle,
    SIM_BRIDGE_INSTALL_HINT,
};

use super::bridge_js::BROWSER_BRIDGE_JS;
use super::origin::origin_of_url;
use super::protocol::{ControlError, SnapshotFilter};
use super::snapshot::{build_snapshot, find_node, require_digest, Snapshot, SnapshotNode};

/// Overall deadline for a bridge round-trip (inject + invoke + reply).
const CALL_JS_DEADLINE: Duration = Duration::from_secs(8);
/// Per-attempt wait before retrying injection within the deadline.
const CALL_JS_ATTEMPT_WAIT: Duration = Duration::from_millis(400);
/// Gap between retry attempts so we don't spin-eval the same script.
const CALL_JS_RETRY_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Clone)]
pub struct LiveHost {
    pub app: tauri::AppHandle,
    nonce: String,
    waiters: Arc<Mutex<HashMap<String, oneshot::Sender<serde_json::Value>>>>,
    port: Arc<AtomicU16>,
}

impl LiveHost {
    pub fn new(
        app: tauri::AppHandle,
        nonce: String,
        waiters: Arc<Mutex<HashMap<String, oneshot::Sender<serde_json::Value>>>>,
        port: Arc<AtomicU16>,
    ) -> Self {
        Self {
            app,
            nonce,
            waiters,
            port,
        }
    }

    pub fn emit_ui(&self, payload: serde_json::Value) {
        let _ = self.app.emit("agent-control", payload);
    }

    pub fn emit_evidence(&self, payload: serde_json::Value) {
        let _ = self.app.emit("agent-control-evidence", payload);
    }

    fn governor(&self) -> Result<ResourceGovernorHandle, ControlError> {
        self.app
            .try_state::<ResourceGovernorHandle>()
            .map(|s| (*s).clone())
            .ok_or_else(|| {
                ControlError::instrument_unreachable("Resource Governor is not attached")
            })
    }

    fn reply_url(&self) -> Option<String> {
        let port = self.port.load(Ordering::Relaxed);
        if port == 0 {
            None
        } else {
            Some(format!(
                "http://127.0.0.1:{port}/agent-control/bridge-reply"
            ))
        }
    }

    /// Injects the bridge and invokes it, retrying within `CALL_JS_DEADLINE`
    /// instead of a single eval-then-wait shot (#247). `inject_bridge` +
    /// `eval_js` are fire-and-forget: on a fresh webview or right after
    /// `browser_navigate`, the injected script can race the page's own load
    /// (or get wiped by a navigation landing a moment later) and silently
    /// never run, so the first attempt's reply never arrives. Retrying with a
    /// short per-attempt wait lets a page that becomes ready partway through
    /// the deadline still answer, instead of surfacing a hard
    /// "browser bridge did not reply" every time injection loses the race.
    async fn call_js(
        &self,
        channel_id: &str,
        invoke: &str,
    ) -> Result<serde_json::Value, ControlError> {
        let deadline = tokio::time::Instant::now() + CALL_JS_DEADLINE;
        let mut last_error = ControlError::timeout("browser bridge did not reply");
        loop {
            if let Err(error) = self.inject_bridge(channel_id) {
                last_error = error;
            } else {
                let id = super::token::generate_token();
                let (tx, rx) = oneshot::channel();
                self.waiters.lock().await.insert(id.clone(), tx);
                let js = format!("window.__CREW_AGENT_BRIDGE__.{invoke};");
                let js = js.replace(
                    "__ID__",
                    &serde_json::to_string(&id).unwrap_or_else(|_| "\"\"".into()),
                );
                if let Err(error) = self.eval_js(channel_id, &js) {
                    self.waiters.lock().await.remove(&id);
                    last_error = error;
                } else {
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    let attempt_wait = remaining.min(CALL_JS_ATTEMPT_WAIT);
                    match tokio::time::timeout(attempt_wait, rx).await {
                        Ok(Ok(value)) => return Ok(value),
                        Ok(Err(_)) => {
                            last_error = ControlError::instrument_unreachable(
                                "browser bridge reply channel closed",
                            );
                        }
                        Err(_) => {
                            self.waiters.lock().await.remove(&id);
                        }
                    }
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(last_error);
            }
            tokio::time::sleep(CALL_JS_RETRY_INTERVAL).await;
        }
    }

    pub fn ensure_sim(&self, channel_id: &str) -> Result<String, ControlError> {
        match discover_sim_bridge() {
            BridgeAvailability::Available { .. } => {}
            BridgeAvailability::Missing { install_hint } => {
                return Err(ControlError::bridge_missing(install_hint));
            }
            BridgeAvailability::Failed { message } => {
                return Err(ControlError::bridge_missing(format!(
                    "{SIM_BRIDGE_INSTALL_HINT}\n{message}"
                )));
            }
        }
        let handle = self.governor()?;
        let _ = handle.agent_note_activity(channel_id);
        match handle.agent_boot_sim(channel_id) {
            Ok(holding) => holding.udid.ok_or_else(|| {
                ControlError::instrument_unreachable("sim boot did not yield a UDID")
            }),
            Err(err) if err.starts_with("cap:") || err.contains("boot cap") => {
                let holders = handle
                    .agent_status()
                    .map(|status| {
                        status
                            .sims
                            .iter()
                            .filter(|s| {
                                matches!(
                                    s.lifecycle,
                                    DeviceLifecycle::Booted | DeviceLifecycle::Mirroring
                                )
                            })
                            .map(|s| {
                                s.channel_name
                                    .clone()
                                    .unwrap_or_else(|| s.channel_id.clone())
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Err(ControlError::boot_capacity(&holders))
            }
            Err(err) => Err(ControlError::timeout(err)),
        }
    }

    pub fn ensure_browser(&self, channel_id: &str) -> Result<String, ControlError> {
        let handle = self.governor()?;
        let subject = handle
            .agent_status()
            .ok()
            .and_then(|status| {
                status
                    .servers
                    .iter()
                    .find(|s| s.channel_id == channel_id)
                    .and_then(|s| s.url.clone())
            })
            .unwrap_or_else(|| "http://127.0.0.1:5173".into());
        let hidden = handle
            .agent_attach_webview(channel_id, &subject)
            .unwrap_or(true);
        self.open_or_keep_webview(channel_id, &subject, hidden)?;
        self.inject_bridge(channel_id)?;
        Ok(subject)
    }

    fn open_or_keep_webview(
        &self,
        channel_id: &str,
        url: &str,
        hidden: bool,
    ) -> Result<(), ControlError> {
        let label = window_label(channel_id);
        if self.app.get_webview_window(&label).is_some() {
            return Ok(());
        }
        let parsed = url::Url::parse(url).map_err(|e| {
            ControlError::instrument_unreachable(format!("invalid browser url: {e}"))
        })?;
        let (w, h) = if hidden { (1.0, 1.0) } else { (960.0, 720.0) };
        let use_window =
            matches!(backend(), BrowserBackend::Window) || cfg!(not(target_os = "macos"));
        if use_window {
            let mut builder =
                WebviewWindowBuilder::new(&self.app, &label, WebviewUrl::External(parsed))
                    .title("Browser")
                    .inner_size(w, h)
                    .visible(!hidden);
            if let Some(dir) = self.profile_dir(channel_id) {
                builder = builder.data_directory(dir);
            }
            builder
                .build()
                .map_err(|e| ControlError::instrument_unreachable(format!("webview: {e}")))?;
            return Ok(());
        }
        #[cfg(target_os = "macos")]
        {
            self.open_child(channel_id, url)?;
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn open_child(&self, channel_id: &str, url: &str) -> Result<(), ControlError> {
        use tauri::webview::WebviewBuilder;
        let Some(window) = self.app.get_window("main") else {
            return Err(ControlError::instrument_unreachable("main window missing"));
        };
        let label = window_label(channel_id);
        if self.app.get_webview(&label).is_some() {
            return Ok(());
        }
        let parsed = url::Url::parse(url).map_err(|e| {
            ControlError::instrument_unreachable(format!("invalid browser url: {e}"))
        })?;
        let mut builder = WebviewBuilder::new(&label, WebviewUrl::External(parsed));
        if let Some(dir) = self.profile_dir(channel_id) {
            builder = builder.data_directory(dir);
        }
        window
            .add_child(
                builder,
                tauri::LogicalPosition::new(0.0, 0.0),
                tauri::LogicalSize::new(1.0, 1.0),
            )
            .map_err(|e| ControlError::instrument_unreachable(format!("child webview: {e}")))?;
        Ok(())
    }

    fn profile_dir(&self, channel_id: &str) -> Option<std::path::PathBuf> {
        let dir = self
            .app
            .path()
            .app_cache_dir()
            .ok()?
            .join("crew-agent-browser")
            .join(channel_id);
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    fn inject_bridge(&self, channel_id: &str) -> Result<(), ControlError> {
        self.eval_js(channel_id, BROWSER_BRIDGE_JS)?;
        let Some(url) = self.reply_url() else {
            return Ok(());
        };
        let url_js = serde_json::to_string(&url).unwrap_or_else(|_| "\"\"".into());
        let nonce_js = serde_json::to_string(&self.nonce).unwrap_or_else(|_| "\"\"".into());
        self.eval_js(
            channel_id,
            &format!(
                "window.__CREW_BRIDGE_REPLY_URL={url_js};window.__CREW_BRIDGE_NONCE={nonce_js};"
            ),
        )
    }

    fn eval_js(&self, channel_id: &str, js: &str) -> Result<(), ControlError> {
        let label = window_label(channel_id);
        if let Some(window) = self.app.get_webview_window(&label) {
            window
                .eval(js)
                .map_err(|e| ControlError::instrument_unreachable(format!("eval: {e}")))?;
            return Ok(());
        }
        #[cfg(target_os = "macos")]
        if let Some(view) = self.app.get_webview(&label) {
            view.eval(js)
                .map_err(|e| ControlError::instrument_unreachable(format!("eval: {e}")))?;
            return Ok(());
        }
        Err(ControlError::instrument_unreachable(
            "browser webview is not attached",
        ))
    }

    pub async fn browser_snapshot(
        &self,
        channel_id: &str,
        filter: SnapshotFilter,
    ) -> Result<Snapshot, ControlError> {
        let filter_js = match filter {
            SnapshotFilter::All => "all",
            SnapshotFilter::Interactive => "interactive",
        };
        let payload = self
            .call_js(channel_id, &format!("replySnapshot(__ID__, {filter_js:?})"))
            .await?;
        let origin = payload
            .get("origin")
            .and_then(|v| v.as_str())
            .unwrap_or("about:blank");
        let nodes = payload
            .get("nodes")
            .cloned()
            .and_then(|v| serde_json::from_value::<Vec<SnapshotNode>>(v).ok())
            .unwrap_or_default();
        Ok(build_snapshot(origin, nodes))
    }

    pub async fn browser_click_ref(
        &self,
        channel_id: &str,
        r#ref: &str,
    ) -> Result<serde_json::Value, ControlError> {
        let ref_js = serde_json::to_string(r#ref).unwrap_or_else(|_| "\"\"".into());
        let payload = self
            .call_js(channel_id, &format!("replyClickRef(__ID__, {ref_js})"))
            .await?;
        if payload.get("ok").and_then(|v| v.as_bool()) == Some(false) {
            return Err(ControlError::not_actionable(r#ref));
        }
        Ok(payload)
    }

    pub async fn browser_type_ref(
        &self,
        channel_id: &str,
        r#ref: &str,
        text: &str,
        submit: bool,
    ) -> Result<serde_json::Value, ControlError> {
        let ref_js = serde_json::to_string(r#ref).unwrap_or_else(|_| "\"\"".into());
        let text_js = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
        let payload = self
            .call_js(
                channel_id,
                &format!("replyTypeRef(__ID__, {ref_js}, {text_js}, {submit})"),
            )
            .await?;
        if payload.get("ok").and_then(|v| v.as_bool()) == Some(false) {
            return Err(ControlError::not_actionable(r#ref));
        }
        Ok(payload)
    }

    pub fn browser_scroll(
        &self,
        channel_id: &str,
        r#ref: Option<&str>,
        direction: &str,
        amount: Option<f64>,
    ) -> Result<serde_json::Value, ControlError> {
        self.inject_bridge(channel_id)?;
        let ref_js = serde_json::to_string(&r#ref).unwrap_or_else(|_| "null".into());
        let dir_js = serde_json::to_string(direction).unwrap_or_else(|_| "\"down\"".into());
        let amt = amount.unwrap_or(400.0);
        self.eval_js(
            channel_id,
            &format!("window.__CREW_AGENT_BRIDGE__.scroll({ref_js}, {dir_js}, {amt});"),
        )?;
        Ok(serde_json::json!({ "ok": true }))
    }

    pub async fn browser_evaluate_js(
        &self,
        channel_id: &str,
        js: &str,
    ) -> Result<serde_json::Value, ControlError> {
        let js_lit = serde_json::to_string(js).unwrap_or_else(|_| "\"\"".into());
        self.call_js(channel_id, &format!("replyEvaluate(__ID__, {js_lit})"))
            .await
    }

    pub async fn browser_console(
        &self,
        channel_id: &str,
        since: Option<u64>,
    ) -> Result<serde_json::Value, ControlError> {
        let since_js = since
            .map(|s| s.to_string())
            .unwrap_or_else(|| "null".into());
        self.call_js(channel_id, &format!("replyConsole(__ID__, {since_js})"))
            .await
    }

    pub async fn browser_navigate(&self, channel_id: &str, url: &str) -> Result<(), ControlError> {
        let escaped = serde_json::to_string(url).unwrap_or_else(|_| "\"about:blank\"".into());
        if self
            .eval_js(channel_id, &format!("location.href = {escaped};"))
            .is_ok()
        {
            // The navigation triggered above is async from Rust's viewpoint:
            // `location.href` tears down the current document and the new one
            // has not necessarily painted by the time this call returns. Give
            // it a short, non-blocking beat (yields the runtime instead of
            // `std::thread::sleep`'s hard stall) before the first re-inject —
            // `call_js`'s own retry loop is the real safety net for any
            // subsequent snapshot/click that races this navigation (#247).
            tokio::time::sleep(Duration::from_millis(50)).await;
            let _ = self.inject_bridge(channel_id);
            return Ok(());
        }
        self.open_or_keep_webview(channel_id, url, true)
    }

    pub fn current_origin(&self, url: &str) -> Result<String, ControlError> {
        origin_of_url(url)
    }

    pub fn browser_screenshot_png(&self) -> Vec<u8> {
        PNG_1X1.to_vec()
    }

    pub fn sim_snapshot(&self, udid: &str) -> Result<Snapshot, ControlError> {
        let json = run_bridge(udid, bridge_describe_ui_args)?;
        let nodes = parse_ax_tree(&json);
        Ok(build_snapshot("simulator://channel", nodes))
    }

    pub fn sim_tap(&self, udid: &str, x: f64, y: f64) -> Result<(), ControlError> {
        let _ = run_bridge(udid, |binary, id| bridge_tap_args(binary, id, x, y))?;
        Ok(())
    }

    pub fn sim_swipe(
        &self,
        udid: &str,
        from: (f64, f64),
        to: (f64, f64),
    ) -> Result<(), ControlError> {
        run_hid(udid, "swipe", Some(from), Some(to), None, None)
    }

    pub fn sim_type(&self, udid: &str, text: &str) -> Result<(), ControlError> {
        run_hid(udid, "text", None, None, None, Some(text))
    }

    pub fn sim_press(&self, udid: &str, button: &str) -> Result<(), ControlError> {
        run_hid(udid, button, None, None, None, None)
    }

    pub fn sim_launch(
        &self,
        udid: &str,
        bundle_id: Option<&str>,
        install_path: Option<&str>,
    ) -> Result<serde_json::Value, ControlError> {
        if let Some(path) = install_path {
            let output = Command::new("xcrun")
                .args(["simctl", "install", udid, path])
                .output()
                .map_err(|e| ControlError::instrument_unreachable(format!("sim install: {e}")))?;
            if !output.status.success() {
                return Err(ControlError::instrument_unreachable(format!(
                    "sim install failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }
        if let Some(bundle) = bundle_id {
            let output = Command::new("xcrun")
                .args(["simctl", "launch", udid, bundle])
                .output()
                .map_err(|e| ControlError::instrument_unreachable(format!("sim launch: {e}")))?;
            if !output.status.success() {
                return Err(ControlError::instrument_unreachable(format!(
                    "sim launch failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }
        Ok(serde_json::json!({ "ok": true }))
    }

    pub fn sim_screenshot_png(&self, udid: &str) -> Result<Vec<u8>, ControlError> {
        let output = Command::new("xcrun")
            .args(["simctl", "io", udid, "screenshot", "-"])
            .output()
            .map_err(|e| ControlError::instrument_unreachable(format!("sim screenshot: {e}")))?;
        if !output.status.success() || output.stdout.is_empty() {
            return Ok(PNG_1X1.to_vec());
        }
        Ok(output.stdout)
    }

    pub fn sim_record_png(&self, udid: &str, seconds: u32) -> Result<Vec<u8>, ControlError> {
        let _ = (udid, seconds.clamp(5, 60));
        self.sim_screenshot_png(udid)
    }

    pub fn sim_logs(
        &self,
        udid: &str,
        predicate: Option<&str>,
    ) -> Result<Vec<String>, ControlError> {
        let pred = predicate.unwrap_or("eventType == logEvent");
        let output = Command::new("xcrun")
            .args([
                "simctl",
                "spawn",
                udid,
                "log",
                "show",
                "--last",
                "2m",
                "--predicate",
                pred,
            ])
            .output()
            .map_err(|e| ControlError::instrument_unreachable(format!("sim logs: {e}")))?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text.lines().rev().take(200).map(str::to_string).collect())
    }

    pub fn status_json(&self, channel_id: &str) -> serde_json::Value {
        let Ok(handle) = self.governor() else {
            return serde_json::json!({ "channel_id": channel_id });
        };
        let Ok(status) = handle.agent_status() else {
            return serde_json::json!({ "channel_id": channel_id });
        };
        let sim = status.sims.iter().find(|s| s.channel_id == channel_id);
        let server = status.servers.iter().find(|s| s.channel_id == channel_id);
        serde_json::json!({
            "channel_id": channel_id,
            "browser_url": server.and_then(|s| s.url.clone()),
            "sim_state": sim.map(|s| format!("{:?}", s.lifecycle)),
            "dev_server_port": server.map(|s| s.port),
            "governor": {
                "booted": format!("{}/{}", status.booted_count, status.policy.max_booted_sims),
            },
        })
    }
}

const PNG_1X1: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 0, 0, 3, 1,
    1, 0, 24, 221, 141, 176, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn run_hid(
    udid: &str,
    action: &str,
    from: Option<(f64, f64)>,
    to: Option<(f64, f64)>,
    delta: Option<f64>,
    text: Option<&str>,
) -> Result<(), ControlError> {
    match discover_sim_bridge() {
        BridgeAvailability::Available { path, .. } => {
            let mut cmd = Command::new(&path);
            cmd.arg(action).arg("--udid").arg(udid);
            if let Some((x, y)) = from {
                cmd.arg("--x")
                    .arg(x.to_string())
                    .arg("--y")
                    .arg(y.to_string());
            }
            if let Some((x, y)) = to {
                cmd.arg("--x2")
                    .arg(x.to_string())
                    .arg("--y2")
                    .arg(y.to_string());
            }
            if let Some(d) = delta {
                cmd.arg("--delta").arg(d.to_string());
            }
            if let Some(t) = text {
                cmd.arg("--text").arg(t);
            }
            let output = cmd
                .output()
                .map_err(|e| ControlError::instrument_unreachable(format!("sim {action}: {e}")))?;
            if !output.status.success() {
                return Err(ControlError::instrument_unreachable(format!(
                    "sim {action} failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            Ok(())
        }
        BridgeAvailability::Missing { install_hint } => {
            Err(ControlError::bridge_missing(install_hint))
        }
        BridgeAvailability::Failed { message } => Err(ControlError::bridge_missing(format!(
            "{SIM_BRIDGE_INSTALL_HINT}\n{message}"
        ))),
    }
}

fn run_bridge(
    udid: &str,
    args: impl FnOnce(&str, &str) -> Vec<String>,
) -> Result<String, ControlError> {
    match discover_sim_bridge() {
        BridgeAvailability::Available { binary, path } => {
            let argv = args(&binary, udid);
            let output = Command::new(&path)
                .args(&argv)
                .output()
                .map_err(|e| ControlError::instrument_unreachable(format!("sim bridge: {e}")))?;
            if !output.status.success() {
                return Err(ControlError::instrument_unreachable(format!(
                    "sim bridge failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        BridgeAvailability::Missing { install_hint } => {
            Err(ControlError::bridge_missing(install_hint))
        }
        BridgeAvailability::Failed { message } => Err(ControlError::bridge_missing(format!(
            "{SIM_BRIDGE_INSTALL_HINT}\n{message}"
        ))),
    }
}

fn parse_ax_tree(raw: &str) -> Vec<SnapshotNode> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let nodes = if let Some(arr) = value.as_array() {
        arr.clone()
    } else if let Some(arr) = value.get("nodes").and_then(|v| v.as_array()) {
        arr.clone()
    } else if let Some(arr) = value.get("children").and_then(|v| v.as_array()) {
        arr.clone()
    } else {
        vec![value]
    };
    nodes.into_iter().filter_map(ax_node).collect()
}

fn ax_node(value: serde_json::Value) -> Option<SnapshotNode> {
    let obj = value.as_object()?;
    let role = obj
        .get("role")
        .or_else(|| obj.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("generic")
        .to_string();
    let name = obj
        .get("name")
        .or_else(|| obj.get("label"))
        .or_else(|| obj.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let bounds = obj.get("frame").or_else(|| obj.get("rect")).and_then(|f| {
        Some(super::snapshot::Bounds {
            x: f.get("x")?.as_f64().unwrap_or(0.0),
            y: f.get("y")?.as_f64().unwrap_or(0.0),
            w: f.get("width")
                .or_else(|| f.get("w"))?
                .as_f64()
                .unwrap_or(0.0),
            h: f.get("height")
                .or_else(|| f.get("h"))?
                .as_f64()
                .unwrap_or(0.0),
        })
    });
    let children = obj
        .get("children")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().cloned().filter_map(ax_node).collect())
        .unwrap_or_default();
    Some(SnapshotNode {
        r#ref: String::new(),
        role,
        name,
        value: None,
        actionable: true,
        bounds,
        children,
    })
}

pub fn click_point_from_snapshot(
    snap: &Snapshot,
    r#ref: Option<&str>,
    point: Option<(f64, f64)>,
    digest: Option<&str>,
) -> Result<(f64, f64), ControlError> {
    require_digest(&snap.snapshot_digest, digest)?;
    if let Some(r) = r#ref {
        let node = find_node(&snap.nodes, r).ok_or_else(|| ControlError::not_actionable(r))?;
        let bounds = node
            .bounds
            .as_ref()
            .ok_or_else(|| ControlError::not_actionable(r))?;
        Ok((bounds.x + bounds.w / 2.0, bounds.y + bounds.h / 2.0))
    } else {
        point.ok_or_else(|| ControlError::not_actionable("x,y"))
    }
}
