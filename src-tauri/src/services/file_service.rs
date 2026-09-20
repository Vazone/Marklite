use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::{
    models::{
        app_error::AppError,
        document::{DocumentDto, FileVersionDto},
    },
    utils::{
        atomic_write::{atomic_write_checked, CheckedWriteError},
        bounded_read::OpenedFile,
        file_identity::file_identity,
        path_utils::{canonicalize_path, path_to_utf8, title_from_path},
    },
};

pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

#[cfg(test)]
type ReadHook = std::sync::Arc<dyn Fn(&Path) + Send + Sync>;

#[cfg(test)]
static READ_HOOK: std::sync::OnceLock<std::sync::Mutex<Option<ReadHook>>> =
    std::sync::OnceLock::new();

pub fn read_markdown_file(path: &str) -> Result<DocumentDto, AppError> {
    let path_buf = PathBuf::from(path);
    ensure_allowed_file(&path_buf)?;
    if !path_buf.is_absolute() {
        return Err(AppError::invalid_file_path(path));
    }
    if !path_buf.exists() {
        return Err(AppError::file_not_found(path));
    }

    let canonical =
        canonicalize_path(&path_buf).map_err(|err| AppError::file_read_failed(path, err))?;
    ensure_allowed_file(&canonical)?;
    let opened =
        OpenedFile::open(&canonical).map_err(|error| AppError::file_read_failed(path, error))?;
    if !opened.metadata().is_file() {
        return Err(AppError::invalid_file_target(path));
    }
    if opened.metadata().len() > MAX_FILE_SIZE {
        return Err(AppError::file_too_large(path, "打开"));
    }
    let modified_at = opened.metadata().modified().ok();
    let identity =
        file_identity(opened.file()).map_err(|error| AppError::file_read_failed(path, error))?;

    #[cfg(test)]
    run_read_hook(&canonical);

    let bytes = opened.read_bounded(MAX_FILE_SIZE).map_err(|error| {
        if error.kind() == std::io::ErrorKind::FileTooLarge {
            AppError::file_too_large(path, "打开")
        } else {
            AppError::file_read_failed(path, error)
        }
    })?;
    let content_version = content_version(&bytes);
    let content =
        String::from_utf8(bytes).map_err(|error| AppError::file_read_failed(path, error))?;
    let file_size = content.len() as u64;

    Ok(DocumentDto {
        path: Some(path_to_utf8(&canonical)?.to_string()),
        file_identity: Some(identity),
        content_version: Some(content_version),
        title: title_from_path(&canonical),
        content,
        is_dirty: false,
        last_saved_at: modified_at.map(system_time_to_rfc3339),
        file_size: Some(file_size),
    })
}

