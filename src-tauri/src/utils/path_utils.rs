use std::{fs, path::PathBuf};

use crate::models::app_error::AppError;

pub fn app_data_dir() -> Result<PathBuf, AppError> {
    let base = dirs::data_dir()
        .ok_or_else(|| AppError::new("APP_DATA_UNAVAILABLE", "无法找到 Windows 应用数据目录"))?;
    let dir = base.join("MarkLite");
    fs::create_dir_all(&dir).map_err(|err| AppError::settings_write_failed(err))?;
    Ok(dir)
}

pub fn settings_path() -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("settings.json"))
}

pub fn recent_files_path() -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("recent-files.json"))
}

pub fn title_from_path(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled")
        .to_string()
}
