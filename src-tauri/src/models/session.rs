use serde::{Deserialize, Serialize};

pub const SESSION_VERSION: u32 = 1;
pub const MAX_SESSION_PATHS: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub version: u32,
    pub paths: Vec<String>,
    pub active_path: Option<String>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            version: SESSION_VERSION,
            paths: Vec::new(),
            active_path: None,
        }
    }
}
