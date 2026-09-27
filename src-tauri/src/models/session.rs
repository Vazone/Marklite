use super::{app_error::AppError, resource::ResourceRef};
use serde::{Deserialize, Serialize};

pub const SESSION_VERSION: u32 = 1;
pub const MAX_SESSION_PATHS: usize = 50;

pub const RESOURCE_SESSION_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceSession {
    pub version: u32,
    pub resources: Vec<ResourceRef>,
    pub active_resource: Option<ResourceRef>,
}

impl From<&SessionState> for ResourceSession {
    fn from(session: &SessionState) -> Self {
        Self {
            version: RESOURCE_SESSION_VERSION,
            resources: session
                .paths
                .iter()
                .map(|path| ResourceRef::DesktopFile { path: path.clone() })
                .collect(),
            active_resource: session
                .active_path
                .as_ref()
                .map(|path| ResourceRef::DesktopFile { path: path.clone() }),
        }
    }
}

impl ResourceSession {
    /// Legacy desktop projection must fail rather than discard an unrepresentable resource.
    pub fn desktop(self) -> Result<SessionState, AppError> {
        fn path(resource: ResourceRef) -> Result<String, AppError> {
            match resource {
                ResourceRef::DesktopFile { path } => Ok(path),
                _ => Err(AppError::new(
                    "RESOURCE_UNSUPPORTED",
                    "当前桌面会话接口无法表示 URI 资源",
                )),
            }
        }
        Ok(SessionState {
            version: SESSION_VERSION,
            paths: self
                .resources
                .into_iter()
                .map(path)
                .collect::<Result<_, _>>()?,
            active_path: self.active_resource.map(path).transpose()?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
