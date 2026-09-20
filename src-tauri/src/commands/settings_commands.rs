use crate::{
    commands::background::run_background,
    models::{app_error::AppError, settings::AppSettings},
    services::settings_service,
};

#[tauri::command]
pub async fn get_settings() -> Result<AppSettings, AppError> {
    run_background("读取设置", settings_service::load_settings).await
}

#[tauri::command]
pub async fn update_settings(settings: AppSettings) -> Result<AppSettings, AppError> {
    run_background("保存设置", move || {
        settings_service::save_settings(&settings)
    })
    .await
}

#[tauri::command]
pub async fn reset_settings() -> Result<AppSettings, AppError> {
    run_background("重置设置", settings_service::reset_settings).await
}
