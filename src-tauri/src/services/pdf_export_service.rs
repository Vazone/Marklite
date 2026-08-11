use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    time::Duration,
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
    services::export_service,
};

static PDF_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const RENDER_TIMEOUT: Duration = Duration::from_secs(20);
const PRINT_TIMEOUT: Duration = Duration::from_secs(45);

#[derive(Debug)]
struct PdfRenderJob {
    label: String,
    ready_token: String,
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
        match value {
            "pageLoaded" => Some(Self::PageLoaded),
            "domReady" => Some(Self::DomReady),
            "fontsSettled" => Some(Self::FontsSettled),
            "imagesSettled" => Some(Self::ImagesSettled),
            "layoutReady" => Some(Self::LayoutReady),
            _ => None,
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
use objc2::{AnyThread, DefinedClass};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSObject, NSObjectProtocol};

#[cfg(target_os = "macos")]
struct MacosPrintCompletionIvars {
    sender: std::sync::Mutex<Option<mpsc::Sender<Result<(), AppError>>>>,
    retained_self: std::sync::atomic::AtomicPtr<objc2::runtime::AnyObject>,
}

#[cfg(target_os = "macos")]
objc2::define_class!(
    // SAFETY: NSObject has no subclassing requirements. The delegate stores only
    // thread-safe Rust state because AppKit may invoke the completion selector on
    // its detached printing thread.
    #[unsafe(super = NSObject)]
    #[name = "MarkLitePdfPrintCompletionDelegate"]
    #[ivars = MacosPrintCompletionIvars]
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
            _context_info: *mut std::ffi::c_void,
        ) {
            let result = if success {
                Ok(())
            } else {
                Err(AppError::new(
                    "PDF_PRINT_FAILED",
                    "macOS 打印操作未生成 PDF",
                ))
            };
            self.complete(result);
        }
    }
);

#[cfg(target_os = "macos")]
impl MacosPrintCompletionDelegate {
    fn new(sender: mpsc::Sender<Result<(), AppError>>) -> objc2::rc::Retained<Self> {
        let this = Self::alloc().set_ivars(MacosPrintCompletionIvars {
            sender: std::sync::Mutex::new(Some(sender)),
            retained_self: std::sync::atomic::AtomicPtr::new(std::ptr::null_mut()),
        });
        let delegate: objc2::rc::Retained<Self> = unsafe { objc2::msg_send![super(this), init] };
        let retained_any: objc2::rc::Retained<objc2::runtime::AnyObject> = delegate.clone().into();
        delegate.ivars().retained_self.store(
            objc2::rc::Retained::into_raw(retained_any),
            Ordering::Release,
        );
        delegate
    }

