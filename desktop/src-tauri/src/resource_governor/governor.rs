//! Resource Governor state machine. Caps, LRU, idle, reconciliation.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::bridge::{discover_sim_bridge, BridgeAvailability};
use super::browser::probe_child_webview;
use super::clock::Clock;
use super::device::{crew_device_name, is_crew_device_name};
use super::policy::GovernorPolicy;
use super::port::{allocate_port, expand_command, output_matches_ready};
use super::simctl::{ListedDevice, Simctl};
use super::types::{
    DevServerFace, DevServerHolding, DeviceLifecycle, GovernorStatus, SimHolding, StopKind,
    WebviewHolding,
};

const DEFAULT_DEVICE_TYPE: &str = "iPhone 16 Pro";
const DEFAULT_RUNTIME: &str = "iOS 18";
const MAX_CRASH_RESTARTS: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapConflict {
    pub kind: String,
    pub victim_channel_id: String,
    pub victim_name: String,
    pub incoming_channel_id: String,
    pub incoming_name: String,
    pub idle_ms: u64,
    pub keep_token: String,
}

#[derive(Debug, Clone)]
struct SimRecord {
    holding: SimHolding,
    hidden_since_ms: Option<u64>,
    created_by_governor: bool,
}

#[derive(Debug, Clone)]
struct ServerRecord {
    holding: DevServerHolding,
    last_request_ms: u64,
    agent_activity_ms: u64,
    attached_webview: bool,
}

pub struct ResourceGovernor {
    clock: Clock,
    policy: GovernorPolicy,
    sims: HashMap<String, SimRecord>,
    servers: HashMap<String, ServerRecord>,
    webviews: HashMap<String, WebviewHolding>,
    cap_conflict: Option<CapConflict>,
    bridge: BridgeAvailability,
    shared_runtime: String,
}

impl ResourceGovernor {
    pub fn new() -> Self {
        Self::with_clock(Clock::default())
    }

    pub fn with_clock(clock: Clock) -> Self {
        Self {
            clock,
            policy: GovernorPolicy::with_defaults(),
            sims: HashMap::new(),
            servers: HashMap::new(),
            webviews: HashMap::new(),
            cap_conflict: None,
            bridge: discover_sim_bridge(),
            shared_runtime: DEFAULT_RUNTIME.to_string(),
        }
    }

    pub fn set_bridge_for_test(&mut self, bridge: BridgeAvailability) {
        self.bridge = bridge;
    }

    pub fn set_policy(&mut self, policy: GovernorPolicy) {
        self.policy = policy;
    }

    pub fn policy(&self) -> &GovernorPolicy {
        &self.policy
    }

    pub fn now_ms(&self) -> u64 {
        self.clock.now()
    }

    pub fn advance(&mut self, delta_ms: u64) {
        self.clock.advance(delta_ms);
        self.reap();
    }

    pub fn tick_production(&mut self, simctl: &dyn Simctl) {
        self.reap();
        self.force_idle_shutdowns(simctl);
        if let Ok(listed) = simctl.list_devices() {
            self.reconcile(&listed, simctl);
        }
    }

    pub fn set_now(&mut self, now_ms: u64) {
        self.clock.set(now_ms);
        self.reap();
    }

