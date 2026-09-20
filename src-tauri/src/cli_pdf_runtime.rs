use crate::services::export_progress::ExportReporter;
use std::{sync::atomic::AtomicBool, time::Duration};

#[cfg(not(test))]
use std::{
    sync::{atomic::Ordering, Arc, Mutex},
    thread,
    time::Instant,
};

use crate::{
    models::{
        app_error::AppError,
        export::{ExportRequest, ExportResult},
    },
    services::export_service::ExportCommitPolicy,
};

#[cfg(not(test))]
use crate::models::export::ExportFormat;
#[cfg(not(test))]
use crate::services::pdf_export_service::{self, PdfExportControl};

static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
#[cfg(not(test))]
const CANCEL_FILE_ENV: &str = "MARKLITE_CLI_CANCEL_FILE";
#[cfg(not(test))]
const CLEANUP_RESERVE: Duration = Duration::from_secs(2);

#[cfg(not(test))]
pub fn export_document_with_diagrams(
    request: ExportRequest,
    policy: ExportCommitPolicy,
    document: crate::services::export_semantic::SemanticDocument,
    reporter: ExportReporter,
) -> Result<ExportResult, AppError> {
    ensure_pdf_platform_available()?;
    let control = PdfExportControl::new();
    let task_control = control.clone();
    let outcome = Arc::new(Mutex::new(None));
    let task_outcome = Arc::clone(&outcome);
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .setup(move |app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let result = match request.format {
                    ExportFormat::Html => {
                        crate::services::export_service::export_html_with_runtime_document(
                            &handle,
                            &request,
                            policy,
                            true,
                            &document,
                            || task_control.terminal_error(),
                            &reporter,
                        )
                        .await
                    }
                    ExportFormat::Docx => {
                        crate::services::export_service::export_docx_with_runtime_document(
                            &handle,
                            &request,
                            policy,
                            true,
                            &document,
                            || task_control.terminal_error(),
                            &reporter,
                        )
                        .await
                    }
                    _ => unreachable!("diagram CLI runtime only serves HTML/DOCX"),
                };
                *task_outcome
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(result);
                handle.exit(0);
            });
            Ok(())
        })
        .build(context)
        .map_err(|error| {
            AppError::new(
                "PDF_PLATFORM_UNAVAILABLE",
                format!("无法启动图表原生运行时：{error}"),
            )
        })?;
    let signal_guard = SignalGuard::install()?;
    let cancel_file = std::env::var_os(CANCEL_FILE_ENV).map(std::path::PathBuf::from);
    let done = Arc::new(AtomicBool::new(false));
    let watcher_done = Arc::clone(&done);
    let watcher_control = control.clone();
    let watcher = thread::spawn(move || {
        while !watcher_done.load(Ordering::Acquire) {
            if CANCEL_REQUESTED.load(Ordering::Acquire)
                || cancel_file.as_deref().is_some_and(cancel_file_requested)
            {
                watcher_control.cancel(AppError::new("DIAGRAM_CANCELLED", "图表导出已取消"));
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    });
    let _ = app.run_return(|_, event| {
        if let tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            api.prevent_exit();
        }
    });
    done.store(true, Ordering::Release);
    let _ = watcher.join();
    drop(signal_guard);
    let cleanup_remaining =
        crate::services::pdf_artifact::cleanup_pending_webview_data(Duration::from_secs(2));
    let result = outcome
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
        .ok_or_else(|| {
            AppError::new("DIAGRAM_RUNTIME_CRASHED", "图表原生运行时在任务完成前退出")
        })?;
    result.map(|mut result| {
        if cleanup_remaining > 0 {
            result
                .warnings
                .push(crate::models::export::ExportWarning::new(
                    "DIAGRAM_WEBVIEW_CLEANUP_DELAYED",
                    format!(
                        "图表 WebView 临时数据延迟清理：{cleanup_remaining} 个目录仍被平台占用"
                    ),
                    None,
                ));
        }
        result
    })
}

