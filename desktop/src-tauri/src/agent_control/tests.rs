//! Contract tests for the agent-control endpoint (bridge/webview faked).

use std::time::Instant;

use super::origin::OriginDecision;
use super::protocol::{ControlRequest, ErrorCode, PROTOCOL_VERSION};
use super::runtime::ControlRuntime;
use super::server::{spawn_agent_control, AgentControlHandle};
use super::snapshot::{build_snapshot, SnapshotNode};
use super::token::bearer_matches;

fn req(method: &str, params: serde_json::Value) -> ControlRequest {
    ControlRequest {
        v: PROTOCOL_VERSION,
        id: Some(serde_json::json!(1)),
        method: method.into(),
        params,
        channel_id: "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50".into(),
        thread_root_id: Some("aa".repeat(32)),
        agent_name: Some("Hermes".into()),
        request_id: None,
    }
}

fn err_code(resp: &super::protocol::ControlResponse) -> String {
    resp.error
        .as_ref()
        .map(|e| e.code.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn unknown_version_is_instrument_unreachable() {
    let rt = ControlRuntime::new();
    let mut r = req("desktop_status", serde_json::json!({}));
    r.v = 99;
    let resp = rt.handle(r).await;
    assert_eq!(err_code(&resp), ErrorCode::InstrumentUnreachable.as_str());
}

#[tokio::test]
async fn desktop_status_reports_headroom_and_lease() {
    let rt = ControlRuntime::new();
    let resp = rt
        .handle(req("desktop_status", serde_json::json!({})))
        .await;
    assert!(resp.error.is_none());
    let result = resp.result.expect("result");
    assert!(result.get("governor").is_some());
}

#[tokio::test]
async fn snapshot_refs_stable_across_unchanged_tree() {
    let rt = ControlRuntime::new();
    let a = rt
        .handle(req("browser_snapshot", serde_json::json!({})))
        .await;
    let b = rt
        .handle(req("browser_snapshot", serde_json::json!({})))
        .await;
    let da = a.result.as_ref().unwrap()["snapshot_digest"]
        .as_str()
        .unwrap();
    let db = b.result.as_ref().unwrap()["snapshot_digest"]
        .as_str()
        .unwrap();
    assert_eq!(da, db);
    assert_eq!(
        a.result.as_ref().unwrap()["nodes"][0]["ref"],
        b.result.as_ref().unwrap()["nodes"][0]["ref"]
    );
}

#[tokio::test]
async fn mutating_the_tree_invalidates_digest() {
    let rt = ControlRuntime::new();
    let first = rt
        .handle(req("browser_snapshot", serde_json::json!({})))
        .await;
    let digest = first.result.as_ref().unwrap()["snapshot_digest"]
        .as_str()
        .unwrap()
        .to_string();
    let r#ref = first.result.as_ref().unwrap()["nodes"][0]["ref"]
        .as_str()
        .unwrap()
        .to_string();
    rt.browser
        .set_nodes(vec![
            SnapshotNode {
                r#ref: String::new(),
                role: "button".into(),
                name: "Save".into(),
                value: None,
                actionable: true,
                bounds: None,
                children: vec![],
            },
            SnapshotNode {
                r#ref: String::new(),
                role: "link".into(),
                name: "Docs".into(),
                value: None,
                actionable: true,
                bounds: None,
                children: vec![],
            },
        ])
        .await;
    let click = rt
        .handle(req(
            "browser_click",
            serde_json::json!({ "ref": r#ref, "snapshot_digest": digest }),
        ))
        .await;
    assert_eq!(err_code(&click), ErrorCode::StaleRef.as_str());
}

#[tokio::test]
async fn browser_click_succeeds_while_pane_closed() {
    let rt = ControlRuntime::new();
    rt.browser.set_hidden(true).await;
    let snap = rt
        .handle(req("browser_snapshot", serde_json::json!({})))
        .await;
    let digest = snap.result.as_ref().unwrap()["snapshot_digest"].clone();
    let r#ref = snap.result.as_ref().unwrap()["nodes"][0]["ref"].clone();
    let click = rt
        .handle(req(
            "browser_click",
            serde_json::json!({ "ref": r#ref, "snapshot_digest": digest }),
        ))
        .await;
    assert!(click.error.is_none(), "{click:?}");
    assert!(rt.browser.is_hidden().await);
}

#[tokio::test]
async fn sim_snapshot_refs_match_browser_format() {
    let rt = ControlRuntime::new();
    let browser = rt
        .handle(req("browser_snapshot", serde_json::json!({})))
        .await;
    let sim = rt.handle(req("sim_snapshot", serde_json::json!({}))).await;
    assert!(browser.result.as_ref().unwrap()["source"]
        .as_str()
        .unwrap()
        .starts_with("[content from "));
    assert!(sim.result.as_ref().unwrap()["source"]
        .as_str()
        .unwrap()
        .starts_with("[content from "));
    assert!(sim.result.as_ref().unwrap()["nodes"][0]["ref"]
        .as_str()
        .unwrap()
        .starts_with('e'));
}

#[tokio::test]
async fn sim_snapshot_stable_across_unchanged_tree() {
    let rt = ControlRuntime::new();
    let a = rt.handle(req("sim_snapshot", serde_json::json!({}))).await;
    let b = rt.handle(req("sim_snapshot", serde_json::json!({}))).await;
    assert_eq!(
        a.result.as_ref().unwrap()["snapshot_digest"],
        b.result.as_ref().unwrap()["snapshot_digest"]
    );
}

#[tokio::test]
async fn lease_preempt_mid_action_returns_lease_held() {
    let rt = std::sync::Arc::new(ControlRuntime::new());
    let snap = rt
        .handle(req("browser_snapshot", serde_json::json!({})))
        .await;
    let digest = snap.result.as_ref().unwrap()["snapshot_digest"].clone();
    let r#ref = snap.result.as_ref().unwrap()["nodes"][0]["ref"].clone();
    let rt2 = rt.clone();
    let click = tokio::spawn(async move {
        rt2.handle(req(
            "browser_click",
            serde_json::json!({ "ref": r#ref, "snapshot_digest": digest }),
        ))
        .await
    });
    rt.leases.lock().await.preempt_human(
        "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
        super::protocol::Instrument::Browser,
        1,
    );
    let _ = click.await;
    let second = rt
        .handle(req(
            "browser_type",
            serde_json::json!({ "ref": "e1", "text": "x", "snapshot_digest": "nope" }),
        ))
        .await;
    assert_eq!(err_code(&second), ErrorCode::LeaseHeld.as_str());
}

#[tokio::test]
async fn lease_release_on_turn_end() {
    let rt = ControlRuntime::new();
    let snap = rt
        .handle(req("browser_snapshot", serde_json::json!({})))
        .await;
    let digest = snap.result.as_ref().unwrap()["snapshot_digest"].clone();
    let r#ref = snap.result.as_ref().unwrap()["nodes"][0]["ref"].clone();
    let _ = rt
        .handle(req(
            "browser_click",
            serde_json::json!({ "ref": r#ref, "snapshot_digest": digest }),
        ))
        .await;
    assert!(!rt.lease_views().await.is_empty());
    let _ = rt.handle(req("lease.release", serde_json::json!({}))).await;
    assert!(rt.lease_views().await.is_empty());
}

#[tokio::test]
async fn boot_capacity_at_cap() {
    let rt = ControlRuntime::new();
    rt.governor
        .set_cap_full(vec!["engineering".into(), "random".into()])
        .await;
    let resp = rt
        .handle(req("sim_tap", serde_json::json!({ "x": 1.0, "y": 1.0 })))
        .await;
    assert_eq!(err_code(&resp), ErrorCode::BootCapacity.as_str());
}

#[tokio::test]
async fn bridge_missing_is_structured() {
    let rt = ControlRuntime::new();
    rt.sim.set_bridge(false).await;
    let _standalone = super::instruments::FakeSim::missing_bridge();
    let resp = rt.handle(req("sim_snapshot", serde_json::json!({}))).await;
    assert_eq!(err_code(&resp), ErrorCode::BridgeMissing.as_str());
    assert!(resp
        .error
        .as_ref()
        .unwrap()
        .data
        .as_ref()
        .unwrap()
        .get("install_hint")
        .is_some());
}

#[tokio::test]
async fn request_id_dedupes() {
    let rt = ControlRuntime::new();
    let snap = rt
        .handle(req("browser_snapshot", serde_json::json!({})))
        .await;
    let digest = snap.result.as_ref().unwrap()["snapshot_digest"].clone();
    let r#ref = snap.result.as_ref().unwrap()["nodes"][0]["ref"].clone();
    let mut first = req(
        "browser_click",
        serde_json::json!({ "ref": r#ref, "snapshot_digest": digest }),
    );
    first.request_id = Some("rid-1".into());
    let a = rt.handle(first.clone()).await;
    let b = rt.handle(first).await;
    assert!(a.error.is_none());
    assert_eq!(a.result, b.result);
}

#[tokio::test]
async fn per_instrument_serial_vs_cross_parallel() {
    let rt = std::sync::Arc::new(ControlRuntime::new());
    let start = Instant::now();
    let a = {
        let rt = rt.clone();
        tokio::spawn(async move {
            rt.handle(req("browser_snapshot", serde_json::json!({})))
                .await
        })
    };
    let b = {
        let rt = rt.clone();
        tokio::spawn(async move { rt.handle(req("sim_snapshot", serde_json::json!({}))).await })
    };
    let (ra, rb) = tokio::join!(a, b);
    assert!(ra.unwrap().error.is_none());
    assert!(rb.unwrap().error.is_none());
    assert!(start.elapsed().as_millis() < 300);
}

#[tokio::test]
async fn sim_tap_warm_path_under_300ms() {
    let handle = AgentControlHandle::new();
    handle.runtime.governor.ensure_sim("ch").await.unwrap();
    let port = spawn_agent_control(handle.clone()).await;
    assert!(port > 0);
    let url = handle.listen_url().await.expect("url");
    let snap = handle
        .runtime
        .handle(req("sim_snapshot", serde_json::json!({})))
        .await;
    let digest = snap.result.as_ref().unwrap()["snapshot_digest"].clone();
    let r#ref = snap.result.as_ref().unwrap()["nodes"][0]["ref"].clone();
    let started = Instant::now();
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "v": 1,
        "method": "sim_tap",
        "channel_id": "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
        "agent_name": "Hermes",
        "params": { "ref": r#ref, "snapshot_digest": digest },
    });
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", handle.token))
        .json(&body)
        .send()
        .await
        .expect("http");
    assert!(resp.status().is_success());
    assert!(
        started.elapsed().as_millis() < 300,
        "elapsed {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn bad_token_rejected() {
    let handle = AgentControlHandle::new();
    let port = spawn_agent_control(handle.clone()).await;
    let url = format!("http://127.0.0.1:{port}/agent-control");
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", "Bearer nope")
        .json(&serde_json::json!({
            "v": 1,
            "method": "desktop_status",
            "channel_id": "ch",
            "params": {}
        }))
        .send()
        .await
        .expect("http");
    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn origin_header_rejected() {
    let handle = AgentControlHandle::new();
    let port = spawn_agent_control(handle.clone()).await;
    let url = format!("http://127.0.0.1:{port}/agent-control");
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", handle.token))
        .header("Origin", "https://evil.example")
        .json(&serde_json::json!({
            "v": 1,
            "method": "desktop_status",
            "channel_id": "ch",
            "params": {}
        }))
        .send()
        .await
        .expect("http");
    assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn bridge_reply_accepts_page_origin_with_nonce() {
    let handle = AgentControlHandle::new();
    let port = spawn_agent_control(handle.clone()).await;
    let url = format!("http://127.0.0.1:{port}/agent-control/bridge-reply");
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle
        .runtime
        .bridge_waiters
        .lock()
        .await
        .insert("rid".into(), tx);
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Origin", "http://127.0.0.1:5173")
        .header("content-type", "text/plain")
        .body(
            serde_json::json!({
                "nonce": handle.runtime.bridge_nonce,
                "id": "rid",
                "payload": { "ok": true }
            })
            .to_string(),
        )
        .send()
        .await
        .expect("http");
    assert_eq!(resp.status(), reqwest::StatusCode::NO_CONTENT);
    let payload = rx.await.expect("payload");
    assert_eq!(payload["ok"], true);
}

#[tokio::test]
async fn navigate_to_about_blank_never_needs_origin_approval() {
    // #247: `about:blank` parses to the opaque origin `null`, which never
    // matches the subject or an allowlist, and used to fall into
    // `wait_origin_decision` (up to 300s) with no elicitation answer set —
    // exactly the reported A2 hang. It must resolve immediately.
    let rt = ControlRuntime::new();
    let resp = rt
        .handle(req(
            "browser_navigate",
            serde_json::json!({ "url": "about:blank" }),
        ))
        .await;
    assert!(resp.error.is_none(), "{resp:?}");
}

#[tokio::test]
async fn desktop_status_surfaces_pending_origin_wait() {
    // #247: a stuck `browser_navigate` waiting on owner elicitation must be
    // observable via `desktop_status` instead of looking identical to a
    // genuine ensure/bridge hang (spike 0057 noted `desktop_status` "still
    // answered" with no way to tell the two apart).
    let rt = std::sync::Arc::new(ControlRuntime::new());
    let idle = rt
        .handle(req("desktop_status", serde_json::json!({})))
        .await;
    assert_eq!(
        idle.result.as_ref().unwrap()["pending_origin"],
        serde_json::Value::Null
    );

    // No live host is attached in this contract test, so a blocked-origin
    // navigate resolves immediately without an elicitation card; assert the
    // pending-origin bookkeeping the live path relies on directly instead.
    *rt.pending_origin.lock().await = Some(serde_json::json!({
        "channelId": "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
        "origin": "https://example.com",
        "agentName": "Hermes",
    }));
    let waiting = rt
        .handle(req("desktop_status", serde_json::json!({})))
        .await;
    assert_eq!(
        waiting.result.as_ref().unwrap()["pending_origin"]["origin"],
        "https://example.com"
    );
}

#[tokio::test]
async fn foreign_origin_deny_is_origin_blocked() {
    let rt = ControlRuntime::new();
    rt.set_elicitation(OriginDecision::Deny).await;
    let resp = rt
        .handle(req(
            "browser_navigate",
            serde_json::json!({ "url": "https://api.stripe.com/docs" }),
        ))
        .await;
    assert_eq!(err_code(&resp), ErrorCode::OriginBlocked.as_str());
}

#[tokio::test]
async fn allow_domain_writes_canvas() {
    let rt = ControlRuntime::new();
    rt.set_elicitation(OriginDecision::AllowDomain).await;
    let resp = rt
        .handle(req(
            "browser_navigate",
            serde_json::json!({ "url": "https://api.stripe.com/docs" }),
        ))
        .await;
    assert!(resp.error.is_none(), "{resp:?}");
    let writes = rt.canvas_writes.lock().await.clone();
    assert!(writes.iter().any(|(_, origin)| origin.contains("stripe")));
}

#[tokio::test]
async fn browser_navigate_succeeds_without_canvas_dev_server_tooling() {
    // #236: the browser instrument exists purely off the Resource Governor /
    // fake browser subject. `ControlRuntime` has no `tooling.devServer`
    // concept at all, so a bare `browser_navigate` (no url override) must
    // still succeed — agent parity with the UI's setup-wall removal.
    let rt = ControlRuntime::new();
    let resp = rt
        .handle(req("browser_navigate", serde_json::json!({})))
        .await;
    assert!(resp.error.is_none(), "{resp:?}");
}

#[tokio::test]
async fn post_evidence_records_tag() {
    let rt = ControlRuntime::new();
    let resp = rt
        .handle(req(
            "browser_screenshot",
            serde_json::json!({ "post_evidence": true }),
        ))
        .await;
    assert!(resp.error.is_none());
    let ev = rt.evidence.lock().await.clone();
    assert_eq!(ev[0]["tag"], "before-after-visual");
}

#[tokio::test]
async fn overlay_emitted_on_tap() {
    let rt = ControlRuntime::new();
    let snap = rt.handle(req("sim_snapshot", serde_json::json!({}))).await;
    let digest = snap.result.as_ref().unwrap()["snapshot_digest"].clone();
    let r#ref = snap.result.as_ref().unwrap()["nodes"][0]["ref"].clone();
    let _ = rt
        .handle(req(
            "sim_tap",
            serde_json::json!({ "ref": r#ref, "snapshot_digest": digest }),
        ))
        .await;
    let frames = rt.overlay.lock().await.frames.clone();
    assert_eq!(frames.last().unwrap().tool, "sim_tap");
    assert_eq!(
        frames.last().unwrap().kind,
        super::overlay::KIND_AGENT_INSTRUMENT_OVERLAY
    );
    assert!(rt
        .overlay
        .lock()
        .await
        .latest_for("9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50")
        .is_some());
}

#[test]
fn bearer_and_digest_helpers() {
    assert!(bearer_matches(Some("Bearer abc"), "abc"));
    assert!(!bearer_matches(Some("Bearer abc"), "nope"));
    let snap = build_snapshot(
        "http://x",
        vec![SnapshotNode {
            r#ref: String::new(),
            role: "button".into(),
            name: "A".into(),
            value: None,
            actionable: true,
            bounds: None,
            children: vec![],
        }],
    );
    assert!(snap.source.contains("http://x"));
}

#[tokio::test]
async fn host_unreachable_when_governor_says_so() {
    let rt = ControlRuntime::new();
    rt.governor.set_reachable(false).await;
    let resp = rt
        .handle(req("browser_snapshot", serde_json::json!({})))
        .await;
    assert_eq!(err_code(&resp), ErrorCode::InstrumentUnreachable.as_str());
}

#[tokio::test]
async fn full_evaluate_inside_subject_origin() {
    let rt = ControlRuntime::new();
    let resp = rt
        .handle(req("browser_evaluate", serde_json::json!({ "js": "1+1" })))
        .await;
    assert!(resp.error.is_none(), "{resp:?}");
    assert_eq!(rt.browser.last_js().await.as_deref(), Some("1+1"));
}

#[tokio::test]
async fn helpers_cover_blocked_evaluate_console_and_sim_taps() {
    let rt = ControlRuntime::new();
    rt.browser.set_evaluate_blocked(true).await;
    rt.browser
        .push_console(super::instruments::ConsoleEntry {
            t_ms: 1,
            kind: "log".into(),
            text: "hi".into(),
            method: None,
            url: None,
            status: None,
            duration_ms: None,
            size: None,
        })
        .await;
    let console = rt
        .handle(req("browser_console", serde_json::json!({})))
        .await;
    assert!(console.result.as_ref().unwrap()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["text"] == "hi"));
    let blocked = rt
        .handle(req("browser_evaluate", serde_json::json!({ "js": "1+1" })))
        .await;
    assert_eq!(err_code(&blocked), ErrorCode::OriginBlocked.as_str());

    rt.governor
        .set_subject_origin("http://127.0.0.1:5173")
        .await;
    rt.sim.set_booted(true).await;
    let _ = rt
        .handle(req("sim_tap", serde_json::json!({ "x": 10.0, "y": 20.0 })))
        .await;
    let taps = rt.sim.taps().await;
    assert!(!taps.is_empty());
    let lease = rt.leases.lock().await;
    let _ = lease.get(
        "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
        super::protocol::Instrument::Sim,
    );
}

#[tokio::test]
async fn human_held_auto_releases_after_ten_seconds() {
    let rt = ControlRuntime::new();
    rt.set_now(1_000).await;
    let _ = rt
        .handle(req("browser_click", serde_json::json!({ "ref": "e1" })))
        .await;
    rt.leases.lock().await.preempt_human(
        "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
        super::protocol::Instrument::Browser,
        1_000,
    );
    assert!(!rt.lease_views().await.is_empty());
    rt.set_now(1_000 + super::lease::HUMAN_RELEASE_MS).await;
    rt.tick().await;
    assert_eq!(
        rt.leases.lock().await.get(
            "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
            super::protocol::Instrument::Browser,
        ),
        super::lease::LeaseState::Free
    );
    assert!(rt.lease_views().await.is_empty());
}
