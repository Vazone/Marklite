#[cfg(desktop)]
pub mod cli;
#[cfg(desktop)]
mod cli_pdf_runtime;
mod commands;
mod models;
mod platform;
mod runtime;
mod services;
mod utils;

use commands::diagram_commands::{
    get_diagram_runtime_status, install_diagram_runtime, load_diagram_runtime,
    uninstall_diagram_runtime, validate_diagram_svg,
};
use commands::export_commands::{
    cancel_export, cancel_png_export, export_document, remember_export_directory,
    resolve_png_export_directory, suggest_export_path,
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
use commands::platform_commands::get_platform_capabilities;
use commands::recent_commands::{clear_missing_recent_files, get_recent_files, remove_recent_file};
use commands::recovery_commands::{list_recovery, read_recovery, resolve_recovery, save_recovery};
use commands::session_commands::{
    clear_resource_session, clear_session, get_resource_session, get_session,
    update_resource_session, update_session,
};
use commands::settings_commands::{get_settings, reset_settings, update_settings};
use commands::startup_commands::{
    clear_startup_diagnostics, export_startup_diagnostics, mark_frontend_ready,
    record_frontend_startup_event,
};
use services::image_load_service::ImageLoadState;
use services::markdown_service::PreviewState;
use services::open_request_service::OpenRequestState;
use services::startup_diagnostics_service::StartupState;
use tauri::{webview::PageLoadEvent, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(desktop)]
    let startup_state = StartupState::new(env!("CARGO_PKG_VERSION"), tauri::webview_version().ok());
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = runtime::desktop_startup::configure(builder, startup_state.clone());
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    #[cfg(target_os = "android")]
    let builder = builder.setup(platform::android::setup);
    #[cfg(target_os = "android")]
    let builder = builder.plugin(tauri_plugin_marklite_mobile::init());
    #[cfg(target_os = "android")]
    let builder = builder.manage(std::sync::Arc::new(
        platform::android::workspace::AndroidWorkspaceState::default(),
    ));

    let result = builder
        .manage(ImageLoadState::default())
        .manage(PreviewState::default())
        .manage(OpenRequestState::default())
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
            if let Some(state) = webview.try_state::<StartupState>() {
                let _ = state.record_native("webviewNavigation", status, None);
            }
        })
        .invoke_handler(tauri::generate_handler![
            #[cfg(target_os = "android")]
            commands::android_export_commands::pick_android_export_document,
            #[cfg(target_os = "android")]
            commands::android_lifecycle_commands::exit_android_application,
            #[cfg(target_os = "android")]
            commands::android_lifecycle_commands::set_android_status_bar_appearance,
            #[cfg(target_os = "android")]
            commands::android_lifecycle_commands::android_system_insets,
            #[cfg(target_os = "android")]
            commands::android_document_commands::pick_android_document,
            #[cfg(target_os = "android")]
            commands::android_document_commands::create_android_document,
            #[cfg(target_os = "android")]
            commands::android_document_commands::pick_android_tree,
            #[cfg(target_os = "android")]
            commands::android_document_commands::open_android_document,
            #[cfg(target_os = "android")]
            commands::android_document_commands::get_android_recent_documents,
            #[cfg(target_os = "android")]
            commands::android_document_commands::remove_android_recent_document,
            #[cfg(target_os = "android")]
            commands::android_document_commands::android_document_name,
            #[cfg(target_os = "android")]
            commands::android_document_commands::android_document_version,
            #[cfg(target_os = "android")]
            commands::android_document_commands::save_android_document,
            #[cfg(target_os = "android")]
            commands::android_document_commands::drain_android_open_requests,
            #[cfg(target_os = "android")]
            commands::android_workspace_commands::get_android_workspace_preferences,
            #[cfg(target_os = "android")]
            commands::android_workspace_commands::save_android_workspace_preferences,
            #[cfg(target_os = "android")]
            commands::android_workspace_commands::mount_android_workspace,
            #[cfg(target_os = "android")]
            commands::android_workspace_commands::unmount_android_workspace,
            #[cfg(target_os = "android")]
            commands::android_workspace_commands::list_android_workspace,
            #[cfg(target_os = "android")]
            commands::android_workspace_commands::cancel_android_workspace,
            #[cfg(target_os = "android")]
            commands::android_workspace_commands::refresh_android_workspace,
            #[cfg(target_os = "android")]
            commands::android_workspace_commands::collapse_android_workspace,
            #[cfg(target_os = "android")]
            commands::android_workspace_commands::resolve_android_workspace_file,
            #[cfg(target_os = "android")]
            commands::android_workspace_commands::resolve_android_markdown_target,
            #[cfg(target_os = "android")]
            commands::android_workspace_commands::load_android_images,
            #[cfg(desktop)]
            commands::workspace_commands::get_workspace_preferences,
            #[cfg(desktop)]
            commands::workspace_commands::save_workspace_preferences,
            #[cfg(desktop)]
            commands::workspace_commands::mount_workspace,
            #[cfg(desktop)]
            commands::workspace_commands::unmount_workspace,
            #[cfg(desktop)]
            commands::workspace_commands::list_workspace,
            #[cfg(desktop)]
            commands::workspace_commands::cancel_workspace,
            #[cfg(desktop)]
            commands::workspace_commands::refresh_workspace,
            #[cfg(desktop)]
            commands::workspace_commands::collapse_workspace,
            #[cfg(desktop)]
            commands::workspace_commands::resolve_workspace_file,
            cancel_export,
            get_platform_capabilities,
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
            get_resource_session,
            list_recovery,
            read_recovery,
            save_recovery,
            resolve_recovery,
            update_session,
            update_resource_session,
            clear_session,
            clear_resource_session,
            record_frontend_startup_event,
            mark_frontend_ready,
            clear_startup_diagnostics,
            export_startup_diagnostics,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        #[cfg(desktop)]
        let _ = startup_state.record_native("nativeRun", "failed", Some("builderRunFailed"));
        panic!("error while running MarkLite: {error}");
    }
}
