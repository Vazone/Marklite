use std::{fs, path::PathBuf};

use chrono::Utc;

use crate::{
    models::{app_error::AppError, document::RecentFileDto},
    utils::{
        path_utils::{recent_files_path, title_from_path},
    },
};

pub fn get_recent_files() -> Result<Vec<RecentFileDto>, AppError> {
    let path = recent_files_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(&path).map_err(AppError::settings_read_failed)?;
    let mut files = serde_json::from_str::<Vec<RecentFileDto>>(&raw).unwrap_or_default();
    files.sort_by(|a, b| b.last_opened_at.cmp(&a.last_opened_at));
    Ok(files)
}

pub fn add_recent_file(path: &str, limit: usize) -> Result<Vec<RecentFileDto>, AppError> {
    let path_buf = PathBuf::from(path);
    let normalized = path_buf.to_string_lossy().to_string();
    let mut files = get_recent_files()?;
    files.retain(|file| file.path != normalized);
    files.insert(
        0,
        RecentFileDto {
            title: title_from_path(&path_buf),
            path: normalized,
            last_opened_at: Utc::now().to_rfc3339(),
        },
    );
    files.truncate(limit.max(1));
    save_recent_files(&files)?;
    Ok(files)
}

pub fn remove_recent_file(path: &str) -> Result<Vec<RecentFileDto>, AppError> {
    let mut files = get_recent_files()?;
    files.retain(|file| file.path != path);
    save_recent_files(&files)?;
    Ok(files)
}

pub fn clear_missing_recent_files() -> Result<Vec<RecentFileDto>, AppError> {
    let mut files = get_recent_files()?;
    files.retain(|file| PathBuf::from(&file.path).exists());
    save_recent_files(&files)?;
    Ok(files)
}

fn save_recent_files(files: &[RecentFileDto]) -> Result<(), AppError> {
    let path = recent_files_path()?;
    let content = serde_json::to_string_pretty(files).map_err(AppError::settings_write_failed)?;
    fs::write(path, content).map_err(AppError::settings_write_failed)
}
