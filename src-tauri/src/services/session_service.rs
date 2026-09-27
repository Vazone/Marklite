use std::{
    collections::HashSet,
    fs,
    path::Path,
    sync::{Mutex, OnceLock},
};

use chrono::Utc;
use serde_json::Value;

use crate::{
    models::{
        app_error::AppError,
        resource::ResourceRef,
        session::{
            ResourceSession, SessionState, MAX_SESSION_PATHS, RESOURCE_SESSION_VERSION,
            SESSION_VERSION,
        },
    },
    utils::{atomic_write::atomic_write, bounded_read::OpenedFile, path_utils::session_path},
};

const MAX_PATH_LENGTH: usize = 32_768;
const MAX_SESSION_FILE_BYTES: u64 = 2 * 1024 * 1024;
static SESSION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn load_session() -> Result<SessionState, AppError> {
    let _guard = session_lock();
    load_session_from(&session_path()?)
}

pub fn save_session(session: SessionState) -> Result<SessionState, AppError> {
    let _guard = session_lock();
    save_session_to(&session_path()?, session)
}

pub fn load_resource_session() -> Result<ResourceSession, AppError> {
    let _guard = session_lock();
    load_resource_session_from(&session_path()?)
}

pub fn save_resource_session(session: ResourceSession) -> Result<ResourceSession, AppError> {
    let _guard = session_lock();
    save_resource_session_to(&session_path()?, session)
}

pub fn clear_resource_session() -> Result<(), AppError> {
    let _guard = session_lock();
    let path = session_path()?;
    if path.exists() {
        load_resource_session_from(&path)?;
        fs::remove_file(path).map_err(AppError::session_write_failed)?;
    }
    Ok(())
}

pub fn clear_session() -> Result<(), AppError> {
    let _guard = session_lock();
    clear_session_at(&session_path()?)
}

fn clear_session_at(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        ensure_supported_session_version(path)?;
        fs::remove_file(path).map_err(AppError::session_write_failed)?;
    }
    Ok(())
}

fn load_session_from(path: &Path) -> Result<SessionState, AppError> {
    if !path.exists() {
        return Ok(SessionState::default());
    }

    let raw = read_session_source(path)?;
    let document = match serde_json::from_str::<Value>(&raw) {
        Ok(document) => document,
        Err(error) => {
            backup_corrupt_file(path)?;
            return Err(AppError::session_read_failed(error));
        }
    };
    let Some(version) = document.get("version").and_then(Value::as_u64) else {
        backup_corrupt_file(path)?;
        return Err(AppError::session_read_failed("会话 version 必须是非负整数"));
    };
    if version == u64::from(RESOURCE_SESSION_VERSION) {
        let session = serde_json::from_value::<ResourceSession>(document)
            .map_err(AppError::session_read_failed)?;
        return Ok(normalize_session(session.desktop()?));
    }
    if version != u64::from(SESSION_VERSION) {
        return Err(AppError::session_version_unsupported(version));
    }
    let session = match serde_json::from_value::<SessionState>(document) {
        Ok(session) => session,
        Err(error) => {
            backup_corrupt_file(path)?;
            return Err(AppError::session_read_failed(error));
        }
    };

    let session = normalize_session(session);
    backup_v1(path, &raw)?;
    save_session_to(path, session)
}

fn load_resource_session_from(path: &Path) -> Result<ResourceSession, AppError> {
    if !path.exists() {
        return Ok(ResourceSession {
            version: RESOURCE_SESSION_VERSION,
            resources: Vec::new(),
            active_resource: None,
        });
    }
    let raw = read_session_source(path)?;
    let document: Value = serde_json::from_str(&raw).map_err(AppError::session_read_failed)?;
    let version = document
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::session_read_failed("会话 version 必须是非负整数"))?;
    if version == u64::from(SESSION_VERSION) {
        return Ok(ResourceSession::from(&load_session_from(path)?));
    }
    if version != u64::from(RESOURCE_SESSION_VERSION) {
        return Err(AppError::session_version_unsupported(version));
    }
    let session: ResourceSession =
        serde_json::from_value(document).map_err(AppError::session_read_failed)?;
    validate_resource_session(&session)?;
    Ok(normalize_resource_session(session))
}

fn save_resource_session_to(
    path: &Path,
    session: ResourceSession,
) -> Result<ResourceSession, AppError> {
    if session.version != RESOURCE_SESSION_VERSION {
        return Err(AppError::session_version_unsupported(u64::from(
            session.version,
        )));
    }
    if path.exists() {
        load_resource_session_from(path)?;
    }
    validate_resource_session(&session)?;
    let session = normalize_resource_session(session);
    let content = serde_json::to_string_pretty(&session).map_err(AppError::session_write_failed)?;
    if content.len() as u64 > MAX_SESSION_FILE_BYTES {
        return Err(AppError::session_write_failed("会话文件超过允许的字节上限"));
    }
    atomic_write(path, content.as_bytes()).map_err(AppError::session_write_failed)?;
    Ok(session)
}

