use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock, Mutex,
    },
    time::{Duration, Instant, SystemTime},
};

use chrono::Utc;
use serde::Deserialize;

use crate::{
    models::{
        app_error::AppError,
        startup::{
            FrontendStartupCode, FrontendStartupEventDto, FrontendStartupStage,
            FrontendStartupStatus, StartupDiagnosticRecord, StartupDiagnosticsExportDto,
            StartupDiagnosticsExportFile, StartupReadyDto,
        },
    },
    utils::{
        atomic_write::atomic_write, bounded_read::OpenedFile, path_utils::startup_diagnostics_dir,
    },
};

const SCHEMA_VERSION: u8 = 1;
pub const RETENTION_DAYS: u64 = 14;
pub const MAX_RETAINED_FILES: usize = 128;
pub const MAX_DIAGNOSTIC_FILE_BYTES: u64 = 128 * 1024;
const FILE_PREFIX: &str = "startup-";
const FILE_SUFFIX: &str = ".jsonl";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartupDiagnosticSchema {
    schema_version: u8,
    stages: Vec<String>,
    statuses: Vec<String>,
    codes: Vec<String>,
}

static DIAGNOSTIC_SCHEMA: LazyLock<StartupDiagnosticSchema> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../src/shared/startup-diagnostics-schema.json"
    ))
    .expect("bundled startup diagnostics schema must be valid")
});

#[derive(Clone)]
pub struct StartupState {
    inner: Arc<StartupStateInner>,
}

struct StartupStateInner {
    launch_id: String,
    started_at: Instant,
    app_version: String,
    webview_version: Option<String>,
    log_dir: Option<PathBuf>,
    log_path: Option<PathBuf>,
    initialization_error: Option<AppError>,
    write_lock: Mutex<()>,
    ready: AtomicBool,
    retry_claimed: AtomicBool,
}

impl StartupState {
    pub fn new(app_version: &str, webview_version: Option<String>) -> Self {
        match startup_diagnostics_dir() {
            Ok(log_dir) => Self::new_with_directory(app_version, webview_version.clone(), log_dir)
                .unwrap_or_else(|error| {
                    Self::build(
                        app_version,
                        webview_version,
                        None,
                        Some(AppError::startup_diagnostics_failed("清理", error.kind())),
                    )
                }),
            Err(error) => Self::build(app_version, webview_version, None, Some(error)),
        }
    }

    #[cfg(test)]
    fn new_in(
        app_version: &str,
        webview_version: Option<String>,
        log_dir: PathBuf,
    ) -> io::Result<Self> {
        fs::create_dir_all(&log_dir)?;
        Self::new_with_directory(app_version, webview_version, log_dir)
    }

    fn new_with_directory(
        app_version: &str,
        webview_version: Option<String>,
        log_dir: PathBuf,
    ) -> io::Result<Self> {
        prune_diagnostic_files(&log_dir)?;
        Ok(Self::build(
            app_version,
            webview_version,
            Some(log_dir),
            None,
        ))
    }

    fn build(
        app_version: &str,
        webview_version: Option<String>,
        log_dir: Option<PathBuf>,
        initialization_error: Option<AppError>,
    ) -> Self {
        let launch_id = format!(
            "{}-{}",
            Utc::now().format("%Y%m%dT%H%M%S%3fZ"),
            std::process::id()
        );
        let log_path = log_dir
            .as_ref()
            .map(|dir| dir.join(format!("{FILE_PREFIX}{launch_id}{FILE_SUFFIX}")));

        Self {
            inner: Arc::new(StartupStateInner {
                launch_id,
                started_at: Instant::now(),
                app_version: app_version.to_string(),
                webview_version,
                log_dir,
                log_path,
                initialization_error,
                write_lock: Mutex::new(()),
                ready: AtomicBool::new(false),
                retry_claimed: AtomicBool::new(false),
            }),
        }
    }

    pub fn record_native(
        &self,
        stage: &'static str,
        status: &'static str,
        code: Option<&'static str>,
    ) -> Result<(), AppError> {
        self.write_record(stage, status, code, self.elapsed_ms())
    }

