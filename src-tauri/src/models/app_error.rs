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

    pub fn settings_read_failed(err: impl std::fmt::Display) -> Self {
        Self::new("SETTINGS_READ_FAILED", format!("读取设置失败：{err}"))
    }

    pub fn settings_write_failed(err: impl std::fmt::Display) -> Self {
        Self::new("SETTINGS_WRITE_FAILED", format!("保存设置失败：{err}"))
    }
}
