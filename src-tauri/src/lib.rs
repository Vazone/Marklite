mod commands;
mod models;
mod services;
mod utils;

use commands::file_commands::{
    export_html_file, get_startup_file_arg, open_markdown_file, save_markdown_file,
    show_in_file_manager,
};
use commands::markdown_commands::render_markdown;
use commands::navigation_commands::{load_local_image, resolve_markdown_target};
use commands::recent_commands::{clear_missing_recent_files, get_recent_files, remove_recent_file};
use commands::session_commands::{clear_session, get_session, update_session};
use commands::settings_commands::{get_settings, reset_settings, update_settings};
use commands::startup_commands::{
    clear_startup_diagnostics, export_startup_diagnostics, mark_frontend_ready,
    record_frontend_startup_event,
};
use services::startup_diagnostics_service::StartupState;
use std::path::PathBuf;
use std::{thread, time::Duration};
use tauri::{webview::PageLoadEvent, AppHandle, Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

const SINGLE_INSTANCE_OPEN_FILE_EVENT: &str = "single-instance-open-file";
const STARTUP_READY_TIMEOUT: Duration = Duration::from_secs(10);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let startup_state = StartupState::new(env!("CARGO_PKG_VERSION"), tauri::webview_version().ok());
    let _ = startup_state.record_native("nativeProcess", "started", None);
    let navigation_state = startup_state.clone();
    let setup_state = startup_state.clone();

    let result = tauri::Builder::default()
        .manage(startup_state.clone())
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            if let Some(path) = markdown_arg_from_args(&args, &cwd) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit(SINGLE_INSTANCE_OPEN_FILE_EVENT, path);
                    let _ = window.unminimize();
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .on_page_load(move |webview, payload| {
            if webview.label() != "main" {
                return;
            }
            let status = match payload.event() {
                PageLoadEvent::Started => "started",
                PageLoadEvent::Finished => "observed",
            };
            let _ = navigation_state.record_native("webviewNavigation", status, None);
        })
        .setup(move |app| {
            let _ = setup_state.record_native("tauriSetup", "succeeded", None);
            if let Some(window) = app.get_webview_window("main") {
                if services::webview_process_service::install_process_failed_monitor(
                    &window,
                    setup_state.clone(),
                )
                .is_err()
                {
                    let _ = setup_state.record_native(
                        "webviewProcessMonitor",
                        "failed",
                        Some("withWebviewDispatchFailed"),
                    );
                }
            } else {
                let _ = setup_state.record_native(
                    "webviewProcessMonitor",
                    "failed",
                    Some("mainWindowUnavailable"),
                );
            }
            schedule_startup_watchdog(app.handle().clone(), setup_state.clone(), false);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_markdown_file,
            save_markdown_file,
            export_html_file,
            get_startup_file_arg,
            show_in_file_manager,
            render_markdown,
            resolve_markdown_target,
            load_local_image,
            get_settings,
            update_settings,
            reset_settings,
            get_recent_files,
            remove_recent_file,
            clear_missing_recent_files,
            get_session,
            update_session,
            clear_session,
            record_frontend_startup_event,
            mark_frontend_ready,
            clear_startup_diagnostics,
            export_startup_diagnostics,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        let _ = startup_state.record_native("nativeRun", "failed", Some("builderRunFailed"));
        panic!("error while running MarkLite: {error}");
    }
}

fn schedule_startup_watchdog(app: AppHandle, state: StartupState, after_retry: bool) {
    thread::spawn(move || {
        thread::sleep(STARTUP_READY_TIMEOUT);
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
                        schedule_startup_watchdog(retry_app, retry_state, true);
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

fn markdown_arg_from_args(args: &[String], cwd: &str) -> Option<String> {
    args.iter().skip(1).find_map(|arg| {
        let mut path = PathBuf::from(arg);
        if path.is_relative() {
            path = PathBuf::from(cwd).join(path);
        }

        if path.is_file() && services::file_service::ensure_allowed_file(&path).is_ok() {
            utils::path_utils::canonicalize_path(&path)
                .ok()
                .map(|path| path.to_string_lossy().to_string())
        } else {
            None
        }
    })
}
