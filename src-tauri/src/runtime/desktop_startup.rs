use crate::services::{
    startup_diagnostics_service::StartupState,
    webview_process_service::{install_process_failed_monitor, WebviewRecoveryState},
};
use tauri::Manager;

pub(crate) fn configure(
    builder: tauri::Builder<tauri::Wry>,
    state: StartupState,
) -> tauri::Builder<tauri::Wry> {
    let recovery = WebviewRecoveryState::from_environment();
    let _ = state.record_native("nativeProcess", "started", None);
    builder
        .manage(std::sync::Arc::new(
            crate::services::workspace_service::WorkspaceService::default(),
        ))
        .manage(state.clone())
        .plugin(tauri_plugin_single_instance::init(
            super::single_instance::handle,
        ))
        .setup(move |app| {
            let _ = state.record_native("tauriSetup", "succeeded", None);
            if let Some(window) = app.get_webview_window("main") {
                if install_process_failed_monitor(
                    &window,
                    app.handle().clone(),
                    state.clone(),
                    recovery.clone(),
                )
                .is_err()
                {
                    let _ = state.record_native(
                        "webviewProcessMonitor",
                        "failed",
                        Some("withWebviewDispatchFailed"),
                    );
                }
            } else {
                let _ = state.record_native(
                    "webviewProcessMonitor",
                    "failed",
                    Some("mainWindowUnavailable"),
                );
            }
            super::startup_watchdog::schedule(app.handle().clone(), state.clone(), false);
            Ok(())
        })
}
