//! `browser_*` / `sim_*` / `desktop_status` tools (#197).
//! POST the desktop control endpoint; missing URL → `instrument_unreachable`.

use rmcp::{
    model::{CallToolResult, Content},
    ErrorData,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

const URL_ENV: &str = "BUZZ_DESKTOP_CONTROL_URL";
const TOKEN_ENV: &str = "BUZZ_DESKTOP_CONTROL_TOKEN";
const CHANNEL_ENV: &str = "BUZZ_GIT_ORIGIN_CHANNEL_ID";
const THREAD_ENV: &str = "BUZZ_GIT_ORIGIN_THREAD_ROOT_ID";
const AGENT_ENV: &str = "BUZZ_ACP_DISPLAY_NAME";

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct EmptyParams {}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct NavigateParams {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct SnapshotParams {
    #[serde(default)]
    pub filter: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RefParams {
    pub r#ref: String,
    #[serde(default)]
    pub snapshot_digest: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TypeParams {
    pub r#ref: String,
    pub text: String,
    #[serde(default)]
    pub submit: Option<bool>,
    #[serde(default)]
    pub snapshot_digest: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScrollParams {
    #[serde(default)]
    pub r#ref: Option<String>,
    pub direction: String,
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub snapshot_digest: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EvaluateParams {
    pub js: String,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ConsoleParams {
    #[serde(default)]
    pub since: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct ScreenshotParams {
    #[serde(default)]
    pub post_evidence: Option<bool>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct SimTapParams {
    #[serde(default)]
    pub r#ref: Option<String>,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default)]
    pub snapshot_digest: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SimSwipeParams {
    pub from: Vec<f64>,
    pub to: Vec<f64>,
    #[serde(default)]
    pub ms: Option<u64>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SimTypeParams {
    pub text: String,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SimPressParams {
    pub button: String,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct SimLaunchParams {
    #[serde(default)]
    pub bundle_id: Option<String>,
    #[serde(default)]
    pub install_path: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SimRecordParams {
    pub seconds: u32,
    #[serde(default)]
    pub post_evidence: Option<bool>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Default)]
pub struct SimLogsParams {
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(default)]
    pub predicate: Option<String>,
}

fn unreachable(msg: &str) -> CallToolResult {
    CallToolResult::error(vec![Content::text(
        json!({
            "code": "instrument_unreachable",
            "message": msg,
            "data": { "hint": "These tools only work on the founder's desktop. Remote agents cannot tunnel HID/JS." }
        })
        .to_string(),
    )])
}

fn map_response(value: Value) -> CallToolResult {
    if let Some(error) = value.get("error") {
        return CallToolResult::error(vec![Content::text(error.to_string())]);
    }
    let result = value.get("result").cloned().unwrap_or(value);
    if let Some(b64) = result.get("png_base64").and_then(|v| v.as_str()) {
        let mime = result
            .get("mime")
            .and_then(|v| v.as_str())
            .unwrap_or("image/png");
        return CallToolResult::success(vec![
            Content::text(result.to_string()),
            Content::image(b64.to_string(), mime.to_string()),
        ]);
    }
    CallToolResult::success(vec![Content::text(result.to_string())])
}

async fn call(method: &str, params: Value, request_id: Option<String>) -> CallToolResult {
    let Ok(url) = std::env::var(URL_ENV) else {
        return unreachable("BUZZ_DESKTOP_CONTROL_URL is not set (host-bound tools)");
    };
    if url.is_empty() {
        return unreachable("BUZZ_DESKTOP_CONTROL_URL is empty");
    }
    let token = std::env::var(TOKEN_ENV).unwrap_or_default();
    if token.is_empty() {
        return unreachable("BUZZ_DESKTOP_CONTROL_TOKEN is not set (desktop restarted?)");
    }
    let channel_id = std::env::var(CHANNEL_ENV).unwrap_or_default();
    if channel_id.is_empty() {
        return unreachable("session has no channel id (BUZZ_GIT_ORIGIN_CHANNEL_ID)");
    }
    let thread_root_id = std::env::var(THREAD_ENV).ok().filter(|s| !s.is_empty());
    let agent_name = std::env::var(AGENT_ENV).ok().filter(|s| !s.is_empty());
    let body = json!({
        "v": 1,
        "method": method,
        "params": params,
        "channel_id": channel_id,
        "thread_root_id": thread_root_id,
        "agent_name": agent_name,
        "request_id": request_id,
    });
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(75))
        .build()
    {
        Ok(c) => c,
        Err(e) => return unreachable(&format!("http client: {e}")),
    };
    match client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => match resp.json::<Value>().await {
            Ok(value) => map_response(value),
            Err(e) => unreachable(&format!("invalid control response: {e}")),
        },
        Err(e) => unreachable(&format!("desktop control request failed: {e}")),
    }
}

pub async fn desktop_status(_p: EmptyParams) -> Result<CallToolResult, ErrorData> {
    Ok(call("desktop_status", json!({}), None).await)
}
pub async fn browser_navigate(p: NavigateParams) -> Result<CallToolResult, ErrorData> {
    Ok(call("browser_navigate", json!({ "url": p.url }), p.request_id).await)
}
pub async fn browser_snapshot(p: SnapshotParams) -> Result<CallToolResult, ErrorData> {
    Ok(call("browser_snapshot", json!({ "filter": p.filter }), None).await)
}
pub async fn browser_click(p: RefParams) -> Result<CallToolResult, ErrorData> {
    Ok(call(
        "browser_click",
        json!({ "ref": p.r#ref, "snapshot_digest": p.snapshot_digest }),
        p.request_id,
    )
    .await)
}
pub async fn browser_type(p: TypeParams) -> Result<CallToolResult, ErrorData> {
    Ok(call(
        "browser_type",
        json!({ "ref": p.r#ref, "text": p.text, "submit": p.submit, "snapshot_digest": p.snapshot_digest }),
        p.request_id,
    )
    .await)
}
pub async fn browser_scroll(p: ScrollParams) -> Result<CallToolResult, ErrorData> {
    Ok(call(
        "browser_scroll",
        json!({ "ref": p.r#ref, "direction": p.direction, "amount": p.amount, "snapshot_digest": p.snapshot_digest }),
        p.request_id,
    )
    .await)
}
pub async fn browser_evaluate(p: EvaluateParams) -> Result<CallToolResult, ErrorData> {
    Ok(call("browser_evaluate", json!({ "js": p.js }), None).await)
}
pub async fn browser_console(p: ConsoleParams) -> Result<CallToolResult, ErrorData> {
    Ok(call("browser_console", json!({ "since": p.since }), None).await)
}
pub async fn browser_screenshot(p: ScreenshotParams) -> Result<CallToolResult, ErrorData> {
    Ok(call(
        "browser_screenshot",
        json!({ "post_evidence": p.post_evidence }),
        p.request_id,
    )
    .await)
}
pub async fn sim_snapshot(p: SnapshotParams) -> Result<CallToolResult, ErrorData> {
    Ok(call("sim_snapshot", json!({ "filter": p.filter }), None).await)
}
pub async fn sim_tap(p: SimTapParams) -> Result<CallToolResult, ErrorData> {
    Ok(call(
        "sim_tap",
        json!({ "ref": p.r#ref, "x": p.x, "y": p.y, "snapshot_digest": p.snapshot_digest }),
        p.request_id,
    )
    .await)
}
pub async fn sim_swipe(p: SimSwipeParams) -> Result<CallToolResult, ErrorData> {
    Ok(call(
        "sim_swipe",
        json!({ "from": p.from, "to": p.to, "ms": p.ms }),
        p.request_id,
    )
    .await)
}
pub async fn sim_type(p: SimTypeParams) -> Result<CallToolResult, ErrorData> {
    Ok(call("sim_type", json!({ "text": p.text }), p.request_id).await)
}
pub async fn sim_press(p: SimPressParams) -> Result<CallToolResult, ErrorData> {
    Ok(call("sim_press", json!({ "button": p.button }), p.request_id).await)
}
pub async fn sim_launch(p: SimLaunchParams) -> Result<CallToolResult, ErrorData> {
    Ok(call(
        "sim_launch",
        json!({ "bundle_id": p.bundle_id, "install_path": p.install_path }),
        p.request_id,
    )
    .await)
}
pub async fn sim_screenshot(p: ScreenshotParams) -> Result<CallToolResult, ErrorData> {
    Ok(call(
        "sim_screenshot",
        json!({ "post_evidence": p.post_evidence }),
        p.request_id,
    )
    .await)
}
pub async fn sim_record(p: SimRecordParams) -> Result<CallToolResult, ErrorData> {
    Ok(call(
        "sim_record",
        json!({ "seconds": p.seconds, "post_evidence": p.post_evidence }),
        p.request_id,
    )
    .await)
}
pub async fn sim_logs(p: SimLogsParams) -> Result<CallToolResult, ErrorData> {
    Ok(call(
        "sim_logs",
        json!({ "since": p.since, "predicate": p.predicate }),
        None,
    )
    .await)
}

/// Count stays 14 in the issue table: screenshot groups with browser;
/// tap/swipe/type/press share the sim HID row. 18 `#[tool]` methods.
#[cfg(test)]
pub fn tool_names() -> &'static [&'static str] {
    &[
        "desktop_status",
        "browser_navigate",
        "browser_snapshot",
        "browser_click",
        "browser_type",
        "browser_scroll",
        "browser_evaluate",
        "browser_console",
        "browser_screenshot",
        "sim_snapshot",
        "sim_tap",
        "sim_swipe",
        "sim_type",
        "sim_press",
        "sim_launch",
        "sim_screenshot",
        "sim_record",
        "sim_logs",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_url_is_instrument_unreachable() {
        let result = desktop_status(EmptyParams {}).await.unwrap();
        assert_eq!(result.is_error, Some(true));
        let blob = format!("{result:?}");
        assert!(blob.contains("instrument_unreachable"), "{blob}");
    }

    #[test]
    fn image_mapper_uses_content_image() {
        use base64::Engine;
        let mapped = map_response(json!({
            "result": { "png_base64": Engine::encode(&base64::engine::general_purpose::STANDARD, b"hi"), "mime": "image/png" }
        }));
        assert_eq!(mapped.content.len(), 2);
    }

    #[test]
    fn advertised_names_cover_status_browser_and_sim() {
        let names = tool_names();
        assert!(names.contains(&"desktop_status"));
        assert!(names.contains(&"browser_evaluate"));
        assert!(names.contains(&"sim_record"));
        assert_eq!(names.len(), 18);
    }
}
