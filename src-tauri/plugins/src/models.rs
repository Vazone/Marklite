use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct PickResponse {
    pub uri: Option<String>,
}

#[derive(Serialize)]
pub struct StatusBarAppearanceRequest {
    pub dark: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SystemInsetsResponse {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
    #[serde(rename = "imeBottom")]
    pub ime_bottom: f64,
}

#[derive(Debug, Deserialize)]
pub struct DocumentContents {
    pub uri: String,
    pub name: String,
    pub content: String,
    pub version: String,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct DocumentVersion {
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct DocumentName {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DocumentWriteResult {
    pub uri: String,
    pub name: String,
    pub version: String,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct IntentUris {
    pub uris: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct TreeName {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct TreeEntry {
    pub name: String,
    pub uri: String,
    pub kind: String,
    pub size: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct TreeEntries {
    pub entries: Vec<TreeEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeLocation {
    pub relative_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImageBytes {
    pub base64: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreePathRequest<'a> {
    pub tree_uri: &'a str,
    pub relative_path: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeSourceRequest<'a> {
    pub tree_uri: &'a str,
    pub source_uri: &'a str,
}

#[derive(Serialize)]
pub struct UriRequest<'a> {
    pub uri: &'a str,
}

#[derive(Serialize)]
pub struct CreateDocumentRequest<'a> {
    pub title: &'a str,
}

#[derive(Serialize)]
pub struct CreateExportDocumentRequest<'a> {
    pub title: &'a str,
    pub mime: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentWriteRequest<'a> {
    pub uri: &'a str,
    pub content: &'a str,
    pub expected_version: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFileRequest<'a> {
    pub uri: &'a str,
    pub staged_path: &'a str,
    pub bytes: u64,
    pub sha256: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFileResult {
    pub uri: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Serialize)]
pub struct RenderExportRequest<'a> {
    pub html: &'a str,
}

#[derive(Deserialize)]
pub struct RenderExportResponse {
    pub result: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturePngRequest<'a> {
    pub html: &'a str,
    pub staged_path: &'a str,
}

#[derive(Deserialize)]
pub struct CapturePngResult {
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawPdfRequest<'a> {
    pub html: &'a str,
    pub staged_path: &'a str,
    pub paper_size: &'a str,
    pub orientation: &'a str,
    pub margin: &'a str,
}

#[derive(Deserialize)]
pub struct DrawPdfResult {
    pub bytes: u64,
    pub pages: u32,
    pub layout: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginImageExportRequest<'a> {
    pub tree_uri: &'a str,
    pub folder_name: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateImageFileRequest<'a> {
    pub folder_uri: &'a str,
    pub name: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageFolderRequest<'a> {
    pub folder_uri: &'a str,
}

#[derive(Deserialize)]
pub struct ImageUriResult {
    pub uri: String,
}
