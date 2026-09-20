use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecentFileDto {
    pub path: String,
    pub title: String,
    pub last_opened_at: String,
}
