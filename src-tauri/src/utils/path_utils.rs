use std::{
    ffi::{OsStr, OsString},
    fs,
    path::PathBuf,
};

use crate::models::app_error::AppError;

const BENCHMARK_MODE_ENV: &str = "MARKLITE_BENCHMARK_MODE";
const BENCHMARK_DATA_DIR_ENV: &str = "MARKLITE_BENCHMARK_DATA_DIR";

fn benchmark_data_dir(mode: Option<&OsStr>, configured_path: Option<OsString>) -> Option<PathBuf> {
    if mode != Some(OsStr::new("1")) {
        return None;
    }
    configured_path
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

pub fn app_data_dir() -> Result<PathBuf, AppError> {
    let configured = std::env::var_os(BENCHMARK_MODE_ENV);
    let dir = if configured.as_deref() == Some(OsStr::new("1")) {
        benchmark_data_dir(
            configured.as_deref(),
            std::env::var_os(BENCHMARK_DATA_DIR_ENV),
        )
        .ok_or_else(|| {
            AppError::new(
                "BENCHMARK_DATA_DIR_INVALID",
                "性能基准数据目录必须是绝对路径",
            )
        })?
    } else {
        dirs::data_dir()
            .map(|base| base.join("MarkLite"))
            .ok_or_else(|| AppError::new("APP_DATA_UNAVAILABLE", "无法找到应用数据目录"))?
    };
    create_app_data_directory(&dir)?;
    Ok(dir)
}

pub fn settings_path() -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("settings.json"))
}

pub fn recent_files_path() -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("recent-files.json"))
}

pub fn session_path() -> Result<PathBuf, AppError> {
    Ok(app_data_dir()?.join("session.json"))
}

pub fn startup_diagnostics_dir() -> Result<PathBuf, AppError> {
    let dir = app_data_dir()?.join("diagnostics").join("startup");
    create_app_data_directory(&dir)?;
    Ok(dir)
}

fn create_app_data_directory(path: &std::path::Path) -> Result<(), AppError> {
    fs::create_dir_all(path).map_err(|error| AppError::app_data_create_failed(error.kind()))
}

pub fn title_from_path(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

pub fn path_to_utf8(path: &std::path::Path) -> Result<&str, AppError> {
    path.to_str()
        .ok_or_else(AppError::unsupported_path_encoding)
}

pub fn canonicalize_path(path: &std::path::Path) -> std::io::Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        if let Some(value) = canonical.to_str() {
            if let Some(path) = value.strip_prefix(r"\\?\UNC\") {
                return Ok(PathBuf::from(format!(r"\\{path}")));
            }
            if let Some(path) = value.strip_prefix(r"\\?\") {
                return Ok(PathBuf::from(path));
            }
        }
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, fs, path::PathBuf};

    use super::{benchmark_data_dir, create_app_data_directory, path_to_utf8};
    use crate::utils::test_support::TestDirectory;

    #[test]
    fn reports_app_data_directory_creation_with_an_infrastructure_error() {
        let directory = TestDirectory::new("path-utils");
        let occupied = directory.path().join("occupied");
        fs::write(&occupied, "not a directory").unwrap();

        let error = create_app_data_directory(&occupied).unwrap_err();

        assert_eq!(error.code, "APP_DATA_CREATE_FAILED");
    }

    #[test]
    fn rejects_non_utf8_paths_without_lossy_replacement() {
        #[cfg(unix)]
        let value = {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![b'n', 0x80, b'.', b'm', b'd'])
        };
        #[cfg(windows)]
        let value = {
            use std::os::windows::ffi::OsStringExt;
            OsString::from_wide(&[0xD800])
        };

        let error = path_to_utf8(&PathBuf::from(value)).unwrap_err();

        assert_eq!(error.code, "UNSUPPORTED_PATH_ENCODING");
    }

    #[test]
    fn benchmark_data_override_requires_explicit_mode_and_absolute_path() {
        let directory = TestDirectory::new("benchmark-data-override");
        let absolute = directory.path().to_path_buf();
        assert_eq!(
            benchmark_data_dir(
                Some(std::ffi::OsStr::new("1")),
                Some(absolute.clone().into())
            ),
            Some(absolute)
        );
        assert_eq!(
            benchmark_data_dir(
                Some(std::ffi::OsStr::new("0")),
                Some(PathBuf::from("C:/ignored").into())
            ),
            None
        );
        assert_eq!(
            benchmark_data_dir(
                Some(std::ffi::OsStr::new("1")),
                Some(PathBuf::from("relative").into())
            ),
            None
        );
    }
}
