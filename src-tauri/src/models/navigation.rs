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
    Email {
        address: String,
    },
}
