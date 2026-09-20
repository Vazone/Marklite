use super::export_progress::{ExportReporter, ExportStage, ExportWork, WorkKind};
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

use tauri::WebviewUrl;

use crate::{
    models::{
        app_error::AppError,
        export::{
            ExportMarginPreset, ExportOptions, ExportOrientation, ExportPaperSize, ExportRequest,
            ExportResult,
        },
    },
    services::{
        diagram_export_service, export_html_writer,
        export_resources::ExportResourceResolver,
        export_semantic::SemanticDocument,
        export_service,
        export_service::ExportCommitPolicy,
        pdf_artifact::{commit_pdf_file_with_policy, PdfWorkspace},
        pdf_assembler::PdfAssembler,
        pdf_chunks,
        pdf_platform_job::PdfPlatformJob,
        pdf_ready_protocol,
    },
    utils::path_utils::path_to_utf8,
};

static PDF_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const RENDER_TIMEOUT: Duration = Duration::from_secs(20);
const PRINT_TIMEOUT: Duration = Duration::from_secs(45);
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

    fn check_running(&self) -> Result<(), AppError> {
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
struct PdfRenderJob {
    label: String,
    ready_token: String,
}

struct PdfRenderPaths<'a> {
    html: &'a Path,
    pdf: &'a Path,
    webview_data: Option<&'a Path>,
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
enum PdfRenderPhase {
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
    fn parse(value: &str) -> Option<Self> {
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

    fn wire_name(self) -> Option<&'static str> {
        match self {
            Self::PageLoaded => Some(pdf_ready_protocol::RENDER_PHASES[0]),
            Self::DomReady => Some(pdf_ready_protocol::RENDER_PHASES[1]),
            Self::FontsSettled => Some(pdf_ready_protocol::RENDER_PHASES[2]),
            Self::ImagesSettled => Some(pdf_ready_protocol::RENDER_PHASES[3]),
            Self::LayoutReady => Some(pdf_ready_protocol::RENDER_PHASES[4]),
            Self::Created | Self::Printing | Self::PlatformPrinted | Self::Committed => None,
        }
    }

    fn next(self) -> Option<Self> {
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
struct PdfRenderState {
    phase: PdfRenderPhase,
}

impl PdfRenderState {
    fn created() -> Self {
        Self {
            phase: PdfRenderPhase::Created,
        }
    }

    fn advance(&mut self, next: PdfRenderPhase) -> Result<(), AppError> {
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

struct PdfRenderWindow {
    window: Option<tauri::WebviewWindow>,
}

struct PdfPlatformWaitGuard {
    job: Option<PdfPlatformJob>,
}

impl PdfPlatformWaitGuard {
    fn new(job: PdfPlatformJob) -> Self {
        Self { job: Some(job) }
    }

    fn disarm(mut self) {
        self.job.take();
    }
}

impl Drop for PdfPlatformWaitGuard {
    fn drop(&mut self) {
        if let Some(job) = self.job.take() {
            job.cancel(AppError::new(
                "PDF_PRINT_CANCELLED",
                "PDF 平台打印等待已取消",
            ));
        }
    }
}

impl PdfRenderWindow {
    fn new(window: tauri::WebviewWindow) -> Self {
        Self {
            window: Some(window),
        }
    }

    fn window(&self) -> &tauri::WebviewWindow {
        self.window.as_ref().expect("PDF 渲染窗口只能在关闭前访问")
    }

    fn close(mut self) -> Result<(), AppError> {
        let window = self.window.take().expect("PDF 渲染窗口只能关闭一次");
        window.destroy().map_err(|error| {
            AppError::new(
                "PDF_WEBVIEW_DESTROY_FAILED",
                format!("销毁 PDF 导出渲染面失败：{error}"),
            )
        })
    }
}

impl Drop for PdfRenderWindow {
    fn drop(&mut self) {
        if let Some(window) = self.window.take() {
            let _ = window.destroy();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PdfRenderSignal {
    Ready {
        completed: Vec<PdfRenderPhase>,
        phase: PdfRenderPhase,
        image_count: usize,
    },
    Failed {
        completed: Vec<PdfRenderPhase>,
        phase: PdfRenderPhase,
        code: String,
        image_count: usize,
        image_failed: usize,
    },
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq)]
struct MacosPrintLayout {
    paper_width_points: f64,
    paper_height_points: f64,
    margin_points: f64,
    landscape: bool,
}

#[cfg(any(target_os = "macos", test))]
fn macos_print_layout(options: &ExportOptions) -> MacosPrintLayout {
    let (paper_width_points, paper_height_points) = match options.paper_size {
        ExportPaperSize::A4 => (595.275_590_551, 841.889_763_78),
        ExportPaperSize::Letter => (612.0, 792.0),
    };
    let margin_points = match options.margin {
        ExportMarginPreset::Narrow => 36.0,
        ExportMarginPreset::Normal => 72.0,
        ExportMarginPreset::Wide => 108.0,
    };
    MacosPrintLayout {
        paper_width_points,
        paper_height_points,
        margin_points,
        landscape: options.orientation == ExportOrientation::Landscape,
    }
}

#[cfg(target_os = "macos")]
use objc2::AnyThread;
#[cfg(target_os = "macos")]
use objc2_foundation::{NSObject, NSObjectProtocol};

#[cfg(target_os = "macos")]
objc2::define_class!(
    // SAFETY: NSObject has no subclassing requirements. The delegate stores only
    // thread-safe Rust state because AppKit may invoke the completion selector on
    // its detached printing thread.
    #[unsafe(super = NSObject)]
    #[name = "MarkLitePdfPrintCompletionDelegate"]
    #[ivars = ()]
    struct MacosPrintCompletionDelegate;

    unsafe impl NSObjectProtocol for MacosPrintCompletionDelegate {}

    impl MacosPrintCompletionDelegate {
        // SAFETY: The selector signature is the one required by
        // NSPrintOperation::runOperationModalForWindow:delegate:didRunSelector:contextInfo:.
        #[unsafe(method(printOperationDidRun:success:contextInfo:))]
        fn print_operation_did_run(
            &self,
            _operation: &objc2_app_kit::NSPrintOperation,
            success: bool,
            context_info: *mut std::ffi::c_void,
        ) {
            let result = if success {
                Ok(())
            } else {
                Err(AppError::new(
                    "PDF_PRINT_FAILED",
                    "macOS 打印操作未生成 PDF",
                ))
            };
            let job_id = context_info as usize as u64;
            if job_id != 0 {
                macos_complete_print_job(job_id, result);
            }
        }
    }
);

#[cfg(target_os = "macos")]
impl MacosPrintCompletionDelegate {
    fn new() -> objc2::rc::Retained<Self> {
        let this = Self::alloc().set_ivars(());
        unsafe { objc2::msg_send![super(this), init] }
    }
}

#[cfg(target_os = "macos")]
fn macos_print_jobs() -> &'static crate::services::pdf_platform_job::PdfPlatformRegistry {
    static JOBS: std::sync::OnceLock<crate::services::pdf_platform_job::PdfPlatformRegistry> =
        std::sync::OnceLock::new();
    JOBS.get_or_init(crate::services::pdf_platform_job::PdfPlatformRegistry::new)
}

#[cfg(target_os = "macos")]
struct MacosPrintRegistration {
    job_id: u64,
}

#[cfg(target_os = "macos")]
impl MacosPrintRegistration {
    fn register(completion: crate::services::pdf_platform_job::PdfPlatformCompletion) -> Self {
        let job_id = macos_print_jobs().register(completion);
        Self { job_id }
    }
}

#[cfg(target_os = "macos")]
impl Drop for MacosPrintRegistration {
    fn drop(&mut self) {
        macos_print_jobs().unregister(self.job_id);
    }
}

#[cfg(target_os = "macos")]
fn macos_complete_print_job(job_id: u64, result: Result<(), AppError>) {
    macos_print_jobs().complete(job_id, result);
}

#[cfg(target_os = "macos")]
fn macos_print_delegate() -> &'static objc2::runtime::AnyObject {
    static DELEGATE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let pointer = *DELEGATE.get_or_init(|| {
        let delegate: objc2::rc::Retained<objc2::runtime::AnyObject> =
            MacosPrintCompletionDelegate::new().into();
        objc2::rc::Retained::into_raw(delegate) as usize
    });
    // SAFETY: The delegate is intentionally retained for the process lifetime.
    // A single immutable delegate serves all jobs; contextInfo carries only an ID.
    unsafe { &*(pointer as *const objc2::runtime::AnyObject) }
}

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

pub async fn export_pdf_controlled(
    app: tauri::AppHandle,
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
    let diagrams = diagram_export_service::prepare(
        &app,
        &document,
        &target,
        isolate_webview_profile,
        deadline,
        || control.terminal_error(),
        diagram_export_service::DiagramExportMode::Pdf,
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
            export_pdf_inner(
                &app,
                request,
                PdfRenderPaths {
                    html: workspace.html_path(),
                    pdf: workspace.pdf_path(),
                    webview_data: isolate_webview_profile.then(|| workspace.webview_data_path()),
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

async fn export_pdf_inner(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    paths: PdfRenderPaths<'_>,
    job: PdfRenderJob,
    control: &PdfExportControl,
    deadline: Option<Instant>,
    (reporter, work): (&ExportReporter, ExportWork),
) -> Result<PdfRenderState, AppError> {
    let source_url = url::Url::from_file_path(paths.html)
        .map_err(|_| AppError::new("PDF_SOURCE_URL_FAILED", "无法为 PDF 临时 HTML 创建本地 URL"))?;
    let (ready_sender, ready_receiver) = mpsc::channel::<PdfRenderSignal>();
    let ready_token = job.ready_token.clone();
    let terminal_received = Arc::new(AtomicBool::new(false));
    let navigation_terminal = Arc::clone(&terminal_received);
    let mut builder =
        tauri::WebviewWindowBuilder::new(app, job.label, WebviewUrl::External(source_url))
            .title("MarkLite Export")
            .visible(false)
            .focused(false)
            .decorations(false)
            .resizable(false)
            .skip_taskbar(true);
    if let Some(webview_data) = paths.webview_data {
        builder = builder.data_directory(webview_data.to_path_buf());
    }
    let builder = builder.inner_size(1024.0, 768.0).on_navigation(move |url| {
        if url.scheme() == "marklite-export" {
            if let Some(signal) = parse_pdf_render_signal(url, &ready_token) {
                if navigation_terminal
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    let _ = ready_sender.send(signal);
                }
            }
            false
        } else {
            url.scheme() == "file"
        }
    });
    #[cfg(target_os = "macos")]
    let builder =
        builder.background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);

    let window = builder.build().map_err(|error| {
        AppError::new(
            "PDF_WEBVIEW_CREATE_FAILED",
            format!("创建 PDF 导出渲染面失败：{error}"),
        )
    })?;
    let window = PdfRenderWindow::new(window);

    let operation = async {
        let (render_timeout, render_timeout_error) = phase_timeout(
            deadline,
            RENDER_TIMEOUT,
            "PDF_RENDER_TIMEOUT",
            "PDF 渲染面未在 20 秒内发送有效的就绪信号",
        )?;
        let render_control = control.clone();
        let mut render_state = tauri::async_runtime::spawn_blocking(move || {
            wait_for_pdf_ready_with_error(
                ready_receiver,
                render_timeout,
                render_timeout_error,
                Some(&render_control),
            )
        })
        .await
        .map_err(|_| AppError::new("PDF_RENDER_FAILED", "PDF 渲染等待任务异常结束"))??;
        check_deadline(control, deadline)?;

        render_state.advance(PdfRenderPhase::Printing)?;
        reporter.processing(ExportStage::Printing, work);
        let (print_timeout, print_timeout_error) = phase_timeout(
            deadline,
            PRINT_TIMEOUT,
            "PDF_PRINT_TIMEOUT",
            "PDF 打印超过 45 秒",
        )?;

        print_to_pdf(
            window.window(),
            paths.pdf.to_path_buf(),
            &request.options,
            print_timeout,
            print_timeout_error,
            control,
        )
        .await?;
        check_deadline(control, deadline)?;

        render_state.advance(PdfRenderPhase::PlatformPrinted)?;
        Ok(render_state)
    }
    .await;
    let close_result = window.close();
    match (operation, close_result) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(state), Ok(())) => Ok(state),
    }
}

fn check_deadline(control: &PdfExportControl, deadline: Option<Instant>) -> Result<(), AppError> {
    control.check_running()?;
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        let error = AppError::new("PDF_EXPORT_TIMEOUT", "PDF 导出超过全程时限");
        control.cancel(error.clone());
        return Err(error);
    }
    Ok(())
}

fn phase_timeout(
    deadline: Option<Instant>,
    phase_limit: Duration,
    phase_code: &'static str,
    phase_message: &'static str,
) -> Result<(Duration, AppError), AppError> {
    let Some(deadline) = deadline else {
        return Ok((phase_limit, AppError::new(phase_code, phase_message)));
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(AppError::new("PDF_EXPORT_TIMEOUT", "PDF 导出超过全程时限"));
    }
    if remaining <= phase_limit {
        Ok((
            remaining,
            AppError::new("PDF_EXPORT_TIMEOUT", "PDF 导出超过全程时限"),
        ))
    } else {
        Ok((phase_limit, AppError::new(phase_code, phase_message)))
    }
}

fn parse_pdf_render_signal(url: &url::Url, expected_token: &str) -> Option<PdfRenderSignal> {
    if url.scheme() != "marklite-export" {
        return None;
    }
    let token = unique_query_value(url, "token")?;
    if token != expected_token {
        return None;
    }
    let phase = PdfRenderPhase::parse(&unique_query_value(url, "stage")?)?;
    let code = unique_query_value(url, "code")?;
    let completed = parse_completed_phases(&unique_query_value(url, "completed")?)?;
    let image_count = parse_bounded_count(&unique_query_value(url, "images")?)?;
    let image_failed = parse_bounded_count(&unique_query_value(url, "failed")?)?;
    if image_failed > image_count {
        return None;
    }

    match url.host_str()? {
        "ready" if code.is_empty() && image_failed == 0 && phase == PdfRenderPhase::LayoutReady => {
            Some(PdfRenderSignal::Ready {
                completed,
                phase,
                image_count,
            })
        }
        "error" if valid_render_failure(phase, &code, image_count, image_failed) => {
            Some(PdfRenderSignal::Failed {
                completed,
                phase,
                code,
                image_count,
                image_failed,
            })
        }
        _ => None,
    }
}

fn parse_completed_phases(value: &str) -> Option<Vec<PdfRenderPhase>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    value
        .split(',')
        .map(PdfRenderPhase::parse)
        .collect::<Option<Vec<_>>>()
}

fn unique_query_value(url: &url::Url, name: &str) -> Option<String> {
    let mut values = url
        .query_pairs()
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned());
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    Some(value)
}

fn parse_bounded_count(value: &str) -> Option<usize> {
    value
        .parse::<usize>()
        .ok()
        .filter(|count| *count <= 100_000)
}

fn valid_render_failure(
    phase: PdfRenderPhase,
    code: &str,
    image_count: usize,
    image_failed: usize,
) -> bool {
    let Some(stage) = phase.wire_name() else {
        return false;
    };
    if !pdf_ready_protocol::FAILURE_SIGNALS.contains(&(stage, code)) {
        return false;
    }
    match phase {
        PdfRenderPhase::ImagesSettled => image_count > 0 && image_failed > 0,
        _ => image_failed == 0,
    }
}

#[cfg(test)]
fn wait_for_pdf_ready(
    receiver: mpsc::Receiver<PdfRenderSignal>,
    timeout: Duration,
) -> Result<PdfRenderState, AppError> {
    wait_for_pdf_ready_with_error(
        receiver,
        timeout,
        AppError::new(
            "PDF_RENDER_TIMEOUT",
            "PDF 渲染面未在 20 秒内发送有效的就绪信号",
        ),
        None,
    )
}

fn wait_for_pdf_ready_with_error(
    receiver: mpsc::Receiver<PdfRenderSignal>,
    timeout: Duration,
    timeout_error: AppError,
    control: Option<&PdfExportControl>,
) -> Result<PdfRenderState, AppError> {
    let mut state = PdfRenderState::created();
    let deadline = Instant::now().checked_add(timeout);
    let signal = loop {
        if let Some(error) = control.and_then(PdfExportControl::terminal_error) {
            return Err(error);
        }
        let remaining = deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
            .unwrap_or_default();
        if remaining.is_zero() {
            return Err(timeout_error);
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(25))) {
            Ok(signal) => break signal,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(AppError::new(
                    "PDF_RENDER_FAILED",
                    "PDF 渲染就绪通道意外关闭",
                ));
            }
        }
    };
    match signal {
        PdfRenderSignal::Ready {
            completed,
            phase,
            image_count,
        } => {
            for completed_phase in completed {
                state.advance(completed_phase)?;
            }
            debug_assert!(image_count <= 100_000);
            state.advance(phase)?;
            Ok(state)
        }
        PdfRenderSignal::Failed {
            completed,
            phase,
            code,
            image_count,
            image_failed,
        } => {
            for completed_phase in completed {
                state.advance(completed_phase)?;
            }
            if state.phase.next() != Some(phase) {
                return Err(AppError::new(
                    "PDF_READY_PROTOCOL_INVALID",
                    "PDF 渲染失败信号与当前阶段不匹配",
                ));
            }
            Err(render_failure_error(&code, image_count, image_failed))
        }
    }
}

