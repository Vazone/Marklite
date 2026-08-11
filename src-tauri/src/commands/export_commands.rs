use crate::{
    models::{
        app_error::AppError,
        export::{ExportFormat, ExportRequest, ExportResult},
    },
    services::{export_service, pdf_export_service},
};

#[tauri::command]
pub async fn export_document(
    app: tauri::AppHandle,
    request: ExportRequest,
) -> Result<ExportResult, AppError> {
    match request.format {
        ExportFormat::Html => {
            tauri::async_runtime::spawn_blocking(move || export_service::export_html(&request))
                .await
                .map_err(|_| AppError::new("EXPORT_TASK_FAILED", "HTML 导出任务异常结束"))?
        }
        ExportFormat::Docx => {
            tauri::async_runtime::spawn_blocking(move || export_service::export_docx(&request))
                .await
                .map_err(|_| AppError::new("EXPORT_TASK_FAILED", "DOCX 导出任务异常结束"))?
        }
        ExportFormat::Pdf => pdf_export_service::export_pdf(app, &request).await,
    }
}