pub fn save_markdown_file(
    path: &str,
    content: &str,
    expected_file_identity: Option<&str>,
    expected_content_version: Option<&str>,
    overwrite_content_conflict: bool,
) -> Result<DocumentDto, AppError> {
    let path_buf = PathBuf::from(path);
    ensure_allowed_file(&path_buf)?;
    if !path_buf.is_absolute() {
        return Err(AppError::invalid_file_path(path));
    }
    if content.len() as u64 > MAX_FILE_SIZE {
        return Err(AppError::file_too_large(path, "保存"));
    }

    if path_buf.exists() {
        let metadata =
            fs::metadata(&path_buf).map_err(|err| AppError::file_write_failed(path, err))?;
        if !metadata.is_file() {
            return Err(AppError::invalid_file_target(path));
        }
    }

    if let Some(parent) = path_buf
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| AppError::file_write_failed(path, err))?;
    }

    let write_path = if path_buf.exists() {
        canonicalize_path(&path_buf).map_err(|err| AppError::file_write_failed(path, err))?
    } else {
        let parent = path_buf
            .parent()
            .ok_or_else(|| AppError::invalid_file_path(path))?;
        let canonical_parent =
            canonicalize_path(parent).map_err(|err| AppError::file_write_failed(path, err))?;
        canonical_parent.join(
            path_buf
                .file_name()
                .ok_or_else(|| AppError::invalid_file_path(path))?,
        )
    };
    ensure_allowed_file(&write_path)?;
    atomic_write_checked(&write_path, content.as_bytes(), |target| {
        let actual_version = current_file_version(target, path)?;
        if actual_version
            .as_ref()
            .map(|version| version.file_identity.as_str())
            != expected_file_identity
        {
            return Err(AppError::file_target_changed(path));
        }
        if !overwrite_content_conflict
            && actual_version
                .as_ref()
                .map(|version| version.content_version.as_str())
                != expected_content_version
        {
            return Err(AppError::file_content_changed(path));
        }
        Ok(())
    })
    .map_err(|error| match error {
        CheckedWriteError::Check(error) => error,
        CheckedWriteError::Lock(error) | CheckedWriteError::Write(error) => {
            AppError::file_write_failed(path, error)
        }
    })?;
    let canonical =
        canonicalize_path(&write_path).map_err(|err| AppError::file_write_failed(path, err))?;
    let saved_file =
        File::open(&canonical).map_err(|err| AppError::file_write_failed(path, err))?;
    let identity =
        file_identity(&saved_file).map_err(|err| AppError::file_write_failed(path, err))?;

    Ok(DocumentDto {
        path: Some(path_to_utf8(&canonical)?.to_string()),
        file_identity: Some(identity),
        content_version: Some(content_version(content.as_bytes())),
        title: title_from_path(&canonical),
        content: content.to_string(),
        is_dirty: false,
        last_saved_at: Some(Utc::now().to_rfc3339()),
        file_size: Some(content.len() as u64),
    })
}

fn content_version(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn current_file_version(
    target: &Path,
    display_path: &str,
) -> Result<Option<FileVersionDto>, AppError> {
    let opened = match OpenedFile::open(target) {
        Ok(opened) => opened,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppError::file_write_failed(display_path, error)),
    };
    if !opened.metadata().is_file() {
        return Err(AppError::invalid_file_target(display_path));
    }
    if opened.metadata().len() > MAX_FILE_SIZE {
        return Err(AppError::file_too_large(display_path, "检查保存冲突"));
    }
    let identity = file_identity(opened.file())
        .map_err(|error| AppError::file_write_failed(display_path, error))?;
    let bytes = opened.read_bounded(MAX_FILE_SIZE).map_err(|error| {
        if error.kind() == std::io::ErrorKind::FileTooLarge {
            AppError::file_too_large(display_path, "检查保存冲突")
        } else {
            AppError::file_write_failed(display_path, error)
        }
    })?;
    Ok(Some(FileVersionDto {
        file_identity: identity,
        content_version: content_version(&bytes),
    }))
}

pub fn resolve_file_version(
    path: &str,
    allow_missing: bool,
) -> Result<Option<FileVersionDto>, AppError> {
    let path_buf = PathBuf::from(path);
    ensure_allowed_file(&path_buf)?;
    if !path_buf.is_absolute() {
        return Err(AppError::invalid_file_path(path));
    }
    if !path_buf.exists() {
        return if allow_missing {
            Ok(None)
        } else {
            Err(AppError::file_not_found(path))
        };
    }
    let canonical =
        canonicalize_path(&path_buf).map_err(|error| AppError::file_read_failed(path, error))?;
    ensure_allowed_file(&canonical)?;
    current_file_version(&canonical, path)
}

