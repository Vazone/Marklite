use crate::{
    models::{app_error::AppError, diagram_assets::PreparedDiagrams, export::ExportRequest},
    platform::render::PdfRenderer,
    services::{
        diagram_export_service::{self, DiagramExportMode},
        export_progress::{ExportReporter, ExportWork},
        export_semantic::SemanticDocument,
        pdf_export_service::{PdfExportControl, PdfRenderJob, PdfRenderPaths, PdfRenderState},
    },
};
use std::{path::Path, time::Instant};

pub(crate) struct DesktopPdfRenderer<'a>(pub &'a tauri::AppHandle);
impl PdfRenderer for DesktopPdfRenderer<'_> {
    async fn prepare(
        &self,
        document: &SemanticDocument,
        target: &Path,
        isolate: bool,
        deadline: Option<Instant>,
        control: &PdfExportControl,
    ) -> Result<PreparedDiagrams, AppError> {
        diagram_export_service::prepare(
            self.0,
            document,
            target,
            isolate,
            deadline,
            || control.terminal_error(),
            DiagramExportMode::Pdf,
        )
        .await
    }
    async fn render(
        &self,
        request: &ExportRequest,
        paths: PdfRenderPaths<'_>,
        job: PdfRenderJob,
        control: &PdfExportControl,
        deadline: Option<Instant>,
        progress: (&ExportReporter, ExportWork),
    ) -> Result<PdfRenderState, AppError> {
        super::pdf_native::export_pdf_inner(
            self.0, request, paths, job, control, deadline, progress,
        )
        .await
    }
}
