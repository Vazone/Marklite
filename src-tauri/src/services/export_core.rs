use std::path::PathBuf;

use crate::{
    models::{
        app_error::AppError,
        export::{ExportFormat, ExportRequest, ExportResult, ExportTargetKind, ExportWarning},
    },
    utils::atomic_write::{atomic_write, atomic_write_create_new},
};

const MAX_EXPORT_CONTENT_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportCommitPolicy {
    Replace,
    CreateNew,
}

pub(crate) fn validate_request(request: &ExportRequest) -> Result<PathBuf, AppError> {
    if request.snapshot.job_id.trim().is_empty()
        || request.snapshot.tab_id.trim().is_empty()
        || request
            .snapshot
            .job_id
            .chars()
            .chain(request.snapshot.tab_id.chars())
            .any(char::is_control)
    {
        return Err(AppError::new(
            "INVALID_EXPORT_REQUEST",
            "导出任务缺少稳定的 jobId 或 tabId",
        ));
    }
    if request.snapshot.content.len() > MAX_EXPORT_CONTENT_BYTES {
        return Err(AppError::new(
            "EXPORT_CONTENT_TOO_LARGE",
            "导出正文超过 10 MiB",
        ));
    }
    match request.format {
        ExportFormat::Svg if request.mind_map_svg.is_none() => {
            return Err(AppError::new(
                "INVALID_MIND_MAP_SVG",
                "脑图 SVG 导出请求缺少矢量画布内容",
            ));
        }
        ExportFormat::Svg => {}
        _ if request.mind_map_svg.is_some() => {
            return Err(AppError::new(
                "INVALID_EXPORT_REQUEST",
                "非 SVG 导出请求不得携带脑图矢量内容",
            ));
        }
        _ => {}
    }
    let expected_kind = if request.format == ExportFormat::Png {
        ExportTargetKind::Directory
    } else {
        ExportTargetKind::File
    };
    if request.target_kind != expected_kind {
        return Err(AppError::new(
            "INVALID_EXPORT_TARGET",
            "导出格式与文件/目录目标类型不一致",
        ));
    }
    if request.format == ExportFormat::Png {
        return super::png_artifact::validate_target(request);
    }
    let target = PathBuf::from(&request.target_path);
    if !target.is_absolute()
        || !target
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(request.format.extension()))
    {
        return Err(AppError::new(
            "INVALID_EXPORT_TARGET",
            format!(
                "导出目标必须是绝对 .{} 文件路径",
                request.format.extension()
            ),
        ));
    }
    if target.exists() && !target.is_file() {
        return Err(AppError::invalid_file_target(&request.target_path));
    }
    Ok(target)
}

pub(crate) fn result(request: &ExportRequest, warnings: Vec<ExportWarning>) -> ExportResult {
    ExportResult {
        job_id: request.snapshot.job_id.clone(),
        format: request.format,
        path: request.target_path.clone(),
        target_kind: request.target_kind,
        warnings,
    }
}

fn target_exists_error(request: &ExportRequest) -> AppError {
    AppError::new(
        "EXPORT_TARGET_EXISTS",
        format!(
            "导出目标已存在，使用 --overwrite 才可覆盖：{}",
            request.target_path
        ),
    )
}

pub(crate) fn preflight_commit(
    request: &ExportRequest,
    policy: ExportCommitPolicy,
) -> Result<(), AppError> {
    let target = validate_request(request)?;
    if policy == ExportCommitPolicy::CreateNew && target.exists() {
        return Err(target_exists_error(request));
    }
    Ok(())
}

pub(crate) fn commit(
    request: &ExportRequest,
    bytes: &[u8],
    policy: ExportCommitPolicy,
) -> Result<(), AppError> {
    if request.target_kind != ExportTargetKind::File {
        return Err(AppError::new(
            "INVALID_EXPORT_TARGET",
            "目录产物不能使用单文件提交",
        ));
    }
    let target = validate_request(request)?;
    if let Some(parent) = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| AppError::file_write_failed(&request.target_path, error))?;
    }
    let write = match policy {
        ExportCommitPolicy::Replace => atomic_write(&target, bytes),
        ExportCommitPolicy::CreateNew => atomic_write_create_new(&target, bytes),
    };
    write.map_err(|error| {
        if policy == ExportCommitPolicy::CreateNew
            && error.kind() == std::io::ErrorKind::AlreadyExists
        {
            target_exists_error(request)
        } else {
            AppError::file_write_failed(&request.target_path, error)
        }
    })
}
