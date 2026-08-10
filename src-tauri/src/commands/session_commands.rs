use crate::{
    models::{app_error::AppError, session::SessionState},
    services::session_service,
};

#[tauri::command]
pub fn get_session() -> Result<SessionState, AppError> {
    session_service::load_session()
}

#[tauri::command]
pub fn update_session(session: SessionState) -> Result<SessionState, AppError> {
    session_service::save_session(session)
}

#[tauri::command]
pub fn clear_session() -> Result<(), AppError> {
    session_service::clear_session()
}