    /// Never boot on channel open. First use creates a shutdown-state record.
    pub fn ensure_device(
        &mut self,
        channel_id: &str,
        channel_name: Option<&str>,
        device_type: Option<&str>,
        runtime: Option<&str>,
        simctl: &dyn Simctl,
    ) -> Result<SimHolding, String> {
        if let Some(existing) = self.sims.get(channel_id) {
            return Ok(existing.holding.clone());
        }
        let name = crew_device_name(channel_id);
        let listed = simctl.list_devices().unwrap_or_default();
        if let Some(found) = listed.iter().find(|d| d.name == name) {
            let lifecycle = if found.state.eq_ignore_ascii_case("Booted") {
                DeviceLifecycle::Booted
            } else {
                DeviceLifecycle::Shutdown
            };
            let holding = SimHolding {
                channel_id: channel_id.to_string(),
                channel_name: channel_name.map(str::to_string),
                device_name: name,
                udid: Some(found.udid.clone()),
                lifecycle,
                device_type: display_device_type(&found.device_type),
                runtime: runtime
                    .map(str::to_string)
                    .unwrap_or_else(|| self.shared_runtime.clone()),
                foreign: false,
                disk_bytes: simctl.disk_usage(&found.udid).unwrap_or(0),
                last_used_ms: self.clock.now(),
                idle_deadline_ms: None,
                pane_visible: false,
                mirroring: false,
                last_screenshot_data_url: None,
                boot_elapsed_ms: None,
            };
            self.sims.insert(
                channel_id.to_string(),
                SimRecord {
                    holding: holding.clone(),
                    hidden_since_ms: None,
                    created_by_governor: true,
                },
            );
            return Ok(holding);
        }
        let dtype = device_type.unwrap_or(DEFAULT_DEVICE_TYPE);
        let runtime = runtime.unwrap_or(self.shared_runtime.as_str());
        let holding = SimHolding {
            channel_id: channel_id.to_string(),
            channel_name: channel_name.map(str::to_string),
            device_name: name,
            udid: None,
            lifecycle: DeviceLifecycle::Absent,
            device_type: dtype.to_string(),
            runtime: runtime.to_string(),
            foreign: false,
            disk_bytes: 0,
            last_used_ms: self.clock.now(),
            idle_deadline_ms: None,
            pane_visible: false,
            mirroring: false,
            last_screenshot_data_url: None,
            boot_elapsed_ms: None,
        };
        self.sims.insert(
            channel_id.to_string(),
            SimRecord {
                holding: holding.clone(),
                hidden_since_ms: None,
                created_by_governor: true,
            },
        );
        Ok(holding)
    }

    /// Lazy create + boot. Never called on channel open.
    pub fn boot(
        &mut self,
        channel_id: &str,
        channel_name: Option<&str>,
        device_type: Option<&str>,
        runtime: Option<&str>,
        simctl: &dyn Simctl,
    ) -> Result<SimHolding, String> {
        self.ensure_device(channel_id, channel_name, device_type, runtime, simctl)?;
        if self.booted_count() >= self.policy.max_booted_sims {
            if let Some(conflict) = self.propose_sim_eviction(channel_id, channel_name) {
                self.cap_conflict = Some(conflict.clone());
                return Err(format!(
                    "cap: Sim of {} shuts down to make room for {} (idle {} min)",
                    conflict.victim_name,
                    conflict.incoming_name,
                    conflict.idle_ms / 60_000
                ));
            }
            return Err("sim boot cap reached; visible mirror is protected".into());
        }
        let dtype = {
            let rec = self
                .sims
                .get(channel_id)
                .ok_or_else(|| "device missing after ensure".to_string())?;
            rec.holding.device_type.clone()
        };
        let runtime = {
            let rec = self
                .sims
                .get(channel_id)
                .ok_or_else(|| "device missing after ensure".to_string())?;
            rec.holding.runtime.clone()
        };
        let name = crew_device_name(channel_id);
        let rec = self
            .sims
            .get_mut(channel_id)
            .ok_or_else(|| "device missing after ensure".to_string())?;
        if rec.holding.udid.is_none() {
            let udid = simctl.create(&name, &dtype, &runtime)?;
            rec.holding.udid = Some(udid);
            rec.holding.lifecycle = DeviceLifecycle::Shutdown;
            rec.created_by_governor = true;
        }
        let udid = rec
            .holding
            .udid
            .clone()
            .ok_or_else(|| "create did not yield a UDID".to_string())?;
        simctl.boot(&udid)?;
        rec.holding.lifecycle = DeviceLifecycle::Booted;
        rec.holding.last_used_ms = self.clock.now();
        rec.holding.idle_deadline_ms = Some(self.clock.now() + self.policy.sim_idle_shutdown_ms);
        rec.holding.boot_elapsed_ms = Some(0);
        rec.holding.disk_bytes = simctl.disk_usage(&udid).unwrap_or(rec.holding.disk_bytes);
        Ok(rec.holding.clone())
    }

