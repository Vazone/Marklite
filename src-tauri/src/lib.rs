mod commands;
mod models;
mod services;
mod utils;

use commands::file_commands::{
    export_html_file, get_file_metadata, get_startup_file_arg, open_markdown_file,
    save_markdown_file, show_in_file_manager,
};
use commands::markdown_commands::{calculate_markdown_stats, extract_markdown_outline, render_markdown};
use commands::recent_commands::{clear_missing_recent_files, get_recent_files, remove_recent_file};
use commands::settings_commands::{get_settings, reset_settings, update_settings};
use std::path::PathBuf;
use tauri::{Emitter, Manager};

const SINGLE_INSTANCE_OPEN_FILE_EVENT: &str = "single-instance-open-file";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
        .invoke_handler(tauri::generate_handler![
            open_markdown_file,
            save_markdown_file,
            export_html_file,
            get_file_metadata,
            get_startup_file_arg,
            show_in_file_manager,
            render_markdown,
            extract_markdown_outline,
            calculate_markdown_stats,
            get_settings,
            update_settings,
            reset_settings,
            get_recent_files,
            remove_recent_file,
            clear_missing_recent_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running MarkLite");
}

fn markdown_arg_from_args(args: &[String], cwd: &str) -> Option<String> {
    args.iter().skip(1).find_map(|arg| {
        let mut path = PathBuf::from(arg);
        if path.is_relative() {
            path = PathBuf::from(cwd).join(path);
        }

        if path.exists() && services::file_service::ensure_allowed_file(&path).is_ok() {
            Some(path.to_string_lossy().to_string())
        } else {
            None
        }
    })
}