#[cfg(not(test))]
pub fn export_pdf(
    request: ExportRequest,
    policy: ExportCommitPolicy,
    timeout: Duration,
    reporter: ExportReporter,
) -> Result<ExportResult, AppError> {
    let started = Instant::now();
    let overall_deadline = started
        .checked_add(timeout)
        .ok_or_else(|| AppError::new("PDF_EXPORT_TIMEOUT", "PDF 导出时限无效"))?;
    let operation_deadline = overall_deadline
        .checked_sub((timeout / 4).min(CLEANUP_RESERVE))
        .unwrap_or(started);
    ensure_pdf_platform_available()?;
    if Instant::now() >= operation_deadline {
        return Err(AppError::new(
            "PDF_EXPORT_TIMEOUT",
            "PDF 导出超过 CLI 全程时限",
        ));
    }
    let control = PdfExportControl::new();
    let outcome = Arc::new(Mutex::new(None));
    let task_handle = Arc::new(Mutex::new(None));
    let task_outcome = Arc::clone(&outcome);
    let setup_task_handle = Arc::clone(&task_handle);
    let task_control = control.clone();
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();

    let app = tauri::Builder::default()
        .setup(move |app| {
            let handle = app.handle().clone();
            let task = tauri::async_runtime::spawn(async move {
                let result = pdf_export_service::export_pdf_controlled(
                    handle.clone(),
                    &request,
                    policy,
                    task_control,
                    Some(operation_deadline.saturating_duration_since(Instant::now())),
                    true,
                    &reporter,
                )
                .await;
                *task_outcome
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(result);
                handle.exit(0);
            });
            *setup_task_handle
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(task);
            Ok(())
        })
        .build(context)
        .map_err(|error| {
            AppError::new(
                "PDF_PLATFORM_UNAVAILABLE",
                format!("无法启动 PDF 原生运行时：{error}"),
            )
        })?;

    if Instant::now() >= operation_deadline {
        return Err(AppError::new(
            "PDF_EXPORT_TIMEOUT",
            "PDF 导出超过 CLI 全程时限",
        ));
    }
    let signal_guard = SignalGuard::install()?;
    let cancel_file = std::env::var_os(CANCEL_FILE_ENV).map(std::path::PathBuf::from);
    let watcher_control = control.clone();
    thread::spawn(move || loop {
        if !watcher_control.is_running() {
            return;
        }
        if CANCEL_REQUESTED.load(Ordering::Acquire)
            || cancel_file.as_deref().is_some_and(cancel_file_requested)
        {
            let error = AppError::new("PDF_EXPORT_CANCELLED", "PDF 导出已取消");
            watcher_control.cancel(error);
            return;
        }
        if Instant::now() >= operation_deadline {
            let error = AppError::new("PDF_EXPORT_TIMEOUT", "PDF 导出超过 CLI 全程时限");
            watcher_control.cancel(error);
            return;
        }
        thread::sleep(Duration::from_millis(10));
    });

    let _runtime_exit_code = app.run_return(|_, event| {
        if let tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            // Destroying the hidden job WebView can otherwise trigger Tauri's
            // last-window auto-exit before the async task publishes its result.
            api.prevent_exit();
        }
    });
    drop(signal_guard);
    if let Some(task) = task_handle
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    {
        task.abort();
        let _ = tauri::async_runtime::block_on(task);
    }
    let cleanup_remaining = crate::services::pdf_artifact::cleanup_pending_webview_data(
        overall_deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(2)),
    );
    if let Some(result) = outcome
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
    {
        return result.map(|mut result| {
            if cleanup_remaining > 0 {
                result
                    .warnings
                    .push(crate::models::export::ExportWarning::new(
                        "PDF_WEBVIEW_CLEANUP_DELAYED",
                        format!(
                            "PDF WebView 临时数据延迟清理：{cleanup_remaining} 个目录仍被平台占用"
                        ),
                        None,
                    ));
            }
            result
        });
    }
    if let Some(error) = control.terminal_error() {
        return Err(error);
    }
    Err(AppError::new(
        "PDF_RUNTIME_EXITED",
        "PDF 原生运行时在任务到达终态前退出",
    ))
}

