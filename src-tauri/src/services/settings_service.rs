use std::{
    fs,
    path::Path,
    sync::{Mutex, OnceLock},
};

use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Value};

use crate::{
    models::{
        app_error::AppError,
        settings::{AppSettings, SETTINGS_SCHEMA_VERSION},
    },
    utils::{
        atomic_write::{atomic_write, recover_atomic_write},
        path_utils::settings_path,
    },
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsDocument<'a> {
    version: u32,
    settings: &'a AppSettings,
}

enum SettingsParseError {
    UnsupportedVersion(u64),
    Invalid(String),
}

static SETTINGS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn load_settings() -> Result<AppSettings, AppError> {
    let _guard = settings_lock();
    load_settings_from(&settings_path()?)
}

pub fn save_settings(settings: &AppSettings) -> Result<AppSettings, AppError> {
    let _guard = settings_lock();
    save_settings_to(&settings_path()?, settings)
}

pub fn reset_settings() -> Result<AppSettings, AppError> {
    save_settings(&AppSettings::default())
}

fn load_settings_from(path: &Path) -> Result<AppSettings, AppError> {
    recover_atomic_write(path).map_err(AppError::settings_read_failed)?;
    if !path.exists() {
        let defaults = AppSettings::default();
        save_settings_to(path, &defaults)?;
        return Ok(defaults);
    }

    let raw = fs::read_to_string(path).map_err(AppError::settings_read_failed)?;
    match parse_settings(&raw) {
        Ok((settings, needs_rewrite)) => {
            if needs_rewrite {
                save_settings_to(path, &settings)?;
            }
            Ok(settings)
        }
        Err(SettingsParseError::UnsupportedVersion(version)) => {
            Err(AppError::settings_version_unsupported(version))
        }
        Err(SettingsParseError::Invalid(message)) => {
            backup_corrupt_file(path)?;
            save_settings_to(path, &AppSettings::default())?;
            Err(AppError::settings_read_failed(message))
        }
    }
}

fn save_settings_to(path: &Path, settings: &AppSettings) -> Result<AppSettings, AppError> {
    settings.validate().map_err(AppError::invalid_settings)?;
    let document = SettingsDocument {
        version: SETTINGS_SCHEMA_VERSION,
        settings,
    };
    let content =
        serde_json::to_string_pretty(&document).map_err(AppError::settings_write_failed)?;
    atomic_write(path, content.as_bytes()).map_err(AppError::settings_write_failed)?;
    Ok(settings.clone())
}

fn parse_settings(raw: &str) -> Result<(AppSettings, bool), SettingsParseError> {
    let document: Value = serde_json::from_str(raw)
        .map_err(|error| SettingsParseError::Invalid(error.to_string()))?;
    let (payload, legacy) = match document.get("version") {
        Some(version) => {
            let version = version
                .as_u64()
                .ok_or_else(|| SettingsParseError::Invalid("version 必须是非负整数".to_string()))?;
            if version != u64::from(SETTINGS_SCHEMA_VERSION) {
                return Err(SettingsParseError::UnsupportedVersion(version));
            }
            let payload = document
                .get("settings")
                .cloned()
                .ok_or_else(|| SettingsParseError::Invalid("缺少 settings 对象".to_string()))?;
            (payload, false)
        }
        None => (document.clone(), true),
    };

    let payload_object = payload
        .as_object()
        .ok_or_else(|| SettingsParseError::Invalid("settings 必须是对象".to_string()))?;
    let mut merged = serde_json::to_value(AppSettings::default())
        .expect("AppSettings defaults must serialize")
        .as_object()
        .expect("AppSettings must serialize as an object")
        .clone();
    for (key, value) in payload_object {
        if merged.contains_key(key) {
            merged.insert(key.clone(), value.clone());
        }
    }

    let settings: AppSettings = serde_json::from_value(Value::Object(merged))
        .map_err(|error| SettingsParseError::Invalid(error.to_string()))?;
    settings.validate().map_err(SettingsParseError::Invalid)?;
    let normalized = serde_json::to_value(&settings).expect("validated AppSettings must serialize");
    let normalized_document = json!({
        "version": SETTINGS_SCHEMA_VERSION,
        "settings": normalized
    });

    Ok((settings, legacy || document != normalized_document))
}

