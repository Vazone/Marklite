use crate::{
    models::{app_error::AppError, diagram_assets::PreparedDiagrams},
    services::{
        export_control::{CaptureControl, CapturedImage},
        export_progress::{ExportReporter, ExportWork},
        export_semantic::SemanticDocument,
    },
};
use std::{future::Future, path::Path};

/// The chapter pipeline owns staging and commit; the platform owns rendering and cleanup.
pub(crate) trait ChapterRenderer: Sync {
    fn prepare<'a>(
        &'a self,
        document: &'a SemanticDocument,
        target: &'a Path,
        control: &'a CaptureControl,
    ) -> impl Future<Output = Result<PreparedDiagrams, AppError>> + Send + 'a;
    fn surface(&self, html: &str) -> String;
    fn capture<'a>(
        &'a self,
        html: &'a Path,
        control: &'a CaptureControl,
        reporter: &'a ExportReporter,
        work: ExportWork,
    ) -> impl Future<Output = Result<CapturedImage, AppError>> + Send + 'a;
}

use crate::{
    models::export::ExportRequest,
    services::pdf_export_service::{
        PdfExportControl, PdfRenderJob, PdfRenderPaths, PdfRenderState,
    },
};
use std::time::Instant;

pub(crate) trait PdfRenderer: Sync {
    fn prepare<'a>(
        &'a self,
        document: &'a SemanticDocument,
        target: &'a Path,
        isolate: bool,
        deadline: Option<Instant>,
        control: &'a PdfExportControl,
    ) -> impl Future<Output = Result<PreparedDiagrams, AppError>> + Send + 'a;
    fn render<'a>(
        &'a self,
        request: &'a ExportRequest,
        paths: PdfRenderPaths<'a>,
        job: PdfRenderJob,
        control: &'a PdfExportControl,
        deadline: Option<Instant>,
        progress: (&'a ExportReporter, ExportWork),
    ) -> impl Future<Output = Result<PdfRenderState, AppError>> + Send + 'a;
}
