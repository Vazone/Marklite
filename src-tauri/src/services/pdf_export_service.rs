use super::export_progress::{ExportReporter, ExportStage, ExportWork, WorkKind};
use std::{
    fs::File,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use crate::{
    models::{
        app_error::AppError,
        export::{ExportRequest, ExportResult},
    },
    services::{
        export_html_writer,
        export_resources::ExportResourceResolver,
        export_semantic::SemanticDocument,
        export_service,
        export_service::ExportCommitPolicy,
        pdf_artifact::{commit_pdf_file_with_policy, PdfWorkspace},
        pdf_assembler::PdfAssembler,
        pdf_chunks, pdf_ready_protocol,
    },
};

static PDF_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub const DEFAULT_PDF_TIMEOUT_MS: u64 = 240_000;

#[derive(Clone)]
pub struct PdfExportControl {
    state: Arc<Mutex<PdfExportControlState>>,
}

enum PdfExportControlState {
    Running,
    Cancelled(AppError),
    Committed,
}

impl PdfExportControl {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(PdfExportControlState::Running)),
        }
    }

    pub fn cancel(&self, error: AppError) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !matches!(*state, PdfExportControlState::Running) {
            return false;
        }
        *state = PdfExportControlState::Cancelled(error);
        true
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn is_running(&self) -> bool {
        matches!(
            *self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            PdfExportControlState::Running
        )
    }

    #[cfg_attr(test, allow(dead_code))]
    pub fn terminal_error(&self) -> Option<AppError> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match &*state {
            PdfExportControlState::Cancelled(error) => Some(error.clone()),
            PdfExportControlState::Running | PdfExportControlState::Committed => None,
        }
    }

    pub(crate) fn check_running(&self) -> Result<(), AppError> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match &*state {
            PdfExportControlState::Running => Ok(()),
            PdfExportControlState::Cancelled(error) => Err(error.clone()),
            PdfExportControlState::Committed => Err(AppError::new(
                "PDF_EXPORT_STATE_INVALID",
                "PDF 导出任务已提交，不能重复执行",
            )),
        }
    }

    fn commit(
        &self,
        source: &mut File,
        target: &Path,
        target_display: &str,
        policy: ExportCommitPolicy,
    ) -> Result<(), AppError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match &*state {
            PdfExportControlState::Cancelled(error) => return Err(error.clone()),
            PdfExportControlState::Committed => {
                return Err(AppError::new(
                    "PDF_EXPORT_STATE_INVALID",
                    "PDF 导出任务已提交，不能重复执行",
                ))
            }
            PdfExportControlState::Running => {}
        }
        commit_pdf_file_with_policy(source, target, target_display, policy)?;
        *state = PdfExportControlState::Committed;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct PdfRenderJob {
    pub(crate) label: String,
    pub(crate) ready_token: String,
}

pub(crate) struct PdfRenderPaths<'a> {
    pub(crate) html: &'a Path,
    pub(crate) pdf: &'a Path,
    pub(crate) webview_data: Option<&'a Path>,
}

