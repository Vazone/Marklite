use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    models::{app_error::AppError, recent::RecentFileDto},
    services::file_service::{ensure_allowed_file, MAX_FILE_SIZE},
    utils::{
        atomic_write::atomic_write,
        bounded_read::OpenedFile,
        path_utils::{canonicalize_path, path_to_utf8, recent_files_path, title_from_path},
    },
};

static RECENT_FILES_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const RECENT_FILES_VERSION: u32 = 1;
const MAX_RECENT_FILES_BYTES: u64 = 2 * 1024 * 1024;
const MAX_RECENT_FILES_ENTRIES: usize = 100;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecentFilesDocument {
    version: u32,
    files: Vec<RecentFileDto>,
}

enum RecentFilesParseError {
    UnsupportedVersion(u64),
    Invalid(String),
}

pub fn get_recent_files() -> Result<Vec<RecentFileDto>, AppError> {
    let _guard = recent_files_lock();
    get_recent_files_from(&recent_files_path()?)
}

pub fn add_recent_file(path: &str, limit: usize) -> Result<Vec<RecentFileDto>, AppError> {
    add_recent_file_to(&recent_files_path()?, path, limit)
}

pub fn remove_recent_file(path: &str) -> Result<Vec<RecentFileDto>, AppError> {
    let _guard = recent_files_lock();
    let storage_path = recent_files_path()?;
    let mut files = get_recent_files_from(&storage_path)?;
    files.retain(|file| file.path != path);
    save_recent_files_to(&storage_path, &files)?;
    Ok(files)
}

pub fn clear_missing_recent_files() -> Result<Vec<RecentFileDto>, AppError> {
    let _guard = recent_files_lock();
    clear_unavailable_recent_files_from(&recent_files_path()?)
}

fn clear_unavailable_recent_files_from(path: &Path) -> Result<Vec<RecentFileDto>, AppError> {
    let files = get_recent_files_from(path)?;
    let mut retained = Vec::with_capacity(files.len());
    for file in files {
        if is_allowed_readable_document(Path::new(&file.path))? {
            retained.push(file);
        }
    }
    save_recent_files_to(path, &retained)?;
    Ok(retained)
}

fn is_allowed_readable_document(path: &Path) -> Result<bool, AppError> {
    if ensure_allowed_file(path).is_err() {
        return Ok(false);
    }
    let canonical = match canonicalize_path(path) {
        Ok(path) => path,
        Err(_) => return Ok(false),
    };
    path_to_utf8(&canonical)?;
    if ensure_allowed_file(&canonical).is_err() {
        return Ok(false);
    }
    let opened = match OpenedFile::open(&canonical) {
        Ok(opened) => opened,
        Err(_) => return Ok(false),
    };
    Ok(opened.metadata().is_file() && opened.metadata().len() <= MAX_FILE_SIZE)
}

fn get_recent_files_from(path: &Path) -> Result<Vec<RecentFileDto>, AppError> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let raw = read_recent_files_source(path)?;
    let (mut files, legacy) = match parse_recent_files(&raw) {
        Ok(parsed) => parsed,
        Err(RecentFilesParseError::UnsupportedVersion(version)) => {
            return Err(AppError::recent_files_version_unsupported(version));
        }
        Err(RecentFilesParseError::Invalid(message)) => {
            backup_corrupt_file(path)?;
            save_recent_files_to(path, &[])?;
            return Err(AppError::recent_files_read_failed(message));
        }
    };
    files.sort_by(|a, b| b.last_opened_at.cmp(&a.last_opened_at));
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(files.len());
    for mut file in files {
        let stored_path = PathBuf::from(&file.path);
        let identity = match canonicalize_path(&stored_path) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => stored_path,
            Err(error) => return Err(AppError::recent_files_read_failed(error)),
        };
        file.path = path_to_utf8(&identity)?.to_string();
        file.title = title_from_path(&identity);
        if seen.insert(path_identity(&file.path)) {
            normalized.push(file);
        }
    }
    if legacy {
        save_recent_files_to(path, &normalized)?;
    }
    Ok(normalized)
}

