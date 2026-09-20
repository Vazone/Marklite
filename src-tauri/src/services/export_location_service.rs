use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    models::app_error::AppError,
    utils::{
        atomic_write::{atomic_write_checked, CheckedWriteError},
        bounded_read::OpenedFile,
        path_utils::{app_data_dir, path_to_utf8},
    },
};
use serde::{Deserialize, Serialize};

const MAX_BYTES: u64 = 64 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportLocationDocument {
    pub version: u32,
    pub directory: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPathSuggestion {
    pub path: String,
    pub warning: Option<AppError>,
}

fn location_path() -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("export-location.json"))
}

fn read_location(path: &Path) -> Result<Option<PathBuf>, AppError> {
    let opened = match OpenedFile::open(path) {
        Ok(opened) => opened,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(location_error("READ", error)),
    };
    if !opened.metadata().is_file() || opened.metadata().len() > MAX_BYTES {
        return Err(location_error("READ", "状态不是普通文件或超过字节上限"));
    }
    let raw = opened
        .read_to_string_bounded(MAX_BYTES)
        .map_err(|e| location_error("READ", e))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| location_error("INVALID", e))?;
    if let Some(version) = value.get("version").and_then(|v| v.as_u64()) {
        if version != 1 {
            return Err(AppError::new(
                "EXPORT_LOCATION_VERSION_UNSUPPORTED",
                "导出目录状态版本不受支持，保留原文件",
            ));
        }
    }
    let document: ExportLocationDocument =
        serde_json::from_value(value).map_err(|e| location_error("INVALID", e))?;
    let directory = PathBuf::from(document.directory);
    if document.version != 1 || !directory.is_absolute() {
        return Err(location_error(
            "INVALID",
            "目录状态必须是版本 1 和本机绝对路径",
        ));
    }
    Ok(Some(directory))
}

fn usable_directory(path: &Path) -> bool {
    path.is_absolute() && path.is_dir() && fs::read_dir(path).is_ok()
}

fn source_suggestion(default_path: &Path) -> PathBuf {
    match default_path
        .parent()
        .filter(|parent| usable_directory(parent))
    {
        Some(_) => default_path.to_path_buf(),
        None => default_path
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("Untitled")),
    }
}

fn suggest_from(state: Result<&Path, AppError>, default_path: &Path) -> ExportPathSuggestion {
    let mut suggested = source_suggestion(default_path);
    let mut warning = None;
    match state.and_then(read_location) {
        Ok(Some(directory)) if usable_directory(&directory) => {
            if let Some(name) = default_path.file_name() {
                suggested = directory.join(name);
            }
        }
        Ok(Some(_)) => {
            warning = Some(location_error(
                "UNAVAILABLE",
                "上次导出目录不可访问，已回退到当前文档目录或系统位置",
            ))
        }
        Ok(None) => {}
        Err(error) => warning = Some(error),
    }
    ExportPathSuggestion {
        // Both components come from UTF-8 IPC/JSON, never lossy OS path decoding.
        path: suggested
            .to_str()
            .expect("suggestion components are UTF-8")
            .to_owned(),
        warning,
    }
}

pub fn suggest_export_path(default_path: &str) -> ExportPathSuggestion {
    let state = location_path();
    suggest_from(
        state.as_deref().map_err(Clone::clone),
        Path::new(default_path),
    )
}

fn remember_to(state: &Path, target: &Path) -> Result<(), AppError> {
    let directory = target
        .parent()
        .filter(|p| usable_directory(p))
        .ok_or_else(|| location_error("WRITE", "已导出文件的父目录不可访问"))?;
    if !target.is_file() {
        return Err(location_error("WRITE", "已导出文件不存在"));
    }
    let document = ExportLocationDocument {
        version: 1,
        directory: path_to_utf8(directory)?.to_owned(),
    };
    let bytes = serde_json::to_vec_pretty(&document).map_err(|e| location_error("WRITE", e))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(location_error("WRITE", "目录状态超过字节上限"));
    }
    // Check inside the same per-target commit lock: a newer state must never be overwritten.
    atomic_write_checked(state, &bytes, |path| match read_location(path) {
        Err(error) if error.code != "EXPORT_LOCATION_INVALID_FAILED" => Err(error),
        _ => Ok(()),
    })
    .map_err(|error| match error {
        CheckedWriteError::Check(error) => error,
        CheckedWriteError::Write(error) | CheckedWriteError::Lock(error) => {
            location_error("WRITE", error)
        }
    })
}

pub fn remember_export_directory(target_path: &str) -> Result<(), AppError> {
    remember_to(&location_path()?, Path::new(target_path))
}

