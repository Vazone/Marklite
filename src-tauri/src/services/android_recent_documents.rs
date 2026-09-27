use std::path::Path;
#[cfg(target_os = "android")]
use std::sync::Mutex;

use chrono::Utc;
use serde::{Deserialize, Serialize};

#[cfg(target_os = "android")]
use crate::utils::path_utils::app_data_dir;
use crate::{
    models::{app_error::AppError, resource::ResourceRef},
    utils::{atomic_write::atomic_write, bounded_read::OpenedFile},
};

const VERSION: u32 = 1;
const MAX_ENTRIES: usize = 50;
const MAX_BYTES: u64 = 512 * 1024;
#[cfg(target_os = "android")]
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AndroidRecentDocument {
    pub resource: ResourceRef,
    pub title: String,
    pub last_opened_at: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct History {
    version: u32,
    files: Vec<AndroidRecentDocument>,
}

#[cfg(target_os = "android")]
fn path() -> Result<std::path::PathBuf, AppError> {
    Ok(app_data_dir()?.join("android-recent-documents.json"))
}

fn document_uri(resource: &ResourceRef) -> Result<&str, AppError> {
    let ResourceRef::AndroidDocument { uri } = resource else {
        return Err(AppError::new(
            "INVALID_RESOURCE_REF",
            "需要 Android 文档 URI",
        ));
    };
    if uri.len() > 4096
        || uri.chars().any(char::is_control)
        || !url::Url::parse(uri)
            .is_ok_and(|url| url.scheme() == "content" && url.host_str().is_some())
    {
        return Err(AppError::new(
            "INVALID_RESOURCE_REF",
            "Android 文档 URI 无效",
        ));
    }
    Ok(uri)
}

fn valid_title(title: &str) -> bool {
    !title.is_empty()
        && title.len() <= 255
        && !title.chars().any(char::is_control)
        && !title.contains('/')
        && !title.contains('\\')
}

fn load_from(storage: &Path) -> Result<Vec<AndroidRecentDocument>, AppError> {
    if !storage.exists() {
        return Ok(Vec::new());
    }
    let raw = OpenedFile::open(storage)
        .and_then(|opened| opened.read_bounded(MAX_BYTES))
        .map_err(AppError::recent_files_read_failed)?;
    let history: History =
        serde_json::from_slice(&raw).map_err(AppError::recent_files_read_failed)?;
    if history.version != VERSION {
        return Err(AppError::recent_files_version_unsupported(
            history.version.into(),
        ));
    }
    if history.files.len() > MAX_ENTRIES
        || history.files.iter().any(|entry| {
            document_uri(&entry.resource).is_err()
                || !valid_title(&entry.title)
                || chrono::DateTime::parse_from_rfc3339(&entry.last_opened_at).is_err()
        })
    {
        return Err(AppError::recent_files_read_failed(
            "Android 最近文档记录无效",
        ));
    }
    Ok(history.files)
}

fn save_to(storage: &Path, files: &[AndroidRecentDocument]) -> Result<(), AppError> {
    let raw = serde_json::to_vec_pretty(&History {
        version: VERSION,
        files: files.to_vec(),
    })
    .map_err(AppError::recent_files_write_failed)?;
    if raw.len() as u64 > MAX_BYTES {
        return Err(AppError::recent_files_write_failed("最近文档记录过大"));
    }
    atomic_write(storage, &raw).map_err(AppError::recent_files_write_failed)
}

#[cfg(target_os = "android")]
pub fn list() -> Result<Vec<AndroidRecentDocument>, AppError> {
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    load_from(&path()?)
}

#[cfg(target_os = "android")]
pub fn remember(uri: &str, title: &str, limit: usize) -> Result<(), AppError> {
    let resource = ResourceRef::AndroidDocument {
        uri: uri.to_owned(),
    };
    document_uri(&resource)?;
    if !valid_title(title) {
        return Err(AppError::recent_files_write_failed("文档名称无效"));
    }
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    remember_to(&path()?, resource, title, limit)
}

fn remember_to(
    storage: &Path,
    resource: ResourceRef,
    title: &str,
    limit: usize,
) -> Result<(), AppError> {
    let mut files = load_from(storage)?;
    files.retain(|entry| entry.resource != resource);
    files.insert(
        0,
        AndroidRecentDocument {
            resource,
            title: title.to_owned(),
            last_opened_at: Utc::now().to_rfc3339(),
        },
    );
    files.truncate(limit.clamp(1, MAX_ENTRIES));
    save_to(storage, &files)
}

#[cfg(target_os = "android")]
pub fn remove(resource: &ResourceRef) -> Result<Vec<AndroidRecentDocument>, AppError> {
    document_uri(resource)?;
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let storage = path()?;
    remove_from(&storage, resource)
}

fn remove_from(
    storage: &Path,
    resource: &ResourceRef,
) -> Result<Vec<AndroidRecentDocument>, AppError> {
    let mut files = load_from(storage)?;
    files.retain(|entry| &entry.resource != resource);
    save_to(storage, &files)?;
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::{load_from, remember_to, remove_from};
    use crate::{models::resource::ResourceRef, utils::test_support::TestDirectory};

    #[test]
    fn remembers_reorders_and_removes_persistent_android_resources() {
        let directory = TestDirectory::new("android-recent");
        let storage = directory.path().join("recent.json");
        let first = ResourceRef::AndroidDocument {
            uri: "content://provider/1".into(),
        };
        let second = ResourceRef::AndroidDocument {
            uri: "content://provider/2".into(),
        };
        remember_to(&storage, first.clone(), "一.md", 12).unwrap();
        remember_to(&storage, second.clone(), "二.md", 12).unwrap();
        remember_to(&storage, first.clone(), "一.md", 12).unwrap();
        let files = load_from(&storage).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].resource, first);
        assert_eq!(files[1].resource, second);
        let remaining = remove_from(&storage, &first).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(load_from(&storage).unwrap()[0].resource, second);
        let raw = std::fs::read_to_string(&storage).unwrap();
        assert!(!raw.contains("content://provider/1"));
    }

    #[test]
    fn invalid_or_future_history_is_preserved() {
        let directory = TestDirectory::new("android-recent-invalid");
        let storage = directory.path().join("recent.json");
        for raw in [
            r#"{"version":9,"files":[]}"#,
            r#"{"version":1,"files":[{"resource":{"kind":"desktopFile","path":"/tmp/a.md"},"title":"a.md","lastOpenedAt":"2026-09-25T00:00:00Z"}]}"#,
        ] {
            std::fs::write(&storage, raw).unwrap();
            assert!(load_from(&storage).is_err());
            assert_eq!(std::fs::read_to_string(&storage).unwrap(), raw);
        }
    }
}
