use serde::{Deserialize, Serialize};

use super::{app_error::AppError, resource::ResourceRef};

/// A recovery copy, never an instruction to overwrite its source resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoverySnapshot {
    pub id: String,
    pub revision: u64,
    pub resource: Option<ResourceRef>,
    pub title: String,
    pub content: String,
    pub base_file_identity: Option<String>,
    pub base_content_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecoveryReceipt {
    pub id: String,
    pub revision: u64,
    pub checksum: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryEntry {
    pub receipt: RecoveryReceipt,
    pub title: String,
    pub resource: Option<ResourceRef>,
    pub content_bytes: usize,
    pub stale: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryIssue {
    pub id: String,
    pub error: AppError,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInventory {
    pub entries: Vec<RecoveryEntry>,
    pub issues: Vec<RecoveryIssue>,
}
