use std::{fs, path::PathBuf};

use chrono::Utc;

use crate::{
    models::{app_error::AppError, settings::AppSettings},
    utils::path_utils::settings_path,
};

pub fn load_settings() -> Result<AppSettings, AppError> {
    let path = settings_path()?;
    if !path.exists() {
        let defaults = AppSettings::default();
        save_settings(&defaults)?;
        return Ok(defaults);
    }

    let raw = fs::read_to_string(&path).map_err(AppError::settings_read_failed)?;
    match serde_json::from_str::<AppSettings>(&raw) {
        Ok(settings) => Ok(settings),
        Err(err) => {
            backup_corrupt_file(&path)?;
            let defaults = AppSettings::default();
            save_settings(&defaults)?;
            Err(AppError::settings_read_failed(err))
        }
    }
}

pub fn save_settings(settings: &AppSettings) -> Result<AppSettings, AppError> {
    let path = settings_path()?;
    let content = serde_json::to_string_pretty(settings).map_err(AppError::settings_write_failed)?;
    fs::write(&path, content).map_err(AppError::settings_write_failed)?;
    Ok(settings.clone())
}

pub fn reset_settings() -> Result<AppSettings, AppError> {
    save_settings(&AppSettings::default())
}

fn backup_corrupt_file(path: &PathBuf) -> Result<(), AppError> {
    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    let backup_path = path.with_extension(format!("json.corrupt-{timestamp}"));
    fs::rename(path, backup_path).map_err(AppError::settings_write_failed)
}