#[cfg(all(windows, not(test)))]
fn ensure_pdf_platform_available() -> Result<(), AppError> {
    use std::ffi::c_void;
    use webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString;
    use windows::core::{PCWSTR, PWSTR};

    #[link(name = "ole32")]
    extern "system" {
        fn CoTaskMemFree(pointer: *const c_void);
    }

    let mut version = PWSTR::null();
    // SAFETY: The loader initializes `version` with COM task memory on success;
    // it is released exactly once below and is never dereferenced by Rust.
    let result =
        unsafe { GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut version) };
    if !version.is_null() {
        // SAFETY: WebView2 Loader documents CoTaskMemFree for this output.
        unsafe { CoTaskMemFree(version.0.cast()) };
    }
    result.map_err(|error| {
        AppError::new(
            "PDF_PLATFORM_UNAVAILABLE",
            format!("未找到可用的 WebView2 PDF 运行时：{error}"),
        )
    })
}

#[cfg(all(not(windows), not(test)))]
fn ensure_pdf_platform_available() -> Result<(), AppError> {
    Ok(())
}

#[cfg(not(test))]
fn cancel_file_requested(path: &std::path::Path) -> bool {
    std::fs::read(path).is_ok_and(|contents| contents == b"cancel")
}

#[cfg(test)]
pub fn export_pdf(
    _request: ExportRequest,
    _policy: ExportCommitPolicy,
    _timeout: Duration,
    _reporter: ExportReporter,
) -> Result<ExportResult, AppError> {
    Err(AppError::new(
        "PDF_PLATFORM_UNAVAILABLE",
        "PDF 原生运行时不在单元测试进程中启动",
    ))
}

#[cfg(not(test))]
struct SignalGuard {
    #[cfg(windows)]
    installed: bool,
    #[cfg(unix)]
    previous_interrupt: usize,
}

#[cfg(not(test))]
impl SignalGuard {
    fn install() -> Result<Self, AppError> {
        CANCEL_REQUESTED.store(false, Ordering::Release);
        #[cfg(windows)]
        {
            use windows::Win32::System::Console::SetConsoleCtrlHandler;
            // SAFETY: The handler is a process-lifetime function and only sets
            // an atomic flag. The guard unregisters it before returning.
            unsafe { SetConsoleCtrlHandler(Some(console_handler), true) }.map_err(|error| {
                AppError::new(
                    "PDF_CANCEL_HANDLER_FAILED",
                    format!("无法安装 PDF 控制台取消处理器：{error}"),
                )
            })?;
            Ok(Self { installed: true })
        }
        #[cfg(unix)]
        {
            const SIGINT: i32 = 2;
            extern "C" {
                fn signal(signal: i32, handler: usize) -> usize;
            }
            // SAFETY: `unix_signal_handler` has the C signal ABI and performs
            // only an atomic store, which is signal-safe for this target set.
            let previous_interrupt = unsafe { signal(SIGINT, unix_signal_handler as usize) };
            Ok(Self { previous_interrupt })
        }
        #[cfg(not(any(windows, unix)))]
        {
            Ok(Self {})
        }
    }
}

#[cfg(not(test))]
impl Drop for SignalGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        if self.installed {
            use windows::Win32::System::Console::SetConsoleCtrlHandler;
            // SAFETY: This removes the exact static handler installed above.
            let _ = unsafe { SetConsoleCtrlHandler(Some(console_handler), false) };
        }
        #[cfg(unix)]
        {
            const SIGINT: i32 = 2;
            extern "C" {
                fn signal(signal: i32, handler: usize) -> usize;
            }
            // SAFETY: Restores the process handler value returned by `signal`.
            let _ = unsafe { signal(SIGINT, self.previous_interrupt) };
        }
        CANCEL_REQUESTED.store(false, Ordering::Release);
    }
}

#[cfg(all(windows, not(test)))]
unsafe extern "system" fn console_handler(signal: u32) -> windows::core::BOOL {
    if matches!(signal, 0 | 1) {
        CANCEL_REQUESTED.store(true, Ordering::Release);
        true.into()
    } else {
        false.into()
    }
}

#[cfg(all(unix, not(test)))]
extern "C" fn unix_signal_handler(_signal: i32) {
    CANCEL_REQUESTED.store(true, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::CANCEL_REQUESTED;
    use std::sync::atomic::Ordering;

    #[test]
    fn signal_flag_has_an_explicit_reset_boundary() {
        CANCEL_REQUESTED.store(true, Ordering::Release);
        CANCEL_REQUESTED.store(false, Ordering::Release);
        assert!(!CANCEL_REQUESTED.load(Ordering::Acquire));
    }
}
