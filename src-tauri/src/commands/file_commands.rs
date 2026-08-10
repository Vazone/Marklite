use std::{
    env, fs,
    path::{Path, PathBuf},
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
    let target = file_manager_target(&path)?;
    let result = match target {
        FileManagerTarget::Reveal(path) => tauri_plugin_opener::reveal_item_in_dir(path),
        FileManagerTarget::OpenDirectory(path) => {
            tauri_plugin_opener::open_path(path, None::<&str>)
        }
    };

    result.map_err(|err| {
        AppError::new(
            "OPEN_FILE_MANAGER_FAILED",
            format!("在文件管理器中显示失败：{err}"),
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

#[derive(Debug, PartialEq)]
enum FileManagerTarget {
    Reveal(PathBuf),
    OpenDirectory(PathBuf),
}

fn file_manager_target(path: &str) -> Result<FileManagerTarget, AppError> {
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
        Ok(FileManagerTarget::Reveal(canonical))
    } else if let Some(parent) = path_buf.parent().filter(|parent| parent.exists()) {
        let canonical_parent =
            canonicalize_path(parent).map_err(|error| AppError::file_read_failed(path, error))?;
        Ok(FileManagerTarget::OpenDirectory(canonical_parent))
    } else {
        Err(AppError::file_not_found(path))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        canonical_startup_file, complete_document_operation, export_html_file, file_manager_target,
        FileManagerTarget,
    };
    use crate::models::{app_error::AppError, document::DocumentDto};

    #[test]
    fn preserves_the_document_when_an_auxiliary_update_fails() {
        let document = DocumentDto {
            path: Some("C:\\note.md".to_string()),
            title: "note.md".to_string(),
            content: "saved".to_string(),
            is_dirty: false,
            last_saved_at: None,
            file_size: Some(5),
        };

        let result = complete_document_operation(
            document.clone(),
            Err(AppError::recent_files_write_failed("disk full")),
        );
        let serialized = serde_json::to_value(&result).unwrap();

        assert_eq!(result.document.content, document.content);
        assert_eq!(
            result.auxiliary_error.unwrap().code,
            "RECENT_FILES_WRITE_FAILED"
        );
        assert_eq!(
            serialized["auxiliaryError"]["code"],
            "RECENT_FILES_WRITE_FAILED"
        );
    }

    #[test]
    fn rejects_non_absolute_or_non_html_export_targets() {
        assert_eq!(
            export_html_file(
                "relative.html".to_string(),
                "Title".to_string(),
                "Body".to_string()
            )
            .unwrap_err()
            .code,
            "INVALID_EXPORT_TARGET"
        );
        let target = std::env::temp_dir().join(format!(
            "marklite-export-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert_eq!(
            export_html_file(
                target.to_string_lossy().to_string(),
                "Title".to_string(),
                "Body".to_string()
            )
            .unwrap_err()
            .code,
            "INVALID_EXPORT_TARGET"
        );
        assert!(!target.exists());
    }

    #[test]
    fn startup_candidates_resolve_relative_and_require_regular_markdown_files() {
        let dir = std::env::temp_dir().join(format!(
            "marklite-startup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let markdown = dir.join("note.md");
        let other = dir.join("note.exe");
        let directory = dir.join("folder.md");
        fs::write(&markdown, "note").unwrap();
        fs::write(&other, "other").unwrap();
        fs::create_dir(&directory).unwrap();

        assert_eq!(
            canonical_startup_file(&markdown, &dir).unwrap(),
            Some(crate::utils::path_utils::canonicalize_path(&markdown).unwrap())
        );
        assert_eq!(
            canonical_startup_file(std::path::Path::new("note.md"), &dir).unwrap(),
            Some(crate::utils::path_utils::canonicalize_path(&markdown).unwrap())
        );
        assert!(canonical_startup_file(&other, &dir).unwrap().is_none());
        assert!(canonical_startup_file(&directory, &dir).unwrap().is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_manager_target_rejects_broad_or_invalid_paths_without_spawning() {
        let dir = std::env::temp_dir().join(format!(
            "marklite-file-manager-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let markdown = dir.join("note.md");
        let directory = dir.join("folder.md");
        fs::write(&markdown, "note").unwrap();
        fs::create_dir(&directory).unwrap();

        assert_eq!(
            file_manager_target(&markdown.to_string_lossy()).unwrap(),
            FileManagerTarget::Reveal(
                crate::utils::path_utils::canonicalize_path(&markdown).unwrap()
            )
        );
        let missing = dir.join("missing.md");
        assert_eq!(
            file_manager_target(&missing.to_string_lossy()).unwrap(),
            FileManagerTarget::OpenDirectory(
                crate::utils::path_utils::canonicalize_path(&dir).unwrap()
            )
        );
        assert_eq!(
            file_manager_target("relative.md").unwrap_err().code,
            "INVALID_FILE_PATH"
        );
        assert_eq!(
            file_manager_target(&dir.join("note.exe").to_string_lossy())
                .unwrap_err()
                .code,
            "INVALID_FILE_TYPE"
        );
        assert_eq!(
            file_manager_target(&directory.to_string_lossy())
                .unwrap_err()
                .code,
            "INVALID_FILE_TARGET"
        );
        assert_eq!(
            file_manager_target(&dir.join("missing").join("note.md").to_string_lossy())
                .unwrap_err()
                .code,
            "FILE_NOT_FOUND"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
