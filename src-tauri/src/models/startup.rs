use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrontendStartupStage {
    FrontendEntry,
    SvelteMount,
    DomReady,
    Initialization,
    Settings,
    RecentFiles,
    SessionRestore,
    ExternalListeners,
    StartupFile,
    FirstRender,
    DragDrop,
    EditorMount,
}

impl FrontendStartupStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FrontendEntry => "frontendEntry",
            Self::SvelteMount => "svelteMount",
            Self::DomReady => "domReady",
            Self::Initialization => "initialization",
            Self::Settings => "settings",
            Self::RecentFiles => "recentFiles",
            Self::SessionRestore => "sessionRestore",
            Self::ExternalListeners => "externalListeners",
            Self::StartupFile => "startupFile",
            Self::FirstRender => "firstRender",
            Self::DragDrop => "dragDrop",
            Self::EditorMount => "editorMount",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrontendStartupStatus {
    Started,
    Succeeded,
    Failed,
}

impl FrontendStartupStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrontendStartupCode {
    AppRootMissing,
    ModuleLoadFailed,
    SvelteMountFailed,
    ReadySentinelMissing,
    ReadyHandshakeFailed,
    UnhandledError,
    UnhandledRejection,
    InitializationFailed,
    EditorMountFailed,
}

impl FrontendStartupCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AppRootMissing => "appRootMissing",
            Self::ModuleLoadFailed => "moduleLoadFailed",
            Self::SvelteMountFailed => "svelteMountFailed",
            Self::ReadySentinelMissing => "readySentinelMissing",
            Self::ReadyHandshakeFailed => "readyHandshakeFailed",
            Self::UnhandledError => "unhandledError",
            Self::UnhandledRejection => "unhandledRejection",
            Self::InitializationFailed => "initializationFailed",
            Self::EditorMountFailed => "editorMountFailed",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendStartupEventDto {
    pub stage: FrontendStartupStage,
    pub status: FrontendStartupStatus,
    pub code: Option<FrontendStartupCode>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupDiagnosticRecord {
    pub schema_version: u8,
    pub timestamp: String,
    pub launch_id: String,
    pub stage: String,
    pub status: String,
    pub code: Option<String>,
    pub elapsed_ms: u64,
    pub app_version: String,
    pub webview_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupReadyDto {
    pub launch_id: String,
    pub webview_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupDiagnosticsExportDto {
    pub record_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupDiagnosticsExportFile {
    pub schema_version: u8,
    pub exported_at: String,
    pub records: Vec<StartupDiagnosticRecord>,
}
