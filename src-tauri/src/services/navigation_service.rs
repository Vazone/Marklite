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
