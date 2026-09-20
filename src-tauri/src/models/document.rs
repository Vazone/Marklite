use serde::{Deserialize, Serialize};

use super::app_error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentDto {
    pub path: Option<String>,
    pub file_identity: Option<String>,
    pub content_version: Option<String>,
    pub title: String,
    pub content: String,
    pub is_dirty: bool,
    pub last_saved_at: Option<String>,
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileVersionDto {
    pub file_identity: String,
    pub content_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentOperationDto {
    pub document: DocumentDto,
    pub auxiliary_error: Option<AppError>,
}