/// Resolves the identity of an existing save target without reading its content.
/// Missing targets have no real filesystem identity yet and return `None` when
/// explicitly allowed; the save response supplies their identity after creation.
pub fn resolve_file_identity(path: &str, allow_missing: bool) -> Result<Option<String>, AppError> {
    let path_buf = PathBuf::from(path);
    ensure_allowed_file(&path_buf)?;
    if !path_buf.is_absolute() {
        return Err(AppError::invalid_file_path(path));
    }
    if !path_buf.exists() {
        return if allow_missing {
            Ok(None)
        } else {
            Err(AppError::file_not_found(path))
        };
    }

    let canonical =
        canonicalize_path(&path_buf).map_err(|error| AppError::file_read_failed(path, error))?;
    ensure_allowed_file(&canonical)?;
    let file = File::open(&canonical).map_err(|error| AppError::file_read_failed(path, error))?;
    if !file
        .metadata()
        .map_err(|error| AppError::file_read_failed(path, error))?
        .is_file()
    {
        return Err(AppError::invalid_file_target(path));
    }
    file_identity(&file)
        .map(Some)
        .map_err(|error| AppError::file_read_failed(path, error))
}

pub fn ensure_allowed_file(path: &Path) -> Result<(), AppError> {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "md" | "markdown" | "txt" => Ok(()),
        _ => Err(AppError::invalid_file_type(path_to_utf8(path)?)),
    }
}

fn system_time_to_rfc3339(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.to_rfc3339()
}

#[cfg(test)]
fn run_read_hook(path: &Path) {
    let hook = READ_HOOK
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(hook) = hook {
        hook(path);
    }
}

