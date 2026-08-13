//! Sim-bridge discovery ladder (baguette preferred, then idb_companion).
//! Same shape as `gh_cli.rs`: available / missing / failed + install hint.

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

pub fn bridge_tap_args(binary: &str, udid: &str, x: f64, y: f64) -> Vec<String> {
    if binary.contains("baguette") {
        vec![
            "tap".into(),
            "--udid".into(),
            udid.into(),
            "--x".into(),
            format!("{x:.1}"),
            "--y".into(),
            format!("{y:.1}"),
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
    fn baguette_stream_args_request_mjpeg() {
        let args = bridge_mjpeg_args("baguette", "UDID-1");
        assert!(args.iter().any(|a| a == "mjpeg"));
        assert!(args.iter().any(|a| a == "UDID-1"));
    }
}
