use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use percent_encoding::percent_decode_str;
use url::Url;

use crate::{
    models::{
        app_error::AppError,
        navigation::{LocalImageDto, MarkdownTargetDto},
    },
    services::{file_service, settings_service},
    utils::path_utils::canonicalize_path,
};

const MARKLITE_TARGET_PREFIX: &str = "marklite:";
const MAX_IMAGE_SIZE: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportImage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub path: String,
}

pub fn resolve_markdown_target(
    document_path: Option<&str>,
    target: &str,
) -> Result<MarkdownTargetDto, AppError> {
    let target = unwrap_marklite_target(target)?;
    validate_raw_target(&target)?;

    if let Some(fragment) = target.strip_prefix('#') {
        return Ok(MarkdownTargetDto::Anchor {
            fragment: decode_component(fragment)?,
        });
    }
    if is_windows_absolute(&target) {
        let (path, fragment) = split_local_fragment(&target)?;
        return resolve_local_document(
            document_path,
            PathBuf::from(decode_component(path)?),
            fragment,
        );
    }
    #[cfg(not(windows))]
    if is_posix_absolute(&target) {
        let (path, fragment) = split_local_fragment(&target)?;
        return resolve_local_document(
            document_path,
            PathBuf::from(decode_component(path)?),
            fragment,
        );
    }
    if is_drive_relative(&target) {
        return Err(AppError::invalid_markdown_target(
            "不允许 C:foo 形式的 drive-relative 路径",
        ));
    }

    if let Ok(url) = Url::parse(&target) {
        return match url.scheme() {
            "http" | "https" => Ok(MarkdownTargetDto::External {
                url: url.to_string(),
            }),
            "file" => {
                if url
                    .host_str()
                    .is_some_and(|host| !host.is_empty() && host != "localhost")
                {
                    return Err(AppError::invalid_markdown_target("不允许 UNC file URL"));
                }
                let fragment = url.fragment().map(decode_component).transpose()?;
                let path = url
                    .to_file_path()
                    .map_err(|_| AppError::invalid_markdown_target("file URL 不是本机绝对路径"))?;
                resolve_local_document(document_path, path, fragment)
            }
            scheme => Err(AppError::unsupported_link_scheme(scheme)),
        };
    }

    if target.starts_with("//")
        || target.starts_with(r"\\")
        || target.starts_with('/')
        || target.starts_with('\\')
    {
        return Err(AppError::invalid_markdown_target(
            "不允许 protocol-relative、UNC 或根相对路径",
        ));
    }

    let (path, fragment) = split_local_fragment(&target)?;
    let decoded = decode_component(path)?;
    resolve_local_document(document_path, PathBuf::from(decoded), fragment)
}

pub fn load_local_image(
    document_path: Option<&str>,
    target: &str,
) -> Result<LocalImageDto, AppError> {
    load_local_image_with_permission(
        document_path,
        target,
        settings_service::load_settings()?.allow_local_images,
    )
}

fn load_local_image_with_permission(
    document_path: Option<&str>,
    target: &str,
    allow_local_images: bool,
) -> Result<LocalImageDto, AppError> {
    if !allow_local_images {
        return Err(AppError::local_images_disabled());
    }
    let target = unwrap_marklite_target(target)?;
    validate_raw_target(&target)?;
    let path = resolve_local_path(document_path, &target, false)?;
    let metadata = fs::metadata(&path)
        .map_err(|error| AppError::file_read_failed(&path.to_string_lossy(), error))?;
    if metadata.len() > MAX_IMAGE_SIZE {
        return Err(AppError::file_too_large(
            &path.to_string_lossy(),
            "加载图片",
        ));
    }
    let bytes = fs::read(&path)
        .map_err(|error| AppError::file_read_failed(&path.to_string_lossy(), error))?;
    let mime = image_mime(&path, &bytes)?;
    Ok(LocalImageDto {
        data_url: format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
        path: path.to_string_lossy().to_string(),
    })
}

pub fn load_local_image_for_export(
    document_path: Option<&str>,
    target: &str,
) -> Result<ExportImage, AppError> {
    let target = unwrap_marklite_target(target)?;
    validate_raw_target(&target)?;
    let path = resolve_local_path(document_path, &target, false)?;
    let metadata = fs::metadata(&path)
        .map_err(|error| AppError::file_read_failed(&path.to_string_lossy(), error))?;
    if metadata.len() > MAX_IMAGE_SIZE {
        return Err(AppError::file_too_large(
            &path.to_string_lossy(),
            "导出图片",
        ));
    }
    let bytes = fs::read(&path)
        .map_err(|error| AppError::file_read_failed(&path.to_string_lossy(), error))?;
    let mime = image_mime(&path, &bytes)?;
    Ok(ExportImage {
        bytes,
        mime,
        path: path.to_string_lossy().to_string(),
    })
}