#[cfg(test)]
fn set_read_hook(hook: Option<ReadHook>) {
    *READ_HOOK
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = hook;
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_allowed_file, read_markdown_file, resolve_file_identity, resolve_file_version,
        save_markdown_file, set_read_hook, MAX_FILE_SIZE,
    };
    #[cfg(unix)]
    use crate::utils::test_support::TestDirectory;
    use crate::utils::test_support::TestPath;
    use std::{
        fs::{self, OpenOptions},
        path::Path,
        sync::Arc,
        time::Instant,
    };

    fn test_path(name: &str) -> TestPath {
        TestPath::new("file-service", format!("{name}.md"))
    }

    #[test]
    fn allows_markdown_and_text_files() {
        assert!(ensure_allowed_file(Path::new("note.md")).is_ok());
        assert!(ensure_allowed_file(Path::new("note.markdown")).is_ok());
        assert!(ensure_allowed_file(Path::new("note.txt")).is_ok());
    }

    #[test]
    fn rejects_other_extensions() {
        assert!(ensure_allowed_file(Path::new("note.exe")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_canonical_non_utf8_path_instead_of_returning_a_lossy_dto() {
        use std::{
            ffi::OsString,
            os::unix::{ffi::OsStringExt, fs::symlink},
        };

        let directory = TestDirectory::new("file-service-non-utf8");
        let target = directory
            .path()
            .join(OsString::from_vec(b"note-\x80.md".to_vec()));
        let alias = directory.path().join("alias.md");
        fs::write(&target, "content").unwrap();
        symlink(&target, &alias).unwrap();

        let error = read_markdown_file(alias.to_str().unwrap()).unwrap_err();

        assert_eq!(error.code, "UNSUPPORTED_PATH_ENCODING");
    }

    #[test]
    fn saved_documents_can_be_reopened() {
        let path = test_path("round-trip");
        let path_string = path.to_string_lossy();

        save_markdown_file(&path_string, "# Atomic", None, None, false).unwrap();
        let loaded = read_markdown_file(&path_string).unwrap();

        assert_eq!(loaded.content, "# Atomic");
        assert_eq!(
            loaded.file_identity,
            resolve_file_identity(&path_string, false).unwrap()
        );
    }

    #[test]
    fn hard_link_aliases_share_an_identity_and_distinct_files_do_not() {
        let original = test_path("identity-original");
        let alias = test_path("identity-alias");
        let distinct = test_path("identity-distinct");
        fs::write(&original, "original").unwrap();
        fs::hard_link(&original, &alias).unwrap();
        fs::write(&distinct, "distinct").unwrap();

        let original_identity = resolve_file_identity(&original.to_string_lossy(), false).unwrap();
        let alias_identity = resolve_file_identity(&alias.to_string_lossy(), false).unwrap();
        let distinct_identity = resolve_file_identity(&distinct.to_string_lossy(), false).unwrap();

        assert_eq!(original_identity, alias_identity);
        assert_ne!(original_identity, distinct_identity);
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_alias_resolves_to_the_target_identity() {
        use std::os::unix::fs::symlink;

        let original = test_path("identity-symlink-original");
        let alias = test_path("identity-symlink-alias");
        fs::write(&original, "original").unwrap();
        symlink(&original, &alias).unwrap();

        assert_eq!(
            resolve_file_identity(&original.to_string_lossy(), false).unwrap(),
            resolve_file_identity(&alias.to_string_lossy(), false).unwrap()
        );
    }

    #[test]
    fn missing_save_targets_only_resolve_when_explicitly_allowed() {
        let path = test_path("missing-identity");

        assert_eq!(
            resolve_file_identity(&path.to_string_lossy(), true).unwrap(),
            None
        );
        assert_eq!(
            resolve_file_identity(&path.to_string_lossy(), false)
                .unwrap_err()
                .code,
            "FILE_NOT_FOUND"
        );
    }

    #[test]
    fn rejects_oversized_content_before_creating_a_file() {
        let path = test_path("too-large");
        let content = "x".repeat(MAX_FILE_SIZE as usize + 1);

        let error =
            save_markdown_file(&path.to_string_lossy(), &content, None, None, false).unwrap_err();

        assert_eq!(error.code, "FILE_TOO_LARGE");
        assert!(!path.exists());
    }

    #[test]
    fn enforces_the_actual_byte_limit_when_a_file_grows_after_metadata() {
        let path = test_path("grows-after-metadata");
        fs::write(&path, "small").unwrap();
        let hooked_path = path.path().to_path_buf();
        set_read_hook(Some(Arc::new(move |observed| {
            if observed == hooked_path {
                OpenOptions::new()
                    .write(true)
                    .open(observed)
                    .unwrap()
                    .set_len(MAX_FILE_SIZE + 1)
                    .unwrap();
            }
        })));

        let result = read_markdown_file(&path.to_string_lossy());
        set_read_hook(None);

        assert_eq!(
            result.err().map(|error| error.code).as_deref(),
            Some("FILE_TOO_LARGE")
        );
    }

    #[test]
    fn stale_missing_target_observation_cannot_overwrite_a_new_owner() {
        let path = test_path("stale-missing-target");
        let first =
            save_markdown_file(&path.to_string_lossy(), "first", None, None, false).unwrap();

        let error =
            save_markdown_file(&path.to_string_lossy(), "second", None, None, false).unwrap_err();

        assert_eq!(error.code, "FILE_TARGET_CHANGED");
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");
        assert!(first.file_identity.is_some());
    }

    #[test]
    fn stale_existing_identity_cannot_overwrite_a_replaced_target() {
        let path = test_path("stale-existing-target");
        fs::write(&path, "original").unwrap();
        let observed = resolve_file_identity(&path.to_string_lossy(), false)
            .unwrap()
            .unwrap();
        let original = read_markdown_file(&path.to_string_lossy()).unwrap();
        let first = save_markdown_file(
            &path.to_string_lossy(),
            "first",
            Some(observed.as_str()),
            original.content_version.as_deref(),
            false,
        )
        .unwrap();

        let error = save_markdown_file(
            &path.to_string_lossy(),
            "second",
            Some(observed.as_str()),
            original.content_version.as_deref(),
            false,
        )
        .unwrap_err();

        assert_ne!(first.file_identity.as_deref(), Some(observed.as_str()));
        assert_eq!(error.code, "FILE_TARGET_CHANGED");
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");
    }

    #[test]
    fn equal_length_in_place_external_edit_cannot_be_silently_overwritten() {
        let path = test_path("in-place-content-conflict");
        fs::write(&path, "original").unwrap();
        let loaded = read_markdown_file(&path.to_string_lossy()).unwrap();
        fs::write(&path, "external").unwrap();
        let external = resolve_file_version(&path.to_string_lossy(), false)
            .unwrap()
            .unwrap();

        assert_eq!(fs::metadata(&path).unwrap().len(), 8);
        assert_eq!(
            loaded.file_identity.as_deref(),
            Some(external.file_identity.as_str())
        );
        assert_ne!(
            loaded.content_version.as_deref(),
            Some(external.content_version.as_str())
        );

        let error = save_markdown_file(
            &path.to_string_lossy(),
            "my edits",
            loaded.file_identity.as_deref(),
            loaded.content_version.as_deref(),
            false,
        )
        .unwrap_err();

        assert_eq!(error.code, "FILE_CONTENT_CHANGED");
        assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    }

    #[test]
    fn save_as_revalidates_same_object_content_after_preflight() {
        let path = test_path("save-as-content-conflict");
        fs::write(&path, "existing").unwrap();
        let observed = resolve_file_version(&path.to_string_lossy(), false)
            .unwrap()
            .unwrap();
        fs::write(&path, "external").unwrap();

        let error = save_markdown_file(
            &path.to_string_lossy(),
            "save copy",
            Some(&observed.file_identity),
            Some(&observed.content_version),
            false,
        )
        .unwrap_err();

        assert_eq!(error.code, "FILE_CONTENT_CHANGED");
        assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    }

    #[test]
    fn deleted_and_recreated_target_cannot_be_silently_overwritten() {
        let path = test_path("deleted-recreated-target");
        fs::write(&path, "original").unwrap();
        let opened = read_markdown_file(&path.to_string_lossy()).unwrap();
        fs::remove_file(&path).unwrap();
        fs::write(&path, "external").unwrap();

        let error = save_markdown_file(
            &path.to_string_lossy(),
            "my edits",
            opened.file_identity.as_deref(),
            opened.content_version.as_deref(),
            false,
        )
        .unwrap_err();

        assert!(matches!(
            error.code.as_str(),
            "FILE_TARGET_CHANGED" | "FILE_CONTENT_CHANGED"
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    }

    #[test]
    fn explicit_content_overwrite_is_required_and_returns_the_new_version() {
        let path = test_path("explicit-content-overwrite");
        fs::write(&path, "original").unwrap();
        let opened = read_markdown_file(&path.to_string_lossy()).unwrap();
        fs::write(&path, "external").unwrap();
        let latest = resolve_file_version(&path.to_string_lossy(), false)
            .unwrap()
            .unwrap();

        let saved = save_markdown_file(
            &path.to_string_lossy(),
            "my edits",
            Some(&latest.file_identity),
            Some(&latest.content_version),
            true,
        )
        .unwrap();

        assert_ne!(opened.content_version, saved.content_version);
        assert_eq!(fs::read_to_string(&path).unwrap(), "my edits");
    }

    #[test]
    #[ignore = "reference-machine performance probe; run explicitly in release mode"]
    fn measure_ten_mib_content_version_cost() {
        let path = test_path("content-version-10-mib");
        fs::write(&path, vec![b'x'; MAX_FILE_SIZE as usize]).unwrap();
        let mut samples = (0..30)
            .map(|_| {
                let started = Instant::now();
                let version = resolve_file_version(&path.to_string_lossy(), false)
                    .unwrap()
                    .unwrap();
                assert!(version.content_version.starts_with("sha256:"));
                started.elapsed().as_secs_f64() * 1_000.0
            })
            .collect::<Vec<_>>();
        samples.sort_by(f64::total_cmp);
        println!(
            "content-version-10-mib samples=30 p50_ms={:.3} p95_ms={:.3} max_ms={:.3}",
            samples[14], samples[28], samples[29]
        );
    }
}