fn render_failure_error(code: &str, image_count: usize, image_failed: usize) -> AppError {
    let message = match code {
        pdf_ready_protocol::DOM_TIMEOUT => "PDF 页面结构准备超时".to_string(),
        pdf_ready_protocol::DOM_FAILED => "PDF 页面结构准备失败".to_string(),
        pdf_ready_protocol::FONT_TIMEOUT => "PDF 字体准备超时".to_string(),
        pdf_ready_protocol::FONT_FAILED => "PDF 字体准备失败".to_string(),
        pdf_ready_protocol::IMAGE_TIMEOUT => {
            format!("PDF 有 {image_failed} 张图片准备超时（共 {image_count} 张）")
        }
        pdf_ready_protocol::IMAGE_DECODE_FAILED => {
            format!("PDF 有 {image_failed} 张图片解码失败（共 {image_count} 张）")
        }
        pdf_ready_protocol::LAYOUT_FAILED => "PDF 页面布局准备失败".to_string(),
        _ => "PDF 渲染面返回未知错误".to_string(),
    };
    AppError::new(code, message)
}

async fn wait_for_platform_job(
    job: PdfPlatformJob,
    timeout: Duration,
    timeout_error: AppError,
    wait_failed_message: &'static str,
    control: &PdfExportControl,
) -> Result<(), AppError> {
    let cancellation = PdfPlatformWaitGuard::new(job.clone());
    let wait_control = control.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        job.wait_controlled(timeout, timeout_error, || wait_control.terminal_error())
    })
    .await
    .map_err(|_| AppError::new("PDF_PRINT_FAILED", wait_failed_message))?;
    cancellation.disarm();
    result
}

