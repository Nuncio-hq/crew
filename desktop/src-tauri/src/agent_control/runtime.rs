//! Control runtime: lease + origin + instruments + dispatch.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU16};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{oneshot, Mutex};

use super::flight::{CachedOutcome, FlightTable};
use super::instruments::{FakeBrowser, FakeGovernor, FakeSim};
use super::lease::{LeaseMap, LeaseView};
use super::live::LiveHost;
use super::origin::{origin_of_url, OriginDecision, OriginPolicy};
use super::overlay::{OverlayFrame, OverlayLog};
use super::protocol::{
    method_instrument, method_is_input, method_is_mutating, ControlError, ControlRequest,
    ControlResponse, ErrorCode, Instrument, SnapshotFilter, PROTOCOL_VERSION,
};

pub struct ControlRuntime {
    pub leases: Mutex<LeaseMap>,
    pub flights: Mutex<FlightTable>,
    pub origin: Mutex<OriginPolicy>,
    pub overlay: Mutex<OverlayLog>,
    pub evidence: Mutex<Vec<serde_json::Value>>,
    pub canvas_writes: Mutex<Vec<(String, String)>>,
    pub elicitation: Mutex<Option<OriginDecision>>,
    pub origin_waiters: Mutex<Vec<tokio::sync::oneshot::Sender<OriginDecision>>>,
    pub pending_origin: Mutex<Option<serde_json::Value>>,
    pub browser: FakeBrowser,
    pub sim: FakeSim,
    pub governor: FakeGovernor,
    pub live: std::sync::Mutex<Option<LiveHost>>,
    pub now_ms: Mutex<u64>,
    pub recording_abort: Mutex<Option<Arc<AtomicBool>>>,
    pub bridge_nonce: String,
    pub bridge_waiters: Arc<Mutex<HashMap<String, oneshot::Sender<serde_json::Value>>>>,
}

