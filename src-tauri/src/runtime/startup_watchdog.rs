use std::{thread, time::Duration};

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::services::startup_diagnostics_service::StartupState;

const READY_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn schedule(app: AppHandle, state: StartupState, after_retry: bool) {
    thread::spawn(move || {
        thread::sleep(READY_TIMEOUT);
        if state.is_ready() {
            return;
        }

        let code = if after_retry {
            "retryReadyTimeout"
        } else {
            "initialReadyTimeout"
        };
        let _ = state.record_native("startupWatchdog", "timeout", Some(code));
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }

        if after_retry {
            app.dialog()
                .message(
                    "MarkLite 重试后仍未在 10 秒内就绪。启动诊断已保存在本机；请关闭应用后重新启动，并在可进入界面时从“关于 MarkLite”主动导出诊断。",
                )
                .title("MarkLite 启动恢复")
                .kind(MessageDialogKind::Error)
                .buttons(MessageDialogButtons::Ok)
                .show(|_| {});
            return;
        }

        let retry_app = app.clone();
        let retry_state = state.clone();
        app.dialog()
            .message(
                "MarkLite 未在 10 秒内完成前端就绪握手。可以重试一次；重试不会删除文档、设置、最近文件或 WebView 数据。",
            )
            .title("MarkLite 启动恢复")
            .kind(MessageDialogKind::Error)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "重试一次".into(),
                "暂不重试".into(),
            ))
            .show(move |retry| {
                if !retry || retry_state.is_ready() || !retry_state.claim_retry() {
                    let _ = retry_state.record_native(
                        "startupRecovery",
                        "observed",
                        Some("retryDeclined"),
                    );
                    return;
                }

                let _ = retry_state.record_native(
                    "startupRecovery",
                    "started",
                    Some("userRequestedReload"),
                );
                let Some(window) = retry_app.get_webview_window("main") else {
                    let _ = retry_state.record_native(
                        "startupRecovery",
                        "failed",
                        Some("mainWindowUnavailable"),
                    );
                    return;
                };
                match window.eval("window.location.reload()") {
                    Ok(()) => {
                        schedule(retry_app, retry_state, true);
                    }
                    Err(_) => {
                        let _ = retry_state.record_native(
                            "startupRecovery",
                            "failed",
                            Some("reloadEvaluationFailed"),
                        );
                    }
                }
            });
    });
}
