use std::{
    sync::{Arc, Condvar, Mutex, Weak},
    time::Duration,
};

#[cfg(any(target_os = "macos", test))]
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::models::app_error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionDisposition {
    Accepted,
    Duplicate,
    Late,
    Orphaned,
}

#[derive(Clone)]
pub struct PdfPlatformJob {
    inner: Arc<JobInner>,
}

#[derive(Clone)]
pub struct PdfPlatformCompletion {
    inner: Weak<JobInner>,
}

#[cfg(any(target_os = "macos", test))]
pub struct PdfPlatformRegistry {
    next_id: AtomicU64,
    completions: Mutex<HashMap<u64, PdfPlatformCompletion>>,
}

struct JobInner {
    state: Mutex<JobState>,
    terminal: Condvar,
}

enum JobState {
    Pending,
    Completed(Result<(), AppError>),
    Cancelled(AppError),
}

impl PdfPlatformJob {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(JobInner {
                state: Mutex::new(JobState::Pending),
                terminal: Condvar::new(),
            }),
        }
    }

    pub fn completion(&self) -> PdfPlatformCompletion {
        PdfPlatformCompletion {
            inner: Arc::downgrade(&self.inner),
        }
    }

    pub fn cancel(&self, error: AppError) -> bool {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let cancelled = cancel_pending(&mut state, error);
        if cancelled {
            self.inner.terminal.notify_all();
        }
        cancelled
    }

    #[cfg(test)]
    pub fn wait(&self, timeout: Duration, timeout_error: AppError) -> Result<(), AppError> {
        self.wait_controlled(timeout, timeout_error, || None)
    }

    pub fn wait_controlled(
        &self,
        timeout: Duration,
        timeout_error: AppError,
        cancel_error: impl Fn() -> Option<AppError>,
    ) -> Result<(), AppError> {
        // The three native printing APIs expose completion callbacks but no
        // documented, safe cancellation entry point for an operation already
        // running. Cancellation here ends MarkLite's ownership immediately;
        // native callbacks are weak and cannot resurrect a terminal job.
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let deadline = std::time::Instant::now().checked_add(timeout);
        let mut state = state;
        while matches!(*state, JobState::Pending) {
            if let Some(error) = cancel_error() {
                cancel_pending(&mut state, error.clone());
                self.inner.terminal.notify_all();
                return Err(error);
            }
            let remaining = deadline
                .map(|deadline| deadline.saturating_duration_since(std::time::Instant::now()))
                .unwrap_or_default();
            if remaining.is_zero() {
                cancel_pending(&mut state, timeout_error.clone());
                self.inner.terminal.notify_all();
                return Err(timeout_error);
            }
            let (next_state, _) = self
                .inner
                .terminal
                .wait_timeout(state, remaining.min(Duration::from_millis(25)))
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next_state;
        }
        match &*state {
            JobState::Completed(result) => result.clone(),
            JobState::Cancelled(error) => Err(error.clone()),
            JobState::Pending => Err(AppError::new(
                "PDF_PRINT_FAILED",
                "PDF 平台任务在非终态结束等待",
            )),
        }
    }
}

fn cancel_pending(state: &mut JobState, error: AppError) -> bool {
    if !matches!(*state, JobState::Pending) {
        return false;
    }
    *state = JobState::Cancelled(error);
    true
}

impl PdfPlatformCompletion {
    pub fn complete(&self, result: Result<(), AppError>) -> CompletionDisposition {
        let Some(inner) = self.inner.upgrade() else {
            return CompletionDisposition::Orphaned;
        };
        let mut state = inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match &*state {
            JobState::Pending => {
                *state = JobState::Completed(result);
                inner.terminal.notify_all();
                CompletionDisposition::Accepted
            }
            JobState::Completed(_) => CompletionDisposition::Duplicate,
            JobState::Cancelled(_) => CompletionDisposition::Late,
        }
    }
}

