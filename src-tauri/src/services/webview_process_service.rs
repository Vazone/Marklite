#[cfg(windows)]
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[cfg(windows)]
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2ProcessFailedEventArgs2, COREWEBVIEW2_PROCESS_FAILED_KIND,
        COREWEBVIEW2_PROCESS_FAILED_REASON,
    },
    ProcessFailedEventHandler,
};
#[cfg(windows)]
use windows::core::Interface;

use crate::services::startup_diagnostics_service::StartupState;

#[cfg(windows)]
const RECOVERY_MARKER: &str = "MARKLITE_WEBVIEW_RECOVERY_ATTEMPT";
#[cfg(windows)]
const STARTUP_RECOVERY_WINDOW: Duration = Duration::from_secs(30);
#[cfg(windows)]
const BROWSER_PROCESS_EXITED_KIND: i32 = 0;

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryDecision {
    Observe,
    Restart,
    ExitAfterRepeatedFailure,
    ExitAfterLateFailure,
    IgnoreDuplicate,
}

#[cfg(windows)]
#[derive(Clone)]
pub struct WebviewRecoveryState {
    inner: Arc<WebviewRecoveryStateInner>,
}

#[cfg(windows)]
struct WebviewRecoveryStateInner {
    started_at: Instant,
    attempted_in_parent: bool,
    handled_browser_exit: AtomicBool,
}

#[cfg(windows)]
impl WebviewRecoveryState {
    pub fn from_environment() -> Self {
        Self::new(std::env::var_os(RECOVERY_MARKER).is_some())
    }

    fn new(attempted_in_parent: bool) -> Self {
        Self {
            inner: Arc::new(WebviewRecoveryStateInner {
                started_at: Instant::now(),
                attempted_in_parent,
                handled_browser_exit: AtomicBool::new(false),
            }),
        }
    }

    fn decide(&self, kind: i32) -> RecoveryDecision {
        self.decide_at(kind, self.inner.started_at.elapsed())
    }

    fn decide_at(&self, kind: i32, elapsed: Duration) -> RecoveryDecision {
        if kind != BROWSER_PROCESS_EXITED_KIND {
            return RecoveryDecision::Observe;
        }
        if self
            .inner
            .handled_browser_exit
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return RecoveryDecision::IgnoreDuplicate;
        }
        if self.inner.attempted_in_parent {
            RecoveryDecision::ExitAfterRepeatedFailure
        } else if elapsed <= STARTUP_RECOVERY_WINDOW {
            RecoveryDecision::Restart
        } else {
            RecoveryDecision::ExitAfterLateFailure
        }
    }
}

#[cfg(not(windows))]
#[derive(Clone, Default)]
pub struct WebviewRecoveryState;

#[cfg(not(windows))]
impl WebviewRecoveryState {
    pub fn from_environment() -> Self {
        Self
    }
}

#[cfg(windows)]
pub fn clear_recovery_marker() {
    std::env::remove_var(RECOVERY_MARKER);
}

#[cfg(not(windows))]
pub fn clear_recovery_marker() {}

#[cfg(windows)]
pub fn install_process_failed_monitor(
    window: &tauri::WebviewWindow<tauri::Wry>,
    app: tauri::AppHandle,
    state: StartupState,
    recovery: WebviewRecoveryState,
) -> tauri::Result<()> {
    window.with_webview(move |webview| unsafe {
        let core = match webview.controller().CoreWebView2() {
            Ok(core) => core,
            Err(_) => {
                let _ = state.record_native(
                    "webviewProcessMonitor",
                    "failed",
                    Some("coreWebViewUnavailable"),
                );
                return;
            }
        };

        let callback_state = state.clone();
        let callback_app = app.clone();
        let callback_recovery = recovery.clone();
        let handler = ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
            let kind = args
                .as_ref()
                .and_then(|args| {
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND(0);
                    args.ProcessFailedKind(&mut kind).ok().map(|_| kind.0)
                })
                .unwrap_or(-1);
            let code = process_failed_kind_code(kind);
            let _ = callback_state.record_native("webviewProcess", "failed", Some(code));

            if let Some(reason_code) = process_failed_reason(args.as_ref()) {
                let _ = callback_state.record_native(
                    "webviewProcessReason",
                    "observed",
                    Some(reason_code),
                );
            }

            match callback_recovery.decide(kind) {
                RecoveryDecision::Restart => {
                    let _ = callback_state.record_native(
                        "webviewRecovery",
                        "started",
                        Some("browserProcessRestartRequested"),
                    );
                    std::env::set_var(RECOVERY_MARKER, "1");
                    callback_app.request_restart();
                }
                RecoveryDecision::ExitAfterRepeatedFailure => {
                    let _ = callback_state.record_native(
                        "webviewRecovery",
                        "failed",
                        Some("repeatedBrowserProcessExit"),
                    );
                    callback_app.exit(1);
                }
                RecoveryDecision::ExitAfterLateFailure => {
                    let _ = callback_state.record_native(
                        "webviewRecovery",
                        "failed",
                        Some("lateBrowserProcessExit"),
                    );
                    callback_app.exit(1);
                }
                RecoveryDecision::IgnoreDuplicate => {
                    let _ = callback_state.record_native(
                        "webviewRecovery",
                        "observed",
                        Some("duplicateBrowserProcessExit"),
                    );
                }
                RecoveryDecision::Observe => {}
            }
            Ok(())
        }));

        let mut token = 0_i64;
        match core.add_ProcessFailed(&handler, &mut token) {
            Ok(()) => {
                // WebView2 retains the COM handler; its lifetime follows the core webview.
                // We intentionally observe until teardown and do not need the removal token.
                let _ = state.record_native(
                    "webviewProcessMonitor",
                    "succeeded",
                    Some("processFailedHandlerRegistered"),
                );
            }
            Err(_) => {
                let _ = state.record_native(
                    "webviewProcessMonitor",
                    "failed",
                    Some("handlerRegistrationFailed"),
                );
            }
        }
    })
}

