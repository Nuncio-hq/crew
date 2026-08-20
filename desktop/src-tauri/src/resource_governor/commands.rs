//! Tauri commands for the Resource Governor and Tool Pane.

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use super::bridge::{
    bridge_key_args, bridge_press_args, bridge_swipe_args, bridge_tap_args, bridge_type_args,
    discover_sim_bridge, BridgeAvailability, ScreenSize,
};
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
    if let Ok(dir) = app.path().app_data_dir() {
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
    run_bridge_command(&udid, |binary, id| {
        bridge_tap_args(binary, id, point.x, point.y, ScreenSize::DEFAULT)
    })
}

#[tauri::command]
pub fn sim_swipe(udid: String, from: HidPoint, to: HidPoint) -> Result<(), String> {
    run_bridge_command(&udid, |binary, id| {
        bridge_swipe_args(
            binary,
            id,
            (from.x, from.y),
            (to.x, to.y),
            ScreenSize::DEFAULT,
        )
    })
}

/// `baguette` has no scroll primitive; a vertical swipe across the Sim tab's
/// default bezel (`ScreenSize::DEFAULT`, matching the 390×844 space
/// `SimTab.tsx` normalizes pointer events into) is the closest equivalent.
#[tauri::command]
pub fn sim_scroll(udid: String, delta_y: f64) -> Result<(), String> {
    let (from, to) = scroll_delta_to_swipe(delta_y, ScreenSize::DEFAULT);
    run_bridge_command(&udid, |binary, id| {
        bridge_swipe_args(binary, id, from, to, ScreenSize::DEFAULT)
    })
}

/// Maps a wheel/trackpad `delta_y` onto a vertical swipe centered on the
/// screen, clamped so both endpoints stay on-screen even for very large
/// deltas.
fn scroll_delta_to_swipe(delta_y: f64, screen: ScreenSize) -> ((f64, f64), (f64, f64)) {
    let mid_x = screen.width / 2.0;
    let mid_y = screen.height / 2.0;
    let from = (mid_x, mid_y);
    let to = (mid_x, (mid_y - delta_y).clamp(0.0, screen.height));
    (from, to)
}

#[tauri::command]
pub fn sim_key(udid: String, key: String) -> Result<(), String> {
    run_bridge_command(&udid, |binary, id| bridge_key_args(binary, id, &key))
}

#[tauri::command]
pub fn sim_text(udid: String, text: String) -> Result<(), String> {
    run_bridge_command(&udid, |binary, id| bridge_type_args(binary, id, &text))
}

#[tauri::command]
pub fn sim_home(udid: String) -> Result<(), String> {
    run_bridge_command(&udid, |binary, id| bridge_press_args(binary, id, "home"))
}

/// `baguette` has no headless device-rotation command — return an explicit
/// error instead of issuing a subcommand that does not exist.
#[tauri::command]
pub fn sim_rotate(udid: String) -> Result<(), String> {
    let _ = udid;
    Err("sim_rotate is not supported: the sim bridge has no device-rotation command".into())
}

#[tauri::command]
pub fn sim_screenshot_png(udid: String) -> Result<Vec<u8>, String> {
    let _ = udid;
    Ok(super::mjpeg::placeholder_jpeg().to_vec())
}

fn run_bridge_command(
    udid: &str,
    args: impl FnOnce(&str, &str) -> Vec<String>,
) -> Result<(), String> {
    let availability = discover_sim_bridge();
    let BridgeAvailability::Available { binary, path } = availability else {
        return Err("sim bridge is not installed".into());
    };
    let argv = args(&binary, udid);
    let output = std::process::Command::new(&path)
        .args(&argv)
        .output()
        .map_err(|e| format!("sim bridge: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "sim bridge failed: {}",
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
    // `add_child` is on `Window` (unstable), not `WebviewWindow`.
    let Some(window) = app.get_window("main") else {
        return Err("main window missing".into());
    };
    let label = window_label(channel_id);
    if app.get_webview(&label).is_some() {
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
        if let Some(child) = app.get_webview(&label) {
            let _ = child.set_position(tauri::LogicalPosition::new(x, y));
            let _ = child.set_size(tauri::LogicalSize::new(width.max(120.0), height.max(80.0)));
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
    #[cfg(target_os = "macos")]
    if let Some(child) = app.get_webview(&label) {
        let _ = child.hide();
    }
    emit_status(&app, &handle);
    Ok(())
}

#[tauri::command]
pub fn browser_devtools(channel_id: String, app: AppHandle) -> Result<(), String> {
    let label = window_label(&channel_id);
    #[cfg(debug_assertions)]
    {
        if let Some(window) = app.get_webview_window(&label) {
            window.open_devtools();
            return Ok(());
        }
        if let Some(webview) = app.get_webview(&label) {
            webview.open_devtools();
            return Ok(());
        }
        Err("browser window is not open".into())
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = (label, app);
        Err("devtools are unavailable in this build".into())
    }
}

/// Injects history/reload JS into the browser webview for `channel_id`.
///
/// Tauri's `WebviewWindow`/`Webview` have no native back/forward/reload API,
/// so toolbar history chrome drives the loaded page's own `history`/`location`
/// the same way `eval` is already used for the agent-control bridge
/// (`agent_control/live.rs`).
fn eval_browser_js(channel_id: &str, app: &AppHandle, js: &str) -> Result<(), String> {
    let label = window_label(channel_id);
    if let Some(window) = app.get_webview_window(&label) {
        return window.eval(js).map_err(|e| e.to_string());
    }
    #[cfg(target_os = "macos")]
    if let Some(view) = app.get_webview(&label) {
        return view.eval(js).map_err(|e| e.to_string());
    }
    Err("browser window is not open".into())
}

#[tauri::command]
pub fn browser_back(channel_id: String, app: AppHandle) -> Result<(), String> {
    eval_browser_js(&channel_id, &app, "history.back()")
}

#[tauri::command]
pub fn browser_forward(channel_id: String, app: AppHandle) -> Result<(), String> {
    eval_browser_js(&channel_id, &app, "history.forward()")
}

#[tauri::command]
pub fn browser_reload(channel_id: String, app: AppHandle) -> Result<(), String> {
    eval_browser_js(&channel_id, &app, "location.reload()")
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

#[cfg(test)]
mod sim_hid_tests {
    use super::*;

    #[test]
    fn scroll_down_swipes_upward_from_center() {
        let (from, to) = scroll_delta_to_swipe(100.0, ScreenSize::DEFAULT);
        assert_eq!(from, (195.0, 422.0));
        assert_eq!(to, (195.0, 322.0));
    }

    #[test]
    fn scroll_up_swipes_downward_from_center() {
        let (from, to) = scroll_delta_to_swipe(-50.0, ScreenSize::DEFAULT);
        assert_eq!(from, (195.0, 422.0));
        assert_eq!(to, (195.0, 472.0));
    }

    #[test]
    fn scroll_clamps_extreme_delta_to_stay_on_screen() {
        let (_, to) = scroll_delta_to_swipe(10_000.0, ScreenSize::DEFAULT);
        assert_eq!(to.1, 0.0);
        let (_, to) = scroll_delta_to_swipe(-10_000.0, ScreenSize::DEFAULT);
        assert_eq!(to.1, ScreenSize::DEFAULT.height);
    }

    #[test]
    fn sim_rotate_is_an_explicit_unsupported_error() {
        let err = sim_rotate("UDID-1".into()).unwrap_err();
        assert!(err.contains("not supported"));
    }
}
