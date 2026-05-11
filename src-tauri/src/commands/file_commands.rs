use std::{env, fs, path::PathBuf};

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
