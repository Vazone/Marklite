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

#[cfg(test)]
mod tests {
    use super::{ensure_allowed_file, read_markdown_file, save_markdown_file, MAX_FILE_SIZE};
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "marklite-file-{}-{}-{name}.md",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn allows_markdown_and_text_files() {
        assert!(ensure_allowed_file(Path::new("note.md")).is_ok());
        assert!(ensure_allowed_file(Path::new("note.markdown")).is_ok());
        assert!(ensure_allowed_file(Path::new("note.txt")).is_ok());
    }

    #[test]
    fn rejects_other_extensions() {
        assert!(ensure_allowed_file(Path::new("note.exe")).is_err());
    }

    #[test]
    fn saved_documents_can_be_reopened() {
        let path = test_path("round-trip");
        let path_string = path.to_string_lossy();

        save_markdown_file(&path_string, "# Atomic").unwrap();
        let loaded = read_markdown_file(&path_string).unwrap();

        assert_eq!(loaded.content, "# Atomic");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_oversized_content_before_creating_a_file() {
        let path = test_path("too-large");
        let content = "x".repeat(MAX_FILE_SIZE as usize + 1);

        let error = save_markdown_file(&path.to_string_lossy(), &content).unwrap_err();

        assert_eq!(error.code, "FILE_TOO_LARGE");
        assert!(!path.exists());
    }
}
