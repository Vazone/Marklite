use serde::Serialize;

use super::diagram::{DiagramDiagnostic, DiagramSource};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineItem {
    pub level: u8,
    pub title: String,
    pub line: usize,
    pub slug: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentStats {
    pub word_count: usize,
    pub character_count: usize,
    pub line_count: usize,
    pub heading_count: usize,
    pub link_count: usize,
    pub image_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedMarkdownDto {
    pub html: String,
    pub outline: Vec<OutlineItem>,
    pub stats: DocumentStats,
    pub source_blocks: Vec<SourceBlock>,
    pub diagrams: Vec<DiagramSource>,
    pub diagram_diagnostics: Vec<DiagramDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_preview: Option<VirtualPreviewIndex>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualPreviewIndex {
    pub session_id: String,
    pub segments: Vec<VirtualPreviewSegment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualPreviewSegment {
    pub start_utf16: usize,
    pub end_utf16: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub estimated_height: usize,
    pub estimated_nodes: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualPreviewWindow {
    pub session_id: String,
    pub start: usize,
    pub end: usize,
    pub segments: Vec<VirtualPreviewRenderedSegment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualPreviewRenderedSegment {
    pub index: usize,
    pub html: String,
    pub source_block_start: usize,
    pub source_blocks: Vec<SourceBlock>,
    pub diagrams: Vec<DiagramSource>,
    pub diagram_diagnostics: Vec<DiagramDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBlock {
    pub start_utf16: usize,
    pub end_utf16: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownAnalysisDto {
    pub outline: Vec<OutlineItem>,
    pub stats: DocumentStats,
}
