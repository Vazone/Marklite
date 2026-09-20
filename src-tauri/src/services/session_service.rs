use std::{
    collections::HashSet,
    fs,
    path::Path,
    sync::{Mutex, OnceLock},
};

use chrono::Utc;
use serde_json::Value;

use crate::{
    models::{
        app_error::AppError,
        session::{SessionState, MAX_SESSION_PATHS, SESSION_VERSION},
    },
    utils::{atomic_write::atomic_write, bounded_read::OpenedFile, path_utils::session_path},
};

const MAX_PATH_LENGTH: usize = 32_768;
const MAX_SESSION_FILE_BYTES: u64 = 2 * 1024 * 1024;
static SESSION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn load_session() -> Result<SessionState, AppError> {
    let _guard = session_lock();
    load_session_from(&session_path()?)
}

pub fn save_session(session: SessionState) -> Result<SessionState, AppError> {
    let _guard = session_lock();
    save_session_to(&session_path()?, session)
}

pub fn clear_session() -> Result<(), AppError> {
    let _guard = session_lock();
    clear_session_at(&session_path()?)
}

fn clear_session_at(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        ensure_supported_session_version(path)?;
        fs::remove_file(path).map_err(AppError::session_write_failed)?;
    }
    Ok(())
}

fn load_session_from(path: &Path) -> Result<SessionState, AppError> {
    if !path.exists() {
        return Ok(SessionState::default());
    }

    let raw = read_session_source(path)?;
    let document = match serde_json::from_str::<Value>(&raw) {
        Ok(document) => document,
        Err(error) => {
            backup_corrupt_file(path)?;
            return Err(AppError::session_read_failed(error));
        }
    };
    let Some(version) = document.get("version").and_then(Value::as_u64) else {
        backup_corrupt_file(path)?;
        return Err(AppError::session_read_failed("会话 version 必须是非负整数"));
    };
    if version != u64::from(SESSION_VERSION) {
        return Err(AppError::session_version_unsupported(version));
    }
    let session = match serde_json::from_value::<SessionState>(document) {
        Ok(session) => session,
        Err(error) => {
            backup_corrupt_file(path)?;
            return Err(AppError::session_read_failed(error));
        }
    };

    Ok(normalize_session(session))
}

fn save_session_to(path: &Path, session: SessionState) -> Result<SessionState, AppError> {
    ensure_supported_session_version(path)?;
    let session = normalize_session(session);
    let content = serde_json::to_string_pretty(&session).map_err(AppError::session_write_failed)?;
    if content.len() as u64 > MAX_SESSION_FILE_BYTES {
        return Err(AppError::session_write_failed(std::io::Error::new(
            std::io::ErrorKind::FileTooLarge,
            "会话文件超过允许的字节上限",
        )));
    }
    atomic_write(path, content.as_bytes()).map_err(AppError::session_write_failed)?;
    Ok(session)
}

fn ensure_supported_session_version(path: &Path) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }
    let raw = read_session_source(path)?;
    let document = serde_json::from_str::<Value>(&raw).map_err(AppError::session_read_failed)?;
    let version = document
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::session_read_failed("会话 version 必须是非负整数"))?;
    if version != u64::from(SESSION_VERSION) {
        return Err(AppError::session_version_unsupported(version));
    }
    Ok(())
}

fn read_session_source(path: &Path) -> Result<String, AppError> {
    let opened = OpenedFile::open(path).map_err(AppError::session_read_failed)?;
    if !opened.metadata().is_file() {
        return Err(AppError::session_read_failed("会话路径不是普通文件"));
    }
    if opened.metadata().len() > MAX_SESSION_FILE_BYTES {
        return Err(AppError::session_read_failed("会话文件超过允许的字节上限"));
    }
    opened
        .read_to_string_bounded(MAX_SESSION_FILE_BYTES)
        .map_err(AppError::session_read_failed)
}

fn normalize_session(session: SessionState) -> SessionState {
    let mut seen = HashSet::new();
    let paths = session
        .paths
        .into_iter()
        .filter(|path| !path.trim().is_empty() && path.len() <= MAX_PATH_LENGTH)
        .filter(|path| seen.insert(path.clone()))
        .take(MAX_SESSION_PATHS)
        .collect::<Vec<_>>();
    let active_path = session.active_path.filter(|active| paths.contains(active));

    SessionState {
        version: SESSION_VERSION,
        paths,
        active_path,
    }
}

fn backup_corrupt_file(path: &Path) -> Result<(), AppError> {
    let timestamp = Utc::now().format("%Y%m%d%H%M%S%3f");
    let backup_path = path.with_extension(format!("json.corrupt-{timestamp}"));
    fs::rename(path, backup_path).map_err(AppError::session_write_failed)
}

