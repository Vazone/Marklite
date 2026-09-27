//! Shallow directory metadata only. Document reads stay in file_service.
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use crate::{
    models::{
        app_error::AppError,
        resource::ResourceRef,
        workspace::{EntryKind, WorkspaceEntry},
    },
    utils::path_utils::{canonicalize_path, path_to_utf8},
};

pub const MAX_ENTRIES: usize = 200_000;
pub const SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;

pub struct Snapshot {
    pub entries: Vec<WorkspaceEntry>,
    pub bytes: usize,
}

pub fn root_path(resource: &ResourceRef) -> Result<PathBuf, AppError> {
    let ResourceRef::DesktopDirectory { path } = resource else {
        return Err(AppError::new(
            "RESOURCE_UNSUPPORTED",
            "工作区需要桌面目录资源",
        ));
    };
    super::resources::absolute_path(path)?;
    let root = canonicalize_path(Path::new(path)).map_err(|e| io_error(Path::new(path), e))?;
    path_to_utf8(&root)?;
    if !root.is_dir() {
        return Err(AppError::new("WORKSPACE_NOT_DIRECTORY", "工作区根不是目录"));
    }
    Ok(root)
}

pub fn identity(path: &Path) -> Result<String, AppError> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x02000000); // FILE_FLAG_BACKUP_SEMANTICS: directory handle.
    }
    let file = options.open(path).map_err(|e| io_error(path, e))?;
    crate::utils::file_identity::file_identity(&file).map_err(|e| io_error(path, e))
}

pub fn directory(root: &Path, relative: &str) -> Result<PathBuf, AppError> {
    if relative.contains('\0')
        || relative.contains('\\')
        || relative.len() > 16_384
        || (!relative.is_empty()
            && relative
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == ".."))
    {
        return Err(AppError::new(
            "WORKSPACE_INVALID_PATH",
            "工作区相对路径无效",
        ));
    }
    let mut current = root.to_path_buf();
    // Even an in-root link is shown as unavailable: no loops or ambiguous aliases.
    for component in Path::new(relative).components() {
        let Component::Normal(name) = component else {
            return Err(AppError::new(
                "WORKSPACE_INVALID_PATH",
                "工作区路径不能越过授权根",
            ));
        };
        current.push(name);
        let metadata = fs::symlink_metadata(&current).map_err(|e| io_error(&current, e))?;
        if is_link(&metadata) {
            return Err(AppError::new(
                "WORKSPACE_LINK_UNSUPPORTED",
                "目录链接不自动展开",
            ));
        }
        if !metadata.is_dir() {
            return Err(AppError::new("WORKSPACE_NOT_DIRECTORY", "目标不是目录"));
        }
    }
    let canonical = canonicalize_path(&current).map_err(|e| io_error(&current, e))?;
    if !canonical.starts_with(root) {
        return Err(AppError::new(
            "WORKSPACE_OUTSIDE_ROOT",
            "目录已移动到授权根以外",
        ));
    }
    Ok(canonical)
}

pub fn scan(
    root: &Path,
    relative: &str,
    check: impl Fn() -> Result<(), AppError>,
) -> Result<Snapshot, AppError> {
    check()?;
    let path = directory(root, relative)?;
    let original_identity = identity(&path)?;
    let reader = fs::read_dir(&path).map_err(|e| io_error(&path, e))?;
    let mut entries = Vec::new();
    let mut strings = 0;
    for item in reader {
        check()?;
        let item = item.map_err(|e| io_error(&path, e))?;
        let Some(entry) = entry(relative, &item) else {
            continue;
        };
        strings += entry_bytes(&entry);
        entries.push(entry);
        let bytes = strings + entries.capacity() * std::mem::size_of::<WorkspaceEntry>();
        if entries.len() > MAX_ENTRIES || bytes > SNAPSHOT_BYTES {
            return Err(AppError::new(
                "WORKSPACE_DIRECTORY_LIMIT",
                "目录元数据超过工作区预算，请选择较小的子目录",
            ));
        }
    }
    check()?;
    entries.sort_unstable_by(|a, b| {
        rank(&a.kind)
            .cmp(&rank(&b.kind))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.relative_path.cmp(&b.relative_path))
    });
    check()?;
    if directory(root, relative)? != path || identity(&path)? != original_identity {
        return Err(AppError::new("WORKSPACE_STALE", "枚举期间目录发生变化"));
    }
    let bytes = strings + entries.capacity() * std::mem::size_of::<WorkspaceEntry>();
    Ok(Snapshot { entries, bytes })
}

