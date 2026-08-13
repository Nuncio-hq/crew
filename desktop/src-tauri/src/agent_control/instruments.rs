//! Browser and simulator instrument ports. Tests use the fakes; production
//! adapters talk to the #196 Governor / webview / sim bridge.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use super::protocol::{ControlError, SnapshotFilter};
use super::snapshot::{build_snapshot, find_node, require_digest, Bounds, Snapshot, SnapshotNode};

const ACTIONABILITY_WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleEntry {
    pub t_ms: u64,
    pub kind: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Clone)]
pub struct FakeBrowser {
    inner: Arc<Mutex<FakeBrowserState>>,
}

struct FakeBrowserState {
    url: String,
    title: String,
    origin: String,
    nodes: Vec<SnapshotNode>,
    snapshot: Option<Snapshot>,
    console: Vec<ConsoleEntry>,
    #[allow(dead_code)]
    hidden: bool,
    evaluate_blocked: bool,
    last_js: Option<String>,
}

impl Default for FakeBrowser {
    fn default() -> Self {
        Self::new("http://127.0.0.1:5173")
    }
}

impl FakeBrowser {
    pub fn new(origin: &str) -> Self {
        let nodes = vec![SnapshotNode {
            r#ref: String::new(),
            role: "button".into(),
            name: "Save".into(),
            value: None,
            actionable: true,
            bounds: Some(Bounds {
                x: 40.0,
                y: 80.0,
                w: 64.0,
                h: 24.0,
            }),
            children: vec![],
        }];
        Self {
            inner: Arc::new(Mutex::new(FakeBrowserState {
                url: origin.to_string(),
                title: "App".into(),
                origin: origin.to_string(),
                nodes,
                snapshot: None,
                console: vec![],
                hidden: true,
                evaluate_blocked: false,
                last_js: None,
            })),
        }
    }

    #[cfg(test)]
    pub async fn set_nodes(&self, nodes: Vec<SnapshotNode>) {
        let mut state = self.inner.lock().await;
        state.nodes = nodes;
        state.snapshot = None;
    }

    #[cfg(test)]
    pub async fn set_hidden(&self, hidden: bool) {
        self.inner.lock().await.hidden = hidden;
    }

    #[cfg(test)]
    pub async fn set_evaluate_blocked(&self, blocked: bool) {
        self.inner.lock().await.evaluate_blocked = blocked;
    }

    #[cfg(test)]
    pub async fn last_js(&self) -> Option<String> {
        self.inner.lock().await.last_js.clone()
    }

    #[cfg(test)]
    pub async fn is_hidden(&self) -> bool {
        self.inner.lock().await.hidden
    }

    pub async fn current_origin(&self) -> String {
        self.inner.lock().await.origin.clone()
    }

    pub async fn navigate(&self, url: &str) -> Result<serde_json::Value, ControlError> {
        let mut state = self.inner.lock().await;
        state.url = url.to_string();
        if let Ok(parsed) = url::Url::parse(url) {
            state.origin = parsed.origin().ascii_serialization();
        }
        state.snapshot = None;
        let snap = build_snapshot(&state.origin, state.nodes.clone());
        let digest = snap.snapshot_digest.clone();
        state.snapshot = Some(snap);
        Ok(serde_json::json!({
            "url": state.url,
            "title": state.title,
            "snapshot_digest": digest,
        }))
    }

    pub async fn snapshot(&self, _filter: SnapshotFilter) -> Result<Snapshot, ControlError> {
        let mut state = self.inner.lock().await;
        let snap = build_snapshot(&state.origin, state.nodes.clone());
        state.snapshot = Some(snap.clone());
        Ok(snap)
    }

    pub async fn click(
        &self,
        r#ref: &str,
        digest: Option<&str>,
        abort: &std::sync::atomic::AtomicBool,
    ) -> Result<serde_json::Value, ControlError> {
        self.act(r#ref, digest, abort, |node| {
            if !node.actionable {
                return Err(ControlError::not_actionable(&node.r#ref));
            }
            Ok(())
        })
        .await
    }

    pub async fn r#type(
        &self,
        r#ref: &str,
        _text: &str,
        digest: Option<&str>,
        abort: &std::sync::atomic::AtomicBool,
    ) -> Result<serde_json::Value, ControlError> {
        self.click(r#ref, digest, abort).await
    }

    pub async fn scroll(
        &self,
        r#ref: Option<&str>,
        digest: Option<&str>,
        abort: &std::sync::atomic::AtomicBool,
    ) -> Result<serde_json::Value, ControlError> {
        if let Some(r) = r#ref {
            self.click(r, digest, abort).await
        } else {
            super::flight::aborted(abort)?;
            let state = self.inner.lock().await;
            let digest = state
                .snapshot
                .as_ref()
                .map(|s| s.snapshot_digest.clone())
                .unwrap_or_default();
            Ok(serde_json::json!({ "snapshot_digest": digest }))
        }
    }