fn resolve_local_document(
    document_path: Option<&str>,
    path: PathBuf,
    fragment: Option<String>,
) -> Result<MarkdownTargetDto, AppError> {
    let path = resolve_local_path_from_path(document_path, path)?;
    file_service::ensure_allowed_file(&path)?;
    Ok(MarkdownTargetDto::LocalDocument {
        path: path.to_string_lossy().to_string(),
        fragment,
    })
}

fn resolve_local_path(
    document_path: Option<&str>,
    target: &str,
    allow_fragment: bool,
) -> Result<PathBuf, AppError> {
    if is_windows_absolute(target) {
        let (path, fragment) = split_local_fragment(target)?;
        if !allow_fragment && fragment.is_some() {
            return Err(AppError::invalid_markdown_target(
                "图片目标不能包含 fragment",
            ));
        }
        return resolve_local_path_from_path(document_path, PathBuf::from(decode_component(path)?));
    }
    #[cfg(not(windows))]
    if is_posix_absolute(target) {
        let (path, fragment) = split_local_fragment(target)?;
        if !allow_fragment && fragment.is_some() {
            return Err(AppError::invalid_markdown_target(
                "图片目标不能包含 fragment",
            ));
        }
        return resolve_local_path_from_path(document_path, PathBuf::from(decode_component(path)?));
    }
    if let Ok(url) = Url::parse(target) {
        if url.scheme() != "file" {
            return Err(AppError::unsupported_link_scheme(url.scheme()));
        }
        if url
            .host_str()
            .is_some_and(|host| !host.is_empty() && host != "localhost")
        {
            return Err(AppError::invalid_markdown_target("不允许 UNC file URL"));
        }
        if !allow_fragment && url.fragment().is_some() {
            return Err(AppError::invalid_markdown_target(
                "图片目标不能包含 fragment",
            ));
        }
        let path = url
            .to_file_path()
            .map_err(|_| AppError::invalid_markdown_target("file URL 不是本机绝对路径"))?;
        return resolve_local_path_from_path(document_path, path);
    }
    if target.starts_with("//")
        || target.starts_with(r"\\")
        || target.starts_with('/')
        || target.starts_with('\\')
        || is_drive_relative(target)
    {
        return Err(AppError::invalid_markdown_target(
            "不允许 UNC、根相对或 drive-relative 路径",
        ));
    }
    if target.contains('#') || target.contains('?') {
        return Err(AppError::invalid_markdown_target(
            "图片目标不能包含 query 或 fragment",
        ));
    }
    resolve_local_path_from_path(document_path, PathBuf::from(decode_component(target)?))
}

fn resolve_local_path_from_path(
    document_path: Option<&str>,
    path: PathBuf,
) -> Result<PathBuf, AppError> {
    validate_local_path_form(&path)?;
    let candidate = if path.is_absolute() {
        path
    } else {
        let document_path =
            document_path.ok_or_else(AppError::relative_target_requires_saved_document)?;
        let document = Path::new(document_path);
        if !document.is_absolute() {
            return Err(AppError::invalid_markdown_target(
                "当前文档路径必须是绝对路径",
            ));
        }
        let canonical_document = canonicalize_path(document)
            .map_err(|error| AppError::file_read_failed(document_path, error))?;
        let parent = canonical_document
            .parent()
            .ok_or_else(|| AppError::invalid_markdown_target("当前文档没有可用父目录"))?;
        parent.join(path)
    };
    let canonical = canonicalize_path(&candidate)
        .map_err(|_| AppError::file_not_found(&candidate.to_string_lossy()))?;
    validate_local_path_form(&canonical)?;
    let metadata = fs::metadata(&canonical)
        .map_err(|error| AppError::file_read_failed(&canonical.to_string_lossy(), error))?;
    if !metadata.is_file() {
        return Err(AppError::invalid_file_target(&canonical.to_string_lossy()));
    }
    Ok(canonical)
}

