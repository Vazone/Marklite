use super::{
    root::{Job, Root},
    stale, token,
};
use crate::{
    models::{app_error::AppError, workspace::*},
    platform::desktop::workspace::{self, Snapshot, SNAPSHOT_BYTES},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

const MAX_PAGE: usize = 256;

struct ScanGuard<'a>(&'a AtomicBool);
impl Drop for ScanGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Root {
    pub fn page(
        &self,
        relative: &str,
        cursor: Option<PageCursor>,
        request_id: &str,
        limit: usize,
    ) -> Result<WorkspacePage, AppError> {
        if limit == 0
            || limit > MAX_PAGE
            || request_id.is_empty()
            || request_id.len() > 64
            || !request_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(AppError::new(
                "WORKSPACE_INVALID_REQUEST",
                "分页大小或请求标识无效",
            ));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(index) = inner.cancelled.iter().position(|id| id == request_id) {
                inner.cancelled.remove(index);
                return Err(AppError::new("WORKSPACE_CANCELLED", "目录请求已取消"));
            }
            if inner.jobs.len() >= 8 || inner.jobs.contains_key(request_id) {
                return Err(AppError::new("WORKSPACE_BUSY", "目录请求正在执行"));
            }
            inner.jobs.insert(
                request_id.into(),
                Job {
                    relative: relative.into(),
                    cancelled: cancelled.clone(),
                },
            );
        }
        let result = (|| {
            let generation = self.expand(relative)?;
            let expected = generation.load(Ordering::Acquire);
            if cursor
                .as_ref()
                .is_some_and(|cursor| cursor.generation != expected)
            {
                return Err(stale());
            }
            let check = || {
                if cancelled.load(Ordering::Acquire) {
                    return Err(AppError::new("WORKSPACE_CANCELLED", "目录请求已取消"));
                }
                if self.closed.load(Ordering::Acquire)
                    || generation.load(Ordering::Acquire) != expected
                {
                    return Err(stale());
                }
                Ok(())
            };
            check()?;
            let snapshot = self.snapshot(relative, cursor.is_some(), &check)?;
            self.check_root()?;
            check()?;
            let offset = cursor.map_or(0, |cursor| cursor.offset);
            if offset > snapshot.entries.len() {
                return Err(AppError::new(
                    "WORKSPACE_INVALID_CURSOR",
                    "分页游标超过目录范围",
                ));
            }
            let end = offset.saturating_add(limit).min(snapshot.entries.len());
            let watch_issue = self
                .inner
                .lock()
                .unwrap()
                .nodes
                .get(relative)
                .and_then(|node| node.watch_issue.clone());
            Ok(WorkspacePage {
                root_id: self.dto.root_id.clone(),
                relative_path: relative.into(),
                generation: expected,
                entries: snapshot.entries[offset..end].to_vec(),
                next: (end < snapshot.entries.len()).then_some(PageCursor {
                    generation: expected,
                    offset: end,
                }),
                watch_issue,
            })
        })();
        self.inner.lock().unwrap().jobs.remove(request_id);
        result
    }

    fn snapshot(
        &self,
        relative: &str,
        continuation: bool,
        check: &impl Fn() -> Result<(), AppError>,
    ) -> Result<Arc<Snapshot>, AppError> {
        {
            let mut inner = self.inner.lock().unwrap();
            let node = inner.nodes.get_mut(relative).ok_or_else(stale)?;
            node.touched = token();
            if let Some(snapshot) = &node.snapshot {
                return Ok(snapshot.clone());
            }
        }
        // A continuation must use the exact sorted snapshot, never rescan against an old offset.
        if continuation {
            return Err(stale());
        }
        if self.scanning.swap(true, Ordering::AcqRel) {
            return Err(AppError::new(
                "WORKSPACE_BUSY",
                "另一个目录正在枚举，请稍后重试",
            ));
        }
        let _scan = ScanGuard(&self.scanning);
        let snapshot = Arc::new(workspace::scan(&self.path, relative, check)?);
        check()?;
        let mut inner = self.inner.lock().unwrap();
        check()?;
        let mut used: usize = inner
            .nodes
            .values()
            .filter_map(|node| node.snapshot.as_ref())
            .map(|s| s.bytes)
            .sum();
        while used + snapshot.bytes > SNAPSHOT_BYTES {
            let key = inner
                .nodes
                .iter()
                .filter(|(key, node)| key.as_str() != relative && node.snapshot.is_some())
                .min_by_key(|(_, node)| node.touched)
                .map(|(key, _)| key.clone())
                .ok_or_else(stale)?;
            let node = inner.nodes.get_mut(&key).unwrap();
            used -= node.snapshot.take().unwrap().bytes;
            node.generation.store(token(), Ordering::Release);
        }
        inner.nodes.get_mut(relative).ok_or_else(stale)?.snapshot = Some(snapshot.clone());
        Ok(snapshot)
    }
}
