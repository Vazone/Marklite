use std::{fs, path::PathBuf};

use crate::models::app_error::AppError;

pub fn app_data_dir() -> Result<PathBuf, AppError> {
    let base = dirs::data_dir()
        .ok_or_else(|| AppError::new("APP_DATA_UNAVAILABLE", "无法找到应用数据目录"))?;
    let dir = base.join("MarkLite");
    fs::create_dir_all(&dir).map_err(AppError::settings_write_failed)?;
    Ok(dir)
}

pub fn settings_path() -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("settings.json"))
}

pub fn recent_files_path() -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("recent-files.json"))
}

pub fn session_path() -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("session.json"))
}

pub fn startup_diagnostics_dir() -> Result<PathBuf, AppError> {
    let dir = app_data_dir()?.join("diagnostics").join("startup");
    fs::create_dir_all(&dir).map_err(AppError::settings_write_failed)?;
    Ok(dir)
}

pub fn title_from_path(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

pub fn canonicalize_path(path: &std::path::Path) -> std::io::Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        let value = canonical.to_string_lossy();
        if let Some(path) = value.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{path}")));
        }
        if let Some(path) = value.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(path));
        }
    }
    Ok(canonical)
}
