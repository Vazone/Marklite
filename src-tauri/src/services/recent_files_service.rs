use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use chrono::Utc;

use crate::{
    models::{app_error::AppError, document::RecentFileDto},
    utils::{
        atomic_write::{atomic_write, recover_atomic_write},
        path_utils::{canonicalize_path, recent_files_path, title_from_path},
    },
};

static RECENT_FILES_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn get_recent_files() -> Result<Vec<RecentFileDto>, AppError> {
    let _guard = recent_files_lock();
    get_recent_files_from(&recent_files_path()?)
}

pub fn add_recent_file(path: &str, limit: usize) -> Result<Vec<RecentFileDto>, AppError> {
    add_recent_file_to(&recent_files_path()?, path, limit)
}

pub fn remove_recent_file(path: &str) -> Result<Vec<RecentFileDto>, AppError> {
    let _guard = recent_files_lock();
    let storage_path = recent_files_path()?;
    let mut files = get_recent_files_from(&storage_path)?;
    files.retain(|file| file.path != path);
    save_recent_files_to(&storage_path, &files)?;
    Ok(files)
}

pub fn clear_missing_recent_files() -> Result<Vec<RecentFileDto>, AppError> {
    let _guard = recent_files_lock();
    let path = recent_files_path()?;
    let mut files = get_recent_files_from(&path)?;
    files.retain(|file| PathBuf::from(&file.path).exists());
    save_recent_files_to(&path, &files)?;
    Ok(files)
}

fn get_recent_files_from(path: &Path) -> Result<Vec<RecentFileDto>, AppError> {
    recover_atomic_write(path).map_err(AppError::recent_files_read_failed)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(path).map_err(AppError::recent_files_read_failed)?;
    let mut files = match serde_json::from_str::<Vec<RecentFileDto>>(&raw) {
        Ok(files) => files,
        Err(error) => {
            backup_corrupt_file(path)?;
            save_recent_files_to(path, &[])?;
            return Err(AppError::recent_files_read_failed(error));
        }
    };
    files.sort_by(|a, b| b.last_opened_at.cmp(&a.last_opened_at));
    let mut seen = HashSet::new();
    Ok(files
        .into_iter()
        .filter_map(|mut file| {
            let path = canonicalize_path(Path::new(&file.path))
                .unwrap_or_else(|_| PathBuf::from(&file.path));
            file.path = path.to_string_lossy().to_string();
            file.title = title_from_path(&path);
            seen.insert(path_identity(&file.path)).then_some(file)
        })
        .collect())
}

fn add_recent_file_to(
    storage_path: &Path,
    document_path: &str,
    limit: usize,
) -> Result<Vec<RecentFileDto>, AppError> {
    let _guard = recent_files_lock();
    let path_buf = canonicalize_path(Path::new(document_path))
        .unwrap_or_else(|_| PathBuf::from(document_path));
    let normalized = path_buf.to_string_lossy().to_string();
    let mut files = get_recent_files_from(storage_path)?;
    files.retain(|file| file.path != normalized);
    files.insert(
        0,
        RecentFileDto {
            title: title_from_path(&path_buf),
            path: normalized,
            last_opened_at: Utc::now().to_rfc3339(),
        },
    );
    files.truncate(limit);
    save_recent_files_to(storage_path, &files)?;
    Ok(files)
}

fn save_recent_files_to(path: &Path, files: &[RecentFileDto]) -> Result<(), AppError> {
    let content =
        serde_json::to_string_pretty(files).map_err(AppError::recent_files_write_failed)?;
    atomic_write(path, content.as_bytes()).map_err(AppError::recent_files_write_failed)
}

fn backup_corrupt_file(path: &Path) -> Result<(), AppError> {
    let timestamp = Utc::now().format("%Y%m%d%H%M%S%3f");
    let backup_path = path.with_extension(format!("json.corrupt-{timestamp}"));
    fs::rename(path, backup_path).map_err(AppError::recent_files_write_failed)
}

fn recent_files_lock() -> std::sync::MutexGuard<'static, ()> {
    RECENT_FILES_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn path_identity(path: &str) -> String {
    #[cfg(windows)]
    {
        path.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}