fn backup_corrupt_file(path: &Path) -> Result<(), AppError> {
    let timestamp = Utc::now().format("%Y%m%d%H%M%S%3f");
    let backup_path = path.with_extension(format!("json.corrupt-{timestamp}"));
    fs::rename(path, backup_path).map_err(AppError::settings_write_failed)
}

fn settings_lock() -> std::sync::MutexGuard<'static, ()> {
    SETTINGS_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use serde_json::{json, Value};

    use super::{load_settings_from, save_settings_to};
    use crate::models::settings::{AppSettings, SETTINGS_SCHEMA_VERSION};

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "marklite-settings-{}-{}-{name}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn remove_backups(path: &PathBuf) {
        let prefix = path.file_stem().unwrap().to_string_lossy();
        for entry in fs::read_dir(path.parent().unwrap()).unwrap().flatten() {
            if entry.path() != *path
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(prefix.as_ref())
            {
                fs::remove_file(entry.path()).unwrap();
            }
        }
    }

    #[test]
    fn migrates_legacy_objects_and_fills_missing_fields() {
        let path = test_path("legacy");
        let mut legacy = serde_json::to_value(AppSettings::default()).unwrap();
        let object = legacy.as_object_mut().unwrap();
        object.remove("showStatusBar");
        object.insert("allowLocalImages".to_string(), Value::Bool(true));
        fs::write(&path, serde_json::to_string_pretty(&legacy).unwrap()).unwrap();

        let loaded = load_settings_from(&path).unwrap();
        let migrated: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();

        assert!(loaded.show_status_bar);
        assert_eq!(migrated["version"], SETTINGS_SCHEMA_VERSION);
        assert_eq!(migrated["settings"]["allowLocalImages"], true);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn round_trips_the_versioned_document() {
        let path = test_path("round-trip");
        let settings = AppSettings {
            theme: crate::models::settings::ThemeMode::Dark,
            ..AppSettings::default()
        };

        save_settings_to(&path, &settings).unwrap();

        assert_eq!(load_settings_from(&path).unwrap(), settings);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_invalid_updates_without_writing() {
        let path = test_path("invalid-update");
        let settings = AppSettings {
            recent_files_limit: 0,
            ..AppSettings::default()
        };

        let error = save_settings_to(&path, &settings).unwrap_err();

        assert_eq!(error.code, "INVALID_SETTINGS");
        assert!(!path.exists());
    }

    #[test]
    fn backs_up_invalid_known_values_and_restores_defaults() {
        let path = test_path("invalid-file");
        let mut settings = serde_json::to_value(AppSettings::default())
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        settings.insert("recentFilesLimit".to_string(), Value::from(0));
        fs::write(
            &path,
            serde_json::to_string_pretty(&json!({
                "version": SETTINGS_SCHEMA_VERSION,
                "settings": settings
            }))
            .unwrap(),
        )
        .unwrap();

        let error = load_settings_from(&path).unwrap_err();

        assert_eq!(error.code, "SETTINGS_READ_FAILED");
        assert_eq!(load_settings_from(&path).unwrap(), AppSettings::default());
        remove_backups(&path);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn preserves_future_version_files() {
        let path = test_path("future-version");
        let raw = r#"{"version":999,"settings":{}}"#;
        fs::write(&path, raw).unwrap();

        let error = load_settings_from(&path).unwrap_err();

        assert_eq!(error.code, "SETTINGS_VERSION_UNSUPPORTED");
        assert_eq!(fs::read_to_string(&path).unwrap(), raw);
        fs::remove_file(path).unwrap();
    }
}