fn parse_recent_files(raw: &str) -> Result<(Vec<RecentFileDto>, bool), RecentFilesParseError> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| RecentFilesParseError::Invalid(error.to_string()))?;
    let (files, legacy) = if value.is_array() {
        (
            serde_json::from_value::<Vec<RecentFileDto>>(value)
                .map_err(|error| RecentFilesParseError::Invalid(error.to_string()))?,
            true,
        )
    } else if value.is_object() {
        let version = value
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| RecentFilesParseError::Invalid("version 必须是非负整数".to_string()))?;
        if version != u64::from(RECENT_FILES_VERSION) {
            return Err(RecentFilesParseError::UnsupportedVersion(version));
        }
        let document = serde_json::from_value::<RecentFilesDocument>(value)
            .map_err(|error| RecentFilesParseError::Invalid(error.to_string()))?;
        (document.files, false)
    } else {
        return Err(RecentFilesParseError::Invalid(
            "最近文件文档必须是对象或旧版数组".to_string(),
        ));
    };
    if files.len() > MAX_RECENT_FILES_ENTRIES {
        return Err(RecentFilesParseError::Invalid(
            "最近文件条目数量超过允许上限".to_string(),
        ));
    }
    Ok((files, legacy))
}

fn add_recent_file_to(
    storage_path: &Path,
    document_path: &str,
    limit: usize,
) -> Result<Vec<RecentFileDto>, AppError> {
    let _guard = recent_files_lock();
    let requested = Path::new(document_path);
    ensure_allowed_file(requested)?;
    let path_buf = canonicalize_path(requested).map_err(AppError::recent_files_write_failed)?;
    ensure_allowed_file(&path_buf)?;
    let opened = OpenedFile::open(&path_buf).map_err(AppError::recent_files_write_failed)?;
    if !opened.metadata().is_file() || opened.metadata().len() > MAX_FILE_SIZE {
        return Err(AppError::recent_files_write_failed(
            "最近文件目标不是可打开的普通文档",
        ));
    }
    let normalized = path_to_utf8(&path_buf)?.to_string();
    let mut files = get_recent_files_from(storage_path)?;
    files.retain(|file| path_identity(&file.path) != path_identity(&normalized));
    files.insert(
        0,
        RecentFileDto {
            title: title_from_path(&path_buf),
            path: normalized,
            last_opened_at: Utc::now().to_rfc3339(),
        },
    );
    files.truncate(limit.min(MAX_RECENT_FILES_ENTRIES));
    save_recent_files_to(storage_path, &files)?;
    Ok(files)
}

fn save_recent_files_to(path: &Path, files: &[RecentFileDto]) -> Result<(), AppError> {
    if files.len() > MAX_RECENT_FILES_ENTRIES {
        return Err(AppError::recent_files_write_failed(
            "最近文件条目数量超过允许上限",
        ));
    }
    let document = RecentFilesDocument {
        version: RECENT_FILES_VERSION,
        files: files.to_vec(),
    };
    let content =
        serde_json::to_string_pretty(&document).map_err(AppError::recent_files_write_failed)?;
    if content.len() as u64 > MAX_RECENT_FILES_BYTES {
        return Err(AppError::recent_files_write_failed(std::io::Error::new(
            std::io::ErrorKind::FileTooLarge,
            "最近文件存储超过允许的字节上限",
        )));
    }
    atomic_write(path, content.as_bytes()).map_err(AppError::recent_files_write_failed)
}

fn read_recent_files_source(path: &Path) -> Result<String, AppError> {
    let opened = OpenedFile::open(path).map_err(AppError::recent_files_read_failed)?;
    if !opened.metadata().is_file() {
        return Err(AppError::recent_files_read_failed(
            "最近文件存储路径不是普通文件",
        ));
    }
    if opened.metadata().len() > MAX_RECENT_FILES_BYTES {
        return Err(AppError::recent_files_read_failed(
            "最近文件存储超过允许的字节上限",
        ));
    }
    opened
        .read_to_string_bounded(MAX_RECENT_FILES_BYTES)
        .map_err(AppError::recent_files_read_failed)
}

fn backup_corrupt_file(path: &Path) -> Result<(), AppError> {
    let timestamp = Utc::now().format("%Y%m%d%H%M%S%3f");
    let backup_path = path.with_extension(format!("json.corrupt-{timestamp}"));
    fs::rename(path, backup_path).map_err(AppError::recent_files_write_failed)
}

