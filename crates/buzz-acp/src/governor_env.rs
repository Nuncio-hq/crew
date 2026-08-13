//! Per-session Tool Pane env (`BUZZ_SIMULATOR_UDID`, `BUZZ_DEV_SERVER_URL`).
//! Desktop writes a machine-local snapshot; UDIDs never go on the relay.

use std::path::PathBuf;

use serde::Deserialize;

pub const SNAPSHOT_PATH_ENV: &str = "BUZZ_GOVERNOR_SNAPSHOT_PATH";
pub const SIMULATOR_UDID_ENV: &str = "BUZZ_SIMULATOR_UDID";
pub const DEV_SERVER_URL_ENV: &str = "BUZZ_DEV_SERVER_URL";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentChannelEnv {
    simulator_udid: Option<String>,
    dev_server_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    channels: std::collections::BTreeMap<String, AgentChannelEnv>,
}

fn snapshot_path() -> Option<PathBuf> {
    std::env::var_os(SNAPSHOT_PATH_ENV).map(PathBuf::from)
}

pub fn env_for_channel(channel_id: &str) -> Vec<(String, String)> {
    let Some(path) = snapshot_path() else {
        return Vec::new();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    let Ok(snap) = serde_json::from_slice::<Snapshot>(&bytes) else {
        return Vec::new();
    };
    let Some(entry) = snap.channels.get(channel_id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(udid) = entry
        .simulator_udid
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        out.push((SIMULATOR_UDID_ENV.to_string(), udid.to_string()));
    }
    if let Some(url) = entry
        .dev_server_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        out.push((DEV_SERVER_URL_ENV.to_string(), url.to_string()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_empty() {
        std::env::remove_var(SNAPSHOT_PATH_ENV);
        assert!(env_for_channel("abc").is_empty());
    }

    #[test]
    fn reads_channel_env() {
        let path =
            std::env::temp_dir().join(format!("crew-governor-env-{}.json", std::process::id()));
        std::fs::write(
            &path,
            br#"{"channels":{"chan-1":{"simulatorUdid":"UDID-9","devServerUrl":"http://127.0.0.1:5173"}}}"#,
        )
        .expect("write");
        std::env::set_var(SNAPSHOT_PATH_ENV, &path);
        let env = env_for_channel("chan-1");
        std::env::remove_var(SNAPSHOT_PATH_ENV);
        let _ = std::fs::remove_file(&path);
        assert!(env
            .iter()
            .any(|(k, v)| k == SIMULATOR_UDID_ENV && v == "UDID-9"));
        assert!(env
            .iter()
            .any(|(k, v)| k == DEV_SERVER_URL_ENV && v == "http://127.0.0.1:5173"));
    }
}
