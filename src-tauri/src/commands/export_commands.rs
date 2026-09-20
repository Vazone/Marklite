use crate::{
    models::{
        app_error::AppError,
        export::{ExportFormat, ExportRequest, ExportResult},
    },
    services::{export_service, pdf_export_service, png_export_service},
};

#[tauri::command]
pub async fn suggest_export_path(
    default_path: String,
) -> Result<crate::services::export_location_service::ExportPathSuggestion, AppError> {
    super::background::run_background("导出目录建议", move || {
        Ok(crate::services::export_location_service::suggest_export_path(&default_path))
    })
    .await
}

#[tauri::command]
pub async fn remember_export_directory(target_path: String) -> Result<(), AppError> {
    super::background::run_background("记忆导出目录", move || {
        crate::services::export_location_service::remember_export_directory(&target_path)
    })
    .await
}

#[tauri::command]
pub async fn export_document(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    request: ExportRequest,
) -> Result<ExportResult, AppError> {
    let progress = crate::services::export_progress_gui::GuiExportProgress::new(
        app.clone(),
        window.label().to_owned(),
        &request,
    );
    let result = match request.format {
        ExportFormat::Html => {
            export_service::export_html_with_runtime(
                &app,
                &request,
                export_service::ExportCommitPolicy::Replace,
                false,
                &progress.reporter,
            )
            .await
        }
        ExportFormat::Docx => {
            export_service::export_docx_with_runtime(
                &app,
                &request,
                export_service::ExportCommitPolicy::Replace,
                false,
                &progress.reporter,
            )
            .await
        }
        ExportFormat::Pdf => {
            pdf_export_service::export_pdf(app, &request, &progress.reporter).await
        }
        ExportFormat::Png => {
            png_export_service::export_png(&app, &request, &progress.reporter).await
        }
        ExportFormat::Svg => {
            let reporter = progress.reporter.clone();
            tauri::async_runtime::spawn_blocking(move || {
                export_service::export_svg_reported(&request, &reporter)
            })
            .await
            .unwrap_or_else(|_| Err(AppError::new("EXPORT_TASK_FAILED", "SVG 导出任务异常结束")))
        }
    };
    progress.finish(&result);
    result
}

#[tauri::command]
pub async fn resolve_png_export_directory(
    source_path: Option<String>,
    title: String,
    parent_path: Option<String>,
) -> Result<String, AppError> {
    super::background::run_background("图片导出目录", move || {
        crate::services::png_artifact::resolve_target_string(
            source_path.as_deref(),
            &title,
            parent_path.as_deref(),
        )
    })
    .await
}

#[tauri::command]
pub fn cancel_png_export(job_id: String) -> bool {
    png_export_service::cancel(&job_id)
}
