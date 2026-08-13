//! Injected JS bootstrap for the browser instrument (a11y snapshot, actions,
//! console / fetch instrumentation). Idempotent.

pub const BROWSER_BRIDGE_JS: &str = include_str!("browser_bridge.js");