#[cfg(any(target_os = "macos", test))]
impl PdfPlatformRegistry {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            completions: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, completion: PdfPlatformCompletion) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.completions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id, completion);
        id
    }

    pub fn complete(&self, id: u64, result: Result<(), AppError>) -> CompletionDisposition {
        let completion = self
            .completions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&id);
        completion.map_or(CompletionDisposition::Orphaned, |completion| {
            completion.complete(result)
        })
    }

    pub fn unregister(&self, id: u64) {
        self.completions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&id);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.completions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::{CompletionDisposition, PdfPlatformJob, PdfPlatformRegistry};
    use crate::models::app_error::AppError;

    fn error(code: &str) -> AppError {
        AppError::new(code, code)
    }

    #[test]
    fn success_and_failure_complete_the_waiter() {
        let success = PdfPlatformJob::new();
        assert_eq!(
            success.completion().complete(Ok(())),
            CompletionDisposition::Accepted
        );
        assert!(success
            .wait(Duration::from_secs(1), error("TIMEOUT"))
            .is_ok());

        let failure = PdfPlatformJob::new();
        assert_eq!(
            failure.completion().complete(Err(error("NATIVE_FAILED"))),
            CompletionDisposition::Accepted
        );
        assert_eq!(
            failure
                .wait(Duration::from_secs(1), error("TIMEOUT"))
                .unwrap_err()
                .code,
            "NATIVE_FAILED"
        );
    }

    #[test]
    fn cancellation_is_terminal_and_wakes_a_waiter() {
        let job = PdfPlatformJob::new();
        let waiting = job.clone();
        let handle = thread::spawn(move || waiting.wait(Duration::from_secs(5), error("TIMEOUT")));

        assert!(job.cancel(error("CANCELLED")));
        assert_eq!(handle.join().unwrap().unwrap_err().code, "CANCELLED");
        assert!(!job.cancel(error("SECOND_CANCEL")));
    }

    #[test]
    fn controlled_wait_adopts_an_external_terminal_error() {
        let job = PdfPlatformJob::new();
        let late = job.completion();

        let error = job
            .wait_controlled(Duration::from_secs(30), error("TIMEOUT"), || {
                Some(error("CLI_CANCELLED"))
            })
            .unwrap_err();

        assert_eq!(error.code, "CLI_CANCELLED");
        assert_eq!(late.complete(Ok(())), CompletionDisposition::Late);
    }

    #[test]
    fn duplicate_and_late_callbacks_cannot_change_the_terminal_result() {
        let completed = PdfPlatformJob::new();
        let completion = completed.completion();
        assert_eq!(completion.complete(Ok(())), CompletionDisposition::Accepted);
        assert_eq!(
            completion.complete(Err(error("LATE_FAILURE"))),
            CompletionDisposition::Duplicate
        );
        assert!(completed
            .wait(Duration::from_secs(1), error("TIMEOUT"))
            .is_ok());

        let timed_out = PdfPlatformJob::new();
        let late = timed_out.completion();
        assert_eq!(
            timed_out
                .wait(Duration::ZERO, error("TIMEOUT"))
                .unwrap_err()
                .code,
            "TIMEOUT"
        );
        assert_eq!(late.complete(Ok(())), CompletionDisposition::Late);
        assert_eq!(
            timed_out
                .wait(Duration::from_secs(1), error("OTHER"))
                .unwrap_err()
                .code,
            "TIMEOUT"
        );
    }

    #[test]
    fn callback_does_not_retain_an_abandoned_job() {
        let completion = {
            let job = PdfPlatformJob::new();
            job.completion()
        };

        assert_eq!(completion.complete(Ok(())), CompletionDisposition::Orphaned);
    }

    #[test]
    fn dropping_a_wait_owner_can_cancel_without_waiting_for_the_timeout() {
        let job = PdfPlatformJob::new();
        let waiting = job.clone();
        let late = job.completion();
        let handle = thread::spawn(move || waiting.wait(Duration::from_secs(30), error("TIMEOUT")));

        assert!(job.cancel(error("PDF_PRINT_CANCELLED")));
        assert_eq!(
            handle.join().unwrap().unwrap_err().code,
            "PDF_PRINT_CANCELLED"
        );
        assert_eq!(late.complete(Ok(())), CompletionDisposition::Late);
    }

    #[test]
    fn callback_registry_removes_completed_and_unregistered_jobs() {
        let registry = PdfPlatformRegistry::new();
        let completed = PdfPlatformJob::new();
        let completed_id = registry.register(completed.completion());
        assert_eq!(registry.len(), 1);

        assert_eq!(
            registry.complete(completed_id, Ok(())),
            CompletionDisposition::Accepted
        );
        assert_eq!(registry.len(), 0);
        assert_eq!(
            registry.complete(completed_id, Ok(())),
            CompletionDisposition::Orphaned
        );

        let abandoned = PdfPlatformJob::new();
        let abandoned_id = registry.register(abandoned.completion());
        registry.unregister(abandoned_id);
        assert_eq!(registry.len(), 0);
        assert_eq!(
            registry.complete(abandoned_id, Ok(())),
            CompletionDisposition::Orphaned
        );
    }
}