    pub async fn evaluate(&self, js: &str) -> Result<serde_json::Value, ControlError> {
        let mut state = self.inner.lock().await;
        if state.evaluate_blocked {
            return Err(ControlError::origin_blocked(&state.origin));
        }
        state.last_js = Some(js.to_string());
        Ok(serde_json::json!({ "result": serde_json::Value::Null }))
    }

    pub async fn console(&self, since_ms: Option<u64>) -> Vec<ConsoleEntry> {
        let state = self.inner.lock().await;
        state
            .console
            .iter()
            .filter(|e| since_ms.is_none_or(|s| e.t_ms >= s))
            .cloned()
            .collect()
    }

    pub async fn screenshot(&self) -> Vec<u8> {
        // 1×1 PNG
        vec![
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207,
            192, 0, 0, 3, 1, 1, 0, 24, 221, 141, 176, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ]
    }

    #[cfg(test)]
    pub async fn push_console(&self, entry: ConsoleEntry) {
        self.inner.lock().await.console.push(entry);
    }

    async fn act(
        &self,
        r#ref: &str,
        digest: Option<&str>,
        abort: &std::sync::atomic::AtomicBool,
        check: impl Fn(&SnapshotNode) -> Result<(), ControlError>,
    ) -> Result<serde_json::Value, ControlError> {
        let started = Instant::now();
        loop {
            super::flight::aborted(abort)?;
            let mut state = self.inner.lock().await;
            if state.snapshot.is_none() {
                let snap = build_snapshot(&state.origin, state.nodes.clone());
                state.snapshot = Some(snap);
            }
            let snap = state.snapshot.as_ref().ok_or_else(|| {
                ControlError::instrument_unreachable("snapshot missing after mint")
            })?;
            require_digest(&snap.snapshot_digest, digest)?;
            if let Some(node) = find_node(&snap.nodes, r#ref) {
                if node.actionable {
                    check(node)?;
                    let digest = snap.snapshot_digest.clone();
                    return Ok(serde_json::json!({ "snapshot_digest": digest }));
                }
            }
            drop(state);
            if started.elapsed() >= ACTIONABILITY_WAIT {
                return Err(ControlError::not_actionable(r#ref));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

#[derive(Clone)]
pub struct FakeSim {
    inner: Arc<Mutex<FakeSimState>>,
}

struct FakeSimState {
    booted: bool,
    bridge: bool,
    nodes: Vec<SnapshotNode>,
    snapshot: Option<Snapshot>,
    taps: Vec<(f64, f64)>,
    logs: VecDeque<String>,
}

impl Default for FakeSim {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FakeSimState {
                booted: true,
                bridge: true,
                nodes: vec![SnapshotNode {
                    r#ref: String::new(),
                    role: "button".into(),
                    name: "Login".into(),
                    value: None,
                    actionable: true,
                    bounds: Some(Bounds {
                        x: 100.0,
                        y: 200.0,
                        w: 80.0,
                        h: 40.0,
                    }),
                    children: vec![],
                }],
                snapshot: None,
                taps: vec![],
                logs: VecDeque::new(),
            })),
        }
    }
}

impl FakeSim {
    #[cfg(test)]
    pub fn missing_bridge() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub async fn set_bridge(&self, available: bool) {
        self.inner.lock().await.bridge = available;
    }

    #[cfg(test)]
    pub async fn set_booted(&self, booted: bool) {
        self.inner.lock().await.booted = booted;
    }

    #[cfg(test)]
    pub async fn taps(&self) -> Vec<(f64, f64)> {
        self.inner.lock().await.taps.clone()
    }

    pub async fn snapshot(&self, _filter: SnapshotFilter) -> Result<Snapshot, ControlError> {
        let mut state = self.inner.lock().await;
        if !state.bridge {
            return Err(ControlError::bridge_missing(
                "brew install baguette\nbrew tap facebook/fb && brew install idb-companion",
            ));
        }
        let snap = build_snapshot("simulator://channel", state.nodes.clone());
        state.snapshot = Some(snap.clone());
        Ok(snap)
    }

    pub async fn tap(
        &self,
        r#ref: Option<&str>,
        point: Option<(f64, f64)>,
        digest: Option<&str>,
        abort: &std::sync::atomic::AtomicBool,
    ) -> Result<serde_json::Value, ControlError> {
        super::flight::aborted(abort)?;
        let mut state = self.inner.lock().await;
        if !state.bridge {
            return Err(ControlError::bridge_missing(
                "brew install baguette\nbrew tap facebook/fb && brew install idb-companion",
            ));
        }
        if !state.booted {
            return Err(ControlError::timeout("simulator boot timed out"));
        }
        if state.snapshot.is_none() {
            let snap = build_snapshot("simulator://channel", state.nodes.clone());
            state.snapshot = Some(snap);
        }
        let snap = state
            .snapshot
            .as_ref()
            .ok_or_else(|| ControlError::instrument_unreachable("snapshot missing after mint"))?;
        require_digest(&snap.snapshot_digest, digest)?;
        let (x, y) = if let Some(r) = r#ref {
            let node = find_node(&snap.nodes, r).ok_or_else(|| ControlError::not_actionable(r))?;
            let bounds = node
                .bounds
                .as_ref()
                .ok_or_else(|| ControlError::not_actionable(r))?;
            (bounds.x + bounds.w / 2.0, bounds.y + bounds.h / 2.0)
        } else {
            point.ok_or_else(|| ControlError::not_actionable("x,y"))?
        };
        let digest = snap.snapshot_digest.clone();
        state.taps.push((x, y));
        Ok(serde_json::json!({ "snapshot_digest": digest, "x": x, "y": y }))
    }

    pub async fn swipe(
        &self,
        abort: &std::sync::atomic::AtomicBool,
    ) -> Result<serde_json::Value, ControlError> {
        super::flight::aborted(abort)?;
        Ok(serde_json::json!({ "ok": true }))
    }

    pub async fn r#type(
        &self,
        _text: &str,
        abort: &std::sync::atomic::AtomicBool,
    ) -> Result<serde_json::Value, ControlError> {
        super::flight::aborted(abort)?;
        Ok(serde_json::json!({ "ok": true }))
    }