    pub fn keep_sim(&mut self, channel_id: &str) -> Result<SimHolding, String> {
        let rec = self
            .sims
            .get_mut(channel_id)
            .ok_or_else(|| format!("no sim for {channel_id}"))?;
        rec.holding.last_used_ms = self.clock.now();
        rec.holding.idle_deadline_ms = Some(self.clock.now() + self.policy.sim_idle_shutdown_ms);
        if self
            .cap_conflict
            .as_ref()
            .is_some_and(|c| c.victim_channel_id == channel_id)
        {
            self.cap_conflict = None;
        }
        Ok(rec.holding.clone())
    }

    pub fn set_pane_visible(
        &mut self,
        channel_id: &str,
        visible: bool,
    ) -> Result<SimHolding, String> {
        let now = self.clock.now();
        let rec = self
            .sims
            .get_mut(channel_id)
            .ok_or_else(|| format!("no sim for {channel_id}"))?;
        rec.holding.pane_visible = visible;
        if visible {
            rec.hidden_since_ms = None;
            rec.holding.last_used_ms = now;
            rec.holding.idle_deadline_ms = Some(now + self.policy.sim_idle_shutdown_ms);
            if rec.holding.lifecycle == DeviceLifecycle::Booted
                && self.stream_count_excluding(channel_id) < self.policy.max_mirror_streams
            {
                rec.holding.lifecycle = DeviceLifecycle::Mirroring;
                rec.holding.mirroring = true;
            }
        } else {
            rec.hidden_since_ms = Some(now);
        }
        Ok(rec.holding.clone())
    }

    pub fn shutdown_sim(
        &mut self,
        channel_id: &str,
        simctl: &dyn Simctl,
    ) -> Result<SimHolding, String> {
        let rec = self
            .sims
            .get_mut(channel_id)
            .ok_or_else(|| format!("no sim for {channel_id}"))?;
        if rec.holding.foreign {
            return Err("foreign devices are read-only".into());
        }
        if let Some(udid) = rec.holding.udid.as_deref() {
            simctl.shutdown(udid)?;
        }
        rec.holding.lifecycle = DeviceLifecycle::Shutdown;
        rec.holding.mirroring = false;
        rec.holding.pane_visible = false;
        rec.holding.idle_deadline_ms = None;
        rec.hidden_since_ms = None;
        Ok(rec.holding.clone())
    }

    pub fn erase_sim(
        &mut self,
        channel_id: &str,
        simctl: &dyn Simctl,
    ) -> Result<SimHolding, String> {
        let rec = self
            .sims
            .get_mut(channel_id)
            .ok_or_else(|| format!("no sim for {channel_id}"))?;
        if rec.holding.foreign {
            return Err("foreign devices are read-only".into());
        }
        if let Some(udid) = rec.holding.udid.as_deref() {
            let _ = simctl.shutdown(udid);
            simctl.erase(udid)?;
        }
        rec.holding.lifecycle = DeviceLifecycle::Shutdown;
        rec.holding.mirroring = false;
        rec.holding.disk_bytes = rec
            .holding
            .udid
            .as_deref()
            .and_then(|u| simctl.disk_usage(u).ok())
            .unwrap_or(0);
        Ok(rec.holding.clone())
    }

    pub fn delete_sim(&mut self, channel_id: &str, simctl: &dyn Simctl) -> Result<(), String> {
        let Some(rec) = self.sims.remove(channel_id) else {
            return Ok(());
        };
        if rec.holding.foreign {
            self.sims.insert(channel_id.to_string(), rec);
            return Err("foreign devices are read-only".into());
        }
        if let Some(udid) = rec.holding.udid.as_deref() {
            let _ = simctl.shutdown(udid);
            simctl.delete(udid)?;
        }
        Ok(())
    }

    pub fn on_channel_archived(
        &mut self,
        channel_id: &str,
        simctl: &dyn Simctl,
    ) -> Result<(), String> {
        if self.sims.contains_key(channel_id) {
            self.erase_sim(channel_id, simctl)?;
        }
        Ok(())
    }

    pub fn on_channel_deleted(
        &mut self,
        channel_id: &str,
        simctl: &dyn Simctl,
    ) -> Result<(), String> {
        self.delete_sim(channel_id, simctl)?;
        self.servers
            .retain(|_, s| s.holding.channel_id != channel_id);
        self.webviews.retain(|_, w| w.channel_id != channel_id);
        Ok(())
    }

