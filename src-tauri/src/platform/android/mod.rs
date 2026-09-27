use std::{path::PathBuf, sync::OnceLock};
pub(crate) mod diagram;
pub(crate) mod document_identity;
pub(crate) mod export;
pub(crate) mod export_resources;
pub(crate) mod workspace;
use tauri::Manager;

use crate::{models::app_error::AppError, services::startup_diagnostics_service::StartupState};

static DATA_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // Android has no desktop HOME/XDG directory. Resolve storage through the Activity.
    let directory = app.path().app_data_dir()?;
    std::fs::create_dir_all(&directory)?;
    DATA_DIRECTORY.set(directory).map_err(|_| {
        std::io::Error::other("Android application data directory was already initialized")
    })?;
    let state = StartupState::new(env!("CARGO_PKG_VERSION"), tauri::webview_version().ok());
    let _ = state.record_native("nativeProcess", "started", None);
    let _ = state.record_native("tauriSetup", "succeeded", None);
    app.manage(state);
    Ok(())
}

pub(crate) fn app_data_dir() -> Result<PathBuf, AppError> {
    DATA_DIRECTORY
        .get()
        .cloned()
        .ok_or_else(|| AppError::new("APP_DATA_UNAVAILABLE", "Android 应用数据目录尚未初始化"))
}
