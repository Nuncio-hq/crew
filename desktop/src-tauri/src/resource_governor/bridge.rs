//! Sim-bridge discovery ladder (baguette preferred, then idb_companion).
//! Same shape as `gh_cli.rs`: available / missing / failed + install hint.
//!
//! Arg shapes below were verified live against `baguette` 0.1.92
//! (`brew install baguette`) on a booted iPhone 17 Pro simulator (2026-08-20,
//! issue #246 / spike 0057 follow-up): every `baguette` subcommand here was
//! run against a real `xcrun simctl`-booted device and returned `{"ok":true}`.
//! `idb_companion` shapes remain the pre-existing best-guess from spike 0028 —
//! that binary has not been installed or exercised live (D-058 ladder keeps it
//! as the documented fallback regardless).

#![allow(dead_code)]
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::types::BridgeStatus;

/// Copyable install guidance shown on the Sim tab install card.
pub const SIM_BRIDGE_INSTALL_HINT: &str =
    "brew install baguette\nbrew tap facebook/fb && brew install idb-companion";

const CANDIDATES: &[&str] = &["baguette", "idb_companion", "idb-companion"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BridgeAvailability {
    Available { binary: String, path: PathBuf },
    Missing { install_hint: String },
    Failed { message: String },
}

impl BridgeAvailability {
    pub fn to_status(&self) -> BridgeStatus {
        match self {
            Self::Available { binary, path } => BridgeStatus {
                availability: "available".into(),
                binary: Some(binary.clone()),
                path: Some(path.display().to_string()),
                install_hint: None,
                message: None,
            },
            Self::Missing { install_hint } => BridgeStatus {
                availability: "missing".into(),
                binary: None,
                path: None,
                install_hint: Some(install_hint.clone()),
                message: None,
            },
            Self::Failed { message } => BridgeStatus {
                availability: "failed".into(),
                binary: None,
                path: None,
                install_hint: Some(SIM_BRIDGE_INSTALL_HINT.into()),
                message: Some(message.clone()),
            },
        }
    }
}

/// Resolve baguette or idb_companion via the same PATH ladder as `gh`.
///
/// `find_command` already probes `/opt/homebrew/bin` (Apple Silicon Homebrew),
/// `/usr/local/bin` (Intel Homebrew), the login-shell `PATH`, and a handful of
/// other common install locations — a Homebrew install is discovered without
/// restarting the app, no separate Homebrew-path probing is needed here.
pub fn discover_sim_bridge() -> BridgeAvailability {
    for name in CANDIDATES {
        if let Some(path) = crate::managed_agents::find_command(name) {
            return BridgeAvailability::Available {
                binary: (*name).to_string(),
                path,
            };
        }
    }
    BridgeAvailability::Missing {
        install_hint: SIM_BRIDGE_INSTALL_HINT.to_string(),
    }
}

/// Device screen size in points, used to tell the bridge which coordinate
/// space a caller's tap/swipe points were computed in. `baguette` scales
/// `--x/--y` (and `--start-*`/`--end-*`) against `--width/--height` onto the
/// real device, so correctness only requires the caller's declared size to
/// match the space its points came from — not the device's true resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenSize {
    pub width: f64,
    pub height: f64,
}

impl ScreenSize {
    /// Generic iPhone point size used as a fallback when no simulator
    /// accessibility snapshot has been taken yet to learn the real size, and
    /// by the Sim tab's own on-screen bezel (`SimTab.tsx` normalizes pointer
    /// events into this exact 390×844 space before calling `sim_tap`).
    pub const DEFAULT: ScreenSize = ScreenSize {
        width: 390.0,
        height: 844.0,
    };
}

/// HID / stream command templates. Production spawns these; tests stub the
/// process boundary.
pub fn bridge_mjpeg_args(binary: &str, udid: &str) -> Vec<String> {
    if binary.contains("baguette") {
        vec![
            "stream".into(),
            "--udid".into(),
            udid.into(),
            "--format".into(),
            "mjpeg".into(),
        ]
    } else {
        vec!["--udid".into(), udid.into(), "--mjpeg".into()]
    }
}

pub fn bridge_describe_ui_args(binary: &str, udid: &str) -> Vec<String> {
    if binary.contains("baguette") {
        vec!["describe-ui".into(), "--udid".into(), udid.into()]
    } else {
        vec![
            "ui".into(),
            "describe-all".into(),
            "--udid".into(),
            udid.into(),
        ]
    }
}

/// `baguette tap` requires `--width`/`--height` so it can scale the point
/// into device coordinates — omitting them is a hard CLI error (exit 64),
/// not a silent no-op.
pub fn bridge_tap_args(
    binary: &str,
    udid: &str,
    x: f64,
    y: f64,
    screen: ScreenSize,
) -> Vec<String> {
    if binary.contains("baguette") {
        vec![
            "tap".into(),
            "--udid".into(),
            udid.into(),
            "--x".into(),
            format!("{x:.1}"),
            "--y".into(),
            format!("{y:.1}"),
            "--width".into(),
            format!("{:.1}", screen.width),
            "--height".into(),
            format!("{:.1}", screen.height),
        ]
    } else {
        vec![
            "ui".into(),
            "tap".into(),
            "--udid".into(),
            udid.into(),
            format!("{x:.1}"),
            format!("{y:.1}"),
        ]
    }
}