    /// Adopt externally-booted `crew-` devices. Foreign devices are recorded
    /// read-only and never auto-shutdown.
    pub fn reconcile(&mut self, listed: &[ListedDevice], simctl: &dyn Simctl) {
        let now = self.clock.now();
        for device in listed {
            if is_crew_device_name(&device.name) {
                let already = self
                    .sims
                    .values()
                    .any(|r| r.holding.udid.as_deref() == Some(device.udid.as_str()));
                if already {
                    if device.state.eq_ignore_ascii_case("Booted") {
                        for rec in self.sims.values_mut() {
                            if rec.holding.udid.as_deref() == Some(device.udid.as_str())
                                && rec.holding.lifecycle == DeviceLifecycle::Shutdown
                            {
                                rec.holding.lifecycle = DeviceLifecycle::Booted;
                                rec.holding.last_used_ms = now;
                                rec.holding.idle_deadline_ms =
                                    Some(now + self.policy.sim_idle_shutdown_ms);
                            }
                        }
                    }
                    continue;
                }
                let channel_guess = device.name.trim_start_matches("crew-").to_string();
                let key = format!("adopted-{channel_guess}");
                if self.sims.contains_key(&key) {
                    continue;
                }
                let holding = SimHolding {
                    channel_id: key.clone(),
                    channel_name: None,
                    device_name: device.name.clone(),
                    udid: Some(device.udid.clone()),
                    lifecycle: if device.state.eq_ignore_ascii_case("Booted") {
                        DeviceLifecycle::Booted
                    } else {
                        DeviceLifecycle::Shutdown
                    },
                    device_type: display_device_type(&device.device_type),
                    runtime: device.runtime.clone(),
                    foreign: false,
                    disk_bytes: simctl.disk_usage(&device.udid).unwrap_or(0),
                    last_used_ms: now,
                    idle_deadline_ms: if device.state.eq_ignore_ascii_case("Booted") {
                        Some(now + self.policy.sim_idle_shutdown_ms)
                    } else {
                        None
                    },
                    pane_visible: false,
                    mirroring: false,
                    last_screenshot_data_url: None,
                    boot_elapsed_ms: None,
                };
                self.sims.insert(
                    key,
                    SimRecord {
                        holding,
                        hidden_since_ms: None,
                        created_by_governor: true,
                    },
                );
            } else if device.state.eq_ignore_ascii_case("Booted") {
                let key = format!("foreign-{}", device.udid);
                if self.sims.contains_key(&key) {
                    continue;
                }
                let holding = SimHolding {
                    channel_id: key.clone(),
                    channel_name: None,
                    device_name: device.name.clone(),
                    udid: Some(device.udid.clone()),
                    lifecycle: DeviceLifecycle::Booted,
                    device_type: display_device_type(&device.device_type),
                    runtime: device.runtime.clone(),
                    foreign: true,
                    disk_bytes: simctl.disk_usage(&device.udid).unwrap_or(0),
                    last_used_ms: now,
                    idle_deadline_ms: None,
                    pane_visible: false,
                    mirroring: false,
                    last_screenshot_data_url: None,
                    boot_elapsed_ms: None,
                };
                self.sims.insert(
                    key,
                    SimRecord {
                        holding,
                        hidden_since_ms: None,
                        created_by_governor: false,
                    },
                );
            }
        }
    }

    pub fn start_dev_server(
        &mut self,
        channel_id: &str,
        subject: &str,
        command: &str,
        cwd: &str,
        ready_pattern: &str,
        preferred_port: Option<u16>,
    ) -> Result<DevServerHolding, String> {
        let id = format!("{channel_id}:{subject}");
        if let Some(existing) = self.servers.get(&id) {
            return Ok(existing.holding.clone());
        }
        if self.managed_server_count() >= self.policy.max_dev_servers {
            if let Some(victim) = self.lru_server_victim() {
                self.servers.remove(&victim);
            } else {
                return Err("dev server cap reached".into());
            }
        }
        let (port, note) = allocate_port(preferred_port)?;
        let expanded = expand_command(command, port);
        let holding = DevServerHolding {
            id: id.clone(),
            channel_id: channel_id.to_string(),
            subject: subject.to_string(),
            command: expanded,
            port,
            url: None,
            face: if note.is_some() {
                DevServerFace::PortConflict
            } else {
                DevServerFace::Running
            },
            uptime_ms: 0,
            idle_deadline_ms: Some(self.clock.now() + self.policy.dev_server_idle_ms),
            last_log: Vec::new(),
            port_note: note,
            crash_count: 0,
            cwd: cwd.to_string(),
        };
        let _ = ready_pattern;
        self.servers.insert(
            id,
            ServerRecord {
                holding: holding.clone(),
                last_request_ms: self.clock.now(),
                agent_activity_ms: 0,
                attached_webview: false,
            },
        );
        Ok(holding)
    }

