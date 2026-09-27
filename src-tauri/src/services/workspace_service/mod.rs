mod paging;
mod root;

use crate::{
    models::{app_error::AppError, resource::ResourceRef, workspace::*},
    platform::desktop::workspace,
    utils::path_utils::path_to_utf8,
};
use root::Root;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

pub type ChangeSink = Arc<dyn Fn(WorkspaceInvalidation) + Send + Sync>;
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
fn token() -> u64 {
    NEXT_TOKEN.fetch_add(1, Ordering::Relaxed)
}
fn stale() -> AppError {
    AppError::new("WORKSPACE_STALE", "工作区或目录已变化，请刷新")
}

#[derive(Default)]
pub struct WorkspaceService {
    mount_revision: AtomicU64,
    current: Mutex<Option<Arc<Root>>>,
}

impl WorkspaceService {
    pub fn mount(
        &self,
        resource: ResourceRef,
        sink: ChangeSink,
    ) -> Result<WorkspaceRoot, AppError> {
        let revision = self.mount_revision.fetch_add(1, Ordering::AcqRel) + 1;
        let path = workspace::root_path(&resource)?;
        let dto = WorkspaceRoot {
            root_id: format!("workspace-{}", token()),
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            resource: ResourceRef::DesktopDirectory {
                path: path_to_utf8(&path)?.into(),
            },
        };
        let root = Root::new(path, dto.clone(), sink)?;
        let mut current = self.current.lock().unwrap();
        if self.mount_revision.load(Ordering::Acquire) != revision {
            return Err(stale());
        }
        if let Some(previous) = current.replace(root) {
            previous.close();
        }
        Ok(dto)
    }

    pub fn unmount(&self, root_id: &str) -> Result<(), AppError> {
        let mut current = self.current.lock().unwrap();
        if current
            .as_ref()
            .is_none_or(|root| root.dto.root_id != root_id)
        {
            return Err(stale());
        }
        self.mount_revision.fetch_add(1, Ordering::AcqRel);
        if let Some(root) = current.take() {
            root.close();
        }
        Ok(())
    }

    pub fn page(
        &self,
        root_id: &str,
        relative: &str,
        cursor: Option<PageCursor>,
        request_id: &str,
        limit: usize,
    ) -> Result<WorkspacePage, AppError> {
        let root = self.root(root_id)?;
        root.page(relative, cursor, request_id, limit)
    }

    pub fn cancel(&self, root_id: &str, request_id: &str) -> Result<(), AppError> {
        if request_id.is_empty()
            || request_id.len() > 64
            || !request_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(AppError::new(
                "WORKSPACE_INVALID_REQUEST",
                "取消请求标识无效",
            ));
        }
        self.root(root_id)?.cancel(request_id);
        Ok(())
    }

    pub fn resolve(&self, root_id: &str, relative: &str) -> Result<ResourceRef, AppError> {
        let root = self.root(root_id)?;
        root.check_root()?;
        let resource = workspace::file_resource(&root.path, relative)?;
        root.check_root()?;
        Ok(resource)
    }

    pub fn refresh(&self, root_id: &str, relative: &str) -> Result<DirectoryRevision, AppError> {
        self.root(root_id)?.refresh(relative)
    }

    pub fn collapse(&self, root_id: &str, relative: &str) -> Result<(), AppError> {
        self.root(root_id)?.collapse(relative);
        Ok(())
    }

    fn root(&self, root_id: &str) -> Result<Arc<Root>, AppError> {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .filter(|root| root.dto.root_id == root_id)
            .cloned()
            .ok_or_else(stale)
    }
}

#[cfg(test)]
mod tests;
