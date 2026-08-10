use serde::{Deserialize, Serialize};

const DEFAULT_SETTINGS_JSON: &str = include_str!("../../../src/shared/default-settings.json");

pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

impl Default for AppSettings {
    fn default() -> Self {
        serde_json::from_str(DEFAULT_SETTINGS_JSON)
            .expect("src/shared/default-settings.json must match AppSettings")
    }
}

impl AppSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !is_hex_color(&self.accent_color) {
            return Err("accentColor 必须是 #RRGGBB 格式".to_string());
        }
        validate_font_family("editorFontFamily", &self.editor_font_family)?;
        validate_font_family("previewFontFamily", &self.preview_font_family)?;
        validate_range("editorFontSize", self.editor_font_size, 12, 24)?;
        validate_range("previewFontSize", self.preview_font_size, 12, 28)?;
        validate_float_range("lineHeight", self.line_height, 1.2, 2.0)?;
        validate_float_range("interfaceScale", self.interface_scale, 0.9, 1.2)?;
        validate_range("cornerRadius", self.corner_radius, 0, 16)?;
        validate_range("tabSize", self.tab_size, 2, 8)?;
        validate_range(
            "autosaveIntervalMs",
            self.autosave_interval_ms,
            1_000,
            3_600_000,
        )?;
        validate_range("previewDebounceMs", self.preview_debounce_ms, 100, 10_000)?;
        validate_range("recentFilesLimit", self.recent_files_limit, 3, 50)?;
        Ok(())
    }
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

fn validate_font_family(name: &str, value: &str) -> Result<(), String> {
    let length = value.chars().count();
    if value.trim().is_empty() || length > 200 || value.chars().any(char::is_control) {
        return Err(format!("{name} 必须为 1–200 个不含控制字符的字符"));
    }
    Ok(())
}

fn validate_float_range(name: &str, value: f32, min: f32, max: f32) -> Result<(), String> {
    if !value.is_finite() || !(min..=max).contains(&value) {
        return Err(format!("{name} 必须在 {min}–{max} 之间"));
    }
    Ok(())
}

fn validate_range<T>(name: &str, value: T, min: T, max: T) -> Result<(), String>
where
    T: Copy + PartialOrd + std::fmt::Display,
{
    if value < min || value > max {
        return Err(format!("{name} 必须在 {min}–{max} 之间"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::AppSettings;

    #[test]
    fn shared_defaults_are_valid() {
        AppSettings::default().validate().unwrap();
    }

    #[test]
    fn rejects_invalid_color_font_and_numeric_boundaries() {
        let settings = AppSettings {
            accent_color: "red".to_string(),
            ..AppSettings::default()
        };
        assert!(settings.validate().unwrap_err().contains("accentColor"));

        let settings = AppSettings {
            editor_font_family: "\n".to_string(),
            ..AppSettings::default()
        };
        assert!(settings
            .validate()
            .unwrap_err()
            .contains("editorFontFamily"));

        let settings = AppSettings {
            recent_files_limit: 0,
            ..AppSettings::default()
        };
        assert!(settings
            .validate()
            .unwrap_err()
            .contains("recentFilesLimit"));

        let settings = AppSettings {
            interface_scale: f32::NAN,
            ..AppSettings::default()
        };
        assert!(settings.validate().unwrap_err().contains("interfaceScale"));
    }
}
