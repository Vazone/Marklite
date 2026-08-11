use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
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
const READY_TIMEOUT: Duration = Duration::from_secs(15);
const PRINT_TIMEOUT: Duration = Duration::from_secs(45);

pub async fn export_pdf(
    app: tauri::AppHandle,
    request: &ExportRequest,
) -> Result<ExportResult, AppError> {
    let target = export_service::validate_request(request)?;
    let prepared = export_service::prepare_pdf(request)?;
    let html_path = unique_sibling(&target, "html")?;
    let pdf_path = unique_sibling(&target, "pdf")?;
    fs::write(&html_path, prepared.html.as_bytes())
        .map_err(|error| AppError::file_write_failed(&html_path.to_string_lossy(), error))?;

    let operation = export_pdf_inner(&app, request, &html_path, &pdf_path).await;
    let _ = fs::remove_file(&html_path);
    if operation.is_err() {
        let _ = fs::remove_file(&pdf_path);
    }
    operation?;

    let pdf_result = fs::read(&pdf_path)
        .map_err(|error| AppError::file_read_failed(&pdf_path.to_string_lossy(), error));
    let _ = fs::remove_file(&pdf_path);
    let pdf = pdf_result?;
    let mut result = export_service::commit_pdf(request, &pdf)?;
    result.warnings = prepared.warnings;
    Ok(result)
}

async fn export_pdf_inner(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    html_path: &Path,
    pdf_path: &Path,
) -> Result<(), AppError> {
    let source_url = url::Url::from_file_path(html_path)
        .map_err(|_| AppError::new("PDF_SOURCE_URL_FAILED", "无法为 PDF 临时 HTML 创建本地 URL"))?;
    let label = format!(
        "export-{}-{}",
        std::process::id(),
        PDF_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let (ready_sender, ready_receiver) = mpsc::channel();
    let window = tauri::WebviewWindowBuilder::new(app, label, WebviewUrl::External(source_url))
        .title("MarkLite Export")
        .visible(false)
        .focused(false)
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .inner_size(1024.0, 768.0)
        .on_navigation(move |url| {
            if url.scheme() == "marklite-export" && url.host_str() == Some("ready") {
                let _ = ready_sender.send(());
                false
            } else {
                url.scheme() == "file"
            }
        })
        .build()
        .map_err(|error| {
            AppError::new(
                "PDF_WEBVIEW_CREATE_FAILED",
                format!("创建 PDF 导出渲染面失败：{error}"),
            )
        })?;

    let ready = match tauri::async_runtime::spawn_blocking(move || {
        ready_receiver.recv_timeout(READY_TIMEOUT)
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
    if ready.is_err() {
        let _ = window.destroy();
        return Err(AppError::new(
            "PDF_RENDER_TIMEOUT",
            "PDF 渲染面未能在 15 秒内完成字体和图片加载",
        ));
    }

    let print_result = print_to_pdf(&window, pdf_path.to_path_buf(), &request.options).await;
    let _ = window.destroy();
    print_result
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
    _options: &ExportOptions,
) -> Result<(), AppError> {
    use block2::RcBlock;
    use objc2_foundation::{NSData, NSError};
    use objc2_web_kit::WKWebView;

    let (sender, receiver) = mpsc::channel::<Result<Vec<u8>, String>>();
    window
        .with_webview(move |webview| unsafe {
            let view = &*(webview.inner() as *const WKWebView);
            let completion = RcBlock::new(move |data: *mut NSData, error: *mut NSError| {
                if !error.is_null() || data.is_null() {
                    let _ = sender.send(Err("WKWebView createPDF 返回错误".to_string()));
                    return;
                }
                let data = &*data;
                let _ = sender.send(Ok(data.to_vec()));
            });
            view.createPDFWithConfiguration_completionHandler(None, &completion);
        })
        .map_err(|error| AppError::new("PDF_PLATFORM_ADAPTER_FAILED", error.to_string()))?;
    let bytes = tauri::async_runtime::spawn_blocking(move || receiver.recv_timeout(PRINT_TIMEOUT))
        .await
        .map_err(|_| AppError::new("PDF_PRINT_FAILED", "macOS PDF 等待任务异常结束"))?
        .map_err(|_| AppError::new("PDF_PRINT_TIMEOUT", "macOS PDF 导出超过 45 秒"))?
        .map_err(|message| AppError::new("PDF_PRINT_FAILED", message))?;
    fs::write(&pdf_path, bytes)
        .map_err(|error| AppError::file_write_failed(&pdf_path.to_string_lossy(), error))
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
