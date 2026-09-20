use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagramSource {
    pub diagram_id: String,
    pub ordinal: usize,
    pub source_utf8: String,
    pub source_sha256: String,
    pub source_start_byte: usize,
    pub source_end_byte: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiagramTheme {
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagramDiagnostic {
    pub code: String,
    pub diagram_id: String,
    pub source_start_byte: usize,
    pub source_end_byte: usize,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderedDiagram {
    pub diagram_id: String,
    pub source_sha256: String,
    pub cache_key: String,
    pub renderer_id: String,
    pub svg_utf8: String,
    pub width: f64,
    pub height: f64,
    pub view_box: [f64; 4],
    pub accessible_title: Option<String>,
    pub accessible_description: Option<String>,
    pub warnings: Vec<DiagramDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagramRuntimeAsset {
    pub renderer_id: String,
    pub script_utf8: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagramRuntimeStatus {
    pub renderer_id: String,
    pub installed: bool,
}