/// One-finger drag; same `--width`/`--height` requirement as `tap`.
pub fn bridge_swipe_args(
    binary: &str,
    udid: &str,
    from: (f64, f64),
    to: (f64, f64),
    screen: ScreenSize,
) -> Vec<String> {
    if binary.contains("baguette") {
        vec![
            "swipe".into(),
            "--udid".into(),
            udid.into(),
            "--start-x".into(),
            format!("{:.1}", from.0),
            "--start-y".into(),
            format!("{:.1}", from.1),
            "--end-x".into(),
            format!("{:.1}", to.0),
            "--end-y".into(),
            format!("{:.1}", to.1),
            "--width".into(),
            format!("{:.1}", screen.width),
            "--height".into(),
            format!("{:.1}", screen.height),
        ]
    } else {
        vec![
            "ui".into(),
            "swipe".into(),
            "--udid".into(),
            udid.into(),
            format!("{:.1}", from.0),
            format!("{:.1}", from.1),
            format!("{:.1}", to.0),
            format!("{:.1}", to.1),
        ]
    }
}

/// Type ASCII text into the focused field.
pub fn bridge_type_args(binary: &str, udid: &str, text: &str) -> Vec<String> {
    if binary.contains("baguette") {
        vec![
            "type".into(),
            "--udid".into(),
            udid.into(),
            "--text".into(),
            text.into(),
        ]
    } else {
        vec![
            "ui".into(),
            "text".into(),
            "--udid".into(),
            udid.into(),
            text.into(),
        ]
    }
}

/// Press-and-release a hardware button (`home`, `lock`, `power`, …).
pub fn bridge_press_args(binary: &str, udid: &str, button: &str) -> Vec<String> {
    if binary.contains("baguette") {
        vec![
            "press".into(),
            "--udid".into(),
            udid.into(),
            "--button".into(),
            button.into(),
        ]
    } else {
        vec![
            "ui".into(),
            "button".into(),
            "--udid".into(),
            udid.into(),
            button.into(),
        ]
    }
}

/// Press a single keyboard key by W3C `KeyboardEvent.code` (e.g. `KeyA`,
/// `Enter`, `Backspace`).
pub fn bridge_key_args(binary: &str, udid: &str, code: &str) -> Vec<String> {
    if binary.contains("baguette") {
        vec![
            "key".into(),
            "--udid".into(),
            udid.into(),
            "--code".into(),
            code.into(),
        ]
    } else {
        vec![
            "ui".into(),
            "key".into(),
            "--udid".into(),
            udid.into(),
            code.into(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_status_carries_brew_hint() {
        let status = BridgeAvailability::Missing {
            install_hint: SIM_BRIDGE_INSTALL_HINT.into(),
        }
        .to_status();
        assert_eq!(status.availability, "missing");
        assert!(status
            .install_hint
            .as_deref()
            .is_some_and(|h| h.contains("brew install baguette")));
    }

    #[test]
    fn baguette_describe_ui_args() {
        let args = bridge_describe_ui_args("baguette", "UDID-1");
        assert!(args.iter().any(|a| a == "describe-ui"));
        assert!(args.iter().any(|a| a == "UDID-1"));
    }

    #[test]
    fn baguette_mjpeg_args() {
        let args = bridge_mjpeg_args("baguette", "UDID-1");
        assert!(args.iter().any(|a| a == "mjpeg"));
        assert!(args.iter().any(|a| a == "UDID-1"));
    }

    /// Pins the live-verified `baguette tap` contract: `--width`/`--height`
    /// are mandatory (the real CLI exits 64 without them), so the arg list
    /// must always carry both alongside `--x`/`--y`.
    #[test]
    fn baguette_tap_args_include_width_and_height() {
        let args = bridge_tap_args("baguette", "UDID-1", 100.0, 200.0, ScreenSize::DEFAULT);
        assert_eq!(
            args,
            vec![
                "tap", "--udid", "UDID-1", "--x", "100.0", "--y", "200.0", "--width", "390.0",
                "--height", "844.0",
            ]
        );
    }

    #[test]
    fn baguette_swipe_args_include_width_and_height() {
        let args = bridge_swipe_args(
            "baguette",
            "UDID-1",
            (10.0, 20.0),
            (30.0, 40.0),
            ScreenSize::DEFAULT,
        );
        assert_eq!(
            args,
            vec![
                "swipe",
                "--udid",
                "UDID-1",
                "--start-x",
                "10.0",
                "--start-y",
                "20.0",
                "--end-x",
                "30.0",
                "--end-y",
                "40.0",
                "--width",
                "390.0",
                "--height",
                "844.0",
            ]
        );
    }

    #[test]
    fn baguette_type_args_use_type_subcommand() {
        let args = bridge_type_args("baguette", "UDID-1", "hello");
        assert_eq!(args, vec!["type", "--udid", "UDID-1", "--text", "hello"]);
    }

    #[test]
    fn baguette_press_args_use_press_subcommand() {
        let args = bridge_press_args("baguette", "UDID-1", "home");
        assert_eq!(args, vec!["press", "--udid", "UDID-1", "--button", "home"]);
    }

    #[test]
    fn baguette_key_args_use_key_subcommand() {
        let args = bridge_key_args("baguette", "UDID-1", "Enter");
        assert_eq!(args, vec!["key", "--udid", "UDID-1", "--code", "Enter"]);
    }

    #[test]
    fn idb_fallback_args_stay_on_ui_ladder() {
        // Not live-verified (idb_companion is not installed in this repo's CI
        // or on the founder Mac this ladder was checked against) — this only
        // pins the existing best-guess shape so a refactor doesn't silently
        // change it.
        assert_eq!(
            bridge_press_args("idb_companion", "UDID-1", "home"),
            vec!["ui", "button", "--udid", "UDID-1", "home"]
        );
        assert_eq!(
            bridge_type_args("idb_companion", "UDID-1", "hi"),
            vec!["ui", "text", "--udid", "UDID-1", "hi"]
        );
    }
}
