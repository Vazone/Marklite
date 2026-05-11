use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Utc};

use crate::{
    models::{
        app_error::AppError,
        document::{DocumentDto, FileMetadataDto},
    },
    utils::path_utils::title_from_path,
};

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

pub fn read_markdown_file(path: &str) -> Result<DocumentDto, AppError> {
    let path_buf = PathBuf::from(path);
    ensure_allowed_file(&path_buf)?;

    if !path_buf.exists() {
        return Err(AppError::file_not_found(path));
    }

    let metadata = fs::metadata(&path_buf).map_err(|err| AppError::file_read_failed(path, err))?;
    if metadata.len() > MAX_FILE_SIZE {
        return Err(AppError::new(
            "FILE_TOO_LARGE",
            format!("文件超过 10MB，已阻止打开：{path}"),
        ));
    }

    let content = fs::read_to_string(&path_buf).map_err(|err| AppError::file_read_failed(path, err))?;

    Ok(DocumentDto {
        path: Some(path_buf.to_string_lossy().to_string()),
        title: title_from_path(&path_buf),
        content,
        is_dirty: false,
        last_saved_at: metadata.modified().ok().map(system_time_to_rfc3339),
        file_size: Some(metadata.len()),
    })
}

pub fn save_markdown_file(path: &str, content: &str) -> Result<DocumentDto, AppError> {
    let path_buf = PathBuf::from(path);
    ensure_allowed_file(&path_buf)?;

    if let Some(parent) = path_buf.parent() {
        fs::create_dir_all(parent).map_err(|err| AppError::file_write_failed(path, err))?;
    }

    fs::write(&path_buf, content).map_err(|err| AppError::file_write_failed(path, err))?;
    let metadata = fs::metadata(&path_buf).map_err(|err| AppError::file_read_failed(path, err))?;

    Ok(DocumentDto {
        path: Some(path_buf.to_string_lossy().to_string()),
        title: title_from_path(&path_buf),
        content: content.to_string(),
        is_dirty: false,
        last_saved_at: Some(Utc::now().to_rfc3339()),
        file_size: Some(metadata.len()),
    })
}

pub fn get_file_metadata(path: &str) -> Result<FileMetadataDto, AppError> {
    let path_buf = PathBuf::from(path);
    if !path_buf.exists() {
        return Err(AppError::file_not_found(path));
    }

    let metadata = fs::metadata(&path_buf).map_err(|err| AppError::file_read_failed(path, err))?;
    Ok(FileMetadataDto {
        path: path_buf.to_string_lossy().to_string(),
        file_name: title_from_path(&path_buf),
        file_size: metadata.len(),
        modified_at: metadata.modified().ok().map(system_time_to_rfc3339),
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
    use super::ensure_allowed_file;
    use std::path::Path;

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
}
