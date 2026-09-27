use crate::{
    commands::background::run_background,
    models::{
        app_error::AppError,
        markdown::{MarkdownAnalysisDto, RenderedMarkdownDto, VirtualPreviewWindow},
    },
    services::markdown_service,
};
use tauri::State;

use crate::services::markdown_service::PreviewState;

#[tauri::command]
pub async fn render_markdown(
    content: String,
    tab_id: Option<String>,
    content_revision: Option<u64>,
    state: State<'_, PreviewState>,
) -> Result<RenderedMarkdownDto, AppError> {
    let state = state.inner().clone();
    let generation = state.reserve_generation();
    run_background("Markdown 渲染", move || {
        let session_id = match (tab_id.as_deref(), content_revision) {
            (Some(tab_id), Some(revision)) if !tab_id.is_empty() && tab_id.len() <= 128 => {
                format!("{tab_id}:{revision}:{generation}")
            }
            _ => generation.to_string(),
        };
        let (result, session) = markdown_service::prepare_preview(&content, session_id)?;
        let Some(session) = session else {
            state.clear_if_current(generation);
            return Ok(result);
        };
        if !state.install(generation, session) {
            return Err(AppError::new(
                "PREVIEW_SESSION_EXPIRED",
                "预览文档版本已过期",
            ));
        }
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn render_markdown_window(
    session_id: String,
    start: usize,
    end: usize,
    state: State<'_, PreviewState>,
) -> Result<VirtualPreviewWindow, AppError> {
    let session = state.get(&session_id)?;
    run_background("Markdown 视口渲染", move || {
        session.render_window(start, end)
    })
    .await
}

#[tauri::command]
pub fn release_markdown_preview(session_id: String, state: State<'_, PreviewState>) {
    state.release(&session_id);
}

#[tauri::command]
pub async fn analyze_markdown(content: String) -> Result<MarkdownAnalysisDto, AppError> {
    run_background("Markdown 分析", move || {
        Ok(markdown_service::analyze_markdown(&content))
    })
    .await
}