impl PdfRenderJob {
    fn allocate() -> Self {
        let sequence = PDF_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let process_id = std::process::id();
        Self {
            label: format!("export-{process_id}-{sequence}"),
            ready_token: format!("{process_id}-{sequence}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PdfRenderPhase {
    Created,
    PageLoaded,
    DomReady,
    FontsSettled,
    ImagesSettled,
    LayoutReady,
    Printing,
    PlatformPrinted,
    Committed,
}

impl PdfRenderPhase {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match pdf_ready_protocol::RENDER_PHASES
            .iter()
            .position(|phase| *phase == value)?
        {
            0 => Some(Self::PageLoaded),
            1 => Some(Self::DomReady),
            2 => Some(Self::FontsSettled),
            3 => Some(Self::ImagesSettled),
            4 => Some(Self::LayoutReady),
            _ => None,
        }
    }

    pub(crate) fn wire_name(self) -> Option<&'static str> {
        match self {
            Self::PageLoaded => Some(pdf_ready_protocol::RENDER_PHASES[0]),
            Self::DomReady => Some(pdf_ready_protocol::RENDER_PHASES[1]),
            Self::FontsSettled => Some(pdf_ready_protocol::RENDER_PHASES[2]),
            Self::ImagesSettled => Some(pdf_ready_protocol::RENDER_PHASES[3]),
            Self::LayoutReady => Some(pdf_ready_protocol::RENDER_PHASES[4]),
            Self::Created | Self::Printing | Self::PlatformPrinted | Self::Committed => None,
        }
    }

    pub(crate) fn next(self) -> Option<Self> {
        match self {
            Self::Created => Some(Self::PageLoaded),
            Self::PageLoaded => Some(Self::DomReady),
            Self::DomReady => Some(Self::FontsSettled),
            Self::FontsSettled => Some(Self::ImagesSettled),
            Self::ImagesSettled => Some(Self::LayoutReady),
            Self::LayoutReady => Some(Self::Printing),
            Self::Printing => Some(Self::PlatformPrinted),
            Self::PlatformPrinted => Some(Self::Committed),
            Self::Committed => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct PdfRenderState {
    pub(crate) phase: PdfRenderPhase,
}

impl PdfRenderState {
    pub(crate) fn created() -> Self {
        Self {
            phase: PdfRenderPhase::Created,
        }
    }

    pub(crate) fn advance(&mut self, next: PdfRenderPhase) -> Result<(), AppError> {
        if self.phase.next() != Some(next) {
            return Err(AppError::new(
                "PDF_READY_PROTOCOL_INVALID",
                "PDF 渲染阶段顺序无效",
            ));
        }
        self.phase = next;
        Ok(())
    }
}

#[cfg(desktop)]
pub async fn export_pdf(
    app: tauri::AppHandle,
    request: &ExportRequest,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    export_pdf_controlled(
        app,
        request,
        ExportCommitPolicy::Replace,
        PdfExportControl::new(),
        Some(Duration::from_millis(DEFAULT_PDF_TIMEOUT_MS)),
        false,
        reporter,
    )
    .await
}

#[cfg(desktop)]
pub async fn export_pdf_controlled(
    app: tauri::AppHandle,
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    control: PdfExportControl,
    total_timeout: Option<Duration>,
    isolate_webview_profile: bool,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    let renderer = crate::platform::desktop::pdf::DesktopPdfRenderer(&app);
    export_pdf_with_renderer(
        &renderer,
        request,
        policy,
        control,
        total_timeout,
        isolate_webview_profile,
        reporter,
    )
    .await
}

pub(crate) async fn export_pdf_with_renderer(
    renderer: &impl crate::platform::render::PdfRenderer,
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    control: PdfExportControl,
    total_timeout: Option<Duration>,
    isolate_webview_profile: bool,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    let deadline = total_timeout.and_then(|timeout| Instant::now().checked_add(timeout));
    control.check_running()?;
    reporter.phase(ExportStage::Validating);
    let target = export_service::validate_request(request)?;
    reporter.phase(ExportStage::Parsing);
    let mut document = SemanticDocument::parse(
        &request.snapshot.content,
        request.snapshot.source_path.as_deref(),
    );
    pdf_chunks::split_large_text_blocks(&mut document);

    let ranges = pdf_chunks::plan(&document)?;

    reporter.phase(ExportStage::Resources);
    let diagrams = renderer
        .prepare(
            &document,
            &target,
            isolate_webview_profile,
            deadline,
            &control,
        )
        .await?;
    check_deadline(&control, deadline)?;

    let workspace = PdfWorkspace::create(&target)?;
    if isolate_webview_profile {
        workspace.register_post_runtime_cleanup();
    }
    let mut resources = ExportResourceResolver::new(
        request.snapshot.source_path.as_deref(),
        request.options.include_local_images,
    );
    let artifacts = if diagrams.print_artifacts.is_empty() {
        &diagrams.artifacts
    } else {
        &diagrams.print_artifacts
    };
    let total_parts = ranges.len() as u32;
    let multiple = ranges.len() > 1;
    let mut assembler = if multiple {
        Some(PdfAssembler::create(workspace.merged_pdf_path())?)
    } else {
        None
    };
    let mut last_state = None;
    for (index, range) in ranges.into_iter().enumerate() {
        check_deadline(&control, deadline)?;
        workspace.clear_part()?;
        let work = ExportWork {
            kind: WorkKind::Part,
            index: index as u32 + 1,
            total: total_parts,
            completed: index as u32,
        };
        reporter.processing(ExportStage::Rendering, work);
        let job = PdfRenderJob::allocate();
        {
            let body = export_html_writer::render_roots_with_resources(
                &document,
                &document.roots()[range],
                request,
                artifacts,
                &mut resources,
                multiple,
            );
            let html = export_html_writer::standalone_with_title(
                request,
                &body,
                Some(&job.ready_token),
                index == 0 && request.options.include_title,
            );
            workspace.write_html(&html)?;
        }

        check_deadline(&control, deadline)?;
        last_state = Some(
            renderer
                .render(
                    request,
                    PdfRenderPaths {
                        html: workspace.html_path(),
                        pdf: workspace.pdf_path(),
                        webview_data: isolate_webview_profile
                            .then(|| workspace.webview_data_path()),
                    },
                    job,
                    &control,
                    deadline,
                    (reporter, work),
                )
                .await?,
        );
        check_deadline(&control, deadline)?;
        reporter.processing(
            ExportStage::Printing,
            ExportWork {
                completed: index as u32 + 1,
                ..work
            },
        );
        if let Some(assembler) = assembler.as_mut() {
            reporter.processing(
                ExportStage::Merging,
                ExportWork {
                    completed: index as u32 + 1,
                    ..work
                },
            );
            assembler.append(workspace.pdf_path())?;
        }
    }
    let mut warnings = resources.into_warnings();
    warnings.extend(diagrams.warnings);
    drop(document);
    if let Some(assembler) = assembler {
        reporter.phase(ExportStage::Merging);
        assembler.finish()?;
    }
    check_deadline(&control, deadline)?;
    let source = if multiple {
        workspace.merged_pdf_path()
    } else {
        workspace.pdf_path()
    };
    reporter.phase(ExportStage::Validating);
    let mut pdf =
        File::open(source).map_err(|error| AppError::file_read_failed("PDF 平台产物", error))?;
    reporter.phase(ExportStage::Committing);
    control.commit(&mut pdf, &target, &request.target_path, policy)?;
    if let Some(mut state) = last_state {
        state.advance(PdfRenderPhase::Committed)?;
    }
    reporter.phase(ExportStage::CleaningUp);
    drop(pdf);
    drop(workspace);
    Ok(ExportResult {
        job_id: request.snapshot.job_id.clone(),
        format: request.format,
        path: request.target_path.clone(),
        target_kind: request.target_kind,
        warnings,
    })
}

pub(crate) fn check_deadline(
    control: &PdfExportControl,
    deadline: Option<Instant>,
) -> Result<(), AppError> {
    control.check_running()?;
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        let error = AppError::new("PDF_EXPORT_TIMEOUT", "PDF 导出超过全程时限");
        control.cancel(error.clone());
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::TestPath;
    use std::fs;
    struct CancelRenderer;
    impl crate::platform::render::PdfRenderer for CancelRenderer {
        async fn prepare(
            &self,
            _: &super::SemanticDocument,
            _: &std::path::Path,
            _: bool,
            _: Option<std::time::Instant>,
            _: &PdfExportControl,
        ) -> Result<crate::models::diagram_assets::PreparedDiagrams, AppError> {
            Ok(crate::models::diagram_assets::PreparedDiagrams::empty())
        }
        async fn render(
            &self,
            _: &super::ExportRequest,
            _: super::PdfRenderPaths<'_>,
            _: super::PdfRenderJob,
            control: &PdfExportControl,
            _: Option<std::time::Instant>,
            _: (&super::ExportReporter, super::ExportWork),
        ) -> Result<super::PdfRenderState, AppError> {
            let error = AppError::new("EXPORT_CANCELLED", "test cancellation");
            control.cancel(error.clone());
            Err(error)
        }
    }

    #[test]
    fn platform_cancellation_preserves_target_and_cleans_workspace() {
        let directory = crate::utils::test_support::TestDirectory::new("pdf-port-cancel");
        let target = directory.path().join("output.pdf");
        fs::write(&target, "original").unwrap();
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../src/shared/desktop-contract-fixtures.json"
        ))
        .unwrap();
        let mut request: super::ExportRequest =
            serde_json::from_value(fixture["exportRequest"].clone()).unwrap();
        request.snapshot.source_path = None;
        request.snapshot.content = "# Plain".into();
        request.format = crate::models::export::ExportFormat::Pdf;
        request.target_path = target.to_str().unwrap().into();
        let reporter = super::ExportReporter::silent(&request.snapshot.job_id, request.format);
        let error = tauri::async_runtime::block_on(super::export_pdf_with_renderer(
            &CancelRenderer,
            &request,
            ExportCommitPolicy::Replace,
            PdfExportControl::new(),
            None,
            false,
            &reporter,
        ))
        .unwrap_err();
        assert_eq!(error.code, "EXPORT_CANCELLED");
        assert_eq!(fs::read_to_string(&target).unwrap(), "original");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn cancellation_gate_prevents_a_late_target_commit() {
        let source_path = TestPath::new("pdf-control", "source.pdf");
        let target = TestPath::new("pdf-control", "target.pdf");
        fs::write(&source_path, b"late-platform-output").unwrap();
        fs::write(&target, b"keep-existing").unwrap();
        let mut source = fs::File::open(&source_path).unwrap();
        let control = PdfExportControl::new();
        assert!(control.cancel(AppError::new("PDF_EXPORT_CANCELLED", "cancelled by test")));

        let error = control
            .commit(
                &mut source,
                &target,
                &target.to_string_lossy(),
                ExportCommitPolicy::Replace,
            )
            .unwrap_err();

        assert_eq!(error.code, "PDF_EXPORT_CANCELLED");
        assert_eq!(fs::read(&target).unwrap(), b"keep-existing");
        assert!(!control.cancel(AppError::new("LATE_CANCEL", "too late")));
    }
}