    fn complete(&self, result: Result<(), AppError>) {
        let sender = match self.ivars().sender.lock() {
            Ok(mut slot) => slot.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        if let Some(sender) = sender {
            let _ = sender.send(result);
        }

        let retained_self = self
            .ivars()
            .retained_self
            .swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !retained_self.is_null() {
            drop(unsafe {
                objc2::rc::Retained::<objc2::runtime::AnyObject>::from_raw(retained_self)
            });
        }
    }
}

pub async fn export_pdf(
    app: tauri::AppHandle,
    request: &ExportRequest,
) -> Result<ExportResult, AppError> {
    let target = export_service::validate_request(request)?;
    let job = PdfRenderJob::allocate();
    let prepared = export_service::prepare_pdf(request, &job.ready_token)?;
    let html_path = unique_sibling(&target, "html")?;
    let pdf_path = unique_sibling(&target, "pdf")?;
    fs::write(&html_path, prepared.html.as_bytes())
        .map_err(|error| AppError::file_write_failed(&html_path.to_string_lossy(), error))?;

    let operation = export_pdf_inner(&app, request, &html_path, &pdf_path, job).await;
    let _ = fs::remove_file(&html_path);
    if operation.is_err() {
        let _ = fs::remove_file(&pdf_path);
    }
    let mut render_state = operation?;

    let pdf_result = fs::read(&pdf_path)
        .map_err(|error| AppError::file_read_failed(&pdf_path.to_string_lossy(), error));
    let _ = fs::remove_file(&pdf_path);
    let pdf = pdf_result?;
    let mut result = export_service::commit_pdf(request, &pdf)?;
    render_state.advance(PdfRenderPhase::Committed)?;
    result.warnings = prepared.warnings;
    Ok(result)
}

async fn export_pdf_inner(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    html_path: &Path,
    pdf_path: &Path,
    job: PdfRenderJob,
) -> Result<PdfRenderState, AppError> {
    let source_url = url::Url::from_file_path(html_path)
        .map_err(|_| AppError::new("PDF_SOURCE_URL_FAILED", "无法为 PDF 临时 HTML 创建本地 URL"))?;
    let (ready_sender, ready_receiver) = mpsc::channel::<PdfRenderSignal>();
    let ready_token = job.ready_token.clone();
    let terminal_received = Arc::new(AtomicBool::new(false));
    let navigation_terminal = Arc::clone(&terminal_received);
    let builder =
        tauri::WebviewWindowBuilder::new(app, job.label, WebviewUrl::External(source_url))
            .title("MarkLite Export")
            .visible(false)
            .focused(false)
            .decorations(false)
            .resizable(false)
            .skip_taskbar(true)
            .inner_size(1024.0, 768.0)
            .on_navigation(move |url| {
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

    let ready = match tauri::async_runtime::spawn_blocking(move || {
        wait_for_pdf_ready(ready_receiver, RENDER_TIMEOUT)
    })
    .await
    {
        Ok(ready) => ready,
        Err(_) => {
            let _ = window.destroy();
            return Err(AppError::new(
                "PDF_RENDER_FAILED",
                "PDF 渲染等待任务异常结束",
            ));
        }
    };
    let mut render_state = match ready {
        Ok(state) => state,
        Err(error) => {
            let _ = window.destroy();
            return Err(error);
        }
    };
    if let Err(error) = render_state.advance(PdfRenderPhase::Printing) {
        let _ = window.destroy();
        return Err(error);
    }

    let print_result = print_to_pdf(&window, pdf_path.to_path_buf(), &request.options).await;
    let _ = window.destroy();
    print_result?;
    render_state.advance(PdfRenderPhase::PlatformPrinted)?;
    Ok(render_state)
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
    match (phase, code) {
        (PdfRenderPhase::PageLoaded, "PDF_DOM_TIMEOUT" | "PDF_DOM_FAILED")
        | (PdfRenderPhase::FontsSettled, "PDF_FONT_TIMEOUT" | "PDF_FONT_FAILED")
        | (PdfRenderPhase::LayoutReady, "PDF_LAYOUT_FAILED") => image_failed == 0,
        (PdfRenderPhase::ImagesSettled, "PDF_IMAGE_TIMEOUT" | "PDF_IMAGE_DECODE_FAILED") => {
            image_count > 0 && image_failed > 0
        }
        _ => false,
    }
}

fn wait_for_pdf_ready(
    receiver: mpsc::Receiver<PdfRenderSignal>,
    timeout: Duration,
) -> Result<PdfRenderState, AppError> {
    let mut state = PdfRenderState::created();
    let signal = receiver
        .recv_timeout(timeout)
        .map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => AppError::new(
                "PDF_RENDER_TIMEOUT",
                "PDF 渲染面未在 20 秒内发送有效的就绪信号",
            ),
            mpsc::RecvTimeoutError::Disconnected => {
                AppError::new("PDF_RENDER_FAILED", "PDF 渲染就绪通道意外关闭")
            }
        })?;
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
        "PDF_DOM_TIMEOUT" => "PDF 页面结构准备超时".to_string(),
        "PDF_DOM_FAILED" => "PDF 页面结构准备失败".to_string(),
        "PDF_FONT_TIMEOUT" => "PDF 字体准备超时".to_string(),
        "PDF_FONT_FAILED" => "PDF 字体准备失败".to_string(),
        "PDF_IMAGE_TIMEOUT" => {
            format!("PDF 有 {image_failed} 张图片准备超时（共 {image_count} 张）")
        }
        "PDF_IMAGE_DECODE_FAILED" => {
            format!("PDF 有 {image_failed} 张图片解码失败（共 {image_count} 张）")
        }
        "PDF_LAYOUT_FAILED" => "PDF 页面布局准备失败".to_string(),
        _ => "PDF 渲染面返回未知错误".to_string(),
    };
    AppError::new(code, message)
}

