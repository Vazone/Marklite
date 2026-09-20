use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    models::{app_error::AppError, export::ExportRequest},
    utils::path_utils::path_to_utf8,
};

pub(crate) const IMAGE_WIDTH: u32 = 1200;
pub(crate) const MAX_IMAGE_HEIGHT: u32 = 24_000;
pub(crate) const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 1024 * 1024 * 1024;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn error(code: &str, message: impl Into<String>) -> AppError {
    AppError::new(code, message)
}

pub(crate) fn directory_name(title: &str) -> String {
    let title = title.trim();
    let stem = title
        .rsplit_once('.')
        .filter(|(stem, extension)| {
            !stem.is_empty()
                && ["md", "markdown", "txt"].contains(&extension.to_ascii_lowercase().as_str())
        })
        .map_or(title, |(stem, _)| stem);
    let name = super::png_chapters::safe_name(stem);
    let base = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&base.as_str())
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && matches!(base.as_bytes()[3], b'1'..=b'9'))
    {
        format!("_{name}")
    } else {
        name
    }
}

pub(crate) fn resolve_target(
    source: Option<&str>,
    title: &str,
    parent: Option<&str>,
) -> Result<PathBuf, AppError> {
    let (parent, name) =
        if let Some(source) = source {
            if parent.is_some() {
                return Err(error(
                    "INVALID_EXPORT_TARGET",
                    "已保存文档的图片目录必须位于源目录",
                ));
            }
            let source = Path::new(source);
            if !source.is_absolute() {
                return Err(error("INVALID_EXPORT_TARGET", "源文档路径必须是绝对路径"));
            }
            (
                source
                    .parent()
                    .ok_or_else(|| error("INVALID_EXPORT_TARGET", "源文档没有父目录"))?,
                source
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .filter(|stem| !stem.is_empty())
                    .ok_or_else(|| error("INVALID_EXPORT_TARGET", "源文档名称无效"))?
                    .to_owned(),
            )
        } else {
            (
                Path::new(parent.ok_or_else(|| {
                    error("INVALID_EXPORT_TARGET", "未保存文档需要选择图片父目录")
                })?),
                directory_name(title),
            )
        };
    if !parent.is_absolute() || !parent.is_dir() {
        return Err(error(
            "INVALID_EXPORT_TARGET",
            "图片父目录必须是已存在的绝对目录",
        ));
    }
    let target = parent
        .canonicalize()
        .map_err(|e| error("INVALID_EXPORT_TARGET", e.to_string()))?
        .join(name);
    ensure_absent(&target)?;
    Ok(target)
}

pub(crate) fn validate_target(request: &ExportRequest) -> Result<PathBuf, AppError> {
    let target = Path::new(&request.target_path);
    if !target.is_absolute() {
        return Err(error("INVALID_EXPORT_TARGET", "图片目标必须是绝对目录"));
    }
    let parent = target.parent().and_then(Path::to_str);
    let expected = resolve_target(
        request.snapshot.source_path.as_deref(),
        &request.snapshot.title,
        if request.snapshot.source_path.is_none() {
            parent
        } else {
            None
        },
    )?;
    let actual_parent = target
        .parent()
        .ok_or_else(|| error("INVALID_EXPORT_TARGET", "图片目标没有父目录"))?
        .canonicalize()
        .map_err(|e| error("INVALID_EXPORT_TARGET", e.to_string()))?;
    if target.file_name() != expected.file_name()
        || Some(actual_parent.as_path()) != expected.parent()
    {
        return Err(error("INVALID_EXPORT_TARGET", "图片必须存入文档同名目录"));
    }
    Ok(expected)
}

fn ensure_absent(path: &Path) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(error(
            "PNG_TARGET_EXISTS",
            "文档同名目录已存在，请先移动或重命名已有目录",
        )),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(error("INVALID_EXPORT_TARGET", e.to_string())),
    }
}

pub(crate) struct DirectoryArtifact {
    stage: Option<PathBuf>,
    target: PathBuf,
    files: Vec<String>,
    total_bytes: usize,
}

