//! Whether the private Ask command surface exists at all, decided once.
//!
//! This gate lives at the command layer on purpose. A renderer-side flag is not
//! a gate: the webview can invoke any command the builder registered, so a
//! command that checks only a React state has no gate. Every private Ask
//! command asks [`private_ask_dev_enabled`] first and refuses before touching
//! any argument.
//!
//! `cfg!(debug_assertions)` is true under `cargo test`, so a test can never
//! observe the "off" state through the real reader. The decision is therefore a
//! pure function of its two inputs, tested directly in both states, and the
//! process-wide reader is a thin `OnceLock` over it.

use std::ffi::OsStr;
use std::sync::OnceLock;

/// Environment variable that enables the surface in a release build.
pub(crate) const PRIVATE_ASK_DEV_VAR: &str = "BUZZ_PRIVATE_ASK_DEV";

/// The exact value that enables it. Nothing else does — not `true`, not `yes`,
/// not `0`, and not a padded `1`. A fuzzy match here is how a flag ends up on
/// in an environment nobody intended.
const ENABLED_VALUE: &str = "1";

/// Pure decision, so both branches are reachable from a test.
///
/// A non-UTF-8 value is simply not the enabling value; it must not panic.
pub(crate) fn dev_gate_open(debug_build: bool, value: Option<&OsStr>) -> bool {
    debug_build || value.and_then(OsStr::to_str) == Some(ENABLED_VALUE)
}

/// Read once at first use and cache: the answer must not change under a running
/// process, so a later `set_var` cannot open the surface mid-session.
pub(crate) fn private_ask_dev_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        dev_gate_open(
            cfg!(debug_assertions),
            std::env::var_os(PRIVATE_ASK_DEV_VAR).as_deref(),
        )
    })
}

#[cfg(test)]
#[path = "dev_gate_tests.rs"]
mod tests;