    pub fn record_frontend(&self, event: FrontendStartupEventDto) -> Result<(), AppError> {
        if !valid_frontend_event(&event) {
            return Err(AppError::invalid_startup_diagnostics_event());
        }
        self.write_record(
            event.stage.as_str(),
            event.status.as_str(),
            event.code.map(|value| value.as_str()),
            event.elapsed_ms.min(600_000),
        )
    }

    pub fn mark_ready(&self, elapsed_ms: u64) -> Result<StartupReadyDto, AppError> {
        if !self.inner.ready.swap(true, Ordering::SeqCst) {
            let _ = self.write_record("frontendReady", "succeeded", None, elapsed_ms.min(600_000));
        }
        Ok(StartupReadyDto {
            launch_id: self.inner.launch_id.clone(),
            webview_version: self.inner.webview_version.clone(),
        })
    }

    pub fn is_ready(&self) -> bool {
        self.inner.ready.load(Ordering::SeqCst)
    }

    pub fn claim_retry(&self) -> bool {
        self.inner
            .retry_claimed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn clear(&self) -> Result<(), AppError> {
        let _guard = self
            .inner
            .write_lock
            .lock()
            .map_err(|_| AppError::startup_diagnostics_failed("清除", io::ErrorKind::Other))?;
        let Some(dir) = self.inner.log_dir.as_deref() else {
            return Err(self.unavailable_error());
        };
        for path in diagnostic_files(dir)
            .map_err(|error| AppError::startup_diagnostics_failed("清除", error.kind()))?
        {
            fs::remove_file(path)
                .map_err(|error| AppError::startup_diagnostics_failed("清除", error.kind()))?;
        }
        Ok(())
    }

    pub fn export(&self, target: &Path) -> Result<StartupDiagnosticsExportDto, AppError> {
        let target = validate_export_target(target)?;
        let _guard = self
            .inner
            .write_lock
            .lock()
            .map_err(|_| AppError::startup_diagnostics_failed("导出", io::ErrorKind::Other))?;
        let Some(dir) = self.inner.log_dir.as_deref() else {
            return Err(self.unavailable_error());
        };
        let mut records = Vec::new();
        for path in diagnostic_files(dir)
            .map_err(|error| AppError::startup_diagnostics_failed("导出", error.kind()))?
        {
            let opened = OpenedFile::open(&path)
                .map_err(|error| AppError::startup_diagnostics_failed("导出", error.kind()))?;
            if !opened.metadata().is_file() {
                return Err(AppError::startup_diagnostics_failed(
                    "导出",
                    io::ErrorKind::InvalidData,
                ));
            }
            if opened.metadata().len() > MAX_DIAGNOSTIC_FILE_BYTES {
                return Err(AppError::startup_diagnostics_failed(
                    "导出",
                    io::ErrorKind::FileTooLarge,
                ));
            }
            let content = opened
                .read_to_string_bounded(MAX_DIAGNOSTIC_FILE_BYTES)
                .map_err(|error| AppError::startup_diagnostics_failed("导出", error.kind()))?;
            for line in content.lines() {
                let record =
                    serde_json::from_str::<StartupDiagnosticRecord>(line).map_err(|_| {
                        AppError::startup_diagnostics_failed("导出", io::ErrorKind::InvalidData)
                    })?;
                if !valid_diagnostic_record(&record) {
                    return Err(AppError::startup_diagnostics_failed(
                        "导出",
                        io::ErrorKind::InvalidData,
                    ));
                }
                records.push(record);
            }
        }
        records.sort_by(|left, right| left.timestamp.cmp(&right.timestamp));
        let record_count = records.len();
        let export = StartupDiagnosticsExportFile {
            schema_version: SCHEMA_VERSION,
            exported_at: Utc::now().to_rfc3339(),
            records,
        };
        let content = serde_json::to_vec_pretty(&export).map_err(|_| {
            AppError::startup_diagnostics_failed("导出", io::ErrorKind::InvalidData)
        })?;
        atomic_write(&target, &content)
            .map_err(|error| AppError::startup_diagnostics_failed("导出", error.kind()))?;
        Ok(StartupDiagnosticsExportDto { record_count })
    }

    fn elapsed_ms(&self) -> u64 {
        self.inner.started_at.elapsed().as_millis().min(600_000) as u64
    }

    fn write_record(
        &self,
        stage: &str,
        status: &str,
        code: Option<&str>,
        elapsed_ms: u64,
    ) -> Result<(), AppError> {
        if !valid_diagnostic_values(stage, status, code) || elapsed_ms > 600_000 {
            return Err(AppError::invalid_startup_diagnostics_event());
        }
        let _guard = self
            .inner
            .write_lock
            .lock()
            .map_err(|_| AppError::startup_diagnostics_failed("写入", io::ErrorKind::Other))?;
        let Some(path) = self.inner.log_path.as_deref() else {
            return Err(self.unavailable_error());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| AppError::startup_diagnostics_failed("写入", error.kind()))?;
        }
        let record = StartupDiagnosticRecord {
            schema_version: SCHEMA_VERSION,
            timestamp: Utc::now().to_rfc3339(),
            launch_id: self.inner.launch_id.clone(),
            stage: stage.to_string(),
            status: status.to_string(),
            code: code.map(str::to_string),
            elapsed_ms,
            app_version: self.inner.app_version.clone(),
            webview_version: self.inner.webview_version.clone(),
        };
        let mut line = serde_json::to_vec(&record).map_err(|_| {
            AppError::startup_diagnostics_failed("写入", io::ErrorKind::InvalidData)
        })?;
        line.push(b'\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| AppError::startup_diagnostics_failed("写入", error.kind()))?;
        let metadata = file
            .metadata()
            .map_err(|error| AppError::startup_diagnostics_failed("写入", error.kind()))?;
        if !metadata.is_file()
            || metadata.len().saturating_add(line.len() as u64) > MAX_DIAGNOSTIC_FILE_BYTES
        {
            return Err(AppError::startup_diagnostics_failed(
                "写入",
                if metadata.is_file() {
                    io::ErrorKind::FileTooLarge
                } else {
                    io::ErrorKind::InvalidData
                },
            ));
        }
        file.write_all(&line)
            .and_then(|_| file.flush())
            .map_err(|error| AppError::startup_diagnostics_failed("写入", error.kind()))
    }

    fn unavailable_error(&self) -> AppError {
        self.inner
            .initialization_error
            .clone()
            .unwrap_or_else(AppError::startup_diagnostics_unavailable)
    }
}

fn valid_diagnostic_values(stage: &str, status: &str, code: Option<&str>) -> bool {
    DIAGNOSTIC_SCHEMA.schema_version == SCHEMA_VERSION
        && DIAGNOSTIC_SCHEMA.stages.iter().any(|value| value == stage)
        && DIAGNOSTIC_SCHEMA
            .statuses
            .iter()
            .any(|value| value == status)
        && code.is_none_or(|code| DIAGNOSTIC_SCHEMA.codes.iter().any(|value| value == code))
}

fn valid_diagnostic_record(record: &StartupDiagnosticRecord) -> bool {
    record.schema_version == SCHEMA_VERSION
        && !record.launch_id.trim().is_empty()
        && !record.app_version.trim().is_empty()
        && chrono::DateTime::parse_from_rfc3339(&record.timestamp).is_ok()
        && record.elapsed_ms <= 600_000
        && valid_diagnostic_values(&record.stage, &record.status, record.code.as_deref())
}

fn valid_frontend_event(event: &FrontendStartupEventDto) -> bool {
    match (event.status, event.code) {
        (FrontendStartupStatus::Started | FrontendStartupStatus::Succeeded, None) => true,
        (FrontendStartupStatus::Degraded, Some(FrontendStartupCode::StageDegraded)) => matches!(
            event.stage,
            FrontendStartupStage::Settings
                | FrontendStartupStage::RecentFiles
                | FrontendStartupStage::SessionRestore
                | FrontendStartupStage::ExternalListeners
                | FrontendStartupStage::StartupFile
                | FrontendStartupStage::FirstRender
                | FrontendStartupStage::DragDrop
        ),
        (FrontendStartupStatus::Failed, Some(code)) => match code {
            FrontendStartupCode::AppRootMissing
            | FrontendStartupCode::ModuleLoadFailed
            | FrontendStartupCode::UnhandledError
            | FrontendStartupCode::UnhandledRejection => {
                matches!(event.stage, FrontendStartupStage::FrontendEntry)
            }
            FrontendStartupCode::SvelteMountFailed => {
                matches!(event.stage, FrontendStartupStage::SvelteMount)
            }
            FrontendStartupCode::InteractiveFrameTimeout
            | FrontendStartupCode::ReadySentinelMissing
            | FrontendStartupCode::ReadyHandshakeFailed => {
                matches!(event.stage, FrontendStartupStage::DomReady)
            }
            FrontendStartupCode::InitializationFailed => matches!(
                event.stage,
                FrontendStartupStage::Initialization
                    | FrontendStartupStage::Settings
                    | FrontendStartupStage::RecentFiles
                    | FrontendStartupStage::SessionRestore
                    | FrontendStartupStage::ExternalListeners
                    | FrontendStartupStage::StartupFile
                    | FrontendStartupStage::FirstRender
                    | FrontendStartupStage::DragDrop
            ),
            FrontendStartupCode::EditorMountFailed => {
                matches!(event.stage, FrontendStartupStage::EditorMount)
            }
            FrontendStartupCode::StageDegraded => false,
        },
        _ => false,
    }
}

fn diagnostic_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        if !file_name.starts_with(FILE_PREFIX) || !file_name.ends_with(FILE_SUFFIX) {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "启动诊断条目不是普通文件",
            ));
        }
        files.push(path);
    }
    files.sort();
    Ok(files)
}

