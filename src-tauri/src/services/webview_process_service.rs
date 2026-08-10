#[cfg(windows)]
use webview2_com::{
    Microsoft::Web::WebView2::Win32::COREWEBVIEW2_PROCESS_FAILED_KIND, ProcessFailedEventHandler,
};

use crate::services::startup_diagnostics_service::StartupState;

#[cfg(windows)]
pub fn install_process_failed_monitor(
    window: &tauri::WebviewWindow<tauri::Wry>,
    state: StartupState,
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
        let handler = ProcessFailedEventHandler::create(Box::new(move |_sender, args| {
            let code = args
                .as_ref()
                .and_then(|args| {
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND(0);
                    args.ProcessFailedKind(&mut kind)
                        .ok()
                        .map(|_| process_failed_kind_code(kind.0))
                })
                .unwrap_or("processKindUnavailable");
            let _ = callback_state.record_native("webviewProcess", "failed", Some(code));
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
    state: StartupState,
) -> tauri::Result<()> {
    let _ = state.record_native(
        "webviewProcessMonitor",
        "observed",
        Some("unsupportedPlatform"),
    );
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::process_failed_kind_code;

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
}
