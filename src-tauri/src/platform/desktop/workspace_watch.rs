use crate::models::app_error::AppError;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

pub struct ChangeBatch {
    pub paths: BTreeSet<PathBuf>,
    pub overflow: bool,
}

pub struct DirectoryWatch {
    watcher: RecommendedWatcher,
    stopped: Arc<AtomicBool>,
}

impl DirectoryWatch {
    pub fn new(on_change: impl Fn(ChangeBatch) + Send + 'static) -> Result<Self, AppError> {
        // Callback producers never block on UI/IPC. Queue overflow invalidates all nodes.
        let (sender, receiver) = mpsc::sync_channel::<notify::Result<notify::Event>>(128);
        let overflow = Arc::new(AtomicBool::new(false));
        let producer_overflow = overflow.clone();
        let watcher = RecommendedWatcher::new(
            move |event| {
                if matches!(sender.try_send(event), Err(mpsc::TrySendError::Full(_))) {
                    producer_overflow.store(true, Ordering::Release);
                }
            },
            Config::default().with_follow_symlinks(false),
        )
        .map_err(watch_error)?;
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_stopped = stopped.clone();
        std::thread::Builder::new()
            .name("workspace-watch".into())
            .spawn(move || {
                let interval = Duration::from_millis(150);
                let mut deadline = Instant::now() + interval;
                let mut batch = ChangeBatch {
                    paths: BTreeSet::new(),
                    overflow: false,
                };
                while !worker_stopped.load(Ordering::Acquire) {
                    match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    {
                        Ok(Ok(event)) => {
                            if event.need_rescan() {
                                batch.overflow = true;
                            }
                            if !matches!(event.kind, EventKind::Access(_)) && !batch.overflow {
                                batch.paths.extend(event.paths);
                                if batch.paths.len() > 512 {
                                    batch.overflow = true;
                                }
                            }
                        }
                        Ok(Err(_)) => batch.overflow = true,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    batch.overflow |= overflow.swap(false, Ordering::AcqRel);
                    if batch.overflow {
                        batch.paths.clear();
                    }
                    if Instant::now() >= deadline {
                        if batch.overflow || !batch.paths.is_empty() {
                            on_change(batch);
                            batch = ChangeBatch {
                                paths: BTreeSet::new(),
                                overflow: false,
                            };
                        }
                        deadline = Instant::now() + interval;
                    }
                }
            })
            .map_err(|error| AppError::new("WORKSPACE_WATCH_UNAVAILABLE", error.to_string()))?;
        Ok(Self { watcher, stopped })
    }

    pub fn watch(&mut self, path: &Path) -> Result<(), AppError> {
        self.watcher
            .watch(path, RecursiveMode::NonRecursive)
            .map_err(watch_error)
    }

    pub fn unwatch(&mut self, path: &Path) {
        // Deletion may already have removed the native watch; explicit refresh reports absence.
        let _ = self.watcher.unwatch(path);
    }
}

impl Drop for DirectoryWatch {
    fn drop(&mut self) {
        // No join while the caller might hold state used by on_change; worker exits within 150ms.
        self.stopped.store(true, Ordering::Release);
    }
}

fn watch_error(error: notify::Error) -> AppError {
    AppError::new(
        "WORKSPACE_WATCH_UNAVAILABLE",
        format!("目录变化通知不可用，请手动刷新：{error}"),
    )
}
