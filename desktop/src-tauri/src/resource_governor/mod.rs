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

pub use commands::*;
pub use simctl::RealSimctl;
pub use snapshot::apply_snapshot_env;

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
}
