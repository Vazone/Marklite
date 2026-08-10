use std::{
    fs::{self, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime},
};

use chrono::Utc;

use crate::{
    models::{
        app_error::AppError,
        startup::{
            FrontendStartupCode, FrontendStartupEventDto, FrontendStartupStage,
            FrontendStartupStatus, StartupDiagnosticRecord, StartupDiagnosticsExportDto,
            StartupDiagnosticsExportFile, StartupReadyDto,
        },
    },
    utils::{atomic_write::atomic_write, path_utils::startup_diagnostics_dir},
};

const SCHEMA_VERSION: u8 = 1;
pub const RETENTION_DAYS: u64 = 14;
pub const MAX_RETAINED_FILES: usize = 128;
pub const MAX_DIAGNOSTIC_FILE_BYTES: u64 = 128 * 1024;
const FILE_PREFIX: &str = "startup-";
const FILE_SUFFIX: &str = ".jsonl";

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
    write_lock: Mutex<()>,
    ready: AtomicBool,
    retry_claimed: AtomicBool,
}

impl StartupState {
    pub fn new(app_version: &str, webview_version: Option<String>) -> Self {
        let log_dir = startup_diagnostics_dir().ok();
        Self::build(app_version, webview_version, log_dir)
    }

    fn build(app_version: &str, webview_version: Option<String>, log_dir: Option<PathBuf>) -> Self {
        let launch_id = format!(
            "{}-{}",
            Utc::now().format("%Y%m%dT%H%M%S%3fZ"),
            std::process::id()
        );
        if let Some(dir) = log_dir.as_deref() {
            let _ = prune_diagnostic_files(dir);
        }
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
            return Err(AppError::startup_diagnostics_unavailable());
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
            return Err(AppError::startup_diagnostics_unavailable());
        };
        let mut records = Vec::new();
        for path in diagnostic_files(dir)
            .map_err(|error| AppError::startup_diagnostics_failed("导出", error.kind()))?
        {
            let file = fs::File::open(path)
                .map_err(|error| AppError::startup_diagnostics_failed("导出", error.kind()))?;
            for line in BufReader::new(file).lines().map_while(Result::ok) {
                if let Ok(record) = serde_json::from_str::<StartupDiagnosticRecord>(&line) {
                    records.push(record);
                }
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
        let _guard = self
            .inner
            .write_lock
            .lock()
            .map_err(|_| AppError::startup_diagnostics_failed("写入", io::ErrorKind::Other))?;
        let Some(path) = self.inner.log_path.as_deref() else {
            return Err(AppError::startup_diagnostics_unavailable());
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
        let existing_size = fs::metadata(path).map(|value| value.len()).unwrap_or(0);
        if existing_size.saturating_add(line.len() as u64) > MAX_DIAGNOSTIC_FILE_BYTES {
            return Err(AppError::startup_diagnostics_failed(
                "写入",
                io::ErrorKind::FileTooLarge,
            ));
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|error| AppError::startup_diagnostics_failed("写入", error.kind()))?;
        file.write_all(&line)
            .and_then(|_| file.flush())
            .map_err(|error| AppError::startup_diagnostics_failed("写入", error.kind()))
    }
}

fn valid_frontend_event(event: &FrontendStartupEventDto) -> bool {
    match (event.status, event.code) {
        (FrontendStartupStatus::Started | FrontendStartupStatus::Succeeded, None) => true,
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
            FrontendStartupCode::ReadySentinelMissing
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
        },
        _ => false,
    }
}

fn diagnostic_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(FILE_PREFIX) && name.ends_with(FILE_SUFFIX))
                && fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn prune_diagnostic_files(dir: &Path) -> io::Result<()> {
    let retention = Duration::from_secs(RETENTION_DAYS * 24 * 60 * 60);
    let now = SystemTime::now();
    let mut files = diagnostic_files(dir)?
        .into_iter()
        .map(|path| {
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            (path, modified)
        })
        .collect::<Vec<_>>();
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
