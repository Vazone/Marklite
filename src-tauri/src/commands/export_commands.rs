#[cfg(desktop)]
use crate::services::{export_service, pdf_export_service};
use crate::{
    models::{
        app_error::AppError,
        export::{ExportFormat, ExportRequest, ExportResult},
    },
    services::png_export_service,
};
#[cfg(target_os = "android")]
use tauri_plugin_marklite_mobile::MarkliteMobileExt;

#[tauri::command]
pub async fn cancel_export(
    _app: tauri::AppHandle,
    job_id: String,
    format: ExportFormat,
) -> crate::models::export::ExportCancelStatus {
    match format {
        ExportFormat::Png => {
            let status = png_export_service::request_cancel(&job_id);
            #[cfg(target_os = "android")]
            if matches!(status, crate::models::export::ExportCancelStatus::Requested) {
                let app = _app.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    app.marklite_mobile().cancel_capture_png()
                })
                .await;
            }
            status
        }
        #[cfg(target_os = "android")]
        ExportFormat::Pdf => {
            let status = crate::services::export_control::request_cancel(&job_id);
            if matches!(status, crate::models::export::ExportCancelStatus::Requested) {
                let app = _app.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    app.marklite_mobile().cancel_draw_pdf()
                })
                .await;
            }
            status
        }
        _ => crate::models::export::ExportCancelStatus::Unsupported,
    }
}

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
    source_resource: Option<crate::models::resource::ResourceRef>,
    target_resource: Option<crate::models::resource::ResourceRef>,
) -> Result<ExportResult, AppError> {
    crate::platform::capabilities::current().require_export(request.format)?;
    #[cfg(desktop)]
    {
        crate::platform::desktop::export_resources::validate(
            &request,
            source_resource.as_ref(),
            target_resource.as_ref(),
        )?;
        let progress = crate::services::export_progress_gui::GuiExportProgress::new(
            app.clone(),
            window.label().to_owned(),
            &request,
        );
        let result = match request.format {
            ExportFormat::Html => {
                crate::platform::desktop::export::export_html_with_runtime(
                    &app,
                    &request,
                    export_service::ExportCommitPolicy::Replace,
                    false,
                    &progress.reporter,
                )
                .await
            }
            ExportFormat::Docx => {
                crate::platform::desktop::export::export_docx_with_runtime(
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
                png_export_service::export_png(
                    &crate::platform::desktop::chapter::DesktopChapterRenderer(&app),
                    &request,
                    &progress.reporter,
                )
                .await
            }
            ExportFormat::Svg => {
                let reporter = progress.reporter.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    export_service::export_svg_reported(&request, &reporter)
                })
                .await
                .unwrap_or_else(|_| {
                    Err(AppError::new("EXPORT_TASK_FAILED", "SVG 导出任务异常结束"))
                })
            }
        };
        progress.finish(&result);
        result
    }
    #[cfg(target_os = "android")]
    {
        let progress = crate::services::export_progress_gui::GuiExportProgress::new(
            app.clone(),
            window.label().to_owned(),
            &request,
        );
        let result = crate::commands::android_export_commands::export(
            app,
            request,
            source_resource,
            target_resource,
            &progress.reporter,
        )
        .await;
        progress.finish(&result);
        result
    }
    #[cfg(all(mobile, not(target_os = "android")))]
    {
        let _ = (app, window, source_resource, target_resource);
        Err(AppError::new(
            "CAPABILITY_UNAVAILABLE",
            "当前平台尚未实现导出",
        ))
    }
}

#[tauri::command]
pub async fn resolve_png_export_directory(
    source_path: Option<String>,
    title: String,
    parent_path: Option<String>,
) -> Result<String, AppError> {
    #[cfg(target_os = "android")]
    {
        let _ = (source_path, title);
        let parent = parent_path
            .ok_or_else(|| AppError::new("INVALID_EXPORT_TARGET", "请选择 Android 图片目录"))?;
        let parsed = url::Url::parse(&parent)
            .map_err(|_| AppError::new("INVALID_EXPORT_TARGET", "Android 图片目录 URI 无效"))?;
        if parent.len() > 4096
            || parsed.scheme() != "content"
            || parsed.host_str().is_none_or(str::is_empty)
        {
            return Err(AppError::new(
                "INVALID_EXPORT_TARGET",
                "Android 图片目录 URI 无效",
            ));
        }
        return Ok(parent);
    }
    #[cfg(not(target_os = "android"))]
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
