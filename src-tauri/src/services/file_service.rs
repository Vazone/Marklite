use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Utc};

use crate::{
    models::{app_error::AppError, document::DocumentDto},
    utils::{
        atomic_write::{atomic_write, recover_atomic_write},
        path_utils::{canonicalize_path, title_from_path},
    },
};

pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

pub fn read_markdown_file(path: &str) -> Result<DocumentDto, AppError> {
    let path_buf = PathBuf::from(path);
    ensure_allowed_file(&path_buf)?;
    if !path_buf.is_absolute() {
        return Err(AppError::invalid_file_path(path));
    }
    recover_atomic_write(&path_buf).map_err(|err| AppError::file_read_failed(path, err))?;

    if !path_buf.exists() {
        return Err(AppError::file_not_found(path));
    }

    let canonical =
        canonicalize_path(&path_buf).map_err(|err| AppError::file_read_failed(path, err))?;
    ensure_allowed_file(&canonical)?;
    let metadata = fs::metadata(&canonical).map_err(|err| AppError::file_read_failed(path, err))?;
    if !metadata.is_file() {
        return Err(AppError::invalid_file_target(path));
    }
    if metadata.len() > MAX_FILE_SIZE {
        return Err(AppError::file_too_large(path, "打开"));
    }

    let content =
        fs::read_to_string(&canonical).map_err(|err| AppError::file_read_failed(path, err))?;

    Ok(DocumentDto {
        path: Some(canonical.to_string_lossy().to_string()),
        title: title_from_path(&canonical),
        content,
        is_dirty: false,
        last_saved_at: metadata.modified().ok().map(system_time_to_rfc3339),
        file_size: Some(metadata.len()),
    })
}

pub fn save_markdown_file(path: &str, content: &str) -> Result<DocumentDto, AppError> {
    let path_buf = PathBuf::from(path);
    ensure_allowed_file(&path_buf)?;
    if !path_buf.is_absolute() {
        return Err(AppError::invalid_file_path(path));
    }
    if content.len() as u64 > MAX_FILE_SIZE {
        return Err(AppError::file_too_large(path, "保存"));
    }

    recover_atomic_write(&path_buf).map_err(|err| AppError::file_write_failed(path, err))?;
    if path_buf.exists() {
        let metadata =
            fs::metadata(&path_buf).map_err(|err| AppError::file_write_failed(path, err))?;
        if !metadata.is_file() {
            return Err(AppError::invalid_file_target(path));
        }
    }

    if let Some(parent) = path_buf
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| AppError::file_write_failed(path, err))?;
    }

    let write_path = if path_buf.exists() {
        canonicalize_path(&path_buf).map_err(|err| AppError::file_write_failed(path, err))?
    } else {
        let parent = path_buf
            .parent()
            .ok_or_else(|| AppError::invalid_file_path(path))?;
        let canonical_parent =
            canonicalize_path(parent).map_err(|err| AppError::file_write_failed(path, err))?;
        canonical_parent.join(
            path_buf
                .file_name()
                .ok_or_else(|| AppError::invalid_file_path(path))?,
        )
    };
    ensure_allowed_file(&write_path)?;
    atomic_write(&write_path, content.as_bytes())
        .map_err(|err| AppError::file_write_failed(path, err))?;
    let canonical =
        canonicalize_path(&write_path).map_err(|err| AppError::file_write_failed(path, err))?;

    Ok(DocumentDto {
        path: Some(canonical.to_string_lossy().to_string()),
        title: title_from_path(&canonical),
        content: content.to_string(),
        is_dirty: false,
        last_saved_at: Some(Utc::now().to_rfc3339()),
        file_size: Some(content.len() as u64),
    })
}

pub fn ensure_allowed_file(path: &Path) -> Result<(), AppError> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "md" | "markdown" | "txt" => Ok(()),
        _ => Err(AppError::invalid_file_type(&path.to_string_lossy())),
    }
}

fn system_time_to_rfc3339(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.to_rfc3339()
}