#[cfg(windows)]
async fn print_to_pdf(
    window: &tauri::WebviewWindow,
    pdf_path: PathBuf,
    options: &ExportOptions,
    timeout: Duration,
    timeout_error: AppError,
    control: &PdfExportControl,
) -> Result<(), AppError> {
    use webview2_com::{
        Microsoft::Web::WebView2::Win32::{
            ICoreWebView2Environment6, ICoreWebView2_7, COREWEBVIEW2_PRINT_ORIENTATION_LANDSCAPE,
            COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT,
        },
        PrintToPdfCompletedHandler,
    };
    use windows::core::{Interface, HSTRING};

    let job = PdfPlatformJob::new();
    let completion = job.completion();
    let settings = options.clone();
    let pdf_path = path_to_utf8(&pdf_path)?.to_string();
    window
        .with_webview(move |webview| unsafe {
            let operation = (|| -> windows::core::Result<()> {
                let core = webview.controller().CoreWebView2()?;
                let core: ICoreWebView2_7 = core.cast()?;
                let environment: ICoreWebView2Environment6 = webview.environment().cast()?;
                let print_settings = environment.CreatePrintSettings()?;
                let orientation = match settings.orientation {
                    ExportOrientation::Portrait => COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT,
                    ExportOrientation::Landscape => COREWEBVIEW2_PRINT_ORIENTATION_LANDSCAPE,
                };
                let (width, height) = match settings.paper_size {
                    ExportPaperSize::A4 => (8.267_717, 11.692_913),
                    ExportPaperSize::Letter => (8.5, 11.0),
                };
                let margin = match settings.margin {
                    ExportMarginPreset::Narrow => 0.5,
                    ExportMarginPreset::Normal => 1.0,
                    ExportMarginPreset::Wide => 1.5,
                };
                print_settings.SetOrientation(orientation)?;
                print_settings.SetPageWidth(width)?;
                print_settings.SetPageHeight(height)?;
                print_settings.SetMarginTop(margin)?;
                print_settings.SetMarginBottom(margin)?;
                print_settings.SetMarginLeft(margin)?;
                print_settings.SetMarginRight(margin)?;
                print_settings.SetShouldPrintBackgrounds(true)?;
                print_settings.SetShouldPrintHeaderAndFooter(false)?;

                let callback_completion = completion.clone();
                let handler =
                    PrintToPdfCompletedHandler::create(Box::new(move |status, succeeded| {
                        let result = match (status, succeeded) {
                            (Ok(()), true) => Ok(()),
                            (Err(error), _) => Err(AppError::new(
                                "PDF_PRINT_FAILED",
                                format!("WebView2 打印失败：{error}"),
                            )),
                            (Ok(()), false) => {
                                Err(AppError::new("PDF_PRINT_FAILED", "WebView2 未生成 PDF"))
                            }
                        };
                        callback_completion.complete(result);
                        Ok(())
                    }));
                let path = HSTRING::from(pdf_path.as_str());
                core.PrintToPdf(&path, &print_settings, &handler)
            })();
            if let Err(error) = operation {
                completion.complete(Err(AppError::new(
                    "PDF_PRINT_FAILED",
                    format!("初始化 WebView2 PDF 打印失败：{error}"),
                )));
            }
        })
        .map_err(|error| {
            AppError::new(
                "PDF_PLATFORM_ADAPTER_FAILED",
                format!("访问 WebView2 失败：{error}"),
            )
        })?;

    wait_for_platform_job(
        job,
        timeout,
        timeout_error,
        "PDF 打印等待任务异常结束",
        control,
    )
    .await
}

