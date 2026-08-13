//! `simctl` process boundary. Production shells out to `xcrun simctl`.
//! Tests inject [`FakeSimctl`] with fixture JSON — CI never needs a real
//! simulator.

use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedDevice {
    pub udid: String,
    pub name: String,
    pub state: String,
    pub is_available: bool,
    pub runtime: String,
    pub device_type: String,
}

pub trait Simctl: Send {
    fn list_devices(&self) -> Result<Vec<ListedDevice>, String>;
    fn create(&self, name: &str, device_type: &str, runtime: &str) -> Result<String, String>;
    fn boot(&self, udid: &str) -> Result<(), String>;
    fn shutdown(&self, udid: &str) -> Result<(), String>;
    fn erase(&self, udid: &str) -> Result<(), String>;
    fn delete(&self, udid: &str) -> Result<(), String>;
    fn disk_usage(&self, udid: &str) -> Result<u64, String>;
}

/// Real `xcrun simctl`. Missing binary surfaces as an error the UI maps to
/// the bridge/simctl missing card.
pub struct RealSimctl;

impl Simctl for RealSimctl {
    fn list_devices(&self) -> Result<Vec<ListedDevice>, String> {
        let output = simctl_output(&["list", "devices", "-j"])?;
        parse_simctl_list_json(&output)
    }

    fn create(&self, name: &str, device_type: &str, runtime: &str) -> Result<String, String> {
        let output = simctl_output(&["create", name, device_type, runtime])?;
        let udid = output.trim().to_string();
        if udid.is_empty() {
            return Err("simctl create returned an empty UDID".into());
        }
        Ok(udid)
    }

    fn boot(&self, udid: &str) -> Result<(), String> {
        simctl_status(&["boot", udid])
    }

    fn shutdown(&self, udid: &str) -> Result<(), String> {
        simctl_status(&["shutdown", udid])
    }

    fn erase(&self, udid: &str) -> Result<(), String> {
        simctl_status(&["erase", udid])
    }

    fn delete(&self, udid: &str) -> Result<(), String> {
        simctl_status(&["delete", udid])
    }

    fn disk_usage(&self, udid: &str) -> Result<u64, String> {
        // CoreSimulator data lives under ~/Library/Developer/CoreSimulator/Devices/<udid>
        let home = dirs::home_dir().ok_or_else(|| "home directory unavailable".to_string())?;
        let path = home
            .join("Library/Developer/CoreSimulator/Devices")
            .join(udid);
        dir_size(&path)
    }
}

