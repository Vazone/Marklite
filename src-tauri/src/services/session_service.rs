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
