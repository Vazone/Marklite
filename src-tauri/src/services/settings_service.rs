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
    utils::{atomic_write::atomic_write, bounded_read::OpenedFile, path_utils::settings_path},
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
static SETTINGS_CACHE: OnceLock<Mutex<Option<AppSettings>>> = OnceLock::new();
const MAX_SETTINGS_FILE_BYTES: u64 = 256 * 1024;

pub fn load_settings() -> Result<AppSettings, AppError> {
    let _guard = settings_lock();
    if let Some(settings) = settings_cache().clone() {
        return Ok(settings);
    }
    let settings = load_settings_from(&settings_path()?)?;
    *settings_cache() = Some(settings.clone());
    Ok(settings)
}

pub fn save_settings(settings: &AppSettings) -> Result<AppSettings, AppError> {
    let _guard = settings_lock();
    save_settings_at(&settings_path()?, settings)
}

fn save_settings_at(path: &Path, settings: &AppSettings) -> Result<AppSettings, AppError> {
    let saved = save_settings_to(path, settings)?;
    *settings_cache() = Some(saved.clone());
    Ok(saved)
}

pub fn reset_settings() -> Result<AppSettings, AppError> {
    save_settings(&AppSettings::default())
}

fn load_settings_from(path: &Path) -> Result<AppSettings, AppError> {
    if !path.exists() {
        let defaults = AppSettings::default();
        save_settings_to(path, &defaults)?;
        return Ok(defaults);
    }

    let raw = read_settings_source(path)?;
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
    ensure_supported_settings_version(path)?;
    let document = SettingsDocument {
        version: SETTINGS_SCHEMA_VERSION,
        settings,
    };
    let content =
        serde_json::to_string_pretty(&document).map_err(AppError::settings_write_failed)?;
    if content.len() as u64 > MAX_SETTINGS_FILE_BYTES {
        return Err(AppError::settings_write_failed(std::io::Error::new(
            std::io::ErrorKind::FileTooLarge,
            "设置文件超过允许的字节上限",
        )));
    }
    atomic_write(path, content.as_bytes()).map_err(AppError::settings_write_failed)?;
    Ok(settings.clone())
}

fn ensure_supported_settings_version(path: &Path) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }
    let raw = read_settings_source(path)?;
    // Malformed JSON retains the existing save behavior; only a recognized version blocks writes.
    if let Ok(document) = serde_json::from_str::<Value>(&raw) {
        if let Some(raw_version) = document.get("version") {
            let version = raw_version
                .as_u64()
                .ok_or_else(|| AppError::settings_read_failed("version 必须是非负整数"))?;
            if version != u64::from(SETTINGS_SCHEMA_VERSION) {
                return Err(AppError::settings_version_unsupported(version));
            }
        }
    }
    Ok(())
}