    pub fn note_server_output(&mut self, server_id: &str, line: &str, ready_pattern: &str) {
        let now = self.clock.now();
        let Some(rec) = self.servers.get_mut(server_id) else {
            return;
        };
        rec.holding.last_log.push(line.to_string());
        if rec.holding.last_log.len() > 30 {
            rec.holding.last_log.remove(0);
        }
        rec.last_request_ms = now;
        if rec.holding.url.is_none() && output_matches_ready(line, ready_pattern) {
            rec.holding.url = Some(format!("http://127.0.0.1:{}", rec.holding.port));
            rec.holding.face = DevServerFace::Running;
        }
    }

    pub fn mark_server_crashed(&mut self, server_id: &str) {
        let Some(rec) = self.servers.get_mut(server_id) else {
            return;
        };
        rec.holding.crash_count = rec.holding.crash_count.saturating_add(1);
        if rec.holding.crash_count > MAX_CRASH_RESTARTS {
            rec.holding.face = DevServerFace::Crashed;
            rec.holding.url = None;
        }
    }

    pub fn attach_webview(&mut self, channel_id: &str, url: &str) -> WebviewHolding {
        self.reap_hidden_webviews();
        let id = format!("wv-{channel_id}");
        if let Some(existing) = self.webviews.get_mut(&id) {
            existing.url = url.to_string();
            existing.hidden = false;
            existing.hidden_since_ms = None;
            return existing.clone();
        }
        let holding = WebviewHolding {
            id: id.clone(),
            channel_id: channel_id.to_string(),
            url: url.to_string(),
            hidden: false,
            hidden_since_ms: None,
            backend: if probe_child_webview() {
                "child".into()
            } else {
                "window".into()
            },
        };
        self.webviews.insert(id, holding.clone());
        if let Some(server) = self
            .servers
            .values_mut()
            .find(|s| s.holding.channel_id == channel_id)
        {
            server.attached_webview = true;
        }
        holding
    }

    pub fn hide_webview(&mut self, webview_id: &str) {
        let now = self.clock.now();
        if let Some(wv) = self.webviews.get_mut(webview_id) {
            wv.hidden = true;
            wv.hidden_since_ms = Some(now);
        }
        self.reap_hidden_webviews();
    }

    pub fn stop(&mut self, kind: StopKind, id: &str, simctl: &dyn Simctl) -> Result<(), String> {
        match kind {
            StopKind::Sim => {
                self.shutdown_sim(id, simctl)?;
            }
            StopKind::Server => {
                self.servers.remove(id);
            }
            StopKind::Webview => {
                self.webviews.remove(id);
            }
            StopKind::Everything => {
                self.quit_cleanup(simctl)?;
            }
        }
        Ok(())
    }

    /// App quit: shutdown all Crew-booted sims, stop managed servers.
    pub fn quit_cleanup(&mut self, simctl: &dyn Simctl) -> Result<(), String> {
        let ids: Vec<String> = self
            .sims
            .iter()
            .filter(|(_, r)| {
                !r.holding.foreign
                    && matches!(
                        r.holding.lifecycle,
                        DeviceLifecycle::Booted | DeviceLifecycle::Mirroring
                    )
            })
            .map(|(k, _)| k.clone())
            .collect();
        for id in ids {
            let _ = self.shutdown_sim(&id, simctl);
        }
        self.servers.clear();
        self.webviews.clear();
        Ok(())
    }

