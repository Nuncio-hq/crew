//! Agent-facing snapshot: `BUZZ_SIMULATOR_UDID` + `BUZZ_DEV_SERVER_URL` per
//! channel. Machine-local file; never published to the relay.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SNAPSHOT_PATH_ENV: &str = "BUZZ_GOVERNOR_SNAPSHOT_PATH";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentChannelEnv {
    pub simulator_udid: Option<String>,
    pub dev_server_url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernorAgentSnapshot {
    pub channels: BTreeMap<String, AgentChannelEnv>,
}

pub fn snapshot_path(app_data: &Path) -> PathBuf {
    app_data.join("resource-governor").join("snapshot.json")
}

pub fn write_agent_env_snapshot(
    path: &Path,
    snapshot: &GovernorAgentSnapshot,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(snapshot).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

pub fn read_agent_env_snapshot(path: &Path) -> GovernorAgentSnapshot {
    let Ok(bytes) = std::fs::read(path) else {
        return GovernorAgentSnapshot::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn lookup(snapshot: &GovernorAgentSnapshot, channel_id: &str) -> AgentChannelEnv {
    snapshot
        .channels
        .get(channel_id)
        .cloned()
        .unwrap_or_default()
}

pub fn apply_snapshot_env(command: &mut std::process::Command, app_data: &Path) {
    command.env(SNAPSHOT_PATH_ENV, snapshot_path(app_data).as_os_str());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_snapshot() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = snapshot_path(dir.path());
        let mut snap = GovernorAgentSnapshot::default();
        snap.channels.insert(
            "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50".into(),
            AgentChannelEnv {
                simulator_udid: Some("UDID-1".into()),
                dev_server_url: Some("http://127.0.0.1:5173".into()),
            },
        );
        write_agent_env_snapshot(&path, &snap).expect("write");
        let loaded = read_agent_env_snapshot(&path);
        assert_eq!(loaded, snap);
        let env = lookup(&loaded, "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50");
        assert_eq!(env.simulator_udid.as_deref(), Some("UDID-1"));
    }
}