fn read_settings_source(path: &Path) -> Result<String, AppError> {
    let opened = OpenedFile::open(path).map_err(AppError::settings_read_failed)?;
    if !opened.metadata().is_file() {
        return Err(AppError::settings_read_failed("设置路径不是普通文件"));
    }
    if opened.metadata().len() > MAX_SETTINGS_FILE_BYTES {
        return Err(AppError::settings_read_failed("设置文件超过允许的字节上限"));
    }
    opened
        .read_to_string_bounded(MAX_SETTINGS_FILE_BYTES)
        .map_err(AppError::settings_read_failed)
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

fn settings_cache() -> std::sync::MutexGuard<'static, Option<AppSettings>> {
    SETTINGS_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{json, Value};

    use super::{
        load_settings_from, save_settings_at, save_settings_to, settings_cache, settings_lock,
        MAX_SETTINGS_FILE_BYTES,
    };
    use crate::{
        models::settings::{AppSettings, SETTINGS_SCHEMA_VERSION},
        utils::test_support::TestDirectory,
    };

    #[test]
    fn migrates_legacy_objects_and_fills_missing_fields() {
        let directory = TestDirectory::new("settings-legacy");
        let path = directory.path().join("settings.json");
        let mut legacy = serde_json::to_value(AppSettings::default()).unwrap();
        let object = legacy.as_object_mut().unwrap();
        object.remove("showStatusBar");
        object.remove("language");
        object.insert("allowLocalImages".to_string(), Value::Bool(true));
        fs::write(&path, serde_json::to_string_pretty(&legacy).unwrap()).unwrap();

        let loaded = load_settings_from(&path).unwrap();
        let migrated: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();

        assert!(loaded.show_status_bar);
        assert_eq!(
            loaded.language,
            crate::models::settings::AppLanguage::English
        );
        assert_eq!(migrated["version"], SETTINGS_SCHEMA_VERSION);
        assert_eq!(migrated["settings"]["language"], "en");
        assert_eq!(migrated["settings"]["allowLocalImages"], true);
    }

    #[test]
    fn removes_retired_zoom_field_without_losing_other_settings() {
        for versioned in [false, true] {
            let directory = TestDirectory::new("settings-retired-zoom");
            let path = directory.path().join("settings.json");
            let mut payload = serde_json::to_value(AppSettings::default()).unwrap();
            payload["interfaceScale"] = json!(1.25);
            payload["editorFontSize"] = json!(20);
            let document = if versioned {
                json!({ "version": SETTINGS_SCHEMA_VERSION, "settings": payload })
            } else {
                payload
            };
            fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();

            assert_eq!(load_settings_from(&path).unwrap().editor_font_size, 20);
            let rewritten: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(rewritten["settings"]["editorFontSize"], 20);
            assert!(rewritten["settings"].get("interfaceScale").is_none());
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn round_trips_the_versioned_document() {
        let directory = TestDirectory::new("settings-round-trip");
        let path = directory.path().join("settings.json");
        let settings = AppSettings {
            language: crate::models::settings::AppLanguage::SimplifiedChinese,
            theme: crate::models::settings::ThemeMode::Dark,
            ..AppSettings::default()
        };

        save_settings_to(&path, &settings).unwrap();

        assert_eq!(load_settings_from(&path).unwrap(), settings);
    }

    #[test]
    fn rejects_invalid_updates_without_writing() {
        let directory = TestDirectory::new("settings-invalid-update");
        let path = directory.path().join("settings.json");
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
        let directory = TestDirectory::new("settings-invalid-file");
        let path = directory.path().join("settings.json");
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
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
        let changed = AppSettings {
            editor_font_size: 20,
            ..AppSettings::default()
        };
        assert_eq!(save_settings_to(&path, &changed).unwrap(), changed);
        assert_eq!(load_settings_from(&path).unwrap(), changed);
    }

    #[test]
    fn preserves_future_version_files() {
        let directory = TestDirectory::new("settings-future-version");
        let path = directory.path().join("settings.json");
        let raw = r#"{"version":999,"settings":{}}"#;
        fs::write(&path, raw).unwrap();

        let error = load_settings_from(&path).unwrap_err();

        assert_eq!(error.code, "SETTINGS_VERSION_UNSUPPORTED");
        assert_eq!(fs::read_to_string(&path).unwrap(), raw);
    }

    #[test]
    fn save_and_reset_cannot_replace_a_future_version() {
        let directory = TestDirectory::new("settings-future-write");
        let path = directory.path().join("settings.json");
        let raw = br#"{"version":999,"settings":{"newFeature":true}}"#;
        fs::write(&path, raw).unwrap();

        assert_eq!(
            load_settings_from(&path).unwrap_err().code,
            "SETTINGS_VERSION_UNSUPPORTED"
        );
        for replacement in [
            AppSettings {
                editor_font_size: 20,
                ..AppSettings::default()
            },
            AppSettings::default(),
        ] {
            assert_eq!(
                save_settings_to(&path, &replacement).unwrap_err().code,
                "SETTINGS_VERSION_UNSUPPORTED"
            );
            assert_eq!(fs::read(&path).unwrap(), raw);
        }
    }

    #[test]
    fn save_rechecks_the_file_after_a_successful_load() {
        let directory = TestDirectory::new("settings-replaced-after-read");
        let path = directory.path().join("settings.json");
        save_settings_to(&path, &AppSettings::default()).unwrap();
        load_settings_from(&path).unwrap();
        let future = br#"{"version":999,"settings":{"newFeature":true}}"#;
        fs::write(&path, future).unwrap();

        assert_eq!(
            save_settings_to(&path, &AppSettings::default())
                .unwrap_err()
                .code,
            "SETTINGS_VERSION_UNSUPPORTED"
        );
        assert_eq!(fs::read(&path).unwrap(), future);
    }

    #[test]
    fn rejected_future_write_leaves_cached_settings_unchanged() {
        let directory = TestDirectory::new("settings-future-cache");
        let path = directory.path().join("settings.json");
        fs::write(&path, br#"{"version":999,"settings":{}}"#).unwrap();
        let _guard = settings_lock();
        let prior = settings_cache().clone();
        let cached = AppSettings {
            editor_font_size: 20,
            ..AppSettings::default()
        };
        *settings_cache() = Some(cached.clone());

        assert_eq!(
            save_settings_at(&path, &AppSettings::default())
                .unwrap_err()
                .code,
            "SETTINGS_VERSION_UNSUPPORTED"
        );
        assert_eq!(settings_cache().as_ref(), Some(&cached));
        *settings_cache() = prior;
    }

    #[test]
    fn rejects_oversized_settings_before_deserializing() {
        let directory = TestDirectory::new("settings-oversized");
        let path = directory.path().join("settings.json");
        let mut raw = vec![b' '; MAX_SETTINGS_FILE_BYTES as usize + 1];
        raw.extend(serde_json::to_vec(&AppSettings::default()).unwrap());
        fs::write(&path, &raw).unwrap();

        let result = load_settings_from(&path);

        assert_eq!(result.unwrap_err().code, "SETTINGS_READ_FAILED");
        assert_eq!(fs::read(path).unwrap(), raw);
    }
}