fn simctl_output(args: &[&str]) -> Result<String, String> {
    let output = Command::new("xcrun")
        .arg("simctl")
        .args(args)
        .output()
        .map_err(|e| format!("xcrun simctl {}: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "xcrun simctl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}

fn simctl_status(args: &[&str]) -> Result<(), String> {
    simctl_output(args).map(|_| ())
}

fn dir_size(path: &std::path::Path) -> Result<u64, String> {
    fn rec(dir: &std::path::Path) -> Result<u64, String> {
        if !dir.exists() {
            return Ok(0);
        }
        let mut total = 0_u64;
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return Ok(0),
        };
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let ft = entry.file_type().map_err(|e| e.to_string())?;
            if ft.is_dir() {
                total += rec(&entry.path())?;
            } else if ft.is_file() {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        Ok(total)
    }
    rec(path)
}

#[derive(Deserialize)]
struct SimctlListRoot {
    devices: HashMap<String, Vec<SimctlListDevice>>,
}

#[derive(Deserialize)]
struct SimctlListDevice {
    udid: String,
    name: String,
    state: String,
    #[serde(rename = "isAvailable", default)]
    is_available: bool,
    #[serde(rename = "deviceTypeIdentifier", default)]
    device_type_identifier: String,
}

/// Parse `simctl list devices -j` (faked in tests with fixture JSON).
pub fn parse_simctl_list_json(json: &str) -> Result<Vec<ListedDevice>, String> {
    let root: SimctlListRoot =
        serde_json::from_str(json).map_err(|e| format!("simctl list json: {e}"))?;
    let mut out = Vec::new();
    for (runtime, devices) in root.devices {
        for device in devices {
            out.push(ListedDevice {
                udid: device.udid,
                name: device.name,
                state: device.state,
                is_available: device.is_available,
                runtime: runtime.clone(),
                device_type: device.device_type_identifier,
            });
        }
    }
    Ok(out)
}

/// In-memory simctl for unit tests.
#[derive(Default)]
pub struct FakeSimctl {
    pub devices: Vec<ListedDevice>,
    pub boots: Vec<String>,
    pub shutdowns: Vec<String>,
    pub erases: Vec<String>,
    pub deletes: Vec<String>,
    pub create_error: Option<String>,
    next_udid: u32,
}

impl FakeSimctl {
    pub fn with_devices(devices: Vec<ListedDevice>) -> Self {
        Self {
            devices,
            ..Self::default()
        }
    }
}

impl Simctl for FakeSimctl {
    fn list_devices(&self) -> Result<Vec<ListedDevice>, String> {
        Ok(self.devices.clone())
    }

    fn create(&self, name: &str, device_type: &str, runtime: &str) -> Result<String, String> {
        if let Some(err) = &self.create_error {
            return Err(err.clone());
        }
        Ok(format!("fake-{name}-{device_type}-{runtime}"))
    }

    fn boot(&self, udid: &str) -> Result<(), String> {
        let _ = udid;
        Ok(())
    }

    fn shutdown(&self, udid: &str) -> Result<(), String> {
        let _ = udid;
        Ok(())
    }

    fn erase(&self, udid: &str) -> Result<(), String> {
        let _ = udid;
        Ok(())
    }

    fn delete(&self, udid: &str) -> Result<(), String> {
        let _ = udid;
        Ok(())
    }

    fn disk_usage(&self, udid: &str) -> Result<u64, String> {
        let _ = udid;
        Ok(1_200_000_000)
    }
}

/// Mutable fake used when tests need to observe side effects.
#[derive(Default)]
pub struct RecordingSimctl {
    pub inner: std::sync::Mutex<FakeSimctl>,
}

impl RecordingSimctl {
    pub fn new(devices: Vec<ListedDevice>) -> Self {
        Self {
            inner: std::sync::Mutex::new(FakeSimctl::with_devices(devices)),
        }
    }
}

impl Simctl for RecordingSimctl {
    fn list_devices(&self) -> Result<Vec<ListedDevice>, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "fake simctl poisoned".to_string())?;
        Ok(guard.devices.clone())
    }

    fn create(&self, name: &str, device_type: &str, runtime: &str) -> Result<String, String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "fake simctl poisoned".to_string())?;
        if let Some(err) = &guard.create_error {
            return Err(err.clone());
        }
        guard.next_udid += 1;
        let udid = format!("UDID-{:04}", guard.next_udid);
        guard.devices.push(ListedDevice {
            udid: udid.clone(),
            name: name.to_string(),
            state: "Shutdown".into(),
            is_available: true,
            runtime: runtime.to_string(),
            device_type: device_type.to_string(),
        });
        Ok(udid)
    }

    fn boot(&self, udid: &str) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "fake simctl poisoned".to_string())?;
        guard.boots.push(udid.to_string());
        if let Some(device) = guard.devices.iter_mut().find(|d| d.udid == udid) {
            device.state = "Booted".into();
        }
        Ok(())
    }

    fn shutdown(&self, udid: &str) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "fake simctl poisoned".to_string())?;
        guard.shutdowns.push(udid.to_string());
        if let Some(device) = guard.devices.iter_mut().find(|d| d.udid == udid) {
            device.state = "Shutdown".into();
        }
        Ok(())
    }

    fn erase(&self, udid: &str) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "fake simctl poisoned".to_string())?;
        guard.erases.push(udid.to_string());
        Ok(())
    }

    fn delete(&self, udid: &str) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "fake simctl poisoned".to_string())?;
        guard.deletes.push(udid.to_string());
        guard.devices.retain(|d| d.udid != udid);
        Ok(())
    }

    fn disk_usage(&self, udid: &str) -> Result<u64, String> {
        let _ = udid;
        Ok(1_200_000_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-18-0": [
                {
                    "udid": "AAAA-1111",
                    "name": "crew-9a1657ac",
                    "state": "Booted",
                    "isAvailable": true,
                    "deviceTypeIdentifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-16-Pro"
                },
                {
                    "udid": "BBBB-2222",
                    "name": "iPhone 15",
                    "state": "Shutdown",
                    "isAvailable": true,
                    "deviceTypeIdentifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-15"
                }
            ]
        }
    }"#;

    #[test]
    fn parses_fixture_json() {
        let devices = parse_simctl_list_json(FIXTURE).expect("json");
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name, "crew-9a1657ac");
        assert_eq!(devices[0].state, "Booted");
        assert_eq!(devices[1].name, "iPhone 15");
    }
}
