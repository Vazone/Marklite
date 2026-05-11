use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: ThemeMode,
    pub accent_color: String,
    pub editor_font_family: String,
    pub preview_font_family: String,
    pub editor_font_size: u32,
    pub preview_font_size: u32,
    pub line_height: f32,
    pub interface_scale: f32,
    pub corner_radius: u32,
    pub show_line_numbers: bool,
    pub word_wrap: bool,
    pub tab_size: u8,
    pub insert_spaces: bool,
    pub autosave_enabled: bool,
    pub autosave_interval_ms: u64,
    pub live_preview_enabled: bool,
    pub preview_debounce_ms: u64,
    pub sync_scroll: bool,
    pub show_sidebar: bool,
    pub show_status_bar: bool,
    pub restore_last_session: bool,
    pub recent_files_limit: usize,
    pub markdown_toolbar_enabled: bool,
    pub allow_local_images: bool,
    pub confirm_external_links: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            accent_color: "#0f8b8d".to_string(),
            editor_font_family: "Cascadia Code, Consolas, monospace".to_string(),
            preview_font_family: "Inter, Segoe UI, system-ui, sans-serif".to_string(),
            editor_font_size: 15,
            preview_font_size: 16,
            line_height: 1.65,
            interface_scale: 1.0,
            corner_radius: 8,
            show_line_numbers: true,
            word_wrap: true,
            tab_size: 2,
            insert_spaces: true,
            autosave_enabled: false,
            autosave_interval_ms: 3000,
            live_preview_enabled: true,
            preview_debounce_ms: 250,
            sync_scroll: true,
            show_sidebar: true,
            show_status_bar: true,
            restore_last_session: true,
            recent_files_limit: 12,
            markdown_toolbar_enabled: true,
            allow_local_images: false,
            confirm_external_links: true,
        }
    }
}
