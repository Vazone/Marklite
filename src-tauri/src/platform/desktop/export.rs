//! Desktop WebView orchestration. Shared writers and commit logic receive prepared assets.
use crate::{
    models::{
        app_error::AppError,
        export::{ExportRequest, ExportResult},
    },
    services::{
        diagram_export_service::{self, DiagramExportMode, PreparedDiagrams},
        export_core::{self, ExportCommitPolicy},
        export_progress::{ExportReporter, ExportStage},
        export_semantic::SemanticDocument,
        export_service::{
            export_docx_from_document_controlled, export_html_from_document_controlled,
        },
    },
};

pub async fn export_html_with_runtime(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    isolate_profile: bool,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Parsing);
    let document = SemanticDocument::parse(
        &request.snapshot.content,
        request.snapshot.source_path.as_deref(),
    );
    export_html_with_runtime_document(
        app,
        request,
        policy,
        isolate_profile,
        &document,
        || None,
        reporter,
    )
    .await
}

pub(crate) async fn export_html_with_runtime_document(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    isolate_profile: bool,
    document: &SemanticDocument,
    cancelled: impl Fn() -> Option<AppError> + Send + Sync,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Validating);
    let target = export_core::validate_request(request)?;
    export_core::preflight_commit(request, policy)?;
    reporter.phase(ExportStage::Resources);
    if document.diagram_sources().is_empty() {
        return export_html_from_document_controlled(
            request,
            policy,
            document,
            PreparedDiagrams::empty(),
            cancelled,
            reporter,
        );
    }
    let prepared = diagram_export_service::prepare(
        app,
        document,
        &target,
        isolate_profile,
        None,
        &cancelled,
        DiagramExportMode::Html,
    )
    .await?;
    export_html_from_document_controlled(request, policy, document, prepared, cancelled, reporter)
}

pub async fn export_docx_with_runtime(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    isolate_profile: bool,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Parsing);
    let document = SemanticDocument::parse(
        &request.snapshot.content,
        request.snapshot.source_path.as_deref(),
    );
    export_docx_with_runtime_document(
        app,
        request,
        policy,
        isolate_profile,
        &document,
        || None,
        reporter,
    )
    .await
}

pub(crate) async fn export_docx_with_runtime_document(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    isolate_profile: bool,
    document: &SemanticDocument,
    cancelled: impl Fn() -> Option<AppError> + Send + Sync,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Validating);
    let target = export_core::validate_request(request)?;
    export_core::preflight_commit(request, policy)?;
    reporter.phase(ExportStage::Resources);
    if document.diagram_sources().is_empty() {
        return export_docx_from_document_controlled(
            request,
            policy,
            document,
            PreparedDiagrams::empty(),
            cancelled,
            reporter,
        );
    }
    let prepared = diagram_export_service::prepare(
        app,
        document,
        &target,
        isolate_profile,
        None,
        &cancelled,
        DiagramExportMode::Docx,
    )
    .await?;
    export_docx_from_document_controlled(request, policy, document, prepared, cancelled, reporter)
}
