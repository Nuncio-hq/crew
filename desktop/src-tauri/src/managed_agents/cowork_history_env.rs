//! Inject Cowork history directory so the harness and Desktop share one root.

use std::path::Path;
use std::process::Command;

use buzz_cowork::HISTORY_DIR_ENV;
use tauri::{AppHandle, Manager};

/// `<app-data>/cowork-history`
pub fn cowork_history_dir(app_data_dir: &Path) -> std::path::PathBuf {
    app_data_dir.join("cowork-history")
}

pub fn apply_cowork_history_env(command: &mut Command, app_data_dir: &Path) {
    command.env(HISTORY_DIR_ENV, cowork_history_dir(app_data_dir));
}

pub fn apply_cowork_history_env_from_app(command: &mut Command, app: &AppHandle) {
    if let Ok(data_dir) = app.path().app_data_dir() {
        apply_cowork_history_env(command, &data_dir);
        // Tool Pane (#196): same app-data dir as Cowork history (D-058).
        crate::resource_governor::apply_snapshot_env(command, &data_dir);
    }
}