impl DirectoryArtifact {
    pub(crate) fn create(target: PathBuf) -> Result<Self, AppError> {
        ensure_absent(&target)?;
        let parent = target
            .parent()
            .ok_or_else(|| error("INVALID_EXPORT_TARGET", "图片目标没有父目录"))?;
        for _ in 0..100 {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let stage = parent.join(format!(
                ".marklite-png-work-{}-{stamp}-{sequence}",
                std::process::id()
            ));
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            let builder = {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = builder;
                builder.mode(0o700);
                builder
            };
            match builder.create(&stage) {
                Ok(()) => {
                    return Ok(Self {
                        stage: Some(stage),
                        target,
                        files: Vec::new(),
                        total_bytes: 0,
                    })
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(error("PNG_TEMP_FAILED", e.to_string())),
            }
        }
        Err(error("PNG_TEMP_FAILED", "无法创建图片暂存目录"))
    }

    pub(crate) fn stage(&self) -> &Path {
        self.stage.as_deref().expect("stage exists before commit")
    }

    pub(crate) fn write_png(
        &mut self,
        filename: &str,
        bytes: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), AppError> {
        validate_png(bytes, width, height)?;
        if Path::new(filename).components().count() != 1
            || !filename.ends_with(".png")
            || self.files.iter().any(|name| name == filename)
        {
            return Err(error(
                "PNG_INVALID_FILENAME",
                "图片文件名必须是唯一的普通文件名",
            ));
        }
        let total = self
            .total_bytes
            .checked_add(bytes.len())
            .filter(|total| *total <= MAX_TOTAL_BYTES)
            .ok_or_else(|| error("PNG_OUTPUT_TOO_LARGE", "图片总输出超过 1 GiB，未提交目录"))?;
        let path = self.stage().join(filename);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| error("PNG_WRITE_FAILED", e.to_string()))?;
        self.files.push(filename.to_owned());
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| error("PNG_WRITE_FAILED", e.to_string()))?;
        self.total_bytes = total;
        Ok(())
    }

    pub(crate) fn commit(mut self) -> Result<(), AppError> {
        if self.files.is_empty() {
            return Err(error("PNG_EMPTY_OUTPUT", "没有可提交的章节图片"));
        }
        let entries = fs::read_dir(self.stage())
            .and_then(|entries| entries.collect::<io::Result<Vec<_>>>())
            .map_err(|e| error("PNG_COMMIT_FAILED", e.to_string()))?;
        if entries.len() != self.files.len()
            || entries.iter().any(|entry| {
                !entry.file_type().is_ok_and(|kind| kind.is_file())
                    || !self
                        .files
                        .iter()
                        .any(|name| entry.file_name() == name.as_str())
            })
        {
            return Err(error(
                "PNG_UNEXPECTED_OUTPUT",
                "暂存目录含缺失或未知文件，未提交目录",
            ));
        }
        rename_directory_new(self.stage(), &self.target).map_err(|e| {
            if fs::symlink_metadata(&self.target).is_ok() {
                error(
                    "PNG_TARGET_EXISTS",
                    "输出目录已被其他任务创建，已有内容未被覆盖",
                )
            } else {
                error("PNG_COMMIT_FAILED", e.to_string())
            }
        })?;
        self.stage.take();
        Ok(())
    }
}

impl Drop for DirectoryArtifact {
    fn drop(&mut self) {
        if let Some(stage) = self.stage.take() {
            // Never recursively remove unknown entries or follow a replaced symlink.
            if !fs::symlink_metadata(&stage)
                .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
            {
                return;
            }
            for name in &self.files {
                let _ = fs::remove_file(stage.join(name));
            }
            let _ = fs::remove_dir(stage);
        }
    }
}

#[cfg(windows)]
fn rename_directory_new(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{core::PCWSTR, Win32::Storage::FileSystem::MoveFileW};
    let from = from
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let to = to
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // MoveFileW never replaces an existing target; stage and target are siblings.
    unsafe { MoveFileW(PCWSTR(from.as_ptr()), PCWSTR(to.as_ptr())) }
        .map_err(|_| io::Error::last_os_error())
}

