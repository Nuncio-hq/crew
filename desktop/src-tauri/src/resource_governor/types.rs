//! Shared wire types for the Resource Governor.

use serde::{Deserialize, Serialize};

use super::policy::GovernorPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceLifecycle {
    Absent,
    Shutdown,
    Booted,
    Mirroring,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DevServerFace {
    Running,
    IdleStop,
    Crashed,
    PortConflict,
    Setup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatus {
    pub availability: String,
    pub binary: Option<String>,
    pub path: Option<String>,
    pub install_hint: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimHolding {
    pub channel_id: String,
    pub channel_name: Option<String>,
    pub device_name: String,
    pub udid: Option<String>,
    pub lifecycle: DeviceLifecycle,
    pub device_type: String,
    pub runtime: String,
    pub foreign: bool,
    pub disk_bytes: u64,
    pub last_used_ms: u64,
    pub idle_deadline_ms: Option<u64>,
    pub pane_visible: bool,
    pub mirroring: bool,
    pub last_screenshot_data_url: Option<String>,
    pub boot_elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevServerHolding {
    pub id: String,
    pub channel_id: String,
    pub subject: String,
    pub command: String,
    pub port: u16,
    pub url: Option<String>,
    pub face: DevServerFace,
    pub uptime_ms: u64,
    pub idle_deadline_ms: Option<u64>,
    pub last_log: Vec<String>,
    pub port_note: Option<String>,
    pub crash_count: u32,
    pub cwd: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebviewHolding {
    pub id: String,
    pub channel_id: String,
    pub url: String,
    pub hidden: bool,
    pub hidden_since_ms: Option<u64>,
    pub backend: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopKind {
    Sim,
    Server,
    Webview,
    Everything,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernorStatus {
    pub policy: GovernorPolicy,
    pub now_ms: u64,
    pub sims: Vec<SimHolding>,
    pub servers: Vec<DevServerHolding>,
    pub webviews: Vec<WebviewHolding>,
    pub booted_count: u32,
    pub stream_count: u32,
    pub server_count: u32,
    pub disk_bytes: u64,
    pub cap_conflict: Option<super::governor::CapConflict>,
    pub prune_candidates: Vec<SimHolding>,
    pub bridge: BridgeStatus,
    pub child_webview_available: bool,
}
