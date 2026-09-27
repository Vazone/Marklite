use crate::{
    models::{
        app_error::AppError,
        export::{ExportFormat, ExportRequest, ExportResult, ExportTargetKind, ExportWarning},
        resource::ResourceRef,
    },
    platform::android::{diagram, export::StagedArtifact, export_resources},
    services::{
        android_pdf_export, android_png_export,
        diagram_export_service::DiagramExportMode,
        export_control::{CaptureControl, Registration},
        export_core,
        export_progress::{ExportReporter, ExportStage},
        export_semantic::SemanticDocument,
        export_service::{self, ExportCommitPolicy},
    },
};
use tauri::AppHandle;
use tauri_plugin_marklite_mobile::{ExportFileRequest, MarkliteMobileExt};

fn document_uri(resource: &ResourceRef) -> Result<&str, AppError> {
    let ResourceRef::AndroidDocument { uri } = resource else {
        return Err(AppError::new(
            "INVALID_EXPORT_TARGET",
            "导出目标必须是 Android 文档",
        ));
    };
    let parsed = url::Url::parse(uri)
        .map_err(|_| AppError::new("INVALID_EXPORT_TARGET", "导出目标 URI 无效"))?;
    if uri.len() > 4096
        || parsed.scheme() != "content"
        || parsed.host_str().is_none_or(str::is_empty)
    {
        return Err(AppError::new("INVALID_EXPORT_TARGET", "导出目标 URI 无效"));
    }
    Ok(uri)
}

fn mime(format: ExportFormat) -> Result<&'static str, AppError> {
    match format {
        ExportFormat::Html => Ok("text/html"),
        ExportFormat::Pdf => Ok("application/pdf"),
        ExportFormat::Docx => {
            Ok("application/vnd.openxmlformats-officedocument.wordprocessingml.document")
        }
        ExportFormat::Svg => Ok("image/svg+xml"),
        ExportFormat::Png => Err(AppError::new("INVALID_EXPORT_TARGET", "PNG 导出需要目录")),
    }
}

#[tauri::command]
pub fn pick_android_export_document(
    app: AppHandle,
    format: ExportFormat,
    title: String,
) -> Result<Option<ResourceRef>, AppError> {
    let extension = format.extension();
    if title.len() > 255
        || title.is_empty()
        || title.chars().any(char::is_control)
        || title.contains('/')
        || title.contains('\\')
        || !title
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(extension))
    {
        return Err(AppError::new(
            "INVALID_EXPORT_TARGET",
            "导出文件名与格式不匹配",
        ));
    }
    let picked = app
        .marklite_mobile()
        .create_export_document(&title, mime(format)?)
        .map_err(|error| AppError::new("ANDROID_EXPORT_FAILED", error.to_string()))?;
    picked
        .uri
        .map(|uri| {
            document_uri(&ResourceRef::AndroidDocument { uri: uri.clone() })?;
            Ok(ResourceRef::AndroidDocument { uri })
        })
        .transpose()
}

pub async fn export(
    app: AppHandle,
    request: ExportRequest,
    source_resource: Option<ResourceRef>,
    target_resource: Option<ResourceRef>,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Validating);
    export_core::validate_payload(&request)?;
    if request.format == ExportFormat::Png {
        return android_png_export::export(
            &app,
            &request,
            source_resource.as_ref(),
            target_resource.as_ref(),
            reporter,
        )
        .await;
    }
    if !matches!(
        request.format,
        ExportFormat::Svg | ExportFormat::Html | ExportFormat::Docx | ExportFormat::Pdf
    ) || request.target_kind != ExportTargetKind::File
    {
        return Err(AppError::new(
            "CAPABILITY_UNAVAILABLE",
            "当前 Android 导出格式尚未实现",
        ));
    }
    let target = target_resource
        .as_ref()
        .ok_or_else(|| AppError::new("INVALID_EXPORT_TARGET", "缺少 Android 导出目标"))?;
    let uri = document_uri(target)?;
    if uri != request.target_path {
        return Err(AppError::new(
            "INVALID_EXPORT_TARGET",
            "导出目标 URI 不匹配",
        ));
    }
    let uri = uri.to_owned();
    let pdf_job = (request.format == ExportFormat::Pdf)
        .then(|| Registration::new(&request.snapshot.job_id))
        .transpose()?;
    let operation = async {
        let staged = StagedArtifact::create(request.format)?;
        let warnings = stage_content(
            &app,
            &request,
            source_resource.as_ref(),
            &staged,
            reporter,
            pdf_job.as_ref().map(|job| &job.control),
        )
        .await?;
        if let Some(job) = &pdf_job {
            job.control.check()?;
        }
        let (bytes, sha256) = staged.fingerprint()?;
        let staged_path = staged.output_string()?;
        if let Some(job) = &pdf_job {
            // Provider commit cannot be rolled back reliably after it begins.
            job.control.commit(|| Ok(()))?;
        }
        reporter.phase(ExportStage::Committing);
        let app_for_write = app.clone();
        let commit_uri = uri.clone();
        let expected_hash = sha256.clone();
        let committed = tauri::async_runtime::spawn_blocking(move || {
            app_for_write
                .marklite_mobile()
                .commit_export_file(ExportFileRequest {
                    uri: &commit_uri,
                    staged_path: &staged_path,
                    bytes,
                    sha256: &expected_hash,
                })
                .map_err(|error| AppError::new("ANDROID_EXPORT_FAILED", error.to_string()))
        })
        .await
        .map_err(|_| AppError::new("EXPORT_TASK_FAILED", "Android provider 提交任务异常结束"))??;
        if committed.uri != uri || committed.bytes != bytes || committed.sha256 != sha256 {
            return Err(AppError::new(
                "INVALID_RESPONSE",
                "Android 导出结果与暂存产物不匹配",
            ));
        }
        reporter.phase(ExportStage::CleaningUp);
        Ok(export_core::result(&request, warnings))
    }
    .await;
    if operation.is_err() {
        let app_for_abort = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            app_for_abort.marklite_mobile().abort_export_document(&uri)
        })
        .await;
    }
    operation
}

