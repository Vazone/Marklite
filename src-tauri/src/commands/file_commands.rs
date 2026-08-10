use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    models::{
        app_error::AppError,
        document::{DocumentDto, DocumentOperationDto},
    },
    services::{file_service, markdown_service, recent_files_service, settings_service},
    utils::{atomic_write::atomic_write, path_utils::canonicalize_path},
};

#[tauri::command]
pub fn open_markdown_file(path: String) -> Result<DocumentOperationDto, AppError> {
    let document = file_service::read_markdown_file(&path)?;
    let recent_path = document.path.as_deref().unwrap_or(&path).to_string();
    Ok(complete_document_operation(
        document,
        update_recent(&recent_path),
    ))
}

#[tauri::command]
pub fn save_markdown_file(path: String, content: String) -> Result<DocumentOperationDto, AppError> {
    let document = file_service::save_markdown_file(&path, &content)?;
    let recent_path = document.path.as_deref().unwrap_or(&path).to_string();
    Ok(complete_document_operation(
        document,
        update_recent(&recent_path),
    ))
}

#[tauri::command]
pub fn export_html_file(path: String, title: String, content: String) -> Result<(), AppError> {
    let html = markdown_service::render_standalone_html(&title, &content)?;
    let path_buf = Path::new(&path);
    if !path_buf.is_absolute()
        || !path_buf
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("html"))
    {
        return Err(AppError::new(
            "INVALID_EXPORT_TARGET",
            "HTML 导出目标必须是绝对 .html 文件路径",
        ));
    }
    if path_buf.exists() && !path_buf.is_file() {
        return Err(AppError::invalid_file_target(&path));
    }
    if let Some(parent) = path_buf
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| AppError::file_write_failed(&path, err))?;
    }
    atomic_write(path_buf, html.as_bytes()).map_err(|err| AppError::file_write_failed(&path, err))
}

fn update_recent(path: &str) -> Result<(), AppError> {
    let settings = settings_service::load_settings()?;
    recent_files_service::add_recent_file(path, settings.recent_files_limit).map(|_| ())
}

fn complete_document_operation(
    document: DocumentDto,
    auxiliary_result: Result<(), AppError>,
) -> DocumentOperationDto {
    DocumentOperationDto {
        document,
        auxiliary_error: auxiliary_result.err(),
    }
}

#[tauri::command]
pub fn get_startup_file_arg() -> Result<Option<String>, AppError> {
    let cwd = env::current_dir().map_err(|error| AppError::file_read_failed(".", error))?;
    for arg in env::args().skip(1) {
        if let Some(path) = canonical_startup_file(Path::new(&arg), &cwd)? {
            return Ok(Some(path.to_string_lossy().to_string()));
        }
    }

    Ok(None)
}

#[tauri::command]
pub fn show_in_file_manager(path: String) -> Result<(), AppError> {
    let explorer_target = file_manager_target(&path)?;

    Command::new("explorer.exe")
        .arg(explorer_target)
        .spawn()
        .map_err(|err| {
            AppError::new(
                "OPEN_FILE_MANAGER_FAILED",
                format!("打开文件管理器失败：{err}"),
            )
        })?;

    Ok(())
}

fn canonical_startup_file(path: &Path, cwd: &Path) -> Result<Option<PathBuf>, AppError> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    if !candidate.is_file() || file_service::ensure_allowed_file(&candidate).is_err() {
        return Ok(None);
    }
    canonicalize_path(&candidate)
        .map(Some)
        .map_err(|error| AppError::file_read_failed(&candidate.to_string_lossy(), error))
}

fn file_manager_target(path: &str) -> Result<String, AppError> {
    let path_buf = PathBuf::from(path);
    if !path_buf.is_absolute() {
        return Err(AppError::invalid_file_path(path));
    }
    file_service::ensure_allowed_file(&path_buf)?;
    if path_buf.exists() {
        if !path_buf.is_file() {
            return Err(AppError::invalid_file_target(path));
        }
        let canonical = canonicalize_path(&path_buf)
            .map_err(|error| AppError::file_read_failed(path, error))?;
        Ok(format!("/select,{}", canonical.to_string_lossy()))
    } else if let Some(parent) = path_buf.parent().filter(|parent| parent.exists()) {
        let canonical_parent =
            canonicalize_path(parent).map_err(|error| AppError::file_read_failed(path, error))?;
        Ok(canonical_parent.to_string_lossy().to_string())
    } else {
        Err(AppError::file_not_found(path))
    }
}