fn session_lock() -> std::sync::MutexGuard<'static, ()> {
    SESSION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        clear_session_at, load_session_from, normalize_session, save_session_to,
        MAX_SESSION_FILE_BYTES,
    };
    use crate::{
        models::session::{SessionState, MAX_SESSION_PATHS, SESSION_VERSION},
        utils::test_support::TestDirectory,
    };

    #[test]
    fn normalizes_duplicates_limit_and_active_path() {
        let mut paths = vec!["C:\\a.md".to_string(), "C:\\a.md".to_string()];
        paths.extend((0..MAX_SESSION_PATHS + 5).map(|index| format!("C:\\{index}.md")));

        let normalized = normalize_session(SessionState {
            version: SESSION_VERSION,
            paths,
            active_path: Some("C:\\missing.md".to_string()),
        });

        assert_eq!(normalized.paths.len(), MAX_SESSION_PATHS);
        assert_eq!(normalized.paths[0], "C:\\a.md");
        assert_eq!(normalized.active_path, None);
    }

    #[test]
    fn round_trips_a_valid_session() {
        let directory = TestDirectory::new("session-round-trip");
        let path = directory.path().join("session.json");
        let session = SessionState {
            version: SESSION_VERSION,
            paths: vec!["C:\\docs\\a.md".to_string()],
            active_path: Some("C:\\docs\\a.md".to_string()),
        };

        let saved = save_session_to(&path, session.clone()).unwrap();
        let loaded = load_session_from(&path).unwrap();

        assert_eq!(saved, session);
        assert_eq!(loaded, session);
    }

    #[test]
    fn corrupt_session_is_backed_up_and_rejected() {
        let directory = TestDirectory::new("session-corrupt");
        let path = directory.path().join("session.json");
        fs::write(&path, "{not-json").unwrap();

        let error = load_session_from(&path).unwrap_err();

        assert_eq!(error.code, "SESSION_READ_FAILED");
        assert!(!path.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn clear_removes_session_and_load_falls_back_to_default() {
        let directory = TestDirectory::new("session-clear");
        let path = directory.path().join("session.json");
        save_session_to(
            &path,
            SessionState {
                version: SESSION_VERSION,
                paths: vec!["C:\\docs\\a.md".to_string()],
                active_path: Some("C:\\docs\\a.md".to_string()),
            },
        )
        .unwrap();

        clear_session_at(&path).unwrap();

        assert!(!path.exists());
        assert_eq!(load_session_from(&path).unwrap(), SessionState::default());
    }

    #[test]
    fn preserves_future_session_versions_without_quarantining_them() {
        let directory = TestDirectory::new("session-future-version");
        let path = directory.path().join("session.json");
        let raw = r#"{"version":999,"paths":[],"activePath":null}"#;
        fs::write(&path, raw).unwrap();

        let result = load_session_from(&path);

        assert_eq!(result.unwrap_err().code, "SESSION_VERSION_UNSUPPORTED");
        assert_eq!(fs::read_to_string(path).unwrap(), raw);
    }

    #[test]
    fn future_session_cannot_be_saved_over_or_cleared() {
        let directory = TestDirectory::new("session-future-write-protection");
        let path = directory.path().join("session.json");
        let raw = r#"{"version":99,"paths":["future-field"],"activePath":null}"#;
        fs::write(&path, raw).unwrap();
        let replacement = SessionState {
            version: SESSION_VERSION,
            paths: vec!["C:\\docs\\replacement.md".to_string()],
            active_path: Some("C:\\docs\\replacement.md".to_string()),
        };

        assert_eq!(
            save_session_to(&path, replacement).unwrap_err().code,
            "SESSION_VERSION_UNSUPPORTED"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), raw);
        assert_eq!(
            clear_session_at(&path).unwrap_err().code,
            "SESSION_VERSION_UNSUPPORTED"
        );
        assert_eq!(fs::read_to_string(path).unwrap(), raw);
    }

    #[test]
    fn rejects_oversized_sessions_before_deserializing() {
        let directory = TestDirectory::new("session-oversized");
        let path = directory.path().join("session.json");
        let raw = format!(
            "{}{{\"version\":1,\"paths\":[],\"activePath\":null}}",
            " ".repeat(MAX_SESSION_FILE_BYTES as usize + 1)
        );
        fs::write(&path, &raw).unwrap();

        let result = load_session_from(&path);

        assert_eq!(result.unwrap_err().code, "SESSION_READ_FAILED");
        assert_eq!(fs::read_to_string(path).unwrap(), raw);
    }
}
