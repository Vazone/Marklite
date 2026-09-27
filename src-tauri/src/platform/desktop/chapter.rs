use crate::{
    models::{app_error::AppError, diagram_assets::PreparedDiagrams},
    platform::render::ChapterRenderer,
    services::{
        diagram_export_service::{self, DiagramExportMode},
        export_control::{CaptureControl, CapturedImage},
        export_progress::{ExportReporter, ExportWork},
        export_semantic::SemanticDocument,
        png_capture,
    },
};
use std::path::Path;

pub(crate) struct DesktopChapterRenderer<'a>(pub &'a tauri::AppHandle);
impl ChapterRenderer for DesktopChapterRenderer<'_> {
    async fn prepare(
        &self,
        document: &SemanticDocument,
        target: &Path,
        control: &CaptureControl,
    ) -> Result<PreparedDiagrams, AppError> {
        diagram_export_service::prepare(
            self.0,
            document,
            target,
            false,
            None,
            || control.check().err(),
            DiagramExportMode::Html,
        )
        .await
    }
    fn surface(&self, html: &str) -> String {
        png_capture::image_surface(html)
    }
    async fn capture(
        &self,
        html: &Path,
        control: &CaptureControl,
        reporter: &ExportReporter,
        work: ExportWork,
    ) -> Result<CapturedImage, AppError> {
        png_capture::capture(self.0, html, control, reporter, work).await
    }
}