#[cfg(target_os = "macos")]
async fn print_to_pdf(
    window: &tauri::WebviewWindow,
    pdf_path: PathBuf,
    options: &ExportOptions,
    timeout: Duration,
    timeout_error: AppError,
    control: &PdfExportControl,
) -> Result<(), AppError> {
    use objc2::runtime::NSObjectProtocol;
    use objc2_app_kit::{
        NSPaperOrientation, NSPrintInfo, NSPrintJobSavingURL, NSPrintSaveJob,
        NSPrintingPaginationMode,
    };
    use objc2_foundation::{NSCopying, NSSize, NSString, NSURL};
    use objc2_web_kit::WKWebView;

    let job = PdfPlatformJob::new();
    let registration = MacosPrintRegistration::register(job.completion());
    let job_id = registration.job_id;
    let settings = options.clone();
    let output_path = path_to_utf8(&pdf_path)?.to_string();
    let schedule_result = window.with_webview(move |webview| unsafe {
        let view = &*(webview.inner() as *const WKWebView);
        if !view.respondsToSelector(objc2::sel!(printOperationWithPrintInfo:)) {
            macos_complete_print_job(
                job_id,
                Err(AppError::new(
                    "PDF_PLATFORM_UNSUPPORTED",
                    "macOS 11 或更高版本才支持 PDF 导出",
                )),
            );
            return;
        }
        let Some(document_window) = view.window() else {
            macos_complete_print_job(
                job_id,
                Err(AppError::new(
                    "PDF_PLATFORM_ADAPTER_FAILED",
                    "macOS PDF 渲染面没有可用窗口",
                )),
            );
            return;
        };

        let print_info = NSPrintInfo::sharedPrintInfo().copy();
        let layout = macos_print_layout(&settings);
        print_info.setPaperSize(NSSize::new(
            layout.paper_width_points,
            layout.paper_height_points,
        ));
        print_info.setOrientation(if layout.landscape {
            NSPaperOrientation::Landscape
        } else {
            NSPaperOrientation::Portrait
        });
        print_info.setTopMargin(layout.margin_points);
        print_info.setBottomMargin(layout.margin_points);
        print_info.setLeftMargin(layout.margin_points);
        print_info.setRightMargin(layout.margin_points);
        print_info.setHorizontallyCentered(false);
        print_info.setVerticallyCentered(false);
        print_info.setHorizontalPagination(NSPrintingPaginationMode::Fit);
        print_info.setVerticalPagination(NSPrintingPaginationMode::Automatic);
        print_info.setJobDisposition(NSPrintSaveJob);

        let output_string = NSString::from_str(&output_path);
        let output_url = NSURL::fileURLWithPath(&output_string);
        print_info
            .dictionary()
            .insert(NSPrintJobSavingURL, &output_url);

        let operation = view.printOperationWithPrintInfo(&print_info);
        operation.setShowsPrintPanel(false);
        operation.setShowsProgressPanel(false);
        // WKPrintingView resolves real pagination from the detached print thread;
        // running this operation synchronously here would block the UI thread that
        // must deliver those page-rectangle callbacks.
        operation.setCanSpawnSeparateThread(true);
        let delegate = macos_print_delegate();
        operation.runOperationModalForWindow_delegate_didRunSelector_contextInfo(
            &document_window,
            Some(delegate),
            Some(objc2::sel!(printOperationDidRun:success:contextInfo:)),
            job_id as usize as *mut std::ffi::c_void,
        );
    });
    if let Err(error) = schedule_result {
        macos_complete_print_job(
            job_id,
            Err(AppError::new(
                "PDF_PLATFORM_ADAPTER_FAILED",
                error.to_string(),
            )),
        );
    }
    let print_result = wait_for_platform_job(
        job,
        timeout,
        timeout_error,
        "macOS PDF 等待任务异常结束",
        control,
    )
    .await;
    drop(registration);
    print_result
}

