use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Html,
    Pdf,
    Docx,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Pdf => "pdf",
            Self::Docx => "docx",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExportPaperSize {
    A4,
    Letter,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExportOrientation {
    Portrait,
    Landscape,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExportMarginPreset {
    Narrow,
    Normal,
    Wide,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportSnapshot {
    pub job_id: String,
    pub tab_id: String,
    pub content_revision: u64,
    pub source_path: Option<String>,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    pub paper_size: ExportPaperSize,
    pub orientation: ExportOrientation,
    pub margin: ExportMarginPreset,
    pub include_title: bool,
    pub include_local_images: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub snapshot: ExportSnapshot,
    pub target_path: String,
    pub format: ExportFormat,
    pub options: ExportOptions,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportWarning {
    pub code: String,
    pub message: String,
    pub target: Option<String>,
}

impl ExportWarning {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        target: Option<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            target,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub job_id: String,
    pub format: ExportFormat,
    pub path: String,
    pub warnings: Vec<ExportWarning>,
}
