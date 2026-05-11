mod commands;
mod models;
mod services;
mod utils;

use commands::file_commands::{
    export_html_file, get_file_metadata, get_startup_file_arg, open_markdown_file,
    save_markdown_file,
};
use commands::markdown_commands::{calculate_markdown_stats, extract_markdown_outline, render_markdown};
use commands::recent_commands::{clear_missing_recent_files, get_recent_files, remove_recent_file};
use commands::settings_commands::{get_settings, reset_settings, update_settings};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            open_markdown_file,
            save_markdown_file,
            export_html_file,
            get_file_metadata,
            get_startup_file_arg,
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
