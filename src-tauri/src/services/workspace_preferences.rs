use crate::{
    models::{
        app_error::AppError,
        resource::ResourceRef,
        workspace::{RecentWorkspace, WorkspacePreferences},
    },
    utils::{atomic_write::atomic_write, bounded_read::OpenedFile, path_utils::app_data_dir},
};
use serde::Deserialize;
use std::{collections::BTreeSet, path::Path, sync::Mutex};

const MAX_BYTES: u64 = 512 * 1024;
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyPreferences {
    version: u32,
    root: Option<ResourceRef>,
    expanded: Vec<String>,
}

pub fn load() -> Result<WorkspacePreferences, AppError> {
    let _guard = LOCK.lock().unwrap();
    load_from(&app_data_dir()?.join("workspace.json"))
}

pub fn save(preferences: WorkspacePreferences) -> Result<WorkspacePreferences, AppError> {
    let _guard = LOCK.lock().unwrap();
    save_to(&app_data_dir()?.join("workspace.json"), preferences)
}

fn load_from(path: &Path) -> Result<WorkspacePreferences, AppError> {
    let file = match OpenedFile::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(WorkspacePreferences::default())
        }
        Err(error) => return Err(failure("WORKSPACE_STATE_READ_FAILED", error)),
    };
    let bytes = file
        .read_bounded(MAX_BYTES)
        .map_err(|e| failure("WORKSPACE_STATE_READ_FAILED", e))?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| failure("WORKSPACE_STATE_READ_FAILED", e))?;
    let preferences = match value.get("version").and_then(|v| v.as_u64()) {
        Some(1) => {
            let legacy: LegacyPreferences = serde_json::from_value(value)
                .map_err(|e| failure("WORKSPACE_STATE_READ_FAILED", e))?;
            if legacy.version != 1 {
                return Err(invalid());
            }
            let recent = legacy
                .root
                .clone()
                .map(|resource| RecentWorkspace {
                    name: match &resource {
                        ResourceRef::DesktopDirectory { path } => Path::new(path)
                            .file_name()
                            .and_then(|part| part.to_str())
                            .unwrap_or(path)
                            .to_owned(),
                        _ => "Android directory".into(),
                    },
                    resource,
                })
                .into_iter()
                .collect();
            WorkspacePreferences {
                version: 2,
                root: legacy.root,
                expanded: legacy.expanded,
                restore_on_start: true,
                recent,
            }
        }
        Some(2) => {
            serde_json::from_value(value).map_err(|e| failure("WORKSPACE_STATE_READ_FAILED", e))?
        }
        _ => {
            return Err(AppError::new(
                "WORKSPACE_STATE_VERSION_UNSUPPORTED",
                "工作区记录版本不受支持",
            ))
        }
    };
    normalize(preferences)
}

fn save_to(
    path: &Path,
    preferences: WorkspacePreferences,
) -> Result<WorkspacePreferences, AppError> {
    let preferences = normalize(preferences)?;
    // Preserve corrupt/future records for recovery instead of silently replacing them.
    load_from(path)?;
    let bytes =
        serde_json::to_vec(&preferences).map_err(|e| failure("WORKSPACE_STATE_WRITE_FAILED", e))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    atomic_write(path, &bytes).map_err(|e| failure("WORKSPACE_STATE_WRITE_FAILED", e))?;
    Ok(preferences)
}

fn normalize(mut preferences: WorkspacePreferences) -> Result<WorkspacePreferences, AppError> {
    if preferences.version != 2 || preferences.expanded.len() > 256 || preferences.recent.len() > 12
    {
        return Err(invalid());
    }
    if preferences.root.is_none() && !preferences.expanded.is_empty() {
        return Err(invalid());
    }
    if let Some(resource) = &preferences.root {
        validate_resource(resource)?;
    }
    let mut seen = Vec::new();
    for entry in &preferences.recent {
        validate_resource(&entry.resource)?;
        if entry.name.trim().is_empty()
            || entry.name.len() > 512
            || entry.name.chars().any(char::is_control)
            || seen.contains(&entry.resource)
        {
            return Err(invalid());
        }
        seen.push(entry.resource.clone());
    }
    let mut unique = BTreeSet::new();
    for relative in preferences.expanded {
        if relative.len() > 16_384
            || relative.contains(['\\', '\0'])
            || (!relative.is_empty()
                && relative
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == ".."))
            || Path::new(&relative)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(invalid());
        }
        unique.insert(relative);
    }
    preferences.expanded = unique.into_iter().collect();
    Ok(preferences)
}

