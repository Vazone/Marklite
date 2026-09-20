use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use super::{
    diagram_export_service::{self, DiagramExportMode},
    export_core, export_html_writer,
    export_progress::{ExportReporter, ExportStage, ExportWork, WorkKind},
    export_resources::ExportResourceResolver,
    export_semantic::SemanticDocument,
    pdf_artifact::PdfWorkspace,
    png_artifact::DirectoryArtifact,
    png_capture::{self, CaptureControl},
    png_chapters,
};
use crate::models::{
    app_error::AppError,
    export::{ExportRequest, ExportResult},
};

fn jobs() -> &'static Mutex<HashMap<String, CaptureControl>> {
    static JOBS: OnceLock<Mutex<HashMap<String, CaptureControl>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

struct Registration {
    id: String,
    control: CaptureControl,
}
impl Registration {
    fn new(id: &str) -> Result<Self, AppError> {
        let mut jobs = jobs().lock().unwrap_or_else(|e| e.into_inner());
        if jobs.contains_key(id) {
            return Err(AppError::new("EXPORT_JOB_EXISTS", "导出任务标识已在使用"));
        }
        let control = CaptureControl::default();
        jobs.insert(id.to_owned(), control.clone());
        Ok(Self {
            id: id.to_owned(),
            control,
        })
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.control.cancel();
        jobs()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}

pub fn cancel(job_id: &str) -> bool {
    jobs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(job_id)
        .is_some_and(CaptureControl::cancel)
}

pub async fn export_png(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Validating);
    let target = export_core::validate_request(request)?;
    let job = Registration::new(&request.snapshot.job_id)?;
    reporter.phase(ExportStage::Parsing);
    let document = SemanticDocument::parse(
        &request.snapshot.content,
        request.snapshot.source_path.as_deref(),
    );
    let chapters = png_chapters::plan(&document)?;
    job.control.check()?;
    let mut output = DirectoryArtifact::create(target)?;
    let warnings = {
        let rendering_target = output.stage().join("capture.png");
        reporter.phase(ExportStage::Resources);
        let diagrams = diagram_export_service::prepare(
            app,
            &document,
            &rendering_target,
            false,
            None,
            || job.control.check().err(),
            DiagramExportMode::Html,
        )
        .await?;
        let workspace = PdfWorkspace::create(&rendering_target)?;
        let mut resources = ExportResourceResolver::new(
            request.snapshot.source_path.as_deref(),
            request.options.include_local_images,
        );
        for (index, chapter) in chapters.iter().enumerate() {
            job.control.check()?;
            workspace.clear_part()?;
            let work = ExportWork {
                kind: WorkKind::Chapter,
                index: index as u32 + 1,
                total: chapters.len() as u32,
                completed: index as u32,
            };
            reporter.processing(ExportStage::Rendering, work);
            {
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
                workspace.write_html(&png_capture::image_surface(&html))?;
            }
            let image =
                png_capture::capture(app, workspace.html_path(), &job.control, reporter, work)
                    .await
                    .map_err(|error| {
                        AppError::new(
                            error.code,
                            format!(
                                "第 {} 章（{}）：{}",
                                index + 1,
                                chapter.title,
                                error.message
                            ),
                        )
                    })?;
            job.control.check()?;
            reporter.processing(ExportStage::Writing, work);
            output.write_png(&chapter.filename, &image.bytes, image.width, image.height)?;
            reporter.processing(
                ExportStage::Writing,
                ExportWork {
                    completed: index as u32 + 1,
                    ..work
                },
            );
        }
        reporter.phase(ExportStage::CleaningUp);
        let mut warnings = resources.into_warnings();
        warnings.extend(diagrams.warnings);
        warnings
    };
    reporter.phase(ExportStage::Committing);
    job.control.commit(|| output.commit())?;
    Ok(export_core::result(request, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn job_identity_and_cancellation_have_one_commit_boundary() {
        let job = Registration::new("image-cancel-test").unwrap();
        assert!(Registration::new("image-cancel-test").is_err());
        assert!(cancel("image-cancel-test"));
        assert_eq!(
            job.control
                .commit(|| panic!("cancelled job must not commit"))
                .unwrap_err()
                .code,
            "EXPORT_CANCELLED"
        );
        drop(job);
        assert!(!cancel("image-cancel-test"));
        let job = Registration::new("image-commit-test").unwrap();
        job.control.commit(|| Ok(())).unwrap();
        assert!(!cancel("image-commit-test"));
    }
}
