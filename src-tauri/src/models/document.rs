use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDto {
    pub path: Option<String>,
    pub title: String,
    pub content: String,
    pub is_dirty: bool,
    pub last_saved_at: Option<String>,
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadataDto {
    pub path: String,
    pub file_name: String,
    pub file_size: u64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFileDto {
    pub path: String,
    pub title: String,
    pub last_opened_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineItem {
    pub level: u8,
    pub title: String,
    pub line: usize,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentStats {
    pub word_count: usize,
    pub character_count: usize,
    pub line_count: usize,
    pub heading_count: usize,
    pub link_count: usize,
    pub image_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedMarkdownDto {
    pub html: String,
    pub outline: Vec<OutlineItem>,
    pub stats: DocumentStats,
}