#[cfg(target_os = "linux")]
async fn print_to_pdf(
    window: &tauri::WebviewWindow,
    pdf_path: PathBuf,
    options: &ExportOptions,
    timeout: Duration,
    timeout_error: AppError,
    control: &PdfExportControl,
) -> Result<(), AppError> {
    use gtk::{PageOrientation, PageSetup, PaperSize, PrintSettings, Unit};
    use webkit2gtk::{PrintOperation, PrintOperationExt};

    path_to_utf8(&pdf_path)?;
    let output_uri = url::Url::from_file_path(&pdf_path)
        .map_err(|_| AppError::new("PDF_OUTPUT_URL_FAILED", "无法创建 PDF 输出文件 URL"))?
        .to_string();
    let job = PdfPlatformJob::new();
    let completion = job.completion();
    let settings = options.clone();
    window
        .with_webview(move |webview| {
            let operation = PrintOperation::new(&webview.inner());
            let print_settings = PrintSettings::new();
            print_settings.set_printer("Print to File");
            print_settings.set("output-file-format", Some("pdf"));
            print_settings.set("output-uri", Some(&output_uri));
            let page_setup = PageSetup::new();
            let paper = match settings.paper_size {
                ExportPaperSize::A4 => PaperSize::new(Some("iso_a4")),
                ExportPaperSize::Letter => PaperSize::new(Some("na_letter")),
            };
            page_setup.set_paper_size(&paper);
            page_setup.set_orientation(match settings.orientation {
                ExportOrientation::Portrait => PageOrientation::Portrait,
                ExportOrientation::Landscape => PageOrientation::Landscape,
            });
            let margin = match settings.margin {
                ExportMarginPreset::Narrow => 12.7,
                ExportMarginPreset::Normal => 25.4,
                ExportMarginPreset::Wide => 38.1,
            };
            page_setup.set_top_margin(margin, Unit::Mm);
            page_setup.set_bottom_margin(margin, Unit::Mm);
            page_setup.set_left_margin(margin, Unit::Mm);
            page_setup.set_right_margin(margin, Unit::Mm);
            operation.set_print_settings(&print_settings);
            operation.set_page_setup(&page_setup);
            let failed_completion = completion.clone();
            operation.connect_failed(move |_, error| {
                failed_completion
                    .complete(Err(AppError::new("PDF_PRINT_FAILED", error.to_string())));
            });
            let finished_completion = completion.clone();
            operation.connect_finished(move |_| {
                finished_completion.complete(Ok(()));
            });
            operation.print();
        })
        .map_err(|error| AppError::new("PDF_PLATFORM_ADAPTER_FAILED", error.to_string()))?;
    wait_for_platform_job(
        job,
        timeout,
        timeout_error,
        "Linux PDF 等待任务异常结束",
        control,
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::mpsc, time::Duration};

    use super::{
        macos_print_layout, parse_pdf_render_signal, wait_for_pdf_ready, PdfExportControl,
        PdfRenderPhase, PdfRenderSignal,
    };
    use crate::models::app_error::AppError;
    use crate::models::export::{
        ExportMarginPreset, ExportOptions, ExportOrientation, ExportPaperSize,
    };
    use crate::services::{export_service::ExportCommitPolicy, pdf_ready_protocol};
    use crate::utils::test_support::TestPath;

    fn signal_url(
        host: &str,
        token: &str,
        stage: &str,
        code: &str,
        completed: &str,
        images: usize,
        failed: usize,
    ) -> url::Url {
        url::Url::parse(&format!(
            "marklite-export://{host}?token={token}&stage={stage}&code={code}&completed={completed}&images={images}&failed={failed}"
        ))
        .unwrap()
    }

    #[test]
    fn parses_only_token_bound_typed_render_signals() {
        let ready = signal_url(
            "ready",
            "expected",
            pdf_ready_protocol::LAYOUT_READY,
            "",
            &[
                pdf_ready_protocol::PAGE_LOADED,
                pdf_ready_protocol::DOM_READY,
                pdf_ready_protocol::FONTS_SETTLED,
                pdf_ready_protocol::IMAGES_SETTLED,
            ]
            .join(","),
            3,
            0,
        );
        assert_eq!(
            parse_pdf_render_signal(&ready, "expected"),
            Some(PdfRenderSignal::Ready {
                completed: vec![
                    PdfRenderPhase::PageLoaded,
                    PdfRenderPhase::DomReady,
                    PdfRenderPhase::FontsSettled,
                    PdfRenderPhase::ImagesSettled,
                ],
                phase: PdfRenderPhase::LayoutReady,
                image_count: 3,
            })
        );
        assert_eq!(parse_pdf_render_signal(&ready, "stale"), None);

        let duplicate = url::Url::parse(
            "marklite-export://ready?token=expected&token=expected&stage=layoutReady&code=&completed=pageLoaded,domReady,fontsSettled,imagesSettled&images=0&failed=0",
        )
        .unwrap();
        assert_eq!(parse_pdf_render_signal(&duplicate, "expected"), None);

        let forged = signal_url(
            "ready",
            "expected",
            pdf_ready_protocol::FONTS_SETTLED,
            "",
            &[
                pdf_ready_protocol::PAGE_LOADED,
                pdf_ready_protocol::DOM_READY,
            ]
            .join(","),
            0,
            0,
        );
        assert_eq!(parse_pdf_render_signal(&forged, "expected"), None);
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

    #[test]
    fn parser_accepts_every_failure_emitted_by_the_shared_page_protocol() {
        for (stage, code) in pdf_ready_protocol::FAILURE_SIGNALS {
            let is_image_failure = stage == pdf_ready_protocol::IMAGES_SETTLED;
            let signal = signal_url(
                "error",
                "expected",
                stage,
                code,
                "",
                usize::from(is_image_failure),
                usize::from(is_image_failure),
            );
            assert!(matches!(
                parse_pdf_render_signal(&signal, "expected"),
                Some(PdfRenderSignal::Failed { code: parsed, .. }) if parsed == code
            ));
        }
    }

    #[test]
    fn accepts_the_exact_render_phase_sequence() {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(PdfRenderSignal::Ready {
                completed: vec![
                    PdfRenderPhase::PageLoaded,
                    PdfRenderPhase::DomReady,
                    PdfRenderPhase::FontsSettled,
                    PdfRenderPhase::ImagesSettled,
                ],
                phase: PdfRenderPhase::LayoutReady,
                image_count: 2,
            })
            .unwrap();

        let state = wait_for_pdf_ready(receiver, Duration::from_secs(1)).unwrap();
        assert_eq!(state.phase, PdfRenderPhase::LayoutReady);
    }

    #[test]
    fn rejects_duplicate_or_out_of_order_render_phases() {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(PdfRenderSignal::Ready {
                completed: vec![PdfRenderPhase::PageLoaded, PdfRenderPhase::PageLoaded],
                phase: PdfRenderPhase::LayoutReady,
                image_count: 0,
            })
            .unwrap();

        let error = wait_for_pdf_ready(receiver, Duration::from_secs(1)).unwrap_err();
        assert_eq!(error.code, "PDF_READY_PROTOCOL_INVALID");
    }

    #[test]
    fn preserves_the_failing_resource_stage_and_count() {
        let (sender, receiver) = mpsc::channel();
        sender
            .send(PdfRenderSignal::Failed {
                completed: vec![
                    PdfRenderPhase::PageLoaded,
                    PdfRenderPhase::DomReady,
                    PdfRenderPhase::FontsSettled,
                ],
                phase: PdfRenderPhase::ImagesSettled,
                code: pdf_ready_protocol::IMAGE_TIMEOUT.to_string(),
                image_count: 4,
                image_failed: 1,
            })
            .unwrap();

        let error = wait_for_pdf_ready(receiver, Duration::from_secs(1)).unwrap_err();
        assert_eq!(error.code, pdf_ready_protocol::IMAGE_TIMEOUT);
        assert!(error.message.contains("1"));
        assert!(error.message.contains("4"));
        assert!(!error.message.contains("15 秒"));
    }

    #[test]
    fn maps_macos_paper_and_margin_options_to_points() {
        let mut options = ExportOptions {
            paper_size: ExportPaperSize::A4,
            orientation: ExportOrientation::Portrait,
            margin: ExportMarginPreset::Narrow,
            include_title: true,
            include_local_images: true,
        };
        let a4 = macos_print_layout(&options);
        assert!((a4.paper_width_points - 595.275_590_551).abs() < f64::EPSILON);
        assert!((a4.paper_height_points - 841.889_763_78).abs() < f64::EPSILON);
        assert_eq!(a4.margin_points, 36.0);
        assert!(!a4.landscape);

        options.paper_size = ExportPaperSize::Letter;
        options.orientation = ExportOrientation::Landscape;
        options.margin = ExportMarginPreset::Wide;
        let letter = macos_print_layout(&options);
        assert_eq!(letter.paper_width_points, 612.0);
        assert_eq!(letter.paper_height_points, 792.0);
        assert_eq!(letter.margin_points, 108.0);
        assert!(letter.landscape);
    }
}
