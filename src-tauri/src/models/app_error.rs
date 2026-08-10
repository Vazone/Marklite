use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn file_not_found(path: &str) -> Self {
        Self::new("FILE_NOT_FOUND", format!("文件不存在：{path}"))
    }

    pub fn invalid_file_type(path: &str) -> Self {
        Self::new(
            "INVALID_FILE_TYPE",
            format!("仅支持 .md、.markdown、.txt 文件：{path}"),
        )
    }

    pub fn file_read_failed(path: &str, err: impl std::fmt::Display) -> Self {
        Self::new("FILE_READ_FAILED", format!("读取文件失败：{path}。{err}"))
    }

    pub fn file_write_failed(path: &str, err: impl std::fmt::Display) -> Self {
        Self::new("FILE_WRITE_FAILED", format!("保存文件失败：{path}。{err}"))
    }

    pub fn invalid_file_target(path: &str) -> Self {
        Self::new("INVALID_FILE_TARGET", format!("目标不是普通文件：{path}"))
    }

    pub fn invalid_file_path(path: &str) -> Self {
        Self::new(
            "INVALID_FILE_PATH",
            format!("文件路径必须是绝对路径：{path}"),
        )
    }

    pub fn file_too_large(path: &str, operation: &str) -> Self {
        Self::new(
            "FILE_TOO_LARGE",
            format!("文件超过 10 MiB，已阻止{operation}：{path}"),
        )
    }

    pub fn settings_read_failed(err: impl std::fmt::Display) -> Self {
        Self::new("SETTINGS_READ_FAILED", format!("读取设置失败：{err}"))
    }

    pub fn settings_write_failed(err: impl std::fmt::Display) -> Self {
        Self::new("SETTINGS_WRITE_FAILED", format!("保存设置失败：{err}"))
    }

    pub fn invalid_settings(err: impl std::fmt::Display) -> Self {
        Self::new("INVALID_SETTINGS", format!("设置值无效：{err}"))
    }

    pub fn settings_version_unsupported(version: u64) -> Self {
        Self::new(
            "SETTINGS_VERSION_UNSUPPORTED",
            format!("当前 MarkLite 不支持设置文件版本 {version}，请使用兼容版本"),
        )
    }

    pub fn recent_files_read_failed(err: impl std::fmt::Display) -> Self {
        Self::new(
            "RECENT_FILES_READ_FAILED",
            format!("读取最近文件失败：{err}"),
        )
    }

    pub fn recent_files_write_failed(err: impl std::fmt::Display) -> Self {
        Self::new(
            "RECENT_FILES_WRITE_FAILED",
            format!("保存最近文件失败：{err}"),
        )
    }

    pub fn invalid_markdown_target(err: impl std::fmt::Display) -> Self {
        Self::new(
            "INVALID_MARKDOWN_TARGET",
            format!("Markdown 目标无效：{err}"),
        )
    }

    pub fn unsupported_link_scheme(scheme: &str) -> Self {
        Self::new(
            "UNSUPPORTED_LINK_SCHEME",
            format!("不支持的链接协议：{scheme}"),
        )
    }

    pub fn relative_target_requires_saved_document() -> Self {
        Self::new(
            "RELATIVE_TARGET_REQUIRES_SAVED_DOCUMENT",
            "相对本地路径需要先保存当前文档",
        )
    }

    pub fn local_images_disabled() -> Self {
        Self::new("LOCAL_IMAGES_DISABLED", "请先在设置中允许本地图片")
    }

    pub fn unsupported_image_type(path: &str) -> Self {
        Self::new(
            "UNSUPPORTED_IMAGE_TYPE",
            format!("仅支持 PNG、JPEG、GIF、WebP 图片：{path}"),
        )
    }

    pub fn session_read_failed(err: impl std::fmt::Display) -> Self {
        Self::new("SESSION_READ_FAILED", format!("读取会话失败：{err}"))
    }

    pub fn session_write_failed(err: impl std::fmt::Display) -> Self {
        Self::new("SESSION_WRITE_FAILED", format!("保存会话失败：{err}"))
    }

    pub fn startup_diagnostics_unavailable() -> Self {
        Self::new("STARTUP_DIAGNOSTICS_UNAVAILABLE", "启动诊断目录不可用")
    }

    pub fn startup_diagnostics_failed(operation: &str, kind: std::io::ErrorKind) -> Self {
        Self::new(
            "STARTUP_DIAGNOSTICS_FAILED",
            format!("启动诊断{operation}失败（{kind:?}）"),
        )
    }

    pub fn invalid_startup_diagnostics_target() -> Self {
        Self::new(
            "INVALID_STARTUP_DIAGNOSTICS_TARGET",
            "启动诊断只能导出到绝对 .json 文件路径",
        )
    }

    pub fn invalid_startup_diagnostics_event() -> Self {
        Self::new(
            "INVALID_STARTUP_DIAGNOSTIC_EVENT",
            "启动诊断事件的阶段、状态和错误码不一致",
        )
    }
}