#[cfg(windows)]
async fn print_to_pdf(
    window: &tauri::WebviewWindow,
    pdf_path: PathBuf,
    options: &ExportOptions,
) -> Result<(), AppError> {
    use webview2_com::{
        Microsoft::Web::WebView2::Win32::{
            ICoreWebView2Environment6, ICoreWebView2_7, COREWEBVIEW2_PRINT_ORIENTATION_LANDSCAPE,
            COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT,
        },
        PrintToPdfCompletedHandler,
    };
    use windows::core::{Interface, HSTRING};

    let (sender, receiver) = mpsc::channel::<Result<(), String>>();
    let settings = options.clone();
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

                let callback_sender = sender.clone();
                let handler =
                    PrintToPdfCompletedHandler::create(Box::new(move |status, succeeded| {
                        let result = match (status, succeeded) {
                            (Ok(()), true) => Ok(()),
                            (Err(error), _) => Err(format!("WebView2 打印失败：{error}")),
                            (Ok(()), false) => Err("WebView2 未生成 PDF".to_string()),
                        };
                        let _ = callback_sender.send(result);
                        Ok(())
                    }));
                let path = HSTRING::from(pdf_path.to_string_lossy().as_ref());
                core.PrintToPdf(&path, &print_settings, &handler)
            })();
            if let Err(error) = operation {
                let _ = sender.send(Err(format!("初始化 WebView2 PDF 打印失败：{error}")));
            }
        })
        .map_err(|error| {
            AppError::new(
                "PDF_PLATFORM_ADAPTER_FAILED",
                format!("访问 WebView2 失败：{error}"),
            )
        })?;

    tauri::async_runtime::spawn_blocking(move || receiver.recv_timeout(PRINT_TIMEOUT))
        .await
        .map_err(|_| AppError::new("PDF_PRINT_FAILED", "PDF 打印等待任务异常结束"))?
        .map_err(|_| AppError::new("PDF_PRINT_TIMEOUT", "PDF 打印超过 45 秒"))?
        .map_err(|message| AppError::new("PDF_PRINT_FAILED", message))
}

