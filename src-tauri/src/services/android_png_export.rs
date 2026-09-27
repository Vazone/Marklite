use sha2::{Digest, Sha256};
use tauri_plugin_marklite_mobile::{CapturePngRequest, ExportFileRequest, MarkliteMobileExt};

use crate::{
    models::{
        app_error::AppError,
        export::{ExportRequest, ExportResult, ExportTargetKind},
        resource::ResourceRef,
    },
    platform::android::{diagram, export::StagedArtifact, export_resources},
};

use super::{
    diagram_export_service::DiagramExportMode,
    export_control::Registration,
    export_core, export_html_writer,
    export_progress::{ExportReporter, ExportStage, ExportWork, WorkKind},
    export_semantic::SemanticDocument,
    png_artifact, png_chapters,
};

struct ImageFile {
    name: String,
    path: String,
    bytes: u64,
    sha256: String,
}

pub(crate) async fn export(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    source_resource: Option<&ResourceRef>,
    target_resource: Option<&ResourceRef>,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    export_core::validate_payload(request)?;
    if request.target_kind != ExportTargetKind::Directory {
        return Err(AppError::new(
            "CAPABILITY_UNAVAILABLE",
            "Android 图片导出目标或本地图片选项不可用",
        ));
    }
    let Some(ResourceRef::AndroidTree { uri }) = target_resource else {
        return Err(AppError::new(
            "INVALID_EXPORT_TARGET",
            "图片导出需要 Android 目录授权",
        ));
    };
    if uri != &request.target_path || uri.len() > 4096 || !uri.starts_with("content://") {
        return Err(AppError::new(
            "INVALID_EXPORT_TARGET",
            "图片导出目录 URI 不匹配",
        ));
    }
    let target = uri.clone();
    let folder_name = png_artifact::directory_name(&request.snapshot.title);
    let job = Registration::new(&request.snapshot.job_id)?;
    let staged = StagedArtifact::create(request.format)?;
    reporter.phase(ExportStage::Parsing);
    let content = request.snapshot.content.clone();
    let document =
        tauri::async_runtime::spawn_blocking(move || SemanticDocument::parse(&content, None))
            .await
            .map_err(|_| AppError::new("EXPORT_TASK_FAILED", "Android 图片解析任务异常结束"))?;
    let chapters = png_chapters::plan(&document)?;
    if chapters.len() > 100 {
        return Err(AppError::new(
            "PNG_TOO_MANY_CHAPTERS",
            "Android 单次最多导出 100 章图片",
        ));
    }
    job.control.check()?;
    let diagrams = diagram::prepare(app, &document, DiagramExportMode::Html, reporter).await?;
    let mut resources = export_resources::resolver(
        app,
        &document,
        source_resource,
        request.options.include_local_images,
    )
    .await?;
    let mut files = Vec::with_capacity(chapters.len());
    let mut total_bytes = 0_u64;
    for (index, chapter) in chapters.iter().enumerate() {
        job.control.check()?;
        let work = ExportWork {
            kind: WorkKind::Chapter,
            index: index as u32 + 1,
            total: chapters.len() as u32,
            completed: index as u32,
        };
        reporter.processing(ExportStage::Rendering, work);
        let body = export_html_writer::render_roots_with_resources(
            &document,
            &chapter.roots,
            request,
            &diagrams.artifacts,
            &mut resources,
            false,
        );
        let html = export_html_writer::standalone_with_title(
            request,
            &body,
            None,
            request.options.include_title,
        );
        let path = staged.image_path(&chapter.filename)?;
        let path_string = path
            .to_str()
            .ok_or_else(|| AppError::new("EXPORT_TEMP_FAILED", "图片暂存路径不是 UTF-8"))?
            .to_owned();
        let app_for_capture = app.clone();
        let capture_path = path_string.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            app_for_capture
                .marklite_mobile()
                .capture_export_png(CapturePngRequest {
                    html: &html,
                    staged_path: &capture_path,
                })
                .map_err(|error| AppError::new("PNG_CAPTURE_FAILED", error.to_string()))
        })
        .await
        .map_err(|_| AppError::new("PNG_CAPTURE_FAILED", "Android 图片捕获任务异常结束"))?;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                job.control.check()?;
                return Err(error);
            }
        };
        job.control.check()?;
        reporter.processing(ExportStage::Writing, work);
        let bytes = std::fs::read(&path)
            .map_err(|error| AppError::new("PNG_WRITE_FAILED", error.to_string()))?;
        png_artifact::validate_png(&bytes, result.width, result.height)?;
        if bytes.len() as u64 != result.bytes {
            return Err(AppError::new(
                "PNG_INVALID_OUTPUT",
                "Android 图片大小与平台报告不符",
            ));
        }
        total_bytes = total_bytes
            .checked_add(result.bytes)
            .filter(|value| *value <= 64 * 1024 * 1024)
            .ok_or_else(|| {
                AppError::new("PNG_OUTPUT_TOO_LARGE", "Android 图片总输出超过 64 MiB")
            })?;
        files.push(ImageFile {
            name: chapter.filename.clone(),
            path: path_string,
            bytes: result.bytes,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        });
        reporter.processing(
            ExportStage::Writing,
            ExportWork {
                completed: index as u32 + 1,
                ..work
            },
        );
    }
    let mut warnings = resources.into_warnings();
    warnings.extend(diagrams.warnings);
    job.control.check()?;
    reporter.phase(ExportStage::Committing);
    let app_for_begin = app.clone();
    let folder = tauri::async_runtime::spawn_blocking(move || {
        app_for_begin
            .marklite_mobile()
            .begin_image_export(&target, &folder_name)
            .map_err(|error| AppError::new("PNG_COMMIT_FAILED", error.to_string()))
    })
    .await
    .map_err(|_| AppError::new("PNG_COMMIT_FAILED", "Android 图片目录任务异常结束"))??;
    let folder_uri = folder.uri;
    let operation = async {
        for (index, image) in files.into_iter().enumerate() {
            job.control.check()?;
            let app_for_create = app.clone();
            let folder = folder_uri.clone();
            let name = image.name.clone();
            let created = tauri::async_runtime::spawn_blocking(move || {
                app_for_create
                    .marklite_mobile()
                    .create_image_file(&folder, &name)
                    .map_err(|error| AppError::new("PNG_COMMIT_FAILED", error.to_string()))
            })
            .await
            .map_err(|_| AppError::new("PNG_COMMIT_FAILED", "Android 图片文件任务异常结束"))??;
            let app_for_commit = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                app_for_commit
                    .marklite_mobile()
                    .commit_export_file(ExportFileRequest {
                        uri: &created.uri,
                        staged_path: &image.path,
                        bytes: image.bytes,
                        sha256: &image.sha256,
                    })
                    .map_err(|error| AppError::new("PNG_COMMIT_FAILED", error.to_string()))
            })
            .await
            .map_err(|_| AppError::new("PNG_COMMIT_FAILED", "Android 图片提交任务异常结束"))??;
            reporter.processing(
                ExportStage::Committing,
                ExportWork {
                    kind: WorkKind::Chapter,
                    index: index as u32 + 1,
                    total: chapters.len() as u32,
                    completed: index as u32 + 1,
                },
            );
        }
        job.control.check()?;
        let app_for_finish = app.clone();
        let folder = folder_uri.clone();
        tauri::async_runtime::spawn_blocking(move || {
            app_for_finish
                .marklite_mobile()
                .finish_image_export(&folder)
                .map_err(|error| AppError::new("PNG_COMMIT_FAILED", error.to_string()))
        })
        .await
        .map_err(|_| AppError::new("PNG_COMMIT_FAILED", "Android 图片完成任务异常结束"))??;
        Ok(())
    }
    .await;
    if operation.is_err() {
        let app_for_abort = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            app_for_abort
                .marklite_mobile()
                .abort_image_export(&folder_uri)
        })
        .await;
    }
    operation?;
    reporter.phase(ExportStage::CleaningUp);
    Ok(export_core::result(request, warnings))
}
