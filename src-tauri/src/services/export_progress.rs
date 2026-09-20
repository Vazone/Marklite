//! Format-independent observation only. Cancellation and artifact ownership stay
//! in the export services; a missing observer never changes an export result.
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::models::export::ExportFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportStage {
    Loading,
    Snapshot,
    PreparingTarget,
    ChoosingTarget,
    Parsing,
    Resources,
    Rendering,
    Printing,
    Encoding,
    Merging,
    Validating,
    Writing,
    Committing,
    CleaningUp,
    Finalizing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProgressStatus {
    Running,
    Waiting,
    Succeeded,
    Failed,
    Cancelled,
}

impl ProgressStatus {
    fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkKind {
    Chapter,
    Part,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportWork {
    pub kind: WorkKind,
    /// One-based current processing unit; completed may still be index - 1.
    pub index: u32,
    pub total: u32,
    pub completed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub job_id: String,
    pub format: ExportFormat,
    pub sequence: u64,
    pub stage: ExportStage,
    pub status: ProgressStatus,
    pub work: Option<ExportWork>,
}

type Sink = dyn Fn(ExportProgress) + Send + Sync;

struct ReporterInner {
    job_id: String,
    format: ExportFormat,
    last: Mutex<Option<ExportProgress>>,
    sink: Box<Sink>,
}

#[derive(Clone)]
pub struct ExportReporter(Arc<ReporterInner>);

impl ExportReporter {
    /// The sink must be nonblocking and must not call back into this reporter.
    /// GUI adapters can use LatestProgress to avoid queueing every event.
    pub fn new(
        job_id: impl Into<String>,
        format: ExportFormat,
        sink: impl Fn(ExportProgress) + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(ReporterInner {
            job_id: job_id.into(),
            format,
            last: Mutex::new(None),
            sink: Box::new(sink),
        }))
    }

    pub fn silent(job_id: &str, format: ExportFormat) -> Self {
        Self::new(job_id, format, |_| {})
    }

    pub fn job_id(&self) -> &str {
        &self.0.job_id
    }

    pub fn phase(&self, stage: ExportStage) {
        self.report(stage, ProgressStatus::Running, None);
    }

    pub fn processing(&self, stage: ExportStage, work: ExportWork) {
        self.report(stage, ProgressStatus::Running, Some(work));
    }

    /// Called by the owning entry point after its result/cleanup barrier, not
    /// by a print callback or an individual chapter writer.
    pub fn finish(&self, status: ProgressStatus) {
        if status.terminal() {
            self.report(ExportStage::Finalizing, status, None);
        }
    }

    pub fn report(
        &self,
        stage: ExportStage,
        status: ProgressStatus,
        work: Option<ExportWork>,
    ) -> bool {
        if work.is_some_and(|work| {
            work.total == 0
                || work.index == 0
                || work.index > work.total
                || work.completed > work.index
        }) {
            return false;
        }
        let mut last = self.0.last.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(previous) = last.as_ref() {
            if previous.status.terminal() {
                return false;
            }
            if let (Some(before), Some(after)) = (previous.work, work) {
                if before.kind == after.kind
                    && before.total == after.total
                    && (after.index < before.index || after.completed < before.completed)
                {
                    return false;
                }
            }
            if previous.stage == stage && previous.status == status && previous.work == work {
                return false;
            }
        }
        let event = ExportProgress {
            job_id: self.0.job_id.clone(),
            format: self.0.format,
            sequence: last.as_ref().map_or(1, |last| last.sequence + 1),
            stage,
            status,
            work,
        };
        *last = Some(event.clone());
        (self.0.sink)(event);
        true
    }
}

/// One pending value regardless of producer rate. Terminal events cannot be
/// overwritten because the owning reporter closes its sequence at finish.
#[derive(Clone, Default)]
pub struct LatestProgress(Arc<Mutex<Option<ExportProgress>>>);

impl LatestProgress {
    pub fn push(&self, event: ExportProgress) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(event);
    }

    pub fn take(&self) -> Option<ExportProgress> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_event_matches_the_shared_frontend_fixture() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../../../src/shared/desktop-contract-fixtures.json"
        ))
        .unwrap();
        let event = ExportProgress {
            job_id: "export-progress-fixture".into(),
            format: ExportFormat::Png,
            sequence: 7,
            stage: ExportStage::Writing,
            status: ProgressStatus::Running,
            work: Some(ExportWork {
                kind: WorkKind::Chapter,
                index: 2,
                total: 3,
                completed: 1,
            }),
        };
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            fixtures["exportProgress"]
        );
    }

    #[test]
    fn repeated_chapter_phases_are_valid_but_counts_cannot_go_backwards() {
        let mailbox = LatestProgress::default();
        let sink = mailbox.clone();
        let reporter = ExportReporter::new("job", ExportFormat::Png, move |e| sink.push(e));
        let first = ExportWork {
            kind: WorkKind::Chapter,
            index: 1,
            total: 2,
            completed: 0,
        };
        reporter.processing(ExportStage::Rendering, first);
        reporter.processing(
            ExportStage::Writing,
            ExportWork {
                completed: 1,
                ..first
            },
        );
        assert!(!reporter.report(ExportStage::Rendering, ProgressStatus::Running, Some(first)));
        reporter.processing(
            ExportStage::Rendering,
            ExportWork {
                index: 2,
                completed: 1,
                ..first
            },
        );
        let event = mailbox.take().unwrap();
        assert_eq!(event.sequence, 3);
        assert_eq!(event.work.unwrap().index, 2);
        assert_eq!(event.status, ProgressStatus::Running);
    }

    #[test]
    fn a_finished_processing_unit_does_not_finish_the_job() {
        let mailbox = LatestProgress::default();
        let sink = mailbox.clone();
        let reporter = ExportReporter::new("job", ExportFormat::Pdf, move |e| sink.push(e));
        reporter.processing(
            ExportStage::Printing,
            ExportWork {
                kind: WorkKind::Part,
                index: 1,
                total: 1,
                completed: 1,
            },
        );
        assert_eq!(mailbox.take().unwrap().status, ProgressStatus::Running);
        reporter.phase(ExportStage::Merging);
        reporter.phase(ExportStage::Committing);
        reporter.phase(ExportStage::CleaningUp);
        reporter.finish(ProgressStatus::Succeeded);
        reporter.phase(ExportStage::Rendering);
        reporter.finish(ProgressStatus::Failed);
        assert_eq!(mailbox.take().unwrap().status, ProgressStatus::Succeeded);
        assert!(mailbox.take().is_none());
    }

    #[test]
    fn high_rate_reporting_keeps_only_one_pending_event() {
        let mailbox = LatestProgress::default();
        let sink = mailbox.clone();
        let reporter = ExportReporter::new("job", ExportFormat::Png, move |e| sink.push(e));
        for index in 1..=10_000 {
            reporter.processing(
                ExportStage::Encoding,
                ExportWork {
                    kind: WorkKind::Chapter,
                    index,
                    total: 10_000,
                    completed: index - 1,
                },
            );
        }
        reporter.finish(ProgressStatus::Cancelled);
        let event = mailbox.take().unwrap();
        assert_eq!(event.sequence, 10_001);
        assert_eq!(event.status, ProgressStatus::Cancelled);
        assert!(mailbox.take().is_none());
    }
}