async fn stage_content(
    app: &AppHandle,
    request: &ExportRequest,
    source_resource: Option<&ResourceRef>,
    staged: &StagedArtifact,
    reporter: &ExportReporter,
    pdf_control: Option<&CaptureControl>,
) -> Result<Vec<ExportWarning>, AppError> {
    let mut staged_request = request.clone();
    staged_request.target_path = staged.output_string()?;
    export_core::validate_request(&staged_request)?;
    match request.format {
        ExportFormat::Svg => {
            let svg = request
                .mind_map_svg
                .as_deref()
                .ok_or_else(|| AppError::new("INVALID_MIND_MAP_SVG", "脑图 SVG 内容缺失"))?;
            export_service::validate_mind_map_svg(svg)?;
            let output = staged.output().to_path_buf();
            let bytes = svg.as_bytes().to_vec();
            reporter.phase(ExportStage::Writing);
            tauri::async_runtime::spawn_blocking(move || {
                use std::io::Write;
                let mut file = std::fs::File::options()
                    .write(true)
                    .create_new(true)
                    .open(&output)
                    .map_err(|error| AppError::new("EXPORT_TEMP_FAILED", error.to_string()))?;
                file.write_all(&bytes)
                    .and_then(|_| file.sync_all())
                    .map_err(|error| AppError::new("EXPORT_TEMP_FAILED", error.to_string()))
            })
            .await
            .map_err(|_| AppError::new("EXPORT_TASK_FAILED", "Android 暂存任务异常结束"))??;
            Ok(Vec::new())
        }
        ExportFormat::Pdf => {
            android_pdf_export::stage(
                app,
                request,
                source_resource,
                staged,
                reporter,
                pdf_control.ok_or_else(|| AppError::new("EXPORT_TASK_FAILED", "PDF 任务未注册"))?,
            )
            .await
        }
        ExportFormat::Html | ExportFormat::Docx => {
            reporter.phase(ExportStage::Parsing);
            let content = request.snapshot.content.clone();
            let document = tauri::async_runtime::spawn_blocking(move || {
                SemanticDocument::parse(&content, None)
            })
            .await
            .map_err(|_| AppError::new("EXPORT_TASK_FAILED", "Android 文档解析任务异常结束"))?;
            let mode = if request.format == ExportFormat::Html {
                DiagramExportMode::Html
            } else {
                DiagramExportMode::Docx
            };
            let prepared = diagram::prepare(app, &document, mode, reporter).await?;
            let resources = export_resources::resolver(
                app,
                &document,
                source_resource,
                request.options.include_local_images,
            )
            .await?;
            reporter.phase(ExportStage::Writing);
            let silent = ExportReporter::silent(&request.snapshot.job_id, request.format);
            let result = tauri::async_runtime::spawn_blocking(move || {
                match staged_request.format {
                    ExportFormat::Html => export_service::export_html_from_document_with_resources(
                        &staged_request,
                        // The per-job private stage is exclusively owned; Android denies
                        // the hard link used by CreateNew, while Replace uses rename.
                        ExportCommitPolicy::Replace,
                        &document,
                        prepared,
                        resources,
                        || None,
                        &silent,
                    ),
                    ExportFormat::Docx => export_service::export_docx_from_document_with_resources(
                        &staged_request,
                        ExportCommitPolicy::Replace,
                        &document,
                        prepared,
                        resources,
                        || None,
                        &silent,
                    ),
                    _ => unreachable!(),
                }
            })
            .await
            .map_err(|_| AppError::new("EXPORT_TASK_FAILED", "Android 导出暂存任务异常结束"))??;
            Ok(result.warnings)
        }
        _ => Err(AppError::new(
            "CAPABILITY_UNAVAILABLE",
            "当前 Android 导出格式尚未实现",
        )),
    }
}