fn recent_files_lock() -> std::sync::MutexGuard<'static, ()> {
    RECENT_FILES_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn path_identity(path: &str) -> String {
    #[cfg(windows)]
    {
        path.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, sync::Arc, thread};

    use serde_json::json;

    use super::{
        add_recent_file_to, clear_unavailable_recent_files_from, get_recent_files_from,
        save_recent_files_to, MAX_RECENT_FILES_BYTES, MAX_RECENT_FILES_ENTRIES,
        RECENT_FILES_VERSION,
    };
    use crate::{
        models::recent::RecentFileDto, services::file_service::MAX_FILE_SIZE,
        utils::test_support::TestDirectory,
    };

    fn path_string(path: &Path) -> &str {
        path.to_str().expect("tempfile paths are valid UTF-8")
    }

    fn create_documents(directory: &Path, names: &[&str]) {
        fs::create_dir_all(directory).unwrap();
        for name in names {
            fs::write(directory.join(name), name).unwrap();
        }
    }

    fn recent(path: &Path) -> RecentFileDto {
        RecentFileDto {
            title: path.file_name().unwrap().to_str().unwrap().to_string(),
            path: path_string(path).to_string(),
            last_opened_at: "2026-08-13T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn shared_fixture_matches_the_versioned_persistence_contract() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../../../src/shared/desktop-contract-fixtures.json"
        ))
        .unwrap();
        let persistence = fixtures["recentPersistence"].clone();
        let document: super::RecentFilesDocument =
            serde_json::from_value(persistence.clone()).unwrap();

        assert_eq!(serde_json::to_value(document).unwrap(), persistence);
    }

    #[test]
    fn applies_the_exact_limit_and_round_trips() {
        let directory = TestDirectory::new("recent-limit");
        let storage = directory.path().join("recent.json");
        let documents = directory.path().join("documents");
        create_documents(&documents, &["a.md", "b.md", "c.md"]);

        add_recent_file_to(&storage, path_string(&documents.join("a.md")), 3).unwrap();
        add_recent_file_to(&storage, path_string(&documents.join("b.md")), 3).unwrap();
        let files = add_recent_file_to(&storage, path_string(&documents.join("c.md")), 2).unwrap();

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].title, "c.md");
        assert_eq!(get_recent_files_from(&storage).unwrap().len(), 2);
        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(storage).unwrap()).unwrap();
        assert_eq!(persisted["version"], RECENT_FILES_VERSION);
        assert_eq!(persisted["files"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn migrates_a_legacy_array_to_the_versioned_document() {
        let directory = TestDirectory::new("recent-legacy");
        let storage = directory.path().join("recent.json");
        let document = directory.path().join("legacy.md");
        fs::write(&document, "legacy").unwrap();
        fs::write(&storage, serde_json::to_vec(&[recent(&document)]).unwrap()).unwrap();

        let files = get_recent_files_from(&storage).unwrap();
        let persisted: serde_json::Value =
            serde_json::from_slice(&fs::read(&storage).unwrap()).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(persisted["version"], RECENT_FILES_VERSION);
        assert!(persisted.get("files").is_some());
    }

    #[test]
    fn preserves_an_unsupported_future_version() {
        let directory = TestDirectory::new("recent-future");
        let storage = directory.path().join("recent.json");
        let raw = br#"{"version":999,"files":[]}"#;
        fs::write(&storage, raw).unwrap();

        let error = get_recent_files_from(&storage).unwrap_err();

        assert_eq!(error.code, "RECENT_FILES_VERSION_UNSUPPORTED");
        assert_eq!(fs::read(storage).unwrap(), raw);
    }

    #[test]
    fn backs_up_corrupt_json_and_reports_the_recovery() {
        let directory = TestDirectory::new("recent-corrupt");
        let storage = directory.path().join("recent.json");
        fs::write(&storage, "{not-json").unwrap();

        let error = get_recent_files_from(&storage).unwrap_err();

        assert_eq!(error.code, "RECENT_FILES_READ_FAILED");
        assert!(get_recent_files_from(&storage).unwrap().is_empty());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn clear_keeps_only_allowed_readable_regular_documents() {
        let directory = TestDirectory::new("recent-clear");
        let storage = directory.path().join("recent.json");
        let valid = directory.path().join("valid.md");
        let unsupported = directory.path().join("image.png");
        let oversized = directory.path().join("oversized.md");
        let not_a_file = directory.path().join("directory.md");
        let missing = directory.path().join("missing.md");
        fs::write(&valid, "valid").unwrap();
        fs::write(&unsupported, "image").unwrap();
        fs::File::create(&oversized)
            .unwrap()
            .set_len(MAX_FILE_SIZE + 1)
            .unwrap();
        fs::create_dir(&not_a_file).unwrap();
        save_recent_files_to(
            &storage,
            &[
                recent(&valid),
                recent(&unsupported),
                recent(&oversized),
                recent(&not_a_file),
                recent(&missing),
            ],
        )
        .unwrap();

        let retained = clear_unavailable_recent_files_from(&storage).unwrap();

        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].path, path_string(&valid));
    }

    #[test]
    fn serializes_concurrent_read_modify_write_updates() {
        let directory = TestDirectory::new("recent-concurrent");
        let storage = Arc::new(directory.path().join("recent.json"));
        let documents = directory.path().join("documents");
        let names = (0..8)
            .map(|index| format!("{index}.md"))
            .collect::<Vec<_>>();
        let name_refs = names.iter().map(String::as_str).collect::<Vec<_>>();
        create_documents(&documents, &name_refs);
        let handles = (0..8)
            .map(|index| {
                let storage = Arc::clone(&storage);
                let document = documents.join(format!("{index}.md"));
                thread::spawn(move || {
                    add_recent_file_to(&storage, path_string(&document), 20).unwrap();
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(get_recent_files_from(&storage).unwrap().len(), 8);
    }

    #[test]
    fn rejects_oversized_recent_files_before_deserializing() {
        let directory = TestDirectory::new("recent-oversized");
        let storage = directory.path().join("recent.json");
        let raw = format!("[{}]", " ".repeat(MAX_RECENT_FILES_BYTES as usize + 1));
        fs::write(&storage, &raw).unwrap();

        let error = get_recent_files_from(&storage).unwrap_err();

        assert_eq!(error.code, "RECENT_FILES_READ_FAILED");
        assert_eq!(fs::read_to_string(storage).unwrap(), raw);
    }

    #[test]
    fn refuses_to_add_a_path_without_a_canonical_document_identity() {
        let directory = TestDirectory::new("recent-missing");
        let storage = directory.path().join("recent.json");
        let missing = directory.path().join("missing.md");

        let error = add_recent_file_to(&storage, path_string(&missing), 20).unwrap_err();

        assert_eq!(error.code, "RECENT_FILES_WRITE_FAILED");
    }

    #[test]
    fn rejects_more_than_the_supported_number_of_persisted_entries() {
        let directory = TestDirectory::new("recent-count");
        let storage = directory.path().join("recent.json");
        let entries = (0..=MAX_RECENT_FILES_ENTRIES)
            .map(|index| RecentFileDto {
                title: format!("{index}.md"),
                path: format!("C:\\docs\\{index}.md"),
                last_opened_at: "2026-08-13T00:00:00Z".to_string(),
            })
            .collect::<Vec<_>>();
        fs::write(&storage, serde_json::to_vec(&entries).unwrap()).unwrap();

        let error = get_recent_files_from(&storage).unwrap_err();

        assert_eq!(error.code, "RECENT_FILES_READ_FAILED");
    }

    #[test]
    fn refuses_to_write_a_recent_file_document_over_the_byte_limit() {
        let directory = TestDirectory::new("recent-write-limit");
        let storage = directory.path().join("recent.json");
        let files = vec![RecentFileDto {
            title: "large.md".to_string(),
            path: "x".repeat(MAX_RECENT_FILES_BYTES as usize),
            last_opened_at: "2026-08-13T00:00:00Z".to_string(),
        }];

        let error = save_recent_files_to(&storage, &files).unwrap_err();

        assert_eq!(error.code, "RECENT_FILES_WRITE_FAILED");
        assert!(!storage.exists());
    }

    #[test]
    fn current_version_rejects_unknown_document_fields() {
        let directory = TestDirectory::new("recent-contract");
        let storage = directory.path().join("recent.json");
        fs::write(
            &storage,
            serde_json::to_vec(&json!({"version": 1, "files": [], "filez": []})).unwrap(),
        )
        .unwrap();

        let error = get_recent_files_from(&storage).unwrap_err();

        assert_eq!(error.code, "RECENT_FILES_READ_FAILED");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_utf8_alias_that_canonicalizes_to_a_non_utf8_path() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt, os::unix::fs::symlink};

        let directory = TestDirectory::new("recent-non-utf8");
        let storage = directory.path().join("recent.json");
        let target = directory
            .path()
            .join(OsString::from_vec(vec![b'n', 0x80, b'.', b'm', b'd']));
        let alias = directory.path().join("alias.md");
        fs::write(&target, "content").unwrap();
        symlink(&target, &alias).unwrap();

        let error = add_recent_file_to(&storage, path_string(&alias), 20).unwrap_err();

        assert_eq!(error.code, "UNSUPPORTED_PATH_ENCODING");
        assert!(!storage.exists());
    }
}