impl ControlRuntime {
    pub fn new() -> Self {
        Self {
            leases: Mutex::new(LeaseMap::default()),
            flights: Mutex::new(FlightTable::default()),
            origin: Mutex::new(OriginPolicy::default()),
            overlay: Mutex::new(OverlayLog::default()),
            evidence: Mutex::new(Vec::new()),
            canvas_writes: Mutex::new(Vec::new()),
            elicitation: Mutex::new(None),
            origin_waiters: Mutex::new(Vec::new()),
            pending_origin: Mutex::new(None),
            browser: FakeBrowser::default(),
            sim: FakeSim::default(),
            governor: FakeGovernor::new(),
            live: std::sync::Mutex::new(None),
            now_ms: Mutex::new(now_wall_ms()),
            recording_abort: Mutex::new(None),
            bridge_nonce: super::token::generate_token(),
            bridge_waiters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn attach_live(&self, app: tauri::AppHandle, port: Arc<AtomicU16>) {
        if let Ok(mut slot) = self.live.lock() {
            *slot = Some(LiveHost::new(
                app,
                self.bridge_nonce.clone(),
                Arc::clone(&self.bridge_waiters),
                port,
            ));
        }
    }

    fn live_host(&self) -> Option<LiveHost> {
        self.live.lock().ok().and_then(|slot| slot.clone())
    }

    pub async fn complete_bridge_reply(
        &self,
        nonce: &str,
        id: &str,
        payload: serde_json::Value,
    ) -> bool {
        if nonce != self.bridge_nonce {
            return false;
        }
        if let Some(tx) = self.bridge_waiters.lock().await.remove(id) {
            let _ = tx.send(payload);
            return true;
        }
        false
    }

    #[cfg(test)]
    pub async fn set_now(&self, ms: u64) {
        *self.now_ms.lock().await = ms;
        self.leases.lock().await.tick(ms);
    }

    pub async fn tick(&self) {
        let now = now_wall_ms();
        *self.now_ms.lock().await = now;
        self.leases.lock().await.tick(now);
    }

    pub async fn lease_views(&self) -> Vec<LeaseView> {
        let now = *self.now_ms.lock().await;
        self.leases.lock().await.view_all(now)
    }

    pub async fn set_elicitation(&self, decision: OriginDecision) {
        *self.elicitation.lock().await = Some(decision);
        let waiters = std::mem::take(&mut *self.origin_waiters.lock().await);
        for waiter in waiters {
            let _ = waiter.send(decision);
        }
        *self.pending_origin.lock().await = None;
    }

    pub async fn handle(&self, req: ControlRequest) -> ControlResponse {
        let id = req.id.clone();
        if req.v != PROTOCOL_VERSION {
            return ControlResponse::err(
                id,
                ControlError::instrument_unreachable(format!(
                    "unsupported protocol version {} (expected {PROTOCOL_VERSION})",
                    req.v
                ))
                .into_body(),
            );
        }
        match self.dispatch(req).await {
            Ok(result) => ControlResponse::ok(id, result),
            Err(error) => ControlResponse::err(id, error.into_body()),
        }
    }

    async fn dispatch(&self, req: ControlRequest) -> Result<serde_json::Value, ControlError> {
        if req.method == "lease.release" {
            self.leases.lock().await.release_turn(&req.channel_id);
            if let Some(abort) = self.recording_abort.lock().await.take() {
                abort.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            return Ok(serde_json::json!({ "ok": true }));
        }
        if req.method == "lease.take_over" {
            let instrument = instrument_from_params(&req.params).unwrap_or(Instrument::Browser);
            let now = *self.now_ms.lock().await;
            self.leases
                .lock()
                .await
                .preempt_human(&req.channel_id, instrument, now);
            return Ok(serde_json::json!({ "ok": true }));
        }
        if req.method == "lease.release_human" {
            let instrument = instrument_from_params(&req.params).unwrap_or(Instrument::Browser);
            self.leases
                .lock()
                .await
                .release_human(&req.channel_id, instrument);
            return Ok(serde_json::json!({ "ok": true }));
        }
        if req.method == "lease.note_human" {
            let instrument = instrument_from_params(&req.params).unwrap_or(Instrument::Browser);
            let now = *self.now_ms.lock().await;
            self.leases
                .lock()
                .await
                .note_human_input(&req.channel_id, instrument, now);
            return Ok(serde_json::json!({ "ok": true }));
        }

        if let Some(request_id) = req.request_id.as_deref() {
            if method_is_mutating(&req.method) {
                if let Some(cached) = self.flights.lock().await.lookup(request_id) {
                    if let Some(error) = cached.error {
                        return Err(ControlError {
                            code: parse_code(&error.code),
                            message: error.message,
                            data: error.data,
                        });
                    }
                    return Ok(cached.result.unwrap_or(serde_json::json!({})));
                }
            }
        }

        let instrument = method_instrument(&req.method);
        let flight_lock = if let Some(inst) = instrument {
            Some(self.flights.lock().await.lock_for(&req.channel_id, inst))
        } else {
            None
        };
        let _flight_guard = match &flight_lock {
            Some(lock) => Some(lock.lock().await),
            None => None,
        };

        let abort = if method_is_input(&req.method) {
            let inst = instrument.ok_or_else(|| {
                ControlError::instrument_unreachable("input tool missing instrument")
            })?;
            Some(self.leases.lock().await.acquire_agent(
                &req.channel_id,
                inst,
                req.agent_name.as_deref(),
            )?)
        } else {
            None
        };
        let abort_ref = abort.as_deref();

        let result = self.execute(&req, abort_ref).await;
        if let Some(request_id) = req.request_id.clone() {
            let outcome = match &result {
                Ok(value) => CachedOutcome {
                    result: Some(value.clone()),
                    error: None,
                },
                Err(error) => CachedOutcome {
                    result: None,
                    error: Some(error.clone().into_body()),
                },
            };
            self.flights.lock().await.remember(request_id, outcome);
        }
        result
    }

    async fn execute(
        &self,
        req: &ControlRequest,
        abort: Option<&AtomicBool>,
    ) -> Result<serde_json::Value, ControlError> {
        let abort = abort.unwrap_or(IDLE_ABORT.get_or_init(|| AtomicBool::new(false)));
        match req.method.as_str() {
            "desktop_status" => {
                let mut status = if let Some(live) = self.live_host() {
                    live.status_json(&req.channel_id)
                } else {
                    self.governor.status(&req.channel_id).await
                };
                if let Some(obj) = status.as_object_mut() {
                    obj.insert(
                        "lease".into(),
                        serde_json::to_value(self.lease_views().await)
                            .unwrap_or(serde_json::json!([])),
                    );
                }
                Ok(status)
            }
            "browser_navigate" => self.browser_navigate(req, abort).await,
            "browser_snapshot" => {
                let _ = self.ensure_browser(&req.channel_id).await?;
                let filter = snapshot_filter(&req.params);
                let snap = if let Some(live) = self.live_host() {
                    live.browser_snapshot(&req.channel_id, filter).await?
                } else {
                    self.browser.snapshot(filter).await?
                };
                serde_json::to_value(snap).map_err(|e| {
                    ControlError::instrument_unreachable(format!("serialize snapshot: {e}"))
                })
            }
            "browser_click" => {
                let r = param_str(&req.params, "ref")?;
                let digest = param_opt_str(&req.params, "snapshot_digest");
                self.emit_overlay(req, Instrument::Browser, "browser_click", Some(r), None)
                    .await;
                if let Some(live) = self.live_host() {
                    let snap = live
                        .browser_snapshot(&req.channel_id, SnapshotFilter::Interactive)
                        .await?;
                    super::snapshot::require_digest(&snap.snapshot_digest, digest.as_deref())?;
                    live.browser_click_ref(&req.channel_id, r).await?;
                    return Ok(serde_json::json!({ "snapshot_digest": snap.snapshot_digest }));
                }
                self.browser.click(r, digest.as_deref(), abort).await
            }
            "browser_type" => {
                let r = param_str(&req.params, "ref")?;
                let text = param_str(&req.params, "text")?;
                let digest = param_opt_str(&req.params, "snapshot_digest");
                let submit = req
                    .params
                    .get("submit")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if let Some(live) = self.live_host() {
                    let snap = live
                        .browser_snapshot(&req.channel_id, SnapshotFilter::Interactive)
                        .await?;
                    super::snapshot::require_digest(&snap.snapshot_digest, digest.as_deref())?;
                    live.browser_type_ref(&req.channel_id, r, text, submit)
                        .await?;
                    return Ok(serde_json::json!({ "snapshot_digest": snap.snapshot_digest }));
                }
                self.browser.r#type(r, text, digest.as_deref(), abort).await
            }
            "browser_scroll" => {
                let r = param_opt_str(&req.params, "ref");
                let digest = param_opt_str(&req.params, "snapshot_digest");
                let direction =
                    param_opt_str(&req.params, "direction").unwrap_or_else(|| "down".into());
                let amount = req.params.get("amount").and_then(|v| v.as_f64());
                if let Some(live) = self.live_host() {
                    if let Some(digest) = digest.as_deref() {
                        let snap = live
                            .browser_snapshot(&req.channel_id, SnapshotFilter::Interactive)
                            .await?;
                        super::snapshot::require_digest(&snap.snapshot_digest, Some(digest))?;
                    }
                    return live.browser_scroll(&req.channel_id, r.as_deref(), &direction, amount);
                }
                self.browser
                    .scroll(r.as_deref(), digest.as_deref(), abort)
                    .await
            }
            "browser_evaluate" => self.browser_evaluate(req).await,
            "browser_console" => {
                let since = req.params.get("since").and_then(|v| v.as_u64());
                if let Some(live) = self.live_host() {
                    return live.browser_console(&req.channel_id, since).await;
                }
                let entries = self.browser.console(since).await;
                Ok(serde_json::json!({ "entries": entries }))
            }
            "browser_screenshot" => self.screenshot(req, true).await,
            "sim_snapshot" => {
                self.ensure_sim(&req.channel_id).await?;
                let filter = snapshot_filter(&req.params);
                if let Some(live) = self.live_host() {
                    let udid = live.ensure_sim(&req.channel_id)?;
                    let snap = live.sim_snapshot(&udid)?;
                    return serde_json::to_value(snap).map_err(|e| {
                        ControlError::instrument_unreachable(format!("serialize snapshot: {e}"))
                    });
                }
                let snap = self.sim.snapshot(filter).await?;
                serde_json::to_value(snap).map_err(|e| {
                    ControlError::instrument_unreachable(format!("serialize snapshot: {e}"))
                })
            }
            "sim_tap" => self.sim_tap(req, abort).await,
            "sim_swipe" => {
                self.ensure_sim(&req.channel_id).await?;
                super::flight::aborted(abort)?;
                let from = point_pair(&req.params, "from")?;
                let to = point_pair(&req.params, "to")?;
                if let Some(live) = self.live_host() {
                    let udid = live.ensure_sim(&req.channel_id)?;
                    live.sim_swipe(&udid, from, to)?;
                    return Ok(serde_json::json!({ "ok": true }));
                }
                self.sim.swipe(abort).await
            }
            "sim_type" => {
                let text = param_str(&req.params, "text")?;
                self.ensure_sim(&req.channel_id).await?;
                if let Some(live) = self.live_host() {
                    let udid = live.ensure_sim(&req.channel_id)?;
                    live.sim_type(&udid, text)?;
                    return Ok(serde_json::json!({ "ok": true }));
                }
                self.sim.r#type(text, abort).await
            }
            "sim_press" => {
                let button = param_str(&req.params, "button")?;
                self.ensure_sim(&req.channel_id).await?;
                if let Some(live) = self.live_host() {
                    let udid = live.ensure_sim(&req.channel_id)?;
                    live.sim_press(&udid, button)?;
                    return Ok(serde_json::json!({ "ok": true }));
                }
                self.sim.press(button, abort).await
            }
            "sim_launch" => {
                self.ensure_sim(&req.channel_id).await?;
                let bundle = param_opt_str(&req.params, "bundle_id");
                let install = param_opt_str(&req.params, "install_path");
                if let Some(live) = self.live_host() {
                    let udid = live.ensure_sim(&req.channel_id)?;
                    return live.sim_launch(&udid, bundle.as_deref(), install.as_deref());
                }
                self.sim.launch(bundle.as_deref(), install.as_deref()).await
            }
            "sim_screenshot" => self.screenshot(req, false).await,
            "sim_record" => {
                let seconds = req
                    .params
                    .get("seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5) as u32;
                *self.recording_abort.lock().await = Some(Arc::new({
                    let flag = AtomicBool::new(false);
                    if abort.load(std::sync::atomic::Ordering::SeqCst) {
                        flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    flag
                }));
                let bytes = if let Some(live) = self.live_host() {
                    let udid = live.ensure_sim(&req.channel_id)?;
                    live.sim_record_png(&udid, seconds)?
                } else {
                    self.sim.record(seconds, abort).await?
                };
                self.maybe_evidence(req, &bytes, "clip").await?;
                Ok(serde_json::json!({
                    "png_base64": base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        &bytes,
                    ),
                    "mime": "image/png",
                }))
            }
            "sim_logs" => {
                let since = req.params.get("since").and_then(|v| v.as_u64());
                let predicate = param_opt_str(&req.params, "predicate");
                if let Some(live) = self.live_host() {
                    let udid = live.ensure_sim(&req.channel_id)?;
                    let entries = live.sim_logs(&udid, predicate.as_deref())?;
                    return Ok(serde_json::json!({ "entries": entries }));
                }
                let entries = self.sim.logs(since, predicate.as_deref()).await;
                Ok(serde_json::json!({ "entries": entries }))
            }
            other => Err(ControlError::instrument_unreachable(format!(
                "unknown method {other}"
            ))),
        }
    }

    async fn browser_navigate(
        &self,
        req: &ControlRequest,
        abort: &AtomicBool,
    ) -> Result<serde_json::Value, ControlError> {
        super::flight::aborted(abort)?;
        let subject = self.ensure_browser(&req.channel_id).await?;
        let url = param_opt_str(&req.params, "url").unwrap_or_else(|| subject.clone());
        self.enforce_origin(req, &subject, &url).await?;
        if let Some(live) = self.live_host() {
            live.browser_navigate(&req.channel_id, &url)?;
            return Ok(serde_json::json!({
                "url": url,
                "title": "",
                "snapshot_digest": "",
            }));
        }
        self.browser.navigate(&url).await
    }

    async fn browser_evaluate(
        &self,
        req: &ControlRequest,
    ) -> Result<serde_json::Value, ControlError> {
        let subject = self.ensure_browser(&req.channel_id).await?;
        let js = param_str(&req.params, "js")?;
        if let Some(live) = self.live_host() {
            let origin = live.current_origin(&subject)?;
            self.origin
                .lock()
                .await
                .check(&req.channel_id, &subject, &origin)?;
            return live.browser_evaluate_js(&req.channel_id, js).await;
        }
        let page_origin = self.browser.current_origin().await;
        self.origin
            .lock()
            .await
            .check(&req.channel_id, &subject, &page_origin)?;
        self.browser.evaluate(js).await
    }

    async fn sim_tap(
        &self,
        req: &ControlRequest,
        abort: &AtomicBool,
    ) -> Result<serde_json::Value, ControlError> {
        self.ensure_sim(&req.channel_id).await?;
        let r#ref = param_opt_str(&req.params, "ref");
        let digest = param_opt_str(&req.params, "snapshot_digest");
        let point = match (
            req.params.get("x").and_then(|v| v.as_f64()),
            req.params.get("y").and_then(|v| v.as_f64()),
        ) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        };
        self.emit_overlay(req, Instrument::Sim, "sim_tap", r#ref.as_deref(), point)
            .await;
        if let Some(live) = self.live_host() {
            let udid = live.ensure_sim(&req.channel_id)?;
            let snap = live.sim_snapshot(&udid)?;
            let (x, y) = super::live::click_point_from_snapshot(
                &snap,
                r#ref.as_deref(),
                point,
                digest.as_deref(),
            )?;
            live.sim_tap(&udid, x, y)?;
            return Ok(serde_json::json!({
                "snapshot_digest": snap.snapshot_digest,
                "x": x,
                "y": y,
            }));
        }
        self.sim
            .tap(r#ref.as_deref(), point, digest.as_deref(), abort)
            .await
    }

    async fn screenshot(
        &self,
        req: &ControlRequest,
        browser: bool,
    ) -> Result<serde_json::Value, ControlError> {
        if browser {
            let _ = self.ensure_browser(&req.channel_id).await?;
        } else {
            self.ensure_sim(&req.channel_id).await?;
        }
        let bytes = if let Some(live) = self.live_host() {
            if browser {
                live.browser_screenshot_png()
            } else {
                let udid = live.ensure_sim(&req.channel_id)?;
                live.sim_screenshot_png(&udid)?
            }
        } else if browser {
            self.browser.screenshot().await
        } else {
            self.sim.screenshot().await
        };
        self.maybe_evidence(req, &bytes, "shot").await?;
        Ok(serde_json::json!({
            "png_base64": base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &bytes,
            ),
            "mime": "image/png",
        }))
    }

    async fn maybe_evidence(
        &self,
        req: &ControlRequest,
        bytes: &[u8],
        kind: &str,
    ) -> Result<(), ControlError> {
        let post = req
            .params
            .get("post_evidence")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !post {
            return Ok(());
        }
        self.evidence.lock().await.push(serde_json::json!({
            "channel_id": req.channel_id,
            "thread_root_id": req.thread_root_id,
            "kind": kind,
            "bytes": bytes.len(),
            "tag": "before-after-visual",
        }));
        if let Some(live) = self.live_host() {
            live.emit_evidence(serde_json::json!({
                "channelId": req.channel_id,
                "threadRootId": req.thread_root_id,
                "kind": kind,
                "pngBase64": base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    bytes,
                ),
            }));
        }
        Ok(())
    }

    async fn ensure_browser(&self, channel_id: &str) -> Result<String, ControlError> {
        if let Some(live) = self.live_host() {
            return live.ensure_browser(channel_id);
        }
        self.governor.ensure_browser(channel_id).await
    }

    async fn ensure_sim(&self, channel_id: &str) -> Result<serde_json::Value, ControlError> {
        if let Some(live) = self.live_host() {
            let udid = live.ensure_sim(channel_id)?;
            return Ok(serde_json::json!({ "udid": udid }));
        }
        self.governor.ensure_sim(channel_id).await
    }

    async fn enforce_origin(
        &self,
        req: &ControlRequest,
        subject: &str,
        url: &str,
    ) -> Result<(), ControlError> {
        let target = origin_of_url(url)?;
        if self
            .origin
            .lock()
            .await
            .is_allowed(&req.channel_id, subject, &target)
        {
            return Ok(());
        }
        let live_attached = self.live_host().is_some();
        let decision = match *self.elicitation.lock().await {
            Some(d) => d,
            None if live_attached => self.wait_origin_decision(req, &target).await?,
            None => return Err(ControlError::origin_blocked(&target)),
        };
        match decision {
            OriginDecision::AllowOnce => {
                self.origin
                    .lock()
                    .await
                    .grant_once(&req.channel_id, &target);
                Ok(())
            }
            OriginDecision::AllowDomain => {
                self.origin
                    .lock()
                    .await
                    .grant_domain(&req.channel_id, &target);
                self.canvas_writes
                    .lock()
                    .await
                    .push((req.channel_id.clone(), target));
                Ok(())
            }
            OriginDecision::Deny => Err(ControlError::origin_blocked(&target)),
        }
    }

    async fn wait_origin_decision(
        &self,
        req: &ControlRequest,
        target: &str,
    ) -> Result<OriginDecision, ControlError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.origin_waiters.lock().await.push(tx);
        *self.pending_origin.lock().await = Some(serde_json::json!({
            "channelId": req.channel_id,
            "origin": target,
            "agentName": req.agent_name,
        }));
        self.publish_ui().await;
        match tokio::time::timeout(std::time::Duration::from_secs(300), rx).await {
            Ok(Ok(decision)) => Ok(decision),
            Ok(Err(_)) => Err(ControlError::origin_blocked(target)),
            Err(_) => Err(ControlError::timeout("origin approval timed out")),
        }
    }

    async fn publish_ui(&self) {
        let views = self.lease_views().await;
        let overlay = self.overlay.lock().await.frames.last().cloned();
        let pending = self.pending_origin.lock().await.clone();
        let payload = serde_json::json!({
            "leases": views,
            "overlay": overlay,
            "pendingOrigin": pending,
        });
        if let Some(live) = self.live_host() {
            live.emit_ui(payload);
        }
    }

    async fn emit_overlay(
        &self,
        req: &ControlRequest,
        instrument: Instrument,
        tool: &str,
        r#ref: Option<&str>,
        point: Option<(f64, f64)>,
    ) {
        let now = *self.now_ms.lock().await;
        let frame = OverlayFrame::click(instrument, tool, &req.channel_id, r#ref, point, None, now);
        self.overlay.lock().await.push(frame);
        self.publish_ui().await;
    }
}

impl Default for ControlRuntime {
    fn default() -> Self {
        Self::new()
    }
}

static IDLE_ABORT: std::sync::OnceLock<AtomicBool> = std::sync::OnceLock::new();

fn now_wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn param_str<'a>(params: &'a serde_json::Value, key: &str) -> Result<&'a str, ControlError> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| ControlError::not_actionable(key))
}

fn param_opt_str(params: &serde_json::Value, key: &str) -> Option<String> {
    params.get(key).and_then(|v| v.as_str()).map(str::to_string)
}

fn snapshot_filter(params: &serde_json::Value) -> SnapshotFilter {
    match params.get("filter").and_then(|v| v.as_str()) {
        Some("all") => SnapshotFilter::All,
        _ => SnapshotFilter::Interactive,
    }
}

fn instrument_from_params(params: &serde_json::Value) -> Option<Instrument> {
    match params.get("instrument").and_then(|v| v.as_str()) {
        Some("sim") => Some(Instrument::Sim),
        Some("browser") => Some(Instrument::Browser),
        _ => None,
    }
}

fn point_pair(params: &serde_json::Value, key: &str) -> Result<(f64, f64), ControlError> {
    let arr = params
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or_else(|| ControlError::not_actionable(key))?;
    let x = arr
        .first()
        .and_then(|v| v.as_f64())
        .ok_or_else(|| ControlError::not_actionable(key))?;
    let y = arr
        .get(1)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| ControlError::not_actionable(key))?;
    Ok((x, y))
}

fn parse_code(code: &str) -> ErrorCode {
    match code {
        "bridge_missing" => ErrorCode::BridgeMissing,
        "boot_capacity" => ErrorCode::BootCapacity,
        "stale_ref" => ErrorCode::StaleRef,
        "not_actionable" => ErrorCode::NotActionable,
        "origin_blocked" => ErrorCode::OriginBlocked,
        "lease_held" => ErrorCode::LeaseHeld,
        "timeout" => ErrorCode::Timeout,
        _ => ErrorCode::InstrumentUnreachable,
    }
}
