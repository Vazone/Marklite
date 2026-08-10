use crate::{
    models::{app_error::AppError, document::RenderedMarkdownDto},
    services::markdown_service,
};

#[tauri::command]
pub fn render_markdown(content: String) -> Result<RenderedMarkdownDto, AppError> {
    markdown_service::render_markdown(&content)
}
