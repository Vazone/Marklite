use crate::models::app_error::AppError;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Default)]
enum ControlState {
    #[default]
    Running,
    Cancelled,
    Committed,
}
#[derive(Clone, Default)]
pub(crate) struct CaptureControl(Arc<Mutex<ControlState>>);
impl CaptureControl {
    pub(crate) fn request_cancel(&self) -> crate::models::export::ExportCancelStatus {
        use crate::models::export::ExportCancelStatus;
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match *state {
            ControlState::Committed => ExportCancelStatus::TooLate,
            ControlState::Running | ControlState::Cancelled => {
                *state = ControlState::Cancelled;
                ExportCancelStatus::Requested
            }
        }
    }
    pub(crate) fn cancel(&self) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(*state, ControlState::Running) {
            *state = ControlState::Cancelled;
            true
        } else {
            false
        }
    }
    pub(crate) fn check(&self) -> Result<(), AppError> {
        if matches!(
            *self.0.lock().unwrap_or_else(|e| e.into_inner()),
            ControlState::Cancelled
        ) {
            Err(AppError::new("EXPORT_CANCELLED", "导出已取消"))
        } else {
            Ok(())
        }
    }
    pub(crate) fn commit(
        &self,
        commit: impl FnOnce() -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !matches!(*state, ControlState::Running) {
            return Err(AppError::new("EXPORT_CANCELLED", "导出已取消或完成"));
        }
        let result = commit();
        *state = if result.is_ok() {
            ControlState::Committed
        } else {
            ControlState::Cancelled
        };
        result
    }
}

fn jobs() -> &'static Mutex<HashMap<String, CaptureControl>> {
    static JOBS: OnceLock<Mutex<HashMap<String, CaptureControl>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) struct Registration {
    id: String,
    pub(crate) control: CaptureControl,
}

impl Registration {
    pub(crate) fn new(id: &str) -> Result<Self, AppError> {
        let mut jobs = jobs().lock().unwrap_or_else(|e| e.into_inner());
        if jobs.contains_key(id) {
            return Err(AppError::new("EXPORT_JOB_EXISTS", "导出任务标识已在使用"));
        }
        let control = CaptureControl::default();
        jobs.insert(id.to_owned(), control.clone());
        Ok(Self {
            id: id.to_owned(),
            control,
        })
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        self.control.cancel();
        jobs()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}

pub(crate) fn request_cancel(job_id: &str) -> crate::models::export::ExportCancelStatus {
    jobs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(job_id)
        .map(CaptureControl::request_cancel)
        .unwrap_or(crate::models::export::ExportCancelStatus::NotRunning)
}

pub(crate) fn cancel(job_id: &str) -> bool {
    jobs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(job_id)
        .is_some_and(CaptureControl::cancel)
}

pub(crate) struct CapturedImage {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}
