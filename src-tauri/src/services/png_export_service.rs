#[cfg(test)]
use super::export_control::CaptureControl;
use super::{
    export_control::{self, Registration},
    export_core, export_html_writer,
    export_progress::{ExportReporter, ExportStage, ExportWork, WorkKind},
    export_resources::ExportResourceResolver,
    export_semantic::SemanticDocument,
    pdf_artifact::PdfWorkspace,
    png_artifact::DirectoryArtifact,
    png_chapters,
};
use crate::models::{
    app_error::AppError,
    export::{ExportRequest, ExportResult},
};

pub fn request_cancel(job_id: &str) -> crate::models::export::ExportCancelStatus {
    export_control::request_cancel(job_id)
}

pub fn cancel(job_id: &str) -> bool {
    export_control::cancel(job_id)
}

pub(crate) async fn export_png(
    renderer: &impl crate::platform::render::ChapterRenderer,
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
        let diagrams = renderer
            .prepare(&document, &rendering_target, &job.control)
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
                workspace.write_html(&renderer.surface(&html))?;
            }
            let image = renderer
                .capture(workspace.html_path(), &job.control, reporter, work)
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
    struct LateRenderer;
    impl crate::platform::render::ChapterRenderer for LateRenderer {
        async fn prepare(
            &self,
            _: &SemanticDocument,
            _: &std::path::Path,
            _: &CaptureControl,
        ) -> Result<crate::models::diagram_assets::PreparedDiagrams, AppError> {
            Ok(crate::models::diagram_assets::PreparedDiagrams::empty())
        }
        fn surface(&self, html: &str) -> String {
            html.to_owned()
        }
        async fn capture(
            &self,
            _: &std::path::Path,
            control: &CaptureControl,
            _: &ExportReporter,
            _: ExportWork,
        ) -> Result<crate::services::export_control::CapturedImage, AppError> {
            assert!(control.cancel());
            Ok(crate::services::export_control::CapturedImage {
                bytes: vec![],
                width: 1,
                height: 1,
            })
        }
    }

    #[test]
    fn late_platform_result_after_cancel_never_publishes_a_directory() {
        let directory = crate::utils::test_support::TestDirectory::new("png-late-result");
        let target = directory.path().join("chapters");
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../src/shared/desktop-contract-fixtures.json"
        ))
        .unwrap();
        let mut request: ExportRequest =
            serde_json::from_value(fixture["exportRequest"].clone()).unwrap();
        request.snapshot.job_id = "late-platform-result".into();
        request.snapshot.title = "chapters.md".into();
        request.snapshot.content = "# Chapter\n\nText".into();
        request.snapshot.source_path = None;
        request.target_path = target.to_str().unwrap().into();
        request.target_kind = crate::models::export::ExportTargetKind::Directory;
        request.format = crate::models::export::ExportFormat::Png;
        let reporter = ExportReporter::silent(&request.snapshot.job_id, request.format);
        let error = tauri::async_runtime::block_on(export_png(&LateRenderer, &request, &reporter))
            .unwrap_err();
        assert_eq!(error.code, "EXPORT_CANCELLED");
        assert!(!target.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        assert!(!cancel(&request.snapshot.job_id));
    }

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

    #[test]
    fn acknowledgements_distinguish_registration_and_commit_boundaries() {
        use crate::models::export::ExportCancelStatus::*;
        assert_eq!(request_cancel("ack-cancel"), NotRunning);
        let job = Registration::new("ack-cancel").unwrap();
        assert_eq!(request_cancel("ack-cancel"), Requested);
        assert_eq!(request_cancel("ack-cancel"), Requested);
        assert!(job.control.commit(|| panic!("must not publish")).is_err());
        drop(job);
        assert_eq!(request_cancel("ack-cancel"), NotRunning);
        let job = Registration::new("ack-committed").unwrap();
        job.control.commit(|| Ok(())).unwrap();
        assert_eq!(request_cancel("ack-committed"), TooLate);
    }
}