fn location_error(stage: &str, error: impl std::fmt::Display) -> AppError {
    AppError::new(
        format!("EXPORT_LOCATION_{stage}_FAILED"),
        format!("导出目录记忆：{error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::TestDirectory;

    #[test]
    fn remembers_only_directory_across_formats_and_fresh_reads() {
        let root = TestDirectory::new("export-location");
        let state = root.join("export-location.json");
        let a = root.join("中文 A");
        let b = root.join("B");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        let target = a.join("old.html");
        fs::write(&target, "artifact").unwrap();
        remember_to(&state, &target).unwrap();
        for format in ["html", "pdf", "docx", "svg"] {
            let name = format!("新文档.{format}");
            let suggestion = suggest_from(Ok(&state), &b.join(&name));
            assert_eq!(suggestion.path, a.join(name).to_str().unwrap());
            assert!(suggestion.warning.is_none());
        }
        assert_eq!(read_location(&state).unwrap().unwrap(), a);
        assert!(!fs::read_to_string(&state).unwrap().contains("old.html"));
        assert!(!root.join("settings.json").exists());
        assert!(!root.join("session.json").exists());
    }

    #[test]
    fn missing_deleted_and_file_instead_of_directory_fall_back() {
        let root = TestDirectory::new("export-fallback");
        let state = root.join("state.json");
        let source = root.join("current.pdf");
        assert_eq!(
            suggest_from(Ok(&state), &source).path,
            source.to_str().unwrap()
        );
        assert!(!state.exists());
        for path in [root.join("deleted"), root.join("file")] {
            fs::write(
                &state,
                serde_json::to_vec(&ExportLocationDocument {
                    version: 1,
                    directory: path.to_str().unwrap().into(),
                })
                .unwrap(),
            )
            .unwrap();
            fs::write(root.join("file"), "not a directory").unwrap();
            let suggestion = suggest_from(Ok(&state), &source);
            assert!(suggestion.warning.is_some());
            assert_eq!(suggestion.path, source.to_str().unwrap());
            assert_eq!(
                suggest_from(Ok(&state), &root.join("missing/current.svg")).path,
                "current.svg"
            );
        }
        assert_eq!(
            suggest_from(Ok(&state), Path::new("Untitled.html")).path,
            "Untitled.html"
        );
    }

    #[test]
    fn corrupt_state_warns_then_recovers_only_after_success() {
        let root = TestDirectory::new("export-corrupt");
        let state = root.join("state.json");
        let target = root.join("done.html");
        fs::write(&state, "{").unwrap();
        assert!(suggest_from(Ok(&state), &target).warning.is_some());
        assert_eq!(fs::read_to_string(&state).unwrap(), "{");
        assert!(remember_to(&state, &target).is_err());
        assert_eq!(fs::read_to_string(&state).unwrap(), "{");
        fs::write(&target, "artifact").unwrap();
        remember_to(&state, &target).unwrap();
        assert_eq!(read_location(&state).unwrap().unwrap(), root.path());
    }

    #[test]
    fn future_version_is_preserved_even_after_completed_export() {
        let root = TestDirectory::new("export-future");
        let state = root.join("state.json");
        let target = root.join("done.pdf");
        let future = r#"{"version":999,"newFormat":"keep"}"#;
        fs::write(&state, future).unwrap();
        fs::write(&target, "artifact").unwrap();
        assert_eq!(
            suggest_from(Ok(&state), &target).warning.unwrap().code,
            "EXPORT_LOCATION_VERSION_UNSUPPORTED"
        );
        assert_eq!(
            remember_to(&state, &target).unwrap_err().code,
            "EXPORT_LOCATION_VERSION_UNSUPPORTED"
        );
        assert_eq!(fs::read_to_string(&state).unwrap(), future);
        assert_eq!(fs::read_to_string(&target).unwrap(), "artifact");
    }

    #[test]
    fn unreadable_oversized_and_unwritable_state_never_blocks_suggestion() {
        let root = TestDirectory::new("export-location-failures");
        let state = root.join("state.json");
        let target = root.join("done.pdf");
        fs::write(&target, "artifact").unwrap();
        fs::create_dir(&state).unwrap();
        assert!(suggest_from(Ok(&state), &target).warning.is_some());
        assert!(remember_to(&state, &target).is_err());
        let oversized = root.join("oversized.json");
        fs::write(&oversized, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
        assert!(remember_to(&oversized, &target).is_err());
        assert_eq!(fs::metadata(&oversized).unwrap().len(), MAX_BYTES + 1);
        let unavailable = root.join("missing/state.json");
        assert!(remember_to(&unavailable, &target).is_err());
        let suggestion = suggest_from(Err(location_error("READ", "app data unavailable")), &target);
        assert_eq!(suggestion.path, target.to_str().unwrap());
        assert!(suggestion.warning.is_some());
        assert_eq!(fs::read_to_string(&target).unwrap(), "artifact");
    }

    #[test]
    fn uses_native_roots_and_unc_parents_without_string_splitting() {
        #[cfg(windows)]
        for (file, parent) in [
            (r"C:\新文档.pdf", r"C:\"),
            (r"\\server\share\中文.svg", r"\\server\share\"),
        ] {
            assert_eq!(Path::new(file).parent().unwrap(), Path::new(parent));
            assert_eq!(
                Path::new(parent).join(Path::new(file).file_name().unwrap()),
                Path::new(file)
            );
        }
        #[cfg(unix)]
        {
            assert_eq!(Path::new("/新文档.pdf").parent().unwrap(), Path::new("/"));
            assert_eq!(
                source_suggestion(Path::new("/新文档.pdf")),
                Path::new("/新文档.pdf")
            );
        }
    }
}
