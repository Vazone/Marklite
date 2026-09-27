use super::{stale, token, ChangeSink};
use crate::{
    models::{app_error::AppError, workspace::*},
    platform::desktop::{
        workspace::{self, Snapshot},
        workspace_watch::{ChangeBatch, DirectoryWatch},
    },
};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

pub(super) struct Node {
    pub path: PathBuf,
    pub generation: Arc<AtomicU64>,
    pub snapshot: Option<Arc<Snapshot>>,
    pub touched: u64,
    pub watch_issue: Option<AppError>,
}

pub(super) struct Job {
    pub relative: String,
    pub cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub(super) struct Inner {
    pub nodes: HashMap<String, Node>,
    pub jobs: HashMap<String, Job>,
    pub cancelled: VecDeque<String>,
}

pub(super) struct Root {
    pub path: PathBuf,
    pub identity: String,
    pub dto: WorkspaceRoot,
    pub inner: Mutex<Inner>,
    pub closed: AtomicBool,
    pub scanning: AtomicBool,
    watch: Mutex<Result<DirectoryWatch, AppError>>,
    sink: ChangeSink,
}

impl Root {
    pub fn new(path: PathBuf, dto: WorkspaceRoot, sink: ChangeSink) -> Result<Arc<Self>, AppError> {
        let identity = workspace::identity(&path)?;
        Ok(Arc::new_cyclic(|weak: &std::sync::Weak<Root>| {
            let weak = weak.clone();
            let watch = DirectoryWatch::new(move |batch| {
                if let Some(root) = weak.upgrade() {
                    root.invalidate(batch);
                }
            });
            Self {
                path,
                identity,
                dto,
                inner: Mutex::default(),
                closed: AtomicBool::new(false),
                scanning: AtomicBool::new(false),
                watch: Mutex::new(watch),
                sink,
            }
        }))
    }

    pub fn check_root(&self) -> Result<(), AppError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(stale());
        }
        let canonical = workspace::directory(&self.path, "")?;
        if canonical != self.path || workspace::identity(&canonical)? != self.identity {
            return Err(stale());
        }
        Ok(())
    }

    pub fn expand(&self, relative: &str) -> Result<Arc<AtomicU64>, AppError> {
        self.check_root()?;
        let path = workspace::directory(&self.path, relative)?;
        let mut inner = self.inner.lock().unwrap();
        if let Some(node) = inner.nodes.get(relative) {
            return Ok(node.generation.clone());
        }
        if inner.nodes.len() >= 256 {
            return Err(AppError::new(
                "WORKSPACE_EXPANSION_LIMIT",
                "展开目录过多，请先收起部分目录",
            ));
        }
        let issue = match self.watch.lock().unwrap().as_mut() {
            Ok(watch) => watch.watch(&path).err(),
            Err(error) => Some(error.clone()),
        };
        let generation = Arc::new(AtomicU64::new(token()));
        inner.nodes.insert(
            relative.into(),
            Node {
                path,
                generation: generation.clone(),
                snapshot: None,
                touched: token(),
                watch_issue: issue,
            },
        );
        Ok(generation)
    }

    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        let mut inner = self.inner.lock().unwrap();
        for job in inner.jobs.values() {
            job.cancelled.store(true, Ordering::Release);
        }
        inner.nodes.clear();
        // Drop native handles now even if an in-flight page still holds this Root.
        *self.watch.lock().unwrap() = Err(stale());
    }

    pub fn cancel(&self, request_id: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(job) = inner.jobs.get(request_id) {
            job.cancelled.store(true, Ordering::Release);
        } else if !inner.cancelled.iter().any(|id| id == request_id) {
            // Cancellation can arrive before spawn_blocking starts the matching enumeration.
            if inner.cancelled.len() == 64 {
                inner.cancelled.pop_front();
            }
            inner.cancelled.push_back(request_id.into());
        }
    }

    pub fn collapse(&self, relative: &str) {
        let mut inner = self.inner.lock().unwrap();
        let descendants = |value: &str| {
            relative.is_empty()
                || value == relative
                || value
                    .strip_prefix(relative)
                    .is_some_and(|tail| tail.starts_with('/'))
        };
        for job in inner.jobs.values().filter(|job| descendants(&job.relative)) {
            job.cancelled.store(true, Ordering::Release);
        }
        let keys: Vec<_> = inner
            .nodes
            .keys()
            .filter(|key| descendants(key))
            .cloned()
            .collect();
        for key in keys {
            if let Some(node) = inner.nodes.remove(&key) {
                node.generation.store(token(), Ordering::Release);
                if let Ok(watch) = self.watch.lock().unwrap().as_mut() {
                    watch.unwatch(&node.path);
                }
            }
        }
    }

    pub fn refresh(&self, relative: &str) -> Result<DirectoryRevision, AppError> {
        self.expand(relative)?;
        let mut inner = self.inner.lock().unwrap();
        let node = inner.nodes.get_mut(relative).ok_or_else(stale)?;
        let generation = token();
        node.generation.store(generation, Ordering::Release);
        node.snapshot = None;
        // Retry an unavailable watch only on user refresh, never in a loop.
        if node.watch_issue.is_some() {
            if let Ok(watch) = self.watch.lock().unwrap().as_mut() {
                node.watch_issue = watch.watch(&node.path).err();
            }
        }
        Ok(DirectoryRevision {
            relative_path: relative.into(),
            generation,
        })
    }

    pub(super) fn invalidate(&self, batch: ChangeBatch) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        let mut directories = Vec::new();
        for (relative, node) in &mut inner.nodes {
            if batch.overflow
                || batch
                    .paths
                    .iter()
                    .any(|path| path == &node.path || path.parent() == Some(node.path.as_path()))
            {
                let generation = token();
                node.generation.store(generation, Ordering::Release);
                node.snapshot = None;
                directories.push(DirectoryRevision {
                    relative_path: relative.clone(),
                    generation,
                });
            }
        }
        drop(inner);
        if !directories.is_empty() && !self.closed.load(Ordering::Acquire) {
            (self.sink)(WorkspaceInvalidation {
                root_id: self.dto.root_id.clone(),
                directories,
                overflow: batch.overflow,
            });
        }
    }
}
