//! Resource Governor — single owner of simulator, mirror, webview, and
//! Crew-owned dev-server lifecycle (issue #196 / D-058).
//!
//! UI and agents request; this module decides. Caps, idle timers, LRU, and
//! the `crew-` device identity live here. Machine UDIDs never go on the relay.

mod bridge;
mod browser;
mod clock;
mod commands;
mod dev_server;
mod device;
mod governor;
mod mjpeg;
mod policy;
mod port;
mod simctl;
mod snapshot;
mod types;

pub use bridge::{
    bridge_describe_ui_args, bridge_tap_args, discover_sim_bridge, BridgeAvailability,
    SIM_BRIDGE_INSTALL_HINT,
};
pub use browser::{backend, window_label, BrowserBackend};
pub use commands::*;
pub use simctl::RealSimctl;
pub use snapshot::apply_snapshot_env;
pub use types::{DeviceLifecycle, GovernorStatus, SimHolding};

use governor::ResourceGovernor;
use std::sync::{Arc, Mutex};

/// Shared handle stored in Tauri app state.
#[derive(Clone, Default)]
pub struct ResourceGovernorHandle {
    inner: Arc<Mutex<ResourceGovernor>>,
}

impl ResourceGovernorHandle {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ResourceGovernor::new())),
        }
    }

    pub fn lock(&self) -> Result<std::sync::MutexGuard<'_, ResourceGovernor>, String> {
        self.inner
            .lock()
            .map_err(|_| "resource governor lock poisoned".to_string())
    }

    /// Agent-control ensure-on-use: boot the channel sim within caps.
    pub fn agent_boot_sim(&self, channel_id: &str) -> Result<SimHolding, String> {
        let mut gov = self.lock()?;
        gov.note_agent_boot(channel_id);
        gov.boot(channel_id, None, None, None, &RealSimctl)
    }

    pub fn agent_note_activity(&self, channel_id: &str) -> Result<(), String> {
        self.lock()?.note_agent_boot(channel_id);
        Ok(())
    }

    pub fn agent_attach_webview(&self, channel_id: &str, url: &str) -> Result<bool, String> {
        let mut gov = self.lock()?;
        gov.note_agent_boot(channel_id);
        gov.attach_webview(channel_id, url);
        let hidden = gov
            .status()
            .webviews
            .iter()
            .find(|w| w.channel_id == channel_id)
            .is_none_or(|w| w.hidden);
        Ok(hidden)
    }

    pub fn agent_status(&self) -> Result<GovernorStatus, String> {
        Ok(self.lock()?.status())
    }
}
