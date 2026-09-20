use crate::{
    commands::background::run_background,
    models::{app_error::AppError, recent::RecentFileDto},
    services::recent_files_service,
};

#[tauri::command]
pub async fn get_recent_files() -> Result<Vec<RecentFileDto>, AppError> {
    run_background("读取最近文件", recent_files_service::get_recent_files).await
}

#[tauri::command]
pub async fn remove_recent_file(path: String) -> Result<Vec<RecentFileDto>, AppError> {
    run_background("更新最近文件", move || {
        recent_files_service::remove_recent_file(&path)
    })
    .await
}

#[tauri::command]
pub async fn clear_missing_recent_files() -> Result<Vec<RecentFileDto>, AppError> {
    run_background(
        "清理最近文件",
        recent_files_service::clear_missing_recent_files,
    )
    .await
}