fn rank(kind: &EntryKind) -> u8 {
    match kind {
        EntryKind::Directory => 0,
        EntryKind::Markdown => 1,
        EntryKind::Unavailable => 2,
    }
}

fn entry(relative: &str, item: &fs::DirEntry) -> Option<WorkspaceEntry> {
    let native_name = item.file_name();
    let name = native_name.to_string_lossy().into_owned();
    let relative_path = if relative.is_empty() {
        name.clone()
    } else {
        format!("{relative}/{name}")
    };
    let mut result = WorkspaceEntry {
        name,
        relative_path,
        kind: EntryKind::Unavailable,
        resource: None,
        size: None,
        issue: None,
    };
    if native_name.to_str().is_none() {
        result.issue = Some(AppError::unsupported_path_encoding());
        return Some(result);
    }
    let path = item.path();
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) => {
            result.issue = Some(io_error(&path, error));
            return Some(result);
        }
    };
    if is_link(&metadata) {
        result.issue = Some(AppError::new(
            "WORKSPACE_LINK_UNSUPPORTED",
            "链接未自动跟随",
        ));
    } else if metadata.is_dir() {
        result.kind = EntryKind::Directory;
        result.resource = Some(ResourceRef::DesktopDirectory {
            path: path.to_str()?.into(),
        });
    } else if metadata.is_file() && markdown(&path) {
        result.kind = EntryKind::Markdown;
        result.resource = Some(ResourceRef::DesktopFile {
            path: path.to_str()?.into(),
        });
        result.size = Some(metadata.len());
    } else {
        return None;
    }
    Some(result)
}

pub fn markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case("md") || v.eq_ignore_ascii_case("markdown"))
}

pub fn file_resource(root: &Path, relative: &str) -> Result<ResourceRef, AppError> {
    if relative.len() > 16_384
        || Path::new(relative)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(AppError::new(
            "WORKSPACE_INVALID_PATH",
            "工作区文件路径无效",
        ));
    }
    let (parent, name) = relative.rsplit_once('/').unwrap_or(("", relative));
    if name.is_empty()
        || name.contains(['\\', '\0'])
        || !matches!(
            Path::new(name).components().next(),
            Some(Component::Normal(_))
        )
    {
        return Err(AppError::new(
            "WORKSPACE_INVALID_PATH",
            "工作区文件路径无效",
        ));
    }
    let path = directory(root, parent)?.join(name);
    let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&path, e))?;
    if is_link(&metadata) {
        return Err(AppError::new(
            "WORKSPACE_LINK_UNSUPPORTED",
            "文件链接未自动跟随",
        ));
    }
    if !metadata.is_file() || !markdown(&path) {
        return Err(AppError::new(
            "INVALID_FILE_TYPE",
            "工作区只打开 Markdown 文件",
        ));
    }
    Ok(ResourceRef::DesktopFile {
        path: path_to_utf8(&path)?.into(),
    })
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Includes junctions/reparse points, not just Unix-style symbolic links.
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn entry_bytes(entry: &WorkspaceEntry) -> usize {
    entry.name.capacity()
        + entry.relative_path.capacity()
        + match &entry.resource {
            Some(ResourceRef::DesktopFile { path } | ResourceRef::DesktopDirectory { path }) => {
                path.capacity()
            }
            _ => 0,
        }
        + entry
            .issue
            .as_ref()
            .map_or(0, |e| e.code.capacity() + e.message.capacity())
}

