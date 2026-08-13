//! JSON-RPC-shaped agent control protocol (`v: 1`).

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

pub const ENV_CONTROL_URL: &str = "BUZZ_DESKTOP_CONTROL_URL";
pub const ENV_CONTROL_TOKEN: &str = "BUZZ_DESKTOP_CONTROL_TOKEN";
#[allow(dead_code)]
pub const ENV_CHANNEL_ID: &str = "BUZZ_GIT_ORIGIN_CHANNEL_ID";
#[allow(dead_code)]
pub const ENV_THREAD_ROOT_ID: &str = "BUZZ_GIT_ORIGIN_THREAD_ROOT_ID";
#[allow(dead_code)]
pub const ENV_AGENT_NAME: &str = "BUZZ_ACP_DISPLAY_NAME";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Instrument {
    Browser,
    Sim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotFilter {
    #[default]
    Interactive,
    All,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ControlRequest {
    pub v: u32,
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
    pub channel_id: String,
    #[serde(default)]
    pub thread_root_id: Option<String>,
    #[serde(default)]
    pub agent_name: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControlResponse {
    pub v: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ControlErrorBody>,
}

impl ControlResponse {
    pub fn ok(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Option<serde_json::Value>, error: ControlErrorBody) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InstrumentUnreachable,
    BridgeMissing,
    BootCapacity,
    StaleRef,
    NotActionable,
    OriginBlocked,
    LeaseHeld,
    Timeout,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InstrumentUnreachable => "instrument_unreachable",
            Self::BridgeMissing => "bridge_missing",
            Self::BootCapacity => "boot_capacity",
            Self::StaleRef => "stale_ref",
            Self::NotActionable => "not_actionable",
            Self::OriginBlocked => "origin_blocked",
            Self::LeaseHeld => "lease_held",
            Self::Timeout => "timeout",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct ControlError {
    pub code: ErrorCode,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl ControlError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn into_body(self) -> ControlErrorBody {
        ControlErrorBody {
            code: self.code.as_str().to_string(),
            message: self.message,
            data: self.data,
        }
    }

    pub fn instrument_unreachable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InstrumentUnreachable, message)
    }

    pub fn bridge_missing(hint: impl Into<String>) -> Self {
        let hint = hint.into();
        Self::new(
            ErrorCode::BridgeMissing,
            "Simulator bridge is not installed",
        )
        .with_data(serde_json::json!({
            "install_hint": hint,
            "footnote": "The agent is waiting on this too.",
        }))
    }

    pub fn boot_capacity(holders: &[String]) -> Self {
        Self::new(ErrorCode::BootCapacity, "Simulator boot cap is full")
            .with_data(serde_json::json!({ "holders": holders }))
    }

    pub fn stale_ref(digest: &str) -> Self {
        Self::new(
            ErrorCode::StaleRef,
            "Ref is from an older snapshot; call snapshot again",
        )
        .with_data(serde_json::json!({
            "hint": "re-snapshot",
            "snapshot_digest": digest,
        }))
    }

    pub fn not_actionable(r#ref: &str) -> Self {
        Self::new(
            ErrorCode::NotActionable,
            format!(
                "Element {name} was not visible, enabled, and stable within 5s",
                name = r#ref
            ),
        )
    }

    pub fn origin_blocked(origin: &str) -> Self {
        Self::new(
            ErrorCode::OriginBlocked,
            format!("Origin {origin} is outside the subject origin and canvas allowlist"),
        )
        .with_data(serde_json::json!({
            "origin": origin,
            "approval": "Owner elicitation: Allow once / Allow domain (writes canvas tooling.browserAllowlist) / Deny",
        }))
    }

    pub fn lease_held(holder: &str) -> Self {
        Self::new(
            ErrorCode::LeaseHeld,
            format!("Input lease is held by {holder}"),
        )
        .with_data(serde_json::json!({ "holder": holder }))
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Timeout, message)
    }
}

impl std::fmt::Display for ControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ControlError {}

pub fn method_instrument(method: &str) -> Option<Instrument> {
    if method.starts_with("browser_") {
        Some(Instrument::Browser)
    } else if method.starts_with("sim_") {
        Some(Instrument::Sim)
    } else {
        None
    }
}

pub fn method_is_input(method: &str) -> bool {
    matches!(
        method,
        "browser_click"
            | "browser_type"
            | "browser_scroll"
            | "browser_navigate"
            | "browser_evaluate"
            | "sim_tap"
            | "sim_swipe"
            | "sim_type"
            | "sim_press"
            | "sim_launch"
            | "sim_record"
    )
}

pub fn method_is_mutating(method: &str) -> bool {
    method_is_input(method)
        || matches!(
            method,
            "browser_screenshot" | "sim_screenshot" | "lease.release" | "lease.take_over"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_names_and_mutating_set_are_stable() {
        assert_eq!(ENV_CONTROL_URL, "BUZZ_DESKTOP_CONTROL_URL");
        assert_eq!(ENV_CONTROL_TOKEN, "BUZZ_DESKTOP_CONTROL_TOKEN");
        assert_eq!(ENV_CHANNEL_ID, "BUZZ_GIT_ORIGIN_CHANNEL_ID");
        assert_eq!(ENV_THREAD_ROOT_ID, "BUZZ_GIT_ORIGIN_THREAD_ROOT_ID");
        assert_eq!(ENV_AGENT_NAME, "BUZZ_ACP_DISPLAY_NAME");
        assert!(method_is_mutating("browser_click"));
        assert!(method_is_mutating("sim_screenshot"));
        assert!(!method_is_mutating("desktop_status"));
        assert!(!method_is_mutating("browser_snapshot"));
    }
}