fn prune_diagnostic_files(dir: &Path) -> io::Result<()> {
    let retention = Duration::from_secs(RETENTION_DAYS * 24 * 60 * 60);
    let now = SystemTime::now();
    let mut files = diagnostic_files(dir)?
        .into_iter()
        .map(|path| {
            fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .map(|modified| (path, modified))
        })
        .collect::<io::Result<Vec<_>>>()?;
    files.sort_by(|left, right| right.1.cmp(&left.1));

    for (index, (path, modified)) in files.into_iter().enumerate() {
        let expired = now
            .duration_since(modified)
            .is_ok_and(|age| age > retention);
        if expired || index >= MAX_RETAINED_FILES.saturating_sub(1) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn validate_export_target(target: &Path) -> Result<PathBuf, AppError> {
    if !target.is_absolute()
        || !target
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("json"))
    {
        return Err(AppError::invalid_startup_diagnostics_target());
    }
    if let Ok(metadata) = fs::symlink_metadata(target) {
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(AppError::invalid_startup_diagnostics_target());
        }
    }
    let parent = target
        .parent()
        .ok_or_else(AppError::invalid_startup_diagnostics_target)?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| AppError::startup_diagnostics_failed("导出", error.kind()))?;
    let file_name = target
        .file_name()
        .ok_or_else(AppError::invalid_startup_diagnostics_target)?;
    Ok(canonical_parent.join(file_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::startup::{
        FrontendStartupCode, FrontendStartupStage, FrontendStartupStatus,
    };
    use crate::utils::test_support::TestDirectory;

    #[test]
    fn records_only_typed_privacy_safe_startup_fields() {
        let directory = TestDirectory::new("startup-privacy");
        let dir = directory.path().to_path_buf();
        let state = StartupState::new_in("0.1.2", Some("151.0.0".into()), dir.clone()).unwrap();
        state
            .record_native("nativeProcess", "started", None)
            .unwrap();
        state
            .record_frontend(FrontendStartupEventDto {
                stage: FrontendStartupStage::FrontendEntry,
                status: FrontendStartupStatus::Failed,
                code: Some(FrontendStartupCode::UnhandledError),
                elapsed_ms: 12,
            })
            .unwrap();

        let content = fs::read_to_string(diagnostic_files(&dir).unwrap().pop().unwrap()).unwrap();
        assert!(content.contains("frontendEntry"));
        assert!(content.contains("unhandledError"));
        assert!(!content.contains("Users"));
        assert!(!content.contains("note.md"));
    }

    #[test]
    fn shared_schema_accepts_native_recovery_and_rejects_unknown_values() {
        let directory = TestDirectory::new("startup-shared-schema");
        let dir = directory.path().to_path_buf();
        let state = StartupState::new_in("0.1.6", None, dir.clone()).unwrap();

        state
            .record_native(
                "webviewRecovery",
                "started",
                Some("browserProcessRestartRequested"),
            )
            .unwrap();
        assert_eq!(
            state
                .record_native("inventedRecovery", "observed", None)
                .unwrap_err()
                .code,
            "INVALID_STARTUP_DIAGNOSTIC_EVENT"
        );
        assert_eq!(
            state
                .record_native("webviewRecovery", "inventedStatus", None)
                .unwrap_err()
                .code,
            "INVALID_STARTUP_DIAGNOSTIC_EVENT"
        );
        assert_eq!(
            state
                .record_native("webviewRecovery", "observed", Some("freeText"))
                .unwrap_err()
                .code,
            "INVALID_STARTUP_DIAGNOSTIC_EVENT"
        );

        let content = fs::read_to_string(diagnostic_files(&dir).unwrap().pop().unwrap()).unwrap();
        assert_eq!(content.lines().count(), 1);
        assert!(content.contains("webviewRecovery"));
    }

    #[test]
    fn ready_and_retry_are_idempotent_and_bounded() {
        let directory = TestDirectory::new("startup-state");
        let dir = directory.path().to_path_buf();
        let state = StartupState::new_in("0.1.2", None, dir.clone()).unwrap();
        assert!(!state.is_ready());
        assert!(state.claim_retry());
        assert!(!state.claim_retry());
        state.mark_ready(25).unwrap();
        state.mark_ready(30).unwrap();
        assert!(state.is_ready());
        let content = fs::read_to_string(diagnostic_files(&dir).unwrap().pop().unwrap()).unwrap();
        assert_eq!(content.matches("frontendReady").count(), 1);
    }

    #[test]
    fn rejects_inconsistent_frontend_status_code_pairs() {
        let directory = TestDirectory::new("startup-invalid-event");
        let dir = directory.path().to_path_buf();
        let state = StartupState::new_in("0.1.2", None, dir.clone()).unwrap();
        let error = state
            .record_frontend(FrontendStartupEventDto {
                stage: FrontendStartupStage::DomReady,
                status: FrontendStartupStatus::Succeeded,
                code: Some(FrontendStartupCode::UnhandledError),
                elapsed_ms: 1,
            })
            .unwrap_err();
        assert_eq!(error.code, "INVALID_STARTUP_DIAGNOSTIC_EVENT");
        assert!(diagnostic_files(&dir).unwrap().is_empty());
    }

    #[test]
    fn records_non_fatal_initialization_failures_as_degraded() {
        let directory = TestDirectory::new("startup-degraded-event");
        let dir = directory.path().to_path_buf();
        let state = StartupState::new_in("0.1.5", None, dir.clone()).unwrap();

        state
            .record_frontend(FrontendStartupEventDto {
                stage: FrontendStartupStage::RecentFiles,
                status: FrontendStartupStatus::Degraded,
                code: Some(FrontendStartupCode::StageDegraded),
                elapsed_ms: 8,
            })
            .unwrap();

        let content = fs::read_to_string(diagnostic_files(&dir).unwrap().pop().unwrap()).unwrap();
        assert!(content.contains("\"status\":\"degraded\""));
        assert!(content.contains("\"code\":\"stageDegraded\""));
    }

    #[test]
    fn exports_valid_records_and_clear_removes_only_diagnostic_files() {
        let directory = TestDirectory::new("startup-export");
        let dir = directory.path().to_path_buf();
        let state = StartupState::new_in("0.1.2", None, dir.clone()).unwrap();
        state
            .record_native("nativeProcess", "started", None)
            .unwrap();
        fs::write(dir.join("keep.txt"), "keep").unwrap();
        let export_path = dir.join("export.json");
        let result = state.export(&export_path).unwrap();
        assert_eq!(result.record_count, 1);
        let export: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(export_path).unwrap()).unwrap();
        assert_eq!(export["schemaVersion"], 1);
        assert_eq!(export["records"].as_array().unwrap().len(), 1);

        state.clear().unwrap();
        assert!(dir.join("keep.txt").exists());
        assert!(diagnostic_files(&dir).unwrap().is_empty());
    }

    #[test]
    fn pruning_keeps_a_bounded_number_of_launch_files() {
        let directory = TestDirectory::new("startup-retention");
        let dir = directory.path().to_path_buf();
        fs::create_dir_all(&dir).unwrap();
        for index in 0..(MAX_RETAINED_FILES + 5) {
            fs::write(dir.join(format!("startup-old-{index:02}.jsonl")), "{}\n").unwrap();
        }
        let _state = StartupState::new_in("0.1.2", None, dir.clone()).unwrap();
        assert!(diagnostic_files(&dir).unwrap().len() < MAX_RETAINED_FILES);
    }

    #[test]
    fn initialization_preserves_the_underlying_app_data_error_code() {
        let state = StartupState::build(
            "0.1.5",
            None,
            None,
            Some(AppError::app_data_create_failed(
                io::ErrorKind::PermissionDenied,
            )),
        );

        assert_eq!(state.clear().unwrap_err().code, "APP_DATA_CREATE_FAILED");
    }

    #[test]
    fn matching_non_file_entries_make_diagnostics_initialization_fail() {
        let directory = TestDirectory::new("startup-invalid-entry");
        let dir = directory.path().to_path_buf();
        fs::create_dir_all(dir.join("startup-invalid.jsonl")).unwrap();

        let result = StartupState::new_in("0.1.5", None, dir.clone());

        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn export_rejects_a_diagnostic_file_over_the_byte_limit() {
        let directory = TestDirectory::new("startup-oversized-export");
        let dir = directory.path().to_path_buf();
        let state = StartupState::new_in("0.1.5", None, dir.clone()).unwrap();
        state
            .record_native("nativeProcess", "started", None)
            .unwrap();
        let log = diagnostic_files(&dir).unwrap().pop().unwrap();
        OpenOptions::new()
            .write(true)
            .open(&log)
            .unwrap()
            .set_len(MAX_DIAGNOSTIC_FILE_BYTES + 1)
            .unwrap();

        let error = state.export(&dir.join("export.json")).unwrap_err();

        assert_eq!(error.code, "STARTUP_DIAGNOSTICS_FAILED");
    }

    #[test]
    fn export_rejects_malformed_records_instead_of_silently_skipping_them() {
        let directory = TestDirectory::new("startup-malformed-export");
        let dir = directory.path().to_path_buf();
        let state = StartupState::new_in("0.1.5", None, dir.clone()).unwrap();
        state
            .record_native("nativeProcess", "started", None)
            .unwrap();
        let log = diagnostic_files(&dir).unwrap().pop().unwrap();
        let mut file = OpenOptions::new().append(true).open(&log).unwrap();
        file.write_all(b"{malformed-json\n").unwrap();
        drop(file);
        let target = dir.join("export.json");

        let result = state.export(&target);
        let target_created = target.exists();

        assert_eq!(result.unwrap_err().code, "STARTUP_DIAGNOSTICS_FAILED");
        assert!(!target_created, "partial diagnostics must not be exported");
    }

    #[test]
    fn export_rejects_records_from_an_unknown_schema_version() {
        let directory = TestDirectory::new("startup-future-record");
        let dir = directory.path().to_path_buf();
        let state = StartupState::new_in("0.1.5", None, dir.clone()).unwrap();
        state
            .record_native("nativeProcess", "started", None)
            .unwrap();
        let log = diagnostic_files(&dir).unwrap().pop().unwrap();
        let content = fs::read_to_string(&log).unwrap().replacen(
            "\"schemaVersion\":1",
            "\"schemaVersion\":2",
            1,
        );
        fs::write(&log, content).unwrap();
        let target = dir.join("export.json");

        let error = state.export(&target).unwrap_err();

        assert_eq!(error.code, "STARTUP_DIAGNOSTICS_FAILED");
        assert!(!target.exists());
    }

    #[test]
    fn export_rejects_records_outside_the_shared_schema() {
        let directory = TestDirectory::new("startup-unknown-stage-export");
        let dir = directory.path().to_path_buf();
        let state = StartupState::new_in("0.1.6", None, dir.clone()).unwrap();
        state
            .record_native("nativeProcess", "started", None)
            .unwrap();
        let log = diagnostic_files(&dir).unwrap().pop().unwrap();
        let content =
            fs::read_to_string(&log)
                .unwrap()
                .replacen("\"nativeProcess\"", "\"inventedStage\"", 1);
        fs::write(&log, content).unwrap();
        let target = dir.join("export.json");

        let error = state.export(&target).unwrap_err();

        assert_eq!(error.code, "STARTUP_DIAGNOSTICS_FAILED");
        assert!(!target.exists());
    }
}