pub fn io_error(path: &Path, error: std::io::Error) -> AppError {
    let code = match error.kind() {
        std::io::ErrorKind::PermissionDenied => "WORKSPACE_PERMISSION_DENIED",
        std::io::ErrorKind::NotFound => "WORKSPACE_NOT_FOUND",
        _ => "WORKSPACE_READ_FAILED",
    };
    AppError::new(
        code,
        format!("目录元数据读取失败：{}：{error}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn workspace_enumerates_metadata_without_reading_or_recursing() {
        let fixture = tempfile::tempdir().unwrap();
        fs::create_dir(fixture.path().join("z-folder")).unwrap();
        fs::write(fixture.path().join("z-folder/hidden.md"), "nested").unwrap();
        fs::write(fixture.path().join("B.MARKDOWN"), [0xff, 0xfe]).unwrap();
        fs::write(fixture.path().join("a.md"), "hello").unwrap();
        fs::write(fixture.path().join("ignore.txt"), "text").unwrap();
        let root = canonicalize_path(fixture.path()).unwrap();
        let snapshot = scan(&root, "", || Ok(())).unwrap();
        assert_eq!(
            snapshot
                .entries
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>(),
            ["z-folder", "B.MARKDOWN", "a.md"]
        );
        assert_eq!(snapshot.entries[1].size, Some(2));
        assert!(snapshot.bytes < SNAPSHOT_BYTES);
        assert_eq!(scan(&root, "z-folder", || Ok(())).unwrap().entries.len(), 1);
    }

    #[test]
    fn workspace_cancel_and_invalid_paths_have_explicit_errors() {
        let fixture = tempfile::tempdir().unwrap();
        for i in 0..20 {
            fs::write(fixture.path().join(format!("{i}.md")), "").unwrap();
        }
        let root = canonicalize_path(fixture.path()).unwrap();
        let steps = AtomicUsize::new(0);
        let error = scan(&root, "", || {
            if steps.fetch_add(1, Ordering::Relaxed) >= 5 {
                Err(AppError::new("WORKSPACE_CANCELLED", "cancelled"))
            } else {
                Ok(())
            }
        })
        .err()
        .unwrap();
        assert_eq!(error.code, "WORKSPACE_CANCELLED");
        for relative in ["..", "a/../../b", "/", "C:\\outside", "a\0b"] {
            assert!(directory(&root, relative).is_err(), "{relative}");
        }
        assert_eq!(
            directory(&root, "gone").unwrap_err().code,
            "WORKSPACE_NOT_FOUND"
        );
        assert_eq!(
            io_error(&root, std::io::ErrorKind::PermissionDenied.into()).code,
            "WORKSPACE_PERMISSION_DENIED"
        );
    }

    #[test]
    fn workspace_links_are_visible_but_never_followed() {
        let fixture = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), "outside").unwrap();
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(outside.path(), fixture.path().join("outside"))
                .unwrap();
            std::os::windows::fs::symlink_dir(fixture.path(), fixture.path().join("loop")).unwrap();
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), fixture.path().join("outside")).unwrap();
            std::os::unix::fs::symlink(fixture.path(), fixture.path().join("loop")).unwrap();
        }
        let root = canonicalize_path(fixture.path()).unwrap();
        let snapshot = scan(&root, "", || Ok(())).unwrap();
        assert_eq!(snapshot.entries.len(), 2);
        for entry in snapshot.entries {
            assert_eq!(entry.kind, EntryKind::Unavailable);
            assert!(entry.resource.is_none());
            assert_eq!(
                directory(&root, &entry.relative_path).unwrap_err().code,
                "WORKSPACE_LINK_UNSUPPORTED"
            );
        }
    }
}
