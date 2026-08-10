use std::path::Path;

use tauri::State;

use crate::{
    models::{
        app_error::AppError,
        startup::{FrontendStartupEventDto, StartupDiagnosticsExportDto, StartupReadyDto},
    },
    services::startup_diagnostics_service::StartupState,
};

#[tauri::command]
pub fn record_frontend_startup_event(
    event: FrontendStartupEventDto,
    state: State<'_, StartupState>,
) -> Result<(), AppError> {
    state.record_frontend(event)
}

#[tauri::command]
pub fn mark_frontend_ready(
    elapsed_ms: u64,
    state: State<'_, StartupState>,
) -> Result<StartupReadyDto, AppError> {
    state.mark_ready(elapsed_ms)
}

#[tauri::command]
pub fn clear_startup_diagnostics(state: State<'_, StartupState>) -> Result<(), AppError> {
    state.clear()
}

#[tauri::command]
pub fn export_startup_diagnostics(
    path: String,
    state: State<'_, StartupState>,
) -> Result<StartupDiagnosticsExportDto, AppError> {
    state.export(Path::new(&path))
}