    pub fn status(&self) -> GovernorStatus {
        let sims: Vec<SimHolding> = self.sims.values().map(|r| r.holding.clone()).collect();
        let servers: Vec<DevServerHolding> =
            self.servers.values().map(|r| r.holding.clone()).collect();
        let webviews: Vec<WebviewHolding> = self.webviews.values().cloned().collect();
        let disk_bytes = sims.iter().map(|s| s.disk_bytes).sum();
        let prune_after = self.clock.now().saturating_sub(self.policy.prune_unused_ms);
        let prune_candidates = sims
            .iter()
            .filter(|s| !s.foreign && s.last_used_ms < prune_after)
            .cloned()
            .collect();
        GovernorStatus {
            policy: self.policy.clone(),
            now_ms: self.clock.now(),
            booted_count: self.booted_count(),
            stream_count: self.stream_count_excluding(""),
            server_count: self.managed_server_count(),
            disk_bytes,
            cap_conflict: self.cap_conflict.clone(),
            prune_candidates,
            bridge: self.bridge.to_status(),
            child_webview_available: probe_child_webview(),
            sims,
            servers,
            webviews,
        }
    }

    pub fn agent_env_for_channel(&self, channel_id: &str) -> (Option<String>, Option<String>) {
        let udid = self
            .sims
            .get(channel_id)
            .and_then(|r| r.holding.udid.clone());
        let url = self
            .servers
            .values()
            .find(|s| s.holding.channel_id == channel_id)
            .and_then(|s| s.holding.url.clone());
        (udid, url)
    }

    pub fn note_agent_boot(&mut self, channel_id: &str) {
        if let Some(rec) = self.sims.get_mut(channel_id) {
            rec.holding.last_used_ms = self.clock.now();
        }
        if let Some(server) = self
            .servers
            .values_mut()
            .find(|s| s.holding.channel_id == channel_id)
        {
            server.agent_activity_ms = self.clock.now();
        }
    }

    fn reap(&mut self) {
        let now = self.clock.now();
        let pause_ms = self.policy.stream_pause_hidden_ms;
        let idle_ms = self.policy.sim_idle_shutdown_ms;
        let mut pause = Vec::new();
        let mut shutdown = Vec::new();
        for (id, rec) in &self.sims {
            if rec.holding.foreign {
                continue;
            }
            if rec.holding.mirroring {
                if let Some(hidden) = rec.hidden_since_ms {
                    if now.saturating_sub(hidden) >= pause_ms {
                        pause.push(id.clone());
                    }
                }
            }
            if matches!(
                rec.holding.lifecycle,
                DeviceLifecycle::Booted | DeviceLifecycle::Mirroring
            ) && !rec.holding.pane_visible
            {
                if let Some(deadline) = rec.holding.idle_deadline_ms {
                    if now >= deadline {
                        shutdown.push(id.clone());
                    }
                }
            }
        }
        for id in pause {
            if let Some(rec) = self.sims.get_mut(&id) {
                rec.holding.mirroring = false;
                rec.holding.lifecycle = DeviceLifecycle::Booted;
            }
        }
        // Idle shutdown is scheduled (countdown already visible). Tests call
        // `force_idle_shutdown` after the deadline; production commands do the
        // simctl shutdown when the countdown elapses without Keep.
        let _ = (shutdown, idle_ms);
        self.reap_hidden_webviews();
        self.reap_idle_servers();
    }