fn valid_session_resource(resource: &ResourceRef) -> bool {
    match resource {
        ResourceRef::DesktopFile { path } => {
            !path.trim().is_empty() && path.len() <= MAX_PATH_LENGTH
        }
        ResourceRef::AndroidDocument { uri } => {
            uri.len() <= 4096
                && !uri.chars().any(char::is_control)
                && url::Url::parse(uri)
                    .is_ok_and(|url| url.scheme() == "content" && url.host_str().is_some())
        }
        _ => false,
    }
}

fn validate_resource_session(session: &ResourceSession) -> Result<(), AppError> {
    if session.resources.iter().all(valid_session_resource)
        && session
            .active_resource
            .as_ref()
            .is_none_or(valid_session_resource)
    {
        return Ok(());
    }
    Err(AppError::session_read_failed(
        "会话包含无效或不支持的文档引用",
    ))
}

fn normalize_resource_session(session: ResourceSession) -> ResourceSession {
    let mut resources = Vec::new();
    for resource in session.resources {
        if !resources.contains(&resource) {
            resources.push(resource);
            if resources.len() == MAX_SESSION_PATHS {
                break;
            }
        }
    }
    let active_resource = session
        .active_resource
        .filter(|active| resources.contains(active));
    ResourceSession {
        version: RESOURCE_SESSION_VERSION,
        resources,
        active_resource,
    }
}

fn save_session_to(path: &Path, session: SessionState) -> Result<SessionState, AppError> {
    ensure_supported_session_version(path)?;
    let session = normalize_session(session);
    let content = serde_json::to_string_pretty(&ResourceSession::from(&session))
        .map_err(AppError::session_write_failed)?;
    if content.len() as u64 > MAX_SESSION_FILE_BYTES {
        return Err(AppError::session_write_failed(std::io::Error::new(
            std::io::ErrorKind::FileTooLarge,
            "会话文件超过允许的字节上限",
        )));
    }
    atomic_write(path, content.as_bytes()).map_err(AppError::session_write_failed)?;
    Ok(session)
}

fn ensure_supported_session_version(path: &Path) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }
    let raw = read_session_source(path)?;
    let document = serde_json::from_str::<Value>(&raw).map_err(AppError::session_read_failed)?;
    let version = document
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::session_read_failed("会话 version 必须是非负整数"))?;
    if version == u64::from(RESOURCE_SESSION_VERSION) {
        // A legacy desktop writer cannot erase resource kinds it cannot read.
        serde_json::from_value::<ResourceSession>(document)
            .map_err(AppError::session_read_failed)?
            .desktop()?;
        return Ok(());
    }
    if version != u64::from(SESSION_VERSION) {
        return Err(AppError::session_version_unsupported(version));
    }
    backup_v1(path, &raw)
}

fn backup_v1(path: &Path, raw: &str) -> Result<(), AppError> {
    use crate::utils::atomic_write::atomic_write_create_new;
    let backup = path.with_extension("json.v1.bak");
    match atomic_write_create_new(&backup, raw.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if read_session_source(&backup)? == raw {
                Ok(())
            } else {
                Err(AppError::session_write_failed(
                    "已有不同的 v1 备份，已停止迁移",
                ))
            }
        }
        Err(error) => Err(AppError::session_write_failed(error)),
    }
}

fn read_session_source(path: &Path) -> Result<String, AppError> {
    let opened = OpenedFile::open(path).map_err(AppError::session_read_failed)?;
    if !opened.metadata().is_file() {
        return Err(AppError::session_read_failed("会话路径不是普通文件"));
    }
    if opened.metadata().len() > MAX_SESSION_FILE_BYTES {
        return Err(AppError::session_read_failed("会话文件超过允许的字节上限"));
    }
    opened
        .read_to_string_bounded(MAX_SESSION_FILE_BYTES)
        .map_err(AppError::session_read_failed)
}

fn normalize_session(session: SessionState) -> SessionState {
    let mut seen = HashSet::new();
    let paths = session
        .paths
        .into_iter()
        .filter(|path| !path.trim().is_empty() && path.len() <= MAX_PATH_LENGTH)
        .filter(|path| seen.insert(path.clone()))
        .take(MAX_SESSION_PATHS)
        .collect::<Vec<_>>();
    let active_path = session.active_path.filter(|active| paths.contains(active));

    SessionState {
        version: SESSION_VERSION,
        paths,
        active_path,
    }
}

