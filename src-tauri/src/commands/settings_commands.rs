use crate::{
    models::{app_error::AppError, settings::AppSettings},
    services::settings_service,
};

#[tauri::command]
pub fn get_settings() -> Result<AppSettings, AppError> {
    match settings_service::load_settings() {
        Ok(settings) => Ok(settings),
        Err(_) => Ok(AppSettings::default()),
    }
}

#[tauri::command]
pub fn update_settings(settings: AppSettings) -> Result<AppSettings, AppError> {
    settings_service::save_settings(&settings)
}

#[tauri::command]
pub fn reset_settings() -> Result<AppSettings, AppError> {
    settings_service::reset_settings()
}
