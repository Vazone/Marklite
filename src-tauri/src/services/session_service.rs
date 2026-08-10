use std::{
    collections::HashSet,
    fs,
    path::Path,
    sync::{Mutex, OnceLock},
};

use chrono::Utc;

use crate::{
    models::{
        app_error::AppError,
        session::{SessionState, MAX_SESSION_PATHS, SESSION_VERSION},
    },
    utils::{
        atomic_write::{atomic_write, recover_atomic_write},
        path_utils::session_path,
    },
};

const MAX_PATH_LENGTH: usize = 32_768;
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
    recover_atomic_write(path).map_err(AppError::session_write_failed)?;
    if path.exists() {
        fs::remove_file(path).map_err(AppError::session_write_failed)?;
    }
    Ok(())
}

fn load_session_from(path: &Path) -> Result<SessionState, AppError> {
    recover_atomic_write(path).map_err(AppError::session_read_failed)?;
    if !path.exists() {
        return Ok(SessionState::default());
    }

    let raw = fs::read_to_string(path).map_err(AppError::session_read_failed)?;
    let session = match serde_json::from_str::<SessionState>(&raw) {
        Ok(session) => session,
        Err(error) => {
            backup_corrupt_file(path)?;
            return Err(AppError::session_read_failed(error));
        }
    };

    if session.version != SESSION_VERSION {
        backup_corrupt_file(path)?;
        return Err(AppError::session_read_failed(format!(
            "不支持的会话版本 {}",
            session.version
        )));
    }

    Ok(normalize_session(session))
}

fn save_session_to(path: &Path, session: SessionState) -> Result<SessionState, AppError> {
    let session = normalize_session(session);
    let content = serde_json::to_string_pretty(&session).map_err(AppError::session_write_failed)?;
    atomic_write(path, content.as_bytes()).map_err(AppError::session_write_failed)?;
    Ok(session)
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
    use std::{fs, path::PathBuf};

    use super::{clear_session_at, load_session_from, normalize_session, save_session_to};
    use crate::models::session::{SessionState, MAX_SESSION_PATHS, SESSION_VERSION};

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "marklite-session-{}-{}-{name}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

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
        let path = test_path("round-trip");
        let session = SessionState {
            version: SESSION_VERSION,
            paths: vec!["C:\\docs\\a.md".to_string()],
            active_path: Some("C:\\docs\\a.md".to_string()),
        };

        let saved = save_session_to(&path, session.clone()).unwrap();
        let loaded = load_session_from(&path).unwrap();

        assert_eq!(saved, session);
        assert_eq!(loaded, session);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn corrupt_session_is_backed_up_and_rejected() {
        let path = test_path("corrupt");
        fs::write(&path, "{not-json").unwrap();

        let error = load_session_from(&path).unwrap_err();

        assert_eq!(error.code, "SESSION_READ_FAILED");
        assert!(!path.exists());
        let prefix = path.file_stem().unwrap().to_string_lossy();
        let backups = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(prefix.as_ref())
            })
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        fs::remove_file(backups[0].path()).unwrap();
    }

    #[test]
    fn clear_removes_session_and_load_falls_back_to_default() {
        let path = test_path("clear");
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
}