fn backup_corrupt_file(path: &Path) -> Result<(), AppError> {
    let timestamp = Utc::now().format("%Y%m%d%H%M%S%3f");
    let backup_path = path.with_extension(format!("json.corrupt-{timestamp}"));
    fs::rename(path, backup_path).map_err(AppError::session_write_failed)
}

fn session_lock() -> std::sync::MutexGuard<'static, ()> {
    SESSION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        clear_session_at, load_resource_session_from, load_session_from, normalize_session,
        save_resource_session_to, save_session_to, MAX_SESSION_FILE_BYTES,
    };
    use crate::{
        models::{
            resource::ResourceRef,
            session::{
                ResourceSession, SessionState, MAX_SESSION_PATHS, RESOURCE_SESSION_VERSION,
                SESSION_VERSION,
            },
        },
        utils::test_support::TestDirectory,
    };

    #[test]
    fn migrates_v1_with_backup_and_explicit_resource_tags() {
        let directory = TestDirectory::new("session-v1-migration");
        let path = directory.path().join("session.json");
        let raw = r#"{"version":1,"paths":["C:\\notes\\100% #计划.md"],"activePath":"C:\\notes\\100% #计划.md"}"#;
        fs::write(&path, raw).unwrap();
        let session = load_session_from(&path).unwrap();
        assert_eq!(session.paths, vec![r"C:\notes\100% #计划.md"]);
        assert_eq!(
            fs::read_to_string(path.with_extension("json.v1.bak")).unwrap(),
            raw
        );
        let persisted: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(persisted["version"], 2);
        assert_eq!(persisted["resources"][0]["kind"], "desktopFile");
        assert_eq!(persisted["resources"][0]["path"], session.paths[0]);
        assert_eq!(load_session_from(&path).unwrap(), session);
    }

    #[test]
    fn desktop_projection_never_erases_uri_resources() {
        let directory = TestDirectory::new("session-uri-preservation");
        let path = directory.path().join("session.json");
        let raw = r#"{"version":2,"resources":[{"kind":"androidDocument","uri":"content://provider/document/a"}],"activeResource":null}"#;
        fs::write(&path, raw).unwrap();
        assert_eq!(
            load_session_from(&path).unwrap_err().code,
            "RESOURCE_UNSUPPORTED"
        );
        assert_eq!(
            save_session_to(&path, SessionState::default())
                .unwrap_err()
                .code,
            "RESOURCE_UNSUPPORTED"
        );
        assert_eq!(
            clear_session_at(&path).unwrap_err().code,
            "RESOURCE_UNSUPPORTED"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), raw);
    }

    #[test]
    fn migration_does_not_overwrite_an_existing_different_backup() {
        let directory = TestDirectory::new("session-backup-conflict");
        let path = directory.path().join("session.json");
        let raw = r#"{"version":1,"paths":[],"activePath":null}"#;
        fs::write(&path, raw).unwrap();
        let backup = path.with_extension("json.v1.bak");
        fs::write(&backup, "previous original").unwrap();
        assert!(load_session_from(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), raw);
        assert_eq!(fs::read_to_string(&backup).unwrap(), "previous original");
    }

    #[test]
    fn normalizes_duplicates_limit_and_active_path() {
        let mut paths = vec!["C:\\a.md".to_string(), "C:\\a.md".to_string()];
        paths.extend((0..MAX_SESSION_PATHS + 5).map(|index| format!("C:\\{index}.md")));

        let normalized = normalize_session(SessionState {
            version: SESSION_VERSION,
            paths,
            active_path: Some("C:\\missing.md".to_string()),
        });

        assert_eq!(normalized.paths.len(), MAX_SESSION_PATHS);
        assert_eq!(normalized.paths[0], "C:\\a.md");
        assert_eq!(normalized.active_path, None);
    }

    #[test]
    fn round_trips_a_valid_session() {
        let directory = TestDirectory::new("session-round-trip");
        let path = directory.path().join("session.json");
        let session = SessionState {
            version: SESSION_VERSION,
            paths: vec!["C:\\docs\\a.md".to_string()],
            active_path: Some("C:\\docs\\a.md".to_string()),
        };

        let saved = save_session_to(&path, session.clone()).unwrap();
        let loaded = load_session_from(&path).unwrap();

        assert_eq!(saved, session);
        assert_eq!(loaded, session);
    }

    #[test]
    fn resource_session_round_trips_android_uri_without_desktop_projection() {
        let directory = TestDirectory::new("session-android-uri");
        let path = directory.path().join("session.json");
        let resource = ResourceRef::AndroidDocument {
            uri: "content://provider/document/%E4%B8%AD%E6%96%87.md".to_owned(),
        };
        let session = ResourceSession {
            version: RESOURCE_SESSION_VERSION,
            resources: vec![resource.clone()],
            active_resource: Some(resource),
        };
        assert_eq!(
            save_resource_session_to(&path, session.clone()).unwrap(),
            session
        );
        assert_eq!(load_resource_session_from(&path).unwrap(), session);
        assert_eq!(
            load_session_from(&path).unwrap_err().code,
            "RESOURCE_UNSUPPORTED"
        );
        assert_eq!(load_resource_session_from(&path).unwrap(), session);
    }

    #[test]
    fn resource_session_migrates_v1_with_existing_backup_rule() {
        let directory = TestDirectory::new("session-resource-migration");
        let path = directory.path().join("session.json");
        let raw = r#"{"version":1,"paths":["C:\\notes\\a.md"],"activePath":"C:\\notes\\a.md"}"#;
        fs::write(&path, raw).unwrap();
        let loaded = load_resource_session_from(&path).unwrap();
        assert_eq!(
            loaded.resources,
            vec![ResourceRef::DesktopFile {
                path: r"C:\notes\a.md".to_owned()
            }]
        );
        assert_eq!(
            fs::read_to_string(path.with_extension("json.v1.bak")).unwrap(),
            raw
        );
    }

    #[test]
    fn invalid_resource_session_is_preserved_instead_of_silently_dropping_uris() {
        let directory = TestDirectory::new("session-invalid-resource");
        let path = directory.path().join("session.json");
        let raw = r#"{"version":2,"resources":[{"kind":"androidTree","uri":"content://provider/tree/root"}],"activeResource":null}"#;
        fs::write(&path, raw).unwrap();
        assert_eq!(
            load_resource_session_from(&path).unwrap_err().code,
            "SESSION_READ_FAILED"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), raw);
    }

    #[test]
    fn corrupt_session_is_backed_up_and_rejected() {
        let directory = TestDirectory::new("session-corrupt");
        let path = directory.path().join("session.json");
        fs::write(&path, "{not-json").unwrap();

        let error = load_session_from(&path).unwrap_err();

        assert_eq!(error.code, "SESSION_READ_FAILED");
        assert!(!path.exists());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn clear_removes_session_and_load_falls_back_to_default() {
        let directory = TestDirectory::new("session-clear");
        let path = directory.path().join("session.json");
        save_session_to(
            &path,
            SessionState {
                version: SESSION_VERSION,
                paths: vec!["C:\\docs\\a.md".to_string()],
                active_path: Some("C:\\docs\\a.md".to_string()),
            },
        )
        .unwrap();

        clear_session_at(&path).unwrap();

        assert!(!path.exists());
        assert_eq!(load_session_from(&path).unwrap(), SessionState::default());
    }

    #[test]
    fn preserves_future_session_versions_without_quarantining_them() {
        let directory = TestDirectory::new("session-future-version");
        let path = directory.path().join("session.json");
        let raw = r#"{"version":999,"paths":[],"activePath":null}"#;
        fs::write(&path, raw).unwrap();

        let result = load_session_from(&path);

        assert_eq!(result.unwrap_err().code, "SESSION_VERSION_UNSUPPORTED");
        assert_eq!(fs::read_to_string(path).unwrap(), raw);
    }

    #[test]
    fn future_session_cannot_be_saved_over_or_cleared() {
        let directory = TestDirectory::new("session-future-write-protection");
        let path = directory.path().join("session.json");
        let raw = r#"{"version":99,"paths":["future-field"],"activePath":null}"#;
        fs::write(&path, raw).unwrap();
        let replacement = SessionState {
            version: SESSION_VERSION,
            paths: vec!["C:\\docs\\replacement.md".to_string()],
            active_path: Some("C:\\docs\\replacement.md".to_string()),
        };

        assert_eq!(
            save_session_to(&path, replacement).unwrap_err().code,
            "SESSION_VERSION_UNSUPPORTED"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), raw);
        assert_eq!(
            clear_session_at(&path).unwrap_err().code,
            "SESSION_VERSION_UNSUPPORTED"
        );
        assert_eq!(fs::read_to_string(path).unwrap(), raw);
    }

    #[test]
    fn rejects_oversized_sessions_before_deserializing() {
        let directory = TestDirectory::new("session-oversized");
        let path = directory.path().join("session.json");
        let raw = format!(
            "{}{{\"version\":1,\"paths\":[],\"activePath\":null}}",
            " ".repeat(MAX_SESSION_FILE_BYTES as usize + 1)
        );
        fs::write(&path, &raw).unwrap();

        let result = load_session_from(&path);

        assert_eq!(result.unwrap_err().code, "SESSION_READ_FAILED");
        assert_eq!(fs::read_to_string(path).unwrap(), raw);
    }
}
