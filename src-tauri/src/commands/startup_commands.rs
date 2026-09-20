use std::path::Path;

use tauri::State;

use crate::{
    commands::background::run_background,
    models::{
        app_error::AppError,
        startup::{FrontendStartupEventDto, StartupDiagnosticsExportDto, StartupReadyDto},
    },
    services::{
        startup_diagnostics_service::StartupState, webview_process_service::clear_recovery_marker,
    },
};

#[tauri::command]
pub async fn record_frontend_startup_event(
    event: FrontendStartupEventDto,
    state: State<'_, StartupState>,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    run_background("启动诊断记录", move || state.record_frontend(event)).await
}

#[tauri::command]
pub async fn mark_frontend_ready(
    elapsed_ms: u64,
    state: State<'_, StartupState>,
) -> Result<StartupReadyDto, AppError> {
    let state = state.inner().clone();
    let ready = run_background("启动就绪记录", move || state.mark_ready(elapsed_ms)).await?;
    clear_recovery_marker();
    Ok(ready)
}

#[tauri::command]
pub async fn clear_startup_diagnostics(state: State<'_, StartupState>) -> Result<(), AppError> {
    let state = state.inner().clone();
    run_background("清除启动诊断", move || state.clear()).await
}

#[tauri::command]
pub async fn export_startup_diagnostics(
    path: String,
    state: State<'_, StartupState>,
) -> Result<StartupDiagnosticsExportDto, AppError> {
    let state = state.inner().clone();
    run_background("导出启动诊断", move || state.export(Path::new(&path))).await
}