#[cfg(not(windows))]
pub fn install_process_failed_monitor(
    _window: &tauri::WebviewWindow<tauri::Wry>,
    _app: tauri::AppHandle,
    state: StartupState,
    _recovery: WebviewRecoveryState,
) -> tauri::Result<()> {
    let _ = state.record_native(
        "webviewProcessMonitor",
        "observed",
        Some("unsupportedPlatform"),
    );
    Ok(())
}

#[cfg(windows)]
unsafe fn process_failed_reason(
    args: Option<
        &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2ProcessFailedEventArgs,
    >,
) -> Option<&'static str> {
    let args = args?;
    let args2 = args.cast::<ICoreWebView2ProcessFailedEventArgs2>().ok()?;
    let mut reason = COREWEBVIEW2_PROCESS_FAILED_REASON(0);
    args2
        .Reason(&mut reason)
        .ok()
        .map(|_| process_failed_reason_code(reason.0))
}

#[cfg(windows)]
pub fn process_failed_kind_code(kind: i32) -> &'static str {
    match kind {
        0 => "browserProcessExited",
        1 => "renderProcessExited",
        2 => "renderProcessUnresponsive",
        3 => "frameRenderProcessExited",
        4 => "utilityProcessExited",
        5 => "sandboxHelperProcessExited",
        6 => "gpuProcessExited",
        7 => "ppapiPluginProcessExited",
        8 => "ppapiBrokerProcessExited",
        9 => "unknownProcessExited",
        _ => "unrecognizedProcessFailureKind",
    }
}

#[cfg(windows)]
pub fn process_failed_reason_code(reason: i32) -> &'static str {
    match reason {
        0 => "unexpectedProcessFailure",
        1 => "unresponsiveProcessFailure",
        2 => "terminatedProcessFailure",
        3 => "crashedProcessFailure",
        4 => "processLaunchFailed",
        5 => "processOutOfMemory",
        6 => "profileDeleted",
        _ => "unrecognizedProcessFailureReason",
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn maps_all_webview2_process_failure_kinds_without_free_text() {
        let expected = [
            "browserProcessExited",
            "renderProcessExited",
            "renderProcessUnresponsive",
            "frameRenderProcessExited",
            "utilityProcessExited",
            "sandboxHelperProcessExited",
            "gpuProcessExited",
            "ppapiPluginProcessExited",
            "ppapiBrokerProcessExited",
            "unknownProcessExited",
        ];
        for (kind, code) in expected.into_iter().enumerate() {
            assert_eq!(process_failed_kind_code(kind as i32), code);
        }
        assert_eq!(
            process_failed_kind_code(99),
            "unrecognizedProcessFailureKind"
        );
    }

    #[test]
    fn maps_process_failure_reasons_without_free_text() {
        let expected = [
            "unexpectedProcessFailure",
            "unresponsiveProcessFailure",
            "terminatedProcessFailure",
            "crashedProcessFailure",
            "processLaunchFailed",
            "processOutOfMemory",
            "profileDeleted",
        ];
        for (reason, code) in expected.into_iter().enumerate() {
            assert_eq!(process_failed_reason_code(reason as i32), code);
        }
        assert_eq!(
            process_failed_reason_code(99),
            "unrecognizedProcessFailureReason"
        );
    }

    #[test]
    fn browser_exit_restarts_only_once_during_startup() {
        let recovery = WebviewRecoveryState::new(false);

        assert_eq!(
            recovery.decide_at(BROWSER_PROCESS_EXITED_KIND, Duration::from_secs(4)),
            RecoveryDecision::Restart
        );
        assert_eq!(
            recovery.decide_at(BROWSER_PROCESS_EXITED_KIND, Duration::from_secs(5)),
            RecoveryDecision::IgnoreDuplicate
        );
    }

    #[test]
    fn recovered_launch_exits_instead_of_looping() {
        let recovery = WebviewRecoveryState::new(true);

        assert_eq!(
            recovery.decide_at(BROWSER_PROCESS_EXITED_KIND, Duration::from_secs(4)),
            RecoveryDecision::ExitAfterRepeatedFailure
        );
    }

    #[test]
    fn browser_exit_after_startup_window_exits_cleanly() {
        let recovery = WebviewRecoveryState::new(false);

        assert_eq!(
            recovery.decide_at(BROWSER_PROCESS_EXITED_KIND, Duration::from_secs(31)),
            RecoveryDecision::ExitAfterLateFailure
        );
    }

    #[test]
    fn renderer_failures_are_observed_without_restarting_application() {
        let recovery = WebviewRecoveryState::new(false);

        assert_eq!(
            recovery.decide_at(1, Duration::from_secs(2)),
            RecoveryDecision::Observe
        );
        assert_eq!(
            recovery.decide_at(2, Duration::from_secs(2)),
            RecoveryDecision::Observe
        );
    }
}
