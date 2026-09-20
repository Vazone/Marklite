//! Shared bounded delivery for desktop, terminal and native Shell presenters.
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use super::export_progress::{ExportProgress, ExportReporter, LatestProgress};
use crate::models::export::ExportFormat;

pub struct ProgressPump {
    pub reporter: ExportReporter,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ProgressPump {
    /// Presenter creation, use and destruction all happen on the delivery thread.
    /// This also permits native UI/COM presenters with thread-affine ownership.
    pub fn new<F, D>(job_id: &str, format: ExportFormat, create: F) -> Self
    where
        F: FnOnce() -> D + Send + 'static,
        D: FnMut(ExportProgress) + 'static,
    {
        let mailbox = LatestProgress::default();
        let sink = mailbox.clone();
        let reporter = ExportReporter::new(job_id, format, move |event| sink.push(event));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("export-progress".into())
            .spawn(move || {
                let mut display = create();
                loop {
                    thread::park_timeout(Duration::from_millis(50));
                    let stopping = worker_stop.load(Ordering::Acquire);
                    if let Some(event) = mailbox.take() {
                        display(event);
                    }
                    if stopping {
                        break;
                    }
                }
            })
            .ok();
        Self {
            reporter,
            stop,
            worker,
        }
    }
}

impl Drop for ProgressPump {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::export_progress::{ExportStage, ProgressStatus};
    #[test]
    fn finishing_flushes_the_latest_terminal_event_without_an_animation_delay() {
        let (tx, rx) = std::sync::mpsc::channel();
        let pump = ProgressPump::new("job", ExportFormat::Html, move || {
            move |event| {
                let _ = tx.send(event);
            }
        });
        pump.reporter.phase(ExportStage::Rendering);
        pump.reporter.finish(ProgressStatus::Succeeded);
        drop(pump);
        assert_eq!(
            rx.into_iter().last().unwrap().status,
            ProgressStatus::Succeeded
        );
    }
}