#[cfg(unix)]
fn rename_directory_new(from: &Path, to: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let from = CString::new(from.as_os_str().as_bytes())?;
    let to = CString::new(to.as_os_str().as_bytes())?;
    // Both supported Unix platforms provide an atomic no-replace directory move.
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(target_os = "macos")]
    let result = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub(crate) fn validate_dimensions(width: u32, height: u32) -> Result<(), AppError> {
    if width != IMAGE_WIDTH || !(1..=MAX_IMAGE_HEIGHT).contains(&height) {
        return Err(error(
            "PNG_CHAPTER_TOO_LARGE",
            format!(
                "单章图片必须为 {IMAGE_WIDTH} 像素宽，高度不超过 {MAX_IMAGE_HEIGHT}；未裁切或缩小"
            ),
        ));
    }
    Ok(())
}

fn validate_png(bytes: &[u8], width: u32, height: u32) -> Result<(), AppError> {
    validate_dimensions(width, height)?;
    let invalid = || error("PNG_INVALID_OUTPUT", "平台未生成完整且尺寸一致的 PNG 图片");
    if bytes.len() > MAX_IMAGE_BYTES
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || !imagesize::blob_size(bytes)
            .is_ok_and(|s| s.width == width as usize && s.height == height as usize)
    {
        return Err(invalid());
    }
    let mut at = 8usize;
    let mut data = false;
    while at.checked_add(12).is_some_and(|end| end <= bytes.len()) {
        let length = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        let end = at
            .checked_add(length)
            .and_then(|n| n.checked_add(12))
            .filter(|n| *n <= bytes.len())
            .ok_or_else(invalid)?;
        match &bytes[at + 4..at + 8] {
            b"IDAT" => data = true,
            b"IEND" if length == 0 && data && end == bytes.len() => return Ok(()),
            _ => {}
        }
        at = end;
    }
    Err(invalid())
}

pub(crate) fn resolve_target_string(
    source: Option<&str>,
    title: &str,
    parent: Option<&str>,
) -> Result<String, AppError> {
    path_to_utf8(&resolve_target(source, title, parent)?).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_source_directory_and_unsaved_name_are_explicit() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原稿.md");
        let target = resolve_target(source.to_str(), "Different title", None).unwrap();
        assert_eq!(target.file_name().unwrap(), "原稿");
        assert_eq!(target.parent().unwrap(), dir.path().canonicalize().unwrap());
        assert!(resolve_target(source.to_str(), "x", dir.path().to_str()).is_err());
        assert_eq!(directory_name("CON.md"), "_CON");
        assert_eq!(directory_name(" draft.md "), "draft");
        fs::create_dir(&target).unwrap();
        assert_eq!(
            resolve_target(source.to_str(), "x", None).unwrap_err().code,
            "PNG_TARGET_EXISTS"
        );
    }
    #[test]
    fn atomic_directory_move_refuses_an_existing_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("stage");
        let target = dir.path().join("target");
        fs::create_dir(&from).unwrap();
        fs::write(from.join("one.png"), b"owned").unwrap();
        fs::create_dir(&target).unwrap();
        assert!(rename_directory_new(&from, &target).is_err());
        assert!(from.join("one.png").is_file());
        assert!(!target.join("one.png").exists());
        fs::remove_dir(&target).unwrap();
        rename_directory_new(&from, &target).unwrap();
        assert_eq!(fs::read(target.join("one.png")).unwrap(), b"owned");
    }
    #[test]
    fn commit_rejects_unknown_entries_and_preserves_them() {
        let dir = tempfile::tempdir().unwrap();
        let mut artifact = DirectoryArtifact::create(dir.path().join("result")).unwrap();
        let stage = artifact.stage().to_owned();
        fs::write(stage.join("one.png"), b"owned").unwrap();
        artifact.files.push("one.png".into());
        fs::create_dir(stage.join("leftover-workspace")).unwrap();
        assert_eq!(artifact.commit().unwrap_err().code, "PNG_UNEXPECTED_OUTPUT");
        assert!(!dir.path().join("result").exists());
        assert!(stage.join("leftover-workspace").is_dir());
        assert!(!stage.join("one.png").exists());
    }
    #[test]
    fn cleanup_leaves_unknown_files_alone() {
        let dir = tempfile::tempdir().unwrap();
        let artifact = DirectoryArtifact::create(dir.path().join("result")).unwrap();
        let stage = artifact.stage().to_owned();
        fs::write(stage.join("unknown"), b"keep").unwrap();
        drop(artifact);
        assert_eq!(fs::read(stage.join("unknown")).unwrap(), b"keep");
        assert!(!dir.path().join("result").exists());
    }
}
