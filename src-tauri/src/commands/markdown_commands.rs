use crate::{
    models::{
        app_error::AppError,
        document::{DocumentStats, OutlineItem, RenderedMarkdownDto},
    },
    services::markdown_service,
};

#[tauri::command]
pub fn render_markdown(content: String) -> Result<RenderedMarkdownDto, AppError> {
    markdown_service::render_markdown(&content)
}

#[tauri::command]
pub fn extract_markdown_outline(content: String) -> Result<Vec<OutlineItem>, AppError> {
    Ok(markdown_service::extract_outline(&content))
}

#[tauri::command]
pub fn calculate_markdown_stats(content: String) -> Result<DocumentStats, AppError> {
    Ok(markdown_service::calculate_stats(&content))
}