fn validate_local_path_form(path: &Path) -> Result<(), AppError> {
    let value = path.to_string_lossy();
    let lower = value.to_ascii_lowercase();
    if lower.starts_with(r"\\?\")
        || lower.starts_with(r"\\.\")
        || lower.starts_with(r"\\")
        || lower.starts_with("//")
    {
        return Err(AppError::invalid_markdown_target(
            "不允许 UNC 或 Windows 设备路径",
        ));
    }
    Ok(())
}

fn split_local_fragment(target: &str) -> Result<(&str, Option<String>), AppError> {
    if target.contains('?') {
        return Err(AppError::invalid_markdown_target(
            "本地文件目标不支持 query",
        ));
    }
    match target.split_once('#') {
        Some((path, fragment)) => Ok((path, Some(decode_component(fragment)?))),
        None => Ok((target, None)),
    }
}

fn unwrap_marklite_target(target: &str) -> Result<String, AppError> {
    match target.strip_prefix(MARKLITE_TARGET_PREFIX) {
        Some(encoded) => decode_component(encoded),
        None => Ok(target.to_string()),
    }
}

fn decode_component(value: &str) -> Result<String, AppError> {
    percent_decode_str(value)
        .decode_utf8()
        .map(|value| value.into_owned())
        .map_err(|_| AppError::invalid_markdown_target("URL 编码不是有效 UTF-8"))
}

fn validate_raw_target(target: &str) -> Result<(), AppError> {
    if target.trim().is_empty() || target.chars().any(char::is_control) {
        return Err(AppError::invalid_markdown_target("目标为空或包含控制字符"));
    }
    let lower = target.to_ascii_lowercase();
    if lower.starts_with(r"\\?\") || lower.starts_with(r"\\.\") {
        return Err(AppError::invalid_markdown_target("不允许 Windows 设备路径"));
    }
    Ok(())
}

fn is_windows_absolute(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

#[cfg(any(not(windows), test))]
fn is_posix_absolute(value: &str) -> bool {
    value.starts_with('/') && !value.starts_with("//")
}

fn is_drive_relative(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes.len() == 2 || !matches!(bytes[2], b'/' | b'\\'))
}

fn image_mime(path: &Path, bytes: &[u8]) -> Result<&'static str, AppError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mime = match extension.as_str() {
        "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => "image/png",
        "jpg" | "jpeg" if bytes.starts_with(&[0xff, 0xd8, 0xff]) => "image/jpeg",
        "gif" if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") => "image/gif",
        "webp" if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" => {
            "image/webp"
        }
        _ => return Err(AppError::unsupported_image_type(&path.to_string_lossy())),
    };
    Ok(mime)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use base64::{engine::general_purpose::STANDARD, Engine};
    use url::Url;

    use super::{
        is_posix_absolute, load_local_image_with_permission, resolve_markdown_target,
        MAX_IMAGE_SIZE,
    };
    use crate::models::navigation::MarkdownTargetDto;

    fn test_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "marklite-navigation-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn resolves_encoded_relative_documents_against_the_document_parent() {
        let dir = test_dir("relative");
        let nested = dir.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let current = nested.join("index.md");
        let target = dir.join("说明 file.md");
        fs::write(&current, "index").unwrap();
        fs::write(&target, "target").unwrap();
        let wrapped = "marklite:%2E%2E%2F%E8%AF%B4%E6%98%8E%2520file%2Emd%23heading";

        let resolved = resolve_markdown_target(Some(&current.to_string_lossy()), wrapped).unwrap();

        assert_eq!(
            resolved,
            MarkdownTargetDto::LocalDocument {
                path: target.to_string_lossy().to_string(),
                fragment: Some("heading".to_string())
            }
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn decodes_only_one_semantic_url_layer() {
        let dir = test_dir("double-encoding");
        let current = dir.join("index.md");
        let literal = dir.join("%2e%2e%2ftarget.md");
        fs::write(&current, "index").unwrap();
        fs::write(&literal, "literal").unwrap();

        let resolved = resolve_markdown_target(
            Some(&current.to_string_lossy()),
            "marklite:%25252e%25252e%25252ftarget%2Emd",
        )
        .unwrap();

        assert_eq!(
            resolved,
            MarkdownTargetDto::LocalDocument {
                path: literal.to_string_lossy().to_string(),
                fragment: None,
            }
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn requires_a_saved_document_for_relative_targets() {
        let error = resolve_markdown_target(None, "relative.md").unwrap_err();
        assert_eq!(error.code, "RELATIVE_TARGET_REQUIRES_SAVED_DOCUMENT");
    }

    #[test]
    fn recognizes_only_unambiguous_posix_absolute_paths() {
        assert!(is_posix_absolute("/home/vazone/note.md"));
        assert!(!is_posix_absolute("home/vazone/note.md"));
        assert!(!is_posix_absolute("//server/share/note.md"));
    }

    #[cfg(not(windows))]
    #[test]
    fn resolves_posix_absolute_document_and_image_paths() {
        let dir = test_dir("posix-absolute");
        let current = dir.join("index.md");
        let target = dir.join("target.md");
        let image = dir.join("pixel.png");
        fs::write(&current, "index").unwrap();
        fs::write(&target, "target").unwrap();
        fs::write(&image, b"\x89PNG\r\n\x1a\nrest").unwrap();

        assert!(matches!(
            resolve_markdown_target(Some(&current.to_string_lossy()), &target.to_string_lossy())
                .unwrap(),
            MarkdownTargetDto::LocalDocument { .. }
        ));
        assert!(load_local_image_with_permission(
            Some(&current.to_string_lossy()),
            &image.to_string_lossy(),
            true
        )
        .unwrap()
        .data_url
        .starts_with("data:image/png;base64,"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolves_current_and_child_directory_documents() {
        let dir = test_dir("current-child");
        let child = dir.join("child");
        fs::create_dir(&child).unwrap();
        let current = dir.join("index.md");
        let sibling = dir.join("sibling.md");
        let nested = child.join("nested.markdown");
        fs::write(&current, "index").unwrap();
        fs::write(&sibling, "sibling").unwrap();
        fs::write(&nested, "nested").unwrap();

        for (target, expected) in [("sibling.md", sibling), ("child/nested.markdown", nested)] {
            assert_eq!(
                resolve_markdown_target(Some(&current.to_string_lossy()), target).unwrap(),
                MarkdownTargetDto::LocalDocument {
                    path: expected.to_string_lossy().to_string(),
                    fragment: None,
                }
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn canonicalizes_directory_reparse_aliases_before_returning_identity() {
        use std::process::Command;

        let dir = test_dir("junction");
        let real = dir.join("real");
        let alias = dir.join("alias");
        fs::create_dir(&real).unwrap();
        let current = dir.join("index.md");
        let target = real.join("target.md");
        fs::write(&current, "index").unwrap();
        fs::write(&target, "target").unwrap();
        let output = Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&alias)
            .arg(&real)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "创建测试 junction 失败：{}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert_eq!(
            resolve_markdown_target(Some(&current.to_string_lossy()), "alias/target.md").unwrap(),
            MarkdownTargetDto::LocalDocument {
                path: target.to_string_lossy().to_string(),
                fragment: None,
            }
        );
        fs::remove_dir(&alias).unwrap();
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn accepts_http_and_file_urls_but_rejects_dangerous_forms() {
        let dir = test_dir("urls");
        let current = dir.join("index.md");
        let target = dir.join("target.md");
        fs::write(&current, "index").unwrap();
        fs::write(&target, "target").unwrap();
        let file_url = Url::from_file_path(&target).unwrap().to_string();

        assert!(matches!(
            resolve_markdown_target(Some(&current.to_string_lossy()), "https://example.com/a")
                .unwrap(),
            MarkdownTargetDto::External { .. }
        ));
        assert!(matches!(
            resolve_markdown_target(Some(&current.to_string_lossy()), &file_url).unwrap(),
            MarkdownTargetDto::LocalDocument { .. }
        ));
        assert!(matches!(
            resolve_markdown_target(Some(&current.to_string_lossy()), &target.to_string_lossy())
                .unwrap(),
            MarkdownTargetDto::LocalDocument { .. }
        ));
        for target in [
            "javascript:alert(1)",
            "data:text/plain,hello",
            "ftp://example.com/file.md",
            "//example.com/a",
            r"C:relative.md",
            r"\\?\C:\secret.md",
            r"\\.\C:\secret.md",
            "relative.md?download=1",
            "line\nbreak.md",
        ] {
            assert!(resolve_markdown_target(Some(&current.to_string_lossy()), target).is_err());
        }
        assert!(resolve_markdown_target(
            Some(&current.to_string_lossy()),
            "file://server/share/secret.md"
        )
        .is_err());
        assert_eq!(
            resolve_markdown_target(Some(&current.to_string_lossy()), "missing.md")
                .unwrap_err()
                .code,
            "FILE_NOT_FOUND"
        );
        let directory = dir.join("folder.md");
        fs::create_dir(&directory).unwrap();
        assert_eq!(
            resolve_markdown_target(Some(&current.to_string_lossy()), "folder.md")
                .unwrap_err()
                .code,
            "INVALID_FILE_TARGET"
        );
        let unsupported = dir.join("target.pdf");
        fs::write(&unsupported, "pdf").unwrap();
        assert_eq!(
            resolve_markdown_target(Some(&current.to_string_lossy()), "target.pdf")
                .unwrap_err()
                .code,
            "INVALID_FILE_TYPE"
        );

        #[cfg(windows)]
        {
            let mixed = target.to_string_lossy().replace('\\', "/");
            assert!(matches!(
                resolve_markdown_target(Some(&current.to_string_lossy()), &mixed).unwrap(),
                MarkdownTargetDto::LocalDocument { .. }
            ));
            let upper_case = target.to_string_lossy().to_uppercase();
            assert!(matches!(
                resolve_markdown_target(Some(&current.to_string_lossy()), &upper_case).unwrap(),
                MarkdownTargetDto::LocalDocument { .. }
            ));
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loads_only_enabled_images_with_matching_magic() {
        let dir = test_dir("image");
        let current = dir.join("index.md");
        let image = dir.join("pixel.png");
        let encoded_image = dir.join("pixel space.png");
        let svg = dir.join("vector.svg");
        fs::write(&current, "index").unwrap();
        let png = STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        fs::write(&image, &png).unwrap();
        fs::write(&encoded_image, &png).unwrap();
        fs::write(&svg, "<svg/>").unwrap();

        let disabled =
            load_local_image_with_permission(Some(&current.to_string_lossy()), "pixel.png", false)
                .unwrap_err();
        let loaded =
            load_local_image_with_permission(Some(&current.to_string_lossy()), "pixel.png", true)
                .unwrap();

        assert_eq!(disabled.code, "LOCAL_IMAGES_DISABLED");
        assert!(loaded.data_url.starts_with("data:image/png;base64,"));
        let encoded_absolute = encoded_image.to_string_lossy().replace(' ', "%20");
        assert!(load_local_image_with_permission(
            Some(&current.to_string_lossy()),
            &encoded_absolute,
            true
        )
        .unwrap()
        .data_url
        .starts_with("data:image/png;base64,"));
        assert_eq!(
            load_local_image_with_permission(Some(&current.to_string_lossy()), "vector.svg", true)
                .unwrap_err()
                .code,
            "UNSUPPORTED_IMAGE_TYPE"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn accepts_supported_image_magic_and_rejects_mismatch_or_oversize() {
        let dir = test_dir("image-types");
        let current = dir.join("index.md");
        fs::write(&current, "index").unwrap();
        for (name, bytes, prefix) in [
            (
                "photo.jpg",
                b"\xff\xd8\xffrest".as_slice(),
                "data:image/jpeg;base64,",
            ),
            (
                "animation.gif",
                b"GIF89arest".as_slice(),
                "data:image/gif;base64,",
            ),
            (
                "picture.webp",
                b"RIFF\x04\x00\x00\x00WEBPrest".as_slice(),
                "data:image/webp;base64,",
            ),
        ] {
            fs::write(dir.join(name), bytes).unwrap();
            let loaded =
                load_local_image_with_permission(Some(&current.to_string_lossy()), name, true)
                    .unwrap();
            assert!(loaded.data_url.starts_with(prefix));
        }

        fs::write(dir.join("mismatch.png"), b"GIF89arest").unwrap();
        assert_eq!(
            load_local_image_with_permission(
                Some(&current.to_string_lossy()),
                "mismatch.png",
                true
            )
            .unwrap_err()
            .code,
            "UNSUPPORTED_IMAGE_TYPE"
        );

        let oversized = dir.join("oversized.png");
        let file = fs::File::create(&oversized).unwrap();
        file.set_len(MAX_IMAGE_SIZE + 1).unwrap();
        assert_eq!(
            load_local_image_with_permission(
                Some(&current.to_string_lossy()),
                "oversized.png",
                true
            )
            .unwrap_err()
            .code,
            "FILE_TOO_LARGE"
        );

        assert_eq!(
            load_local_image_with_permission(
                Some(&current.to_string_lossy()),
                "does-not-exist.png",
                false
            )
            .unwrap_err()
            .code,
            "LOCAL_IMAGES_DISABLED"
        );
        assert_eq!(
            load_local_image_with_permission(
                Some(&current.to_string_lossy()),
                "does-not-exist.png",
                true
            )
            .unwrap_err()
            .code,
            "FILE_NOT_FOUND"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
