use serde::{Deserialize, Serialize};

use super::{app_error::AppError, resource::ResourceRef};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspacePreferences {
    pub version: u32,
    pub root: Option<ResourceRef>,
    pub expanded: Vec<String>,
    pub restore_on_start: bool,
    pub recent: Vec<RecentWorkspace>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecentWorkspace {
    pub resource: ResourceRef,
    pub name: String,
}

impl Default for WorkspacePreferences {
    fn default() -> Self {
        Self {
            version: 2,
            root: None,
            expanded: vec![],
            restore_on_start: true,
            recent: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRoot {
    pub root_id: String,
    pub resource: ResourceRef,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    Directory,
    Markdown,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEntry {
    pub name: String,
    pub relative_path: String,
    pub kind: EntryKind,
    pub resource: Option<ResourceRef>,
    pub size: Option<u64>,
    pub issue: Option<AppError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageCursor {
    pub generation: u64,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePage {
    pub root_id: String,
    pub relative_path: String,
    pub generation: u64,
    pub entries: Vec<WorkspaceEntry>,
    pub next: Option<PageCursor>,
    pub watch_issue: Option<AppError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryRevision {
    pub relative_path: String,
    pub generation: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInvalidation {
    pub root_id: String,
    pub directories: Vec<DirectoryRevision>,
    pub overflow: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_output_matches_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../src/shared/workspace-contract-fixtures.json"
        ))
        .unwrap();
        let page = WorkspacePage {
            root_id: "workspace-1".into(),
            relative_path: "".into(),
            generation: 2,
            entries: vec![
                WorkspaceEntry {
                    name: "chapter".into(),
                    relative_path: "chapter".into(),
                    kind: EntryKind::Directory,
                    resource: Some(ResourceRef::DesktopDirectory {
                        path: r"C:\notes\chapter".into(),
                    }),
                    size: None,
                    issue: None,
                },
                WorkspaceEntry {
                    name: "a.md".into(),
                    relative_path: "a.md".into(),
                    kind: EntryKind::Markdown,
                    resource: Some(ResourceRef::DesktopFile {
                        path: r"C:\notes\a.md".into(),
                    }),
                    size: Some(123),
                    issue: None,
                },
                WorkspaceEntry {
                    name: "link".into(),
                    relative_path: "link".into(),
                    kind: EntryKind::Unavailable,
                    resource: None,
                    size: None,
                    issue: Some(AppError::new(
                        "WORKSPACE_LINK_UNSUPPORTED",
                        "Link not followed",
                    )),
                },
            ],
            next: Some(PageCursor {
                generation: 2,
                offset: 3,
            }),
            watch_issue: None,
        };
        assert_eq!(serde_json::to_value(page).unwrap(), fixture["page"]);
        let event = WorkspaceInvalidation {
            root_id: "workspace-1".into(),
            directories: vec![DirectoryRevision {
                relative_path: "".into(),
                generation: 3,
            }],
            overflow: true,
        };
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            fixture["invalidation"]
        );
        let error = AppError::new("WORKSPACE_PERMISSION_DENIED", "Directory access denied");
        assert_eq!(serde_json::to_value(error).unwrap(), fixture["failure"]);
    }
}
