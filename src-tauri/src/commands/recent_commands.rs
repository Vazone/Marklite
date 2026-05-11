use crate::{
    models::{app_error::AppError, document::RecentFileDto},
    services::recent_files_service,
};

#[tauri::command]
pub fn get_recent_files() -> Result<Vec<RecentFileDto>, AppError> {
    recent_files_service::get_recent_files()
}

#[tauri::command]
pub fn remove_recent_file(path: String) -> Result<Vec<RecentFileDto>, AppError> {
    recent_files_service::remove_recent_file(&path)
}

#[tauri::command]
pub fn clear_missing_recent_files() -> Result<Vec<RecentFileDto>, AppError> {
    recent_files_service::clear_missing_recent_files()
}
