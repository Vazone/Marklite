use tauri::Emitter;

use super::export_progress::{ExportReporter, ProgressStatus};
use super::export_progress_pump::ProgressPump;
use crate::models::{
    app_error::AppError,
    export::{ExportRequest, ExportResult},
};

pub struct GuiExportProgress {
    pub reporter: ExportReporter,
    _pump: ProgressPump,
}

impl GuiExportProgress {
    pub fn new(app: tauri::AppHandle, window_label: String, request: &ExportRequest) -> Self {
        let pump = ProgressPump::new(&request.snapshot.job_id, request.format, move || {
            move |event| {
                let _ = app.emit_to(&window_label, "document-export-progress", event);
            }
        });
        Self {
            reporter: pump.reporter.clone(),
            _pump: pump,
        }
    }

    pub fn finish(&self, result: &Result<ExportResult, AppError>) {
        self.reporter.finish(match result {
            Ok(_) => ProgressStatus::Succeeded,
            Err(error)
                if matches!(
                    error.code.as_str(),
                    "EXPORT_CANCELLED"
                        | "PDF_EXPORT_CANCELLED"
                        | "PDF_PRINT_CANCELLED"
                        | "DIAGRAM_CANCELLED"
                ) =>
            {
                ProgressStatus::Cancelled
            }
            Err(_) => ProgressStatus::Failed,
        });
    }
}