    pub async fn press(
        &self,
        _button: &str,
        abort: &std::sync::atomic::AtomicBool,
    ) -> Result<serde_json::Value, ControlError> {
        super::flight::aborted(abort)?;
        Ok(serde_json::json!({ "ok": true }))
    }

    pub async fn launch(
        &self,
        _bundle: Option<&str>,
        _install: Option<&str>,
    ) -> Result<serde_json::Value, ControlError> {
        Ok(serde_json::json!({ "ok": true }))
    }

    pub async fn screenshot(&self) -> Vec<u8> {
        FakeBrowser::default().screenshot().await
    }

    pub async fn record(
        &self,
        seconds: u32,
        abort: &std::sync::atomic::AtomicBool,
    ) -> Result<Vec<u8>, ControlError> {
        let secs = seconds.clamp(5, 60);
        let steps = (secs * 10).min(5);
        for _ in 0..steps {
            super::flight::aborted(abort)?;
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        Ok(self.screenshot().await)
    }

    pub async fn logs(&self, _since: Option<u64>, _predicate: Option<&str>) -> Vec<String> {
        self.inner.lock().await.logs.iter().cloned().collect()
    }
}

#[derive(Clone)]
pub struct FakeGovernor {
    inner: Arc<Mutex<FakeGovernorState>>,
}

struct FakeGovernorState {
    booted: u32,
    max_booted: u32,
    holders: Vec<String>,
    reachable: bool,
    subject_origin: String,
    cap_full: bool,
}

impl FakeGovernor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FakeGovernorState {
                booted: 0,
                max_booted: 2,
                holders: vec![],
                reachable: true,
                subject_origin: "http://127.0.0.1:5173".into(),
                cap_full: false,
            })),
        }
    }
}

impl Default for FakeGovernor {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeGovernor {
    #[cfg(test)]
    pub async fn set_reachable(&self, reachable: bool) {
        self.inner.lock().await.reachable = reachable;
    }

    #[cfg(test)]
    pub async fn set_cap_full(&self, holders: Vec<String>) {
        let mut state = self.inner.lock().await;
        state.cap_full = true;
        state.holders = holders;
        state.booted = state.max_booted;
    }

    #[cfg(test)]
    pub async fn set_subject_origin(&self, origin: &str) {
        self.inner.lock().await.subject_origin = origin.to_string();
    }

    pub async fn ensure_sim(&self, _channel_id: &str) -> Result<serde_json::Value, ControlError> {
        let state = self.inner.lock().await;
        if !state.reachable {
            return Err(ControlError::instrument_unreachable(
                "Desktop control endpoint is not reachable from this host",
            ));
        }
        if state.cap_full {
            return Err(ControlError::boot_capacity(&state.holders));
        }
        Ok(serde_json::json!({
            "booted": state.booted.max(1),
            "max": state.max_booted,
        }))
    }

    pub async fn ensure_browser(&self, _channel_id: &str) -> Result<String, ControlError> {
        let state = self.inner.lock().await;
        if !state.reachable {
            return Err(ControlError::instrument_unreachable(
                "Desktop control endpoint is not reachable from this host",
            ));
        }
        Ok(state.subject_origin.clone())
    }

    pub async fn status(&self, channel_id: &str) -> serde_json::Value {
        let state = self.inner.lock().await;
        serde_json::json!({
            "channel_id": channel_id,
            "browser_url": state.subject_origin,
            "sim_state": "booted",
            "dev_server_port": 5173,
            "governor": {
                "booted": format!("{}/{}", state.booted.max(1), state.max_booted),
            },
        })
    }
}
