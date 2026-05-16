use std::{env, fs, path::PathBuf, process::Command};

use crate::{
    models::{
        app_error::AppError,
        document::{DocumentDto, FileMetadataDto},
    },
    services::{
        file_service,
        markdown_service,
        recent_files_service,
        settings_service,
    },
};

#[tauri::command]
pub fn open_markdown_file(path: String) -> Result<DocumentDto, AppError> {
    let document = file_service::read_markdown_file(&path)?;
    let settings = settings_service::load_settings().unwrap_or_default();
    recent_files_service::add_recent_file(&path, settings.recent_files_limit)?;
    Ok(document)
}

#[tauri::command]
pub fn save_markdown_file(path: String, content: String) -> Result<DocumentDto, AppError> {
    let document = file_service::save_markdown_file(&path, &content)?;
    let settings = settings_service::load_settings().unwrap_or_default();
    recent_files_service::add_recent_file(&path, settings.recent_files_limit)?;
    Ok(document)
}

#[tauri::command]
pub fn export_html_file(path: String, title: String, content: String) -> Result<(), AppError> {
    let html = markdown_service::render_standalone_html(&title, &content)?;
    fs::write(&path, html).map_err(|err| AppError::file_write_failed(&path, err))
}

#[tauri::command]
pub fn get_file_metadata(path: String) -> Result<FileMetadataDto, AppError> {
    file_service::get_file_metadata(&path)
}

#[tauri::command]
pub fn get_startup_file_arg() -> Result<Option<String>, AppError> {
    for arg in env::args().skip(1) {
        let path = PathBuf::from(&arg);
        if path.exists() && file_service::ensure_allowed_file(&path).is_ok() {
            return Ok(Some(path.to_string_lossy().to_string()));
        }
    }

    Ok(None)
}

#[tauri::command]
pub fn show_in_file_manager(path: String) -> Result<(), AppError> {
    let path_buf = PathBuf::from(&path);
    let explorer_target = if path_buf.exists() {
        if path_buf.is_file() {
            format!("/select,{}", path_buf.to_string_lossy())
        } else {
            path_buf.to_string_lossy().to_string()
        }
    } else if let Some(parent) = path_buf.parent().filter(|parent| parent.exists()) {
        parent.to_string_lossy().to_string()
    } else {
        return Err(AppError::file_not_found(&path));
    };

    Command::new("explorer.exe")
        .arg(explorer_target)
        .spawn()
        .map_err(|err| AppError::new("OPEN_FILE_MANAGER_FAILED", format!("打开文件管理器失败：{err}")))?;

    Ok(())
}