fn validate_resource(resource: &ResourceRef) -> Result<(), AppError> {
    match resource {
        ResourceRef::DesktopDirectory { path } if path.len() <= 32_768 => {
            crate::platform::desktop::resources::absolute_path(path)?;
            Ok(())
        }
        ResourceRef::AndroidTree { uri }
            if uri.len() <= 4096
                && !uri.chars().any(char::is_control)
                && url::Url::parse(uri)
                    .is_ok_and(|url| url.scheme() == "content" && url.host_str().is_some()) =>
        {
            Ok(())
        }
        _ => Err(invalid()),
    }
}

fn invalid() -> AppError {
    AppError::new("WORKSPACE_STATE_INVALID", "工作区根引用或展开状态无效")
}
fn failure(code: &str, error: impl std::fmt::Display) -> AppError {
    AppError::new(code, format!("工作区记录操作失败：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn workspace_preferences_round_trip_without_requiring_root_to_exist() {
        let fixture = tempfile::tempdir().unwrap();
        let path = fixture.path().join("workspace.json");
        assert_eq!(load_from(&path).unwrap(), WorkspacePreferences::default());
        let prefs = WorkspacePreferences {
            version: 2,
            root: Some(ResourceRef::DesktopDirectory {
                path: fixture.path().join("gone").to_str().unwrap().into(),
            }),
            expanded: vec!["".into(), "chapter".into(), "chapter".into()],
            restore_on_start: true,
            recent: vec![],
        };
        let saved = save_to(&path, prefs).unwrap();
        assert_eq!(saved.expanded, ["", "chapter"]);
        assert_eq!(load_from(&path).unwrap(), saved);
        save_to(&path, WorkspacePreferences::default()).unwrap();
        assert_eq!(load_from(&path).unwrap(), WorkspacePreferences::default());
    }
    #[test]
    fn workspace_preferences_reject_traversal_and_preserve_future_or_corrupt_records() {
        let fixture = tempfile::tempdir().unwrap();
        let path = fixture.path().join("workspace.json");
        for bytes in [br#"{"version":99}"#.as_slice(), b"broken"] {
            fs::write(&path, bytes).unwrap();
            assert!(save_to(&path, WorkspacePreferences::default()).is_err());
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        for relative in ["../private", "/root", "a//b", "a\\b"] {
            let prefs = WorkspacePreferences {
                version: 2,
                root: Some(ResourceRef::DesktopDirectory {
                    path: fixture.path().to_str().unwrap().into(),
                }),
                expanded: vec![relative.into()],
                restore_on_start: true,
                recent: vec![],
            };
            assert!(normalize(prefs).is_err());
        }
    }
    #[test]
    fn migrates_v1_without_overwriting_the_existing_record_until_saved() {
        let fixture = tempfile::tempdir().unwrap();
        let path = fixture.path().join("workspace.json");
        let old = serde_json::json!({"version": 1, "root": {"kind": "desktopDirectory",
            "path": fixture.path().to_str().unwrap()}, "expanded": ["", "chapter"]});
        let old_bytes = serde_json::to_vec(&old).unwrap();
        fs::write(&path, &old_bytes).unwrap();
        let loaded = load_from(&path).unwrap();
        assert_eq!(loaded.version, 2);
        assert!(loaded.restore_on_start);
        assert_eq!(loaded.recent.len(), 1);
        assert_eq!(loaded.expanded, ["", "chapter"]);
        assert_eq!(fs::read(&path).unwrap(), old_bytes);
        save_to(&path, loaded).unwrap();
        assert_eq!(load_from(&path).unwrap().version, 2);
    }
}