    pub fn force_idle_shutdowns(&mut self, simctl: &dyn Simctl) {
        let now = self.clock.now();
        let ids: Vec<String> = self
            .sims
            .iter()
            .filter(|(_, rec)| {
                !rec.holding.foreign
                    && !rec.holding.pane_visible
                    && rec.holding.idle_deadline_ms.is_some_and(|d| now >= d)
                    && matches!(
                        rec.holding.lifecycle,
                        DeviceLifecycle::Booted | DeviceLifecycle::Mirroring
                    )
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let _ = self.shutdown_sim(&id, simctl);
        }
    }

    fn reap_hidden_webviews(&mut self) {
        let now = self.clock.now();
        let cap = self.policy.hidden_webview_cap as usize;
        let ttl = self.policy.hidden_webview_ttl_ms;
        self.webviews.retain(|_, wv| {
            if !wv.hidden {
                return true;
            }
            match wv.hidden_since_ms {
                Some(since) if now.saturating_sub(since) >= ttl => false,
                _ => true,
            }
        });
        let mut hidden: Vec<(String, u64)> = self
            .webviews
            .iter()
            .filter(|(_, w)| w.hidden)
            .map(|(id, w)| (id.clone(), w.hidden_since_ms.unwrap_or(0)))
            .collect();
        if hidden.len() > cap {
            hidden.sort_by_key(|(_, since)| *since);
            let drop_n = hidden.len() - cap;
            for (id, _) in hidden.into_iter().take(drop_n) {
                self.webviews.remove(&id);
            }
        }
    }

    fn reap_idle_servers(&mut self) {
        let now = self.clock.now();
        for rec in self.servers.values_mut() {
            let idle = !rec.attached_webview
                && rec.agent_activity_ms == 0
                && now.saturating_sub(rec.last_request_ms) > 0;
            if idle {
                if let Some(deadline) = rec.holding.idle_deadline_ms {
                    if now + 60_000 >= deadline && rec.holding.face == DevServerFace::Running {
                        rec.holding.face = DevServerFace::IdleStop;
                    }
                }
            }
        }
        let stop: Vec<String> = self
            .servers
            .iter()
            .filter(|(_, rec)| {
                !rec.attached_webview && rec.holding.idle_deadline_ms.is_some_and(|d| now >= d)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in stop {
            self.servers.remove(&id);
        }
    }

    fn booted_count(&self) -> u32 {
        self.sims
            .values()
            .filter(|r| {
                !r.holding.foreign
                    && matches!(
                        r.holding.lifecycle,
                        DeviceLifecycle::Booted | DeviceLifecycle::Mirroring
                    )
            })
            .count() as u32
    }

    fn stream_count_excluding(&self, channel_id: &str) -> u32 {
        self.sims
            .values()
            .filter(|r| r.holding.mirroring && r.holding.channel_id != channel_id)
            .count() as u32
    }

    fn managed_server_count(&self) -> u32 {
        self.servers.len() as u32
    }

    fn propose_sim_eviction(
        &self,
        incoming_channel_id: &str,
        incoming_name: Option<&str>,
    ) -> Option<CapConflict> {
        let victim = self.lru_sim_victim()?;
        let rec = self.sims.get(&victim)?;
        if rec.holding.mirroring && rec.holding.pane_visible {
            return None;
        }
        let idle_ms = self.clock.now().saturating_sub(rec.holding.last_used_ms);
        Some(CapConflict {
            kind: "sim".into(),
            victim_channel_id: rec.holding.channel_id.clone(),
            victim_name: rec
                .holding
                .channel_name
                .clone()
                .unwrap_or_else(|| rec.holding.device_name.clone()),
            incoming_channel_id: incoming_channel_id.to_string(),
            incoming_name: incoming_name
                .map(str::to_string)
                .unwrap_or_else(|| incoming_channel_id.to_string()),
            idle_ms,
            keep_token: rec.holding.channel_id.clone(),
        })
    }

    /// LRU among Crew-booted sims that are **not** the visible mirror.
    fn lru_sim_victim(&self) -> Option<String> {
        self.sims
            .iter()
            .filter(|(_, rec)| {
                !rec.holding.foreign
                    && matches!(
                        rec.holding.lifecycle,
                        DeviceLifecycle::Booted | DeviceLifecycle::Mirroring
                    )
                    && !(rec.holding.mirroring && rec.holding.pane_visible)
            })
            .min_by_key(|(_, rec)| rec.holding.last_used_ms)
            .map(|(id, _)| id.clone())
    }

    fn lru_server_victim(&self) -> Option<String> {
        self.servers
            .iter()
            .filter(|(_, rec)| !rec.attached_webview)
            .min_by_key(|(_, rec)| rec.last_request_ms)
            .map(|(id, _)| id.clone())
    }
}

impl Default for ResourceGovernor {
    fn default() -> Self {
        Self::new()
    }
}

fn display_device_type(identifier: &str) -> String {
    if identifier.is_empty() {
        return DEFAULT_DEVICE_TYPE.to_string();
    }
    if !identifier.contains('.') {
        return identifier.to_string();
    }
    identifier
        .rsplit('.')
        .next()
        .unwrap_or(DEFAULT_DEVICE_TYPE)
        .replace('-', " ")
}

#[cfg(test)]
#[path = "governor_tests.rs"]
mod tests;
