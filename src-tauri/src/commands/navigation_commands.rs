use crate::{
    models::{
        app_error::AppError,
        navigation::{LocalImageDto, MarkdownTargetDto},
    },
    services::navigation_service,
};

#[tauri::command]
pub fn resolve_markdown_target(
    document_path: Option<String>,
    target: String,
) -> Result<MarkdownTargetDto, AppError> {
    navigation_service::resolve_markdown_target(document_path.as_deref(), &target)
}

#[tauri::command]
pub fn load_local_image(
    document_path: Option<String>,
    target: String,
) -> Result<LocalImageDto, AppError> {
    navigation_service::load_local_image(document_path.as_deref(), &target)
}
