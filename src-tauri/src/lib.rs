pub mod cli;
mod cli_pdf_runtime;
mod commands;
mod models;
mod runtime;
mod services;
mod utils;

use commands::diagram_commands::{
    get_diagram_runtime_status, install_diagram_runtime, load_diagram_runtime,
    uninstall_diagram_runtime, validate_diagram_svg,
};
use commands::export_commands::{
    cancel_png_export, export_document, remember_export_directory, resolve_png_export_directory,
    suggest_export_path,
};
use commands::file_commands::{
    get_startup_file_arg, open_markdown_file, resolve_file_identity, resolve_file_version,
    save_markdown_file, show_in_file_manager,
};
use commands::markdown_commands::{
    analyze_markdown, release_markdown_preview, render_markdown, render_markdown_window,
};
use commands::navigation_commands::{
    cancel_local_image_job, load_local_image, open_validated_email_link, resolve_markdown_target,
};
use commands::open_request_commands::drain_open_file_requests;
use commands::recent_commands::{clear_missing_recent_files, get_recent_files, remove_recent_file};
use commands::session_commands::{clear_session, get_session, update_session};
use commands::settings_commands::{get_settings, reset_settings, update_settings};
use commands::startup_commands::{
    clear_startup_diagnostics, export_startup_diagnostics, mark_frontend_ready,
    record_frontend_startup_event,
};
use services::image_load_service::ImageLoadState;
use services::markdown_service::PreviewState;
use services::open_request_service::OpenRequestState;
use services::startup_diagnostics_service::StartupState;
use services::webview_process_service::WebviewRecoveryState;
use tauri::{webview::PageLoadEvent, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let startup_state = StartupState::new(env!("CARGO_PKG_VERSION"), tauri::webview_version().ok());
    let webview_recovery = WebviewRecoveryState::from_environment();
    let _ = startup_state.record_native("nativeProcess", "started", None);
    let navigation_state = startup_state.clone();
    let setup_state = startup_state.clone();
    let setup_recovery = webview_recovery.clone();

    let result = tauri::Builder::default()
        .manage(startup_state.clone())
        .manage(ImageLoadState::default())
        .manage(PreviewState::default())
        .manage(OpenRequestState::default())
        .plugin(tauri_plugin_single_instance::init(
            runtime::single_instance::handle,
        ))
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
                    app.handle().clone(),
                    setup_state.clone(),
                    setup_recovery.clone(),
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
            runtime::startup_watchdog::schedule(app.handle().clone(), setup_state.clone(), false);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            validate_diagram_svg,
            get_diagram_runtime_status,
            load_diagram_runtime,
            install_diagram_runtime,
            uninstall_diagram_runtime,
            open_markdown_file,
            save_markdown_file,
            resolve_file_identity,
            resolve_file_version,
            export_document,
            resolve_png_export_directory,
            cancel_png_export,
            suggest_export_path,
            remember_export_directory,
            get_startup_file_arg,
            show_in_file_manager,
            render_markdown,
            render_markdown_window,
            release_markdown_preview,
            analyze_markdown,
            resolve_markdown_target,
            open_validated_email_link,
            load_local_image,
            cancel_local_image_job,
            drain_open_file_requests,
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
