//! Policy table from issue #196. Defaults are the contract; marked rows are
//! user-tunable in Settings → Devices & Preview.

use serde::{Deserialize, Serialize};

/// Milliseconds in one minute.
pub const MINUTE_MS: u64 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernorPolicy {
    /// Concurrent Crew-booted simulators.
    pub max_booted_sims: u32,
    /// Concurrent MJPEG mirror streams.
    pub max_mirror_streams: u32,
    /// Adaptive target fps while the pane is visible.
    pub mirror_fps: u32,
    /// Quiet-pane floor fps.
    pub mirror_quiet_fps: u32,
    /// Booted sim idle → shutdown (ms). Visible countdown first.
    pub sim_idle_shutdown_ms: u64,
    /// Stream pause after the pane is hidden (ms). Fixed, not user-tunable.
    pub stream_pause_hidden_ms: u64,
    /// Hidden webviews kept alive (LRU).
    pub hidden_webview_cap: u32,
    /// Destroy a hidden webview after this (ms); URL + nav restored.
    pub hidden_webview_ttl_ms: u64,
    /// Concurrent Crew-owned dev servers.
    pub max_dev_servers: u32,
    /// Dev server idle → stop (ms).
    pub dev_server_idle_ms: u64,
    /// Unused device prune prompt after this (ms).
    pub prune_unused_ms: u64,
}

impl GovernorPolicy {
    pub fn with_defaults() -> Self {
        Self {
            max_booted_sims: 2,
            max_mirror_streams: 1,
            mirror_fps: 20,
            mirror_quiet_fps: 5,
            sim_idle_shutdown_ms: 15 * MINUTE_MS,
            stream_pause_hidden_ms: 2_000,
            hidden_webview_cap: 2,
            hidden_webview_ttl_ms: 10 * MINUTE_MS,
            max_dev_servers: 3,
            dev_server_idle_ms: 25 * MINUTE_MS,
            prune_unused_ms: 30 * 24 * 60 * MINUTE_MS,
        }
    }
}

impl Default for GovernorPolicy {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_issue_table() {
        let p = GovernorPolicy::with_defaults();
        assert_eq!(p.max_booted_sims, 2);
        assert_eq!(p.max_mirror_streams, 1);
        assert_eq!(p.mirror_fps, 20);
        assert_eq!(p.sim_idle_shutdown_ms, 15 * MINUTE_MS);
        assert_eq!(p.stream_pause_hidden_ms, 2_000);
        assert_eq!(p.hidden_webview_cap, 2);
        assert_eq!(p.max_dev_servers, 3);
        assert_eq!(p.dev_server_idle_ms, 25 * MINUTE_MS);
    }
}
