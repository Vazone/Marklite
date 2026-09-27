use crate::{
    commands::background::run_background,
    models::{
        app_error::AppError,
        session::{ResourceSession, SessionState},
    },
    services::session_service,
};

#[tauri::command]
pub async fn get_session() -> Result<SessionState, AppError> {
    run_background("读取会话", session_service::load_session).await
}

#[tauri::command]
pub async fn update_session(session: SessionState) -> Result<SessionState, AppError> {
    run_background("保存会话", move || {
        session_service::save_session(session)
    })
    .await
}

#[tauri::command]
pub async fn clear_session() -> Result<(), AppError> {
    run_background("清除会话", session_service::clear_session).await
}

#[tauri::command]
pub async fn get_resource_session() -> Result<ResourceSession, AppError> {
    run_background("读取资源会话", session_service::load_resource_session).await
}

#[tauri::command]
pub async fn update_resource_session(
    session: ResourceSession,
) -> Result<ResourceSession, AppError> {
    run_background("保存资源会话", move || {
        session_service::save_resource_session(session)
    })
    .await
}

#[tauri::command]
pub async fn clear_resource_session() -> Result<(), AppError> {
    run_background("清除资源会话", session_service::clear_resource_session).await
}