#[cfg(target_os = "macos")]
async fn print_to_pdf(
    window: &tauri::WebviewWindow,
    pdf_path: PathBuf,
    options: &ExportOptions,
) -> Result<(), AppError> {
    use objc2::{runtime::AnyObject, runtime::NSObjectProtocol};
    use objc2_app_kit::{
        NSPaperOrientation, NSPrintInfo, NSPrintJobSavingURL, NSPrintSaveJob,
        NSPrintingPaginationMode,
    };
    use objc2_foundation::{NSCopying, NSSize, NSString, NSURL};
    use objc2_web_kit::WKWebView;

    let (sender, receiver) = mpsc::channel::<Result<(), AppError>>();
    let delegate = MacosPrintCompletionDelegate::new(sender);
    let callback_delegate = delegate.clone();
    let settings = options.clone();
    let output_path = pdf_path.clone();
    let schedule_result = window.with_webview(move |webview| unsafe {
        let view = &*(webview.inner() as *const WKWebView);
        if !view.respondsToSelector(objc2::sel!(printOperationWithPrintInfo:)) {
            callback_delegate.complete(Err(AppError::new(
                "PDF_PLATFORM_UNSUPPORTED",
                "macOS 11 或更高版本才支持 PDF 导出",
            )));
            return;
        }
        let Some(document_window) = view.window() else {
            callback_delegate.complete(Err(AppError::new(
                "PDF_PLATFORM_ADAPTER_FAILED",
                "macOS PDF 渲染面没有可用窗口",
            )));
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

        let output_string = NSString::from_str(output_path.to_string_lossy().as_ref());
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
        let delegate_object: objc2::rc::Retained<AnyObject> = callback_delegate.clone().into();
        operation.runOperationModalForWindow_delegate_didRunSelector_contextInfo(
            &document_window,
            Some(&*delegate_object),
            Some(objc2::sel!(printOperationDidRun:success:contextInfo:)),
            std::ptr::null_mut(),
        );
    });
    if let Err(error) = schedule_result {
        delegate.complete(Err(AppError::new(
            "PDF_PLATFORM_ADAPTER_FAILED",
            error.to_string(),
        )));
    }
    let print_result =
        tauri::async_runtime::spawn_blocking(move || receiver.recv_timeout(PRINT_TIMEOUT))
            .await
            .map_err(|_| AppError::new("PDF_PRINT_FAILED", "macOS PDF 等待任务异常结束"))?
            .map_err(|_| AppError::new("PDF_PRINT_TIMEOUT", "macOS PDF 导出超过 45 秒"))?;
    drop(delegate);
    print_result?;
    if !pdf_path.is_file() {
        return Err(AppError::new(
            "PDF_PRINT_FAILED",
            "macOS 打印操作未写入 PDF 文件",
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
async fn print_to_pdf(
    window: &tauri::WebviewWindow,
    pdf_path: PathBuf,
    options: &ExportOptions,
) -> Result<(), AppError> {
    use gtk::{PageOrientation, PageSetup, PaperSize, PrintSettings, Unit};
    use webkit2gtk::{PrintOperation, PrintOperationExt};

    let (sender, receiver) = mpsc::channel::<Result<(), String>>();
    let settings = options.clone();
    window
        .with_webview(move |webview| {
            let operation = PrintOperation::new(&webview.inner());
            let print_settings = PrintSettings::new();
            print_settings.set_printer("Print to File");
            print_settings.set("output-file-format", Some("pdf"));
            let output_uri = url::Url::from_file_path(&pdf_path)
                .map(|url| url.to_string())
                .unwrap_or_default();
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
            let failed_sender = sender.clone();
            operation.connect_failed(move |_, error| {
                let _ = failed_sender.send(Err(error.to_string()));
            });
            operation.connect_finished(move |_| {
                let _ = sender.send(Ok(()));
            });
            operation.print();
        })
        .map_err(|error| AppError::new("PDF_PLATFORM_ADAPTER_FAILED", error.to_string()))?;
    tauri::async_runtime::spawn_blocking(move || receiver.recv_timeout(PRINT_TIMEOUT))
        .await
        .map_err(|_| AppError::new("PDF_PRINT_FAILED", "Linux PDF 等待任务异常结束"))?
        .map_err(|_| AppError::new("PDF_PRINT_TIMEOUT", "Linux PDF 导出超过 45 秒"))?
        .map_err(|message| AppError::new("PDF_PRINT_FAILED", message))
}

fn unique_sibling(target: &Path, extension: &str) -> Result<PathBuf, AppError> {
    let parent = target
        .parent()
        .ok_or_else(|| AppError::new("INVALID_EXPORT_TARGET", "导出目标没有父目录"))?;
    let base = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("marklite-export");
    for _ in 0..100 {
        let sequence = PDF_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{base}.marklite-pdf-{}-{sequence}.{extension}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(AppError::new(
        "EXPORT_TEMP_FAILED",
        "无法为 PDF 分配唯一临时文件",
    ))
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, time::Duration};

    use super::{
        macos_print_layout, parse_pdf_render_signal, wait_for_pdf_ready, PdfRenderPhase,
        PdfRenderSignal,
    };
    use crate::models::export::{
        ExportMarginPreset, ExportOptions, ExportOrientation, ExportPaperSize,
    };

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
            "layoutReady",
            "",
            "pageLoaded,domReady,fontsSettled,imagesSettled",
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
            "fontsSettled",
            "",
            "pageLoaded,domReady",
            0,
            0,
        );
        assert_eq!(parse_pdf_render_signal(&forged, "expected"), None);
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
                code: "PDF_IMAGE_TIMEOUT".to_string(),
                image_count: 4,
                image_failed: 1,
            })
            .unwrap();

        let error = wait_for_pdf_ready(receiver, Duration::from_secs(1)).unwrap_err();
        assert_eq!(error.code, "PDF_IMAGE_TIMEOUT");
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

    #[test]
    fn macos_adapter_never_runs_appkit_printing_on_the_ui_thread() {
        let source = include_str!("pdf_export_service.rs");
        let adapter = source
            .split("#[cfg(target_os = \"macos\")]\nasync fn print_to_pdf")
            .nth(1)
            .and_then(|source| source.split("#[cfg(target_os = \"linux\")]").next())
            .expect("macOS PDF adapter must remain isolated");
        assert!(!adapter.contains(&["setCanSpawnSeparateThread", "(false)"].concat()));
        assert!(!adapter.contains(&["operation.", "runOperation()"].concat()));
        assert!(adapter.contains("setCanSpawnSeparateThread(true)"));
        assert!(adapter.contains("runOperationModalForWindow_delegate_didRunSelector_contextInfo"));
    }
}
