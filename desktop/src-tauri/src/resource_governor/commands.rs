//! Tauri commands for the Resource Governor and Tool Pane.

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use super::bridge::discover_sim_bridge;
use super::browser::{backend, window_label, BrowserBackend};
use super::device::crew_device_name;
use super::mjpeg::FrameStore;
use super::policy::GovernorPolicy;
use super::simctl::RealSimctl;
use super::snapshot::{write_agent_env_snapshot, AgentChannelEnv, GovernorAgentSnapshot};
use super::types::{GovernorStatus, StopKind};
use super::ResourceGovernorHandle;

const EVENT: &str = "resource-governor";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsureDeviceInput {
    channel_id: String,
    channel_name: Option<String>,
    device_type: Option<String>,
    runtime: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HidPoint {
    x: f64,
    y: f64,
}

fn emit_status(app: &AppHandle, handle: &ResourceGovernorHandle) {
    if let Ok(gov) = handle.lock() {
        let _ = app.emit(EVENT, gov.status());
    }
}

fn persist_agent_env(app: &AppHandle, handle: &ResourceGovernorHandle) {
    let Ok(gov) = handle.lock() else {
        return;
    };
    let status = gov.status();
    let mut snap = GovernorAgentSnapshot::default();
    for sim in &status.sims {
        let url = status
            .servers
            .iter()
            .find(|s| s.channel_id == sim.channel_id)
            .and_then(|s| s.url.clone());
        snap.channels.insert(
            sim.channel_id.clone(),
            AgentChannelEnv {
                simulator_udid: sim.udid.clone(),
                dev_server_url: url,
            },
        );
    }
    drop(gov);
    if let Some(dir) = app.path().app_data_dir().ok() {
        let path = super::snapshot::snapshot_path(&dir);
        let _ = write_agent_env_snapshot(&path, &snap);
    }
}

#[tauri::command]
pub fn governor_status(
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<GovernorStatus, String> {
    Ok(handle.lock()?.status())
}

#[tauri::command]
pub fn governor_set_policy(
    policy: GovernorPolicy,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<GovernorStatus, String> {
    {
        let mut gov = handle.lock()?;
        gov.set_policy(policy);
    }
    emit_status(&app, &handle);
    Ok(handle.lock()?.status())
}

#[tauri::command]
pub fn governor_stop(
    kind: StopKind,
    id: String,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<GovernorStatus, String> {
    {
        let mut gov = handle.lock()?;
        gov.stop(kind, &id, &RealSimctl)?;
    }
    persist_agent_env(&app, &handle);
    emit_status(&app, &handle);
    Ok(handle.lock()?.status())
}

#[tauri::command]
pub fn sim_ensure_device(
    input: EnsureDeviceInput,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<GovernorStatus, String> {
    {
        let mut gov = handle.lock()?;
        gov.ensure_device(
            &input.channel_id,
            input.channel_name.as_deref(),
            input.device_type.as_deref(),
            input.runtime.as_deref(),
            &RealSimctl,
        )?;
    }
    emit_status(&app, &handle);
    Ok(handle.lock()?.status())
}

#[tauri::command]
pub fn sim_boot(
    input: EnsureDeviceInput,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<GovernorStatus, String> {
    {
        let mut gov = handle.lock()?;
        gov.boot(
            &input.channel_id,
            input.channel_name.as_deref(),
            input.device_type.as_deref(),
            input.runtime.as_deref(),
            &RealSimctl,
        )?;
    }
    persist_agent_env(&app, &handle);
    emit_status(&app, &handle);
    Ok(handle.lock()?.status())
}

#[tauri::command]
pub fn sim_shutdown(
    channel_id: String,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<GovernorStatus, String> {
    {
        let mut gov = handle.lock()?;
        gov.shutdown_sim(&channel_id, &RealSimctl)?;
    }
    persist_agent_env(&app, &handle);
    emit_status(&app, &handle);
    Ok(handle.lock()?.status())
}

#[tauri::command]
pub fn sim_erase(
    channel_id: String,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<GovernorStatus, String> {
    {
        let mut gov = handle.lock()?;
        gov.erase_sim(&channel_id, &RealSimctl)?;
    }
    emit_status(&app, &handle);
    Ok(handle.lock()?.status())
}

#[tauri::command]
pub fn sim_delete(
    channel_id: String,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<GovernorStatus, String> {
    {
        let mut gov = handle.lock()?;
        gov.delete_sim(&channel_id, &RealSimctl)?;
    }
    persist_agent_env(&app, &handle);
    emit_status(&app, &handle);
    Ok(handle.lock()?.status())
}

#[tauri::command]
pub fn sim_keep(
    channel_id: String,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<GovernorStatus, String> {
    {
        let mut gov = handle.lock()?;
        gov.keep_sim(&channel_id)?;
    }
    emit_status(&app, &handle);
    Ok(handle.lock()?.status())
}

#[tauri::command]
pub fn sim_set_pane_visible(
    channel_id: String,
    visible: bool,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<GovernorStatus, String> {
    {
        let mut gov = handle.lock()?;
        gov.set_pane_visible(&channel_id, visible)?;
    }
    emit_status(&app, &handle);
    Ok(handle.lock()?.status())
}

#[tauri::command]
pub fn sim_bridge_status() -> Result<super::types::BridgeStatus, String> {
    Ok(discover_sim_bridge().to_status())
}

#[tauri::command]
pub fn sim_mjpeg_url(udid: String, app: AppHandle) -> Result<String, String> {
    let port = app.try_state::<MjpegPort>().map(|p| p.0).unwrap_or(0);
    if port == 0 {
        return Err("mjpeg proxy is not listening".into());
    }
    Ok(format!("http://127.0.0.1:{port}/sim/{udid}/mjpeg"))
}

#[tauri::command]
pub fn sim_tap(udid: String, point: HidPoint) -> Result<(), String> {
    dispatch_hid(&udid, "tap", Some(point), None, None)
}

#[tauri::command]
pub fn sim_swipe(udid: String, from: HidPoint, to: HidPoint) -> Result<(), String> {
    dispatch_hid(&udid, "swipe", Some(from), Some(to), None)
}

#[tauri::command]
pub fn sim_scroll(udid: String, delta_y: f64) -> Result<(), String> {
    dispatch_hid(&udid, "scroll", None, None, Some(delta_y))
}

#[tauri::command]
pub fn sim_key(udid: String, key: String) -> Result<(), String> {
    dispatch_hid(&udid, "key", None, None, None).map(|_| ())?;
    let _ = (udid, key);
    Ok(())
}

#[tauri::command]
pub fn sim_text(udid: String, text: String) -> Result<(), String> {
    let _ = (udid, text);
    dispatch_hid("", "text", None, None, None)
}

#[tauri::command]
pub fn sim_home(udid: String) -> Result<(), String> {
    dispatch_hid(&udid, "home", None, None, None)
}

#[tauri::command]
pub fn sim_rotate(udid: String) -> Result<(), String> {
    dispatch_hid(&udid, "rotate", None, None, None)
}

#[tauri::command]
pub fn sim_screenshot_png(udid: String) -> Result<Vec<u8>, String> {
    let _ = udid;
    Ok(super::mjpeg::placeholder_jpeg().to_vec())
}

fn dispatch_hid(
    udid: &str,
    action: &str,
    from: Option<HidPoint>,
    to: Option<HidPoint>,
    delta: Option<f64>,
) -> Result<(), String> {
    let availability = discover_sim_bridge();
    let super::bridge::BridgeAvailability::Available { binary, path } = availability else {
        return Err("sim bridge is not installed".into());
    };
    let mut cmd = std::process::Command::new(&path);
    cmd.arg(action).arg("--udid").arg(udid);
    if let Some(p) = from {
        cmd.arg("--x")
            .arg(p.x.to_string())
            .arg("--y")
            .arg(p.y.to_string());
    }
    if let Some(p) = to {
        cmd.arg("--x2")
            .arg(p.x.to_string())
            .arg("--y2")
            .arg(p.y.to_string());
    }
    if let Some(d) = delta {
        cmd.arg("--delta").arg(d.to_string());
    }
    let _ = binary;
    let output = cmd
        .output()
        .map_err(|e| format!("sim bridge {action}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "sim bridge {action} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub struct MjpegPort(pub u16);

#[derive(Clone)]
pub struct MjpegFrames(pub FrameStore);

#[tauri::command]
pub fn browser_open(
    channel_id: String,
    url: String,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<String, String> {
    {
        let mut gov = handle.lock()?;
        gov.attach_webview(&channel_id, &url);
    }
    emit_status(&app, &handle);
    match backend() {
        BrowserBackend::Window => {
            let label = window_label(&channel_id);
            if let Some(existing) = app.get_webview_window(&label) {
                existing.show().map_err(|e| e.to_string())?;
                existing.set_focus().map_err(|e| e.to_string())?;
                return Ok(label);
            }
            let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
            WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(parsed))
                .title("Browser")
                .inner_size(960.0, 720.0)
                .build()
                .map_err(|e| e.to_string())?;
            Ok(label)
        }
        BrowserBackend::Child => {
            #[cfg(target_os = "macos")]
            {
                open_child_webview(&app, &channel_id, &url)?;
            }
            Ok(window_label(&channel_id))
        }
    }
}

#[cfg(target_os = "macos")]
fn open_child_webview(app: &AppHandle, channel_id: &str, url: &str) -> Result<(), String> {
    use tauri::webview::WebviewBuilder;
    let Some(window) = app.get_webview_window("main") else {
        return Err("main window missing".into());
    };
    let label = window_label(channel_id);
    if window.get_webview(&label).is_some() {
        return Ok(());
    }
    let parsed = url::Url::parse(url).map_err(|e| e.to_string())?;
    let builder = WebviewBuilder::new(&label, WebviewUrl::External(parsed));
    window
        .add_child(
            builder,
            tauri::LogicalPosition::new(0.0, 0.0),
            tauri::LogicalSize::new(400.0, 400.0),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn set_browser_bounds(
    channel_id: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    app: AppHandle,
) -> Result<(), String> {
    let label = window_label(&channel_id);
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_size(tauri::LogicalSize::new(width.max(120.0), height.max(80.0)));
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(main) = app.get_webview_window("main") {
            if let Some(child) = main.get_webview(&label) {
                let _ = child.set_position(tauri::LogicalPosition::new(x, y));
                let _ = child.set_size(tauri::LogicalSize::new(width.max(120.0), height.max(80.0)));
            }
        }
    }
    let _ = (x, y);
    Ok(())
}

#[tauri::command]
pub fn browser_close(
    channel_id: String,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<(), String> {
    let label = window_label(&channel_id);
    {
        let mut gov = handle.lock()?;
        gov.hide_webview(&format!("wv-{channel_id}"));
    }
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.hide();
    }
    emit_status(&app, &handle);
    Ok(())
}

#[tauri::command]
pub fn browser_devtools(channel_id: String, app: AppHandle) -> Result<(), String> {
    let label = window_label(&channel_id);
    if let Some(window) = app.get_webview_window(&label) {
        window.open_devtools();
        return Ok(());
    }
    Err("browser window is not open".into())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDevServerInput {
    channel_id: String,
    subject: String,
    command: String,
    cwd: String,
    ready_pattern: Option<String>,
}

#[tauri::command]
pub fn governor_start_dev_server(
    input: StartDevServerInput,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<GovernorStatus, String> {
    {
        let mut gov = handle.lock()?;
        gov.start_dev_server(
            &input.channel_id,
            &input.subject,
            &input.command,
            &input.cwd,
            input.ready_pattern.as_deref().unwrap_or("Local:"),
            None,
        )?;
    }
    persist_agent_env(&app, &handle);
    emit_status(&app, &handle);
    Ok(handle.lock()?.status())
}

#[tauri::command]
pub fn governor_note_server_output(
    server_id: String,
    line: String,
    ready_pattern: String,
    app: AppHandle,
    handle: State<'_, ResourceGovernorHandle>,
) -> Result<GovernorStatus, String> {
    {
        let mut gov = handle.lock()?;
        gov.note_server_output(&server_id, &line, &ready_pattern);
    }
    persist_agent_env(&app, &handle);
    emit_status(&app, &handle);
    Ok(handle.lock()?.status())
}

#[tauri::command]
pub fn crew_device_name_for(channel_id: String) -> String {
    crew_device_name(&channel_id)
}

#[tauri::command]
pub fn probe_browser_backend() -> String {
    match backend() {
        BrowserBackend::Child => "child".into(),
        BrowserBackend::Window => "window".into(),
    }
}

/// Pop-out Tool Pane as a labeled companion window (huddle precedent).
#[tauri::command]
pub fn open_tool_pane_window(channel_id: String, app: AppHandle) -> Result<(), String> {
    let compact: String = channel_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    let label = format!("tool-pane-{compact}");
    if let Some(existing) = app.get_webview_window(&label) {
        existing.show().map_err(|e| e.to_string())?;
        existing.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title("Tools")
        .inner_size(480.0, 820.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn quit_governor(app: &AppHandle) {
    if let Some(handle) = app.try_state::<ResourceGovernorHandle>() {
        if let Ok(mut gov) = handle.lock() {
            let _ = gov.quit_cleanup(&RealSimctl);
        }
    }
}

/// Bind the MJPEG proxy and start the one-second reap/reconcile loop.
pub fn start_background(app: &AppHandle) {
    let mjpeg_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let frames = mjpeg_handle.try_state::<MjpegFrames>().map(|s| s.0.clone());
        let Some(store) = frames else {
            return;
        };
        if let Ok(port) = super::mjpeg::bind_local(store).await {
            mjpeg_handle.manage(MjpegPort(port));
        }
    });
    spawn_governor_loop(app.clone());
}

/// Reap idle timers, adopt `crew-` devices, emit status. One-second loop.
pub fn spawn_governor_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            interval.tick().await;
            let Some(handle) = app.try_state::<ResourceGovernorHandle>() else {
                continue;
            };
            let status = {
                let Ok(mut gov) = handle.lock() else {
                    continue;
                };
                gov.tick_production(&RealSimctl);
                gov.status()
            };
            let _ = app.emit(EVENT, &status);
        }
    });
}
