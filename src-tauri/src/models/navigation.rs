use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MarkdownTargetDto {
    Anchor {
        fragment: String,
    },
    LocalDocument {
        path: String,
        fragment: Option<String>,
    },
    External {
        url: String,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalImageDto {
    pub data_url: String,
    pub path: String,
}
