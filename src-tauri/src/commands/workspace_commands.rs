use crate::{
    commands::background::run_background,
    models::{app_error::AppError, resource::ResourceRef, workspace::*},
    services::workspace_service::WorkspaceService,
};
use std::sync::Arc;
use tauri::{Emitter, State};

type Workspace<'a> = State<'a, Arc<WorkspaceService>>;

#[tauri::command]
pub async fn get_workspace_preferences() -> Result<WorkspacePreferences, AppError> {
    run_background(
        "读取工作区位置",
        crate::services::workspace_preferences::load,
    )
    .await
}

#[tauri::command]
pub async fn save_workspace_preferences(
    preferences: WorkspacePreferences,
) -> Result<WorkspacePreferences, AppError> {
    run_background("保存工作区位置", move || {
        crate::services::workspace_preferences::save(preferences)
    })
    .await
}

#[tauri::command]
pub async fn mount_workspace(
    app: tauri::AppHandle,
    state: Workspace<'_>,
    resource: ResourceRef,
) -> Result<WorkspaceRoot, AppError> {
    let service = state.inner().clone();
    run_background("挂载目录", move || {
        service.mount(
            resource,
            Arc::new(move |event| {
                if let Err(error) = app.emit("workspace-invalidated", event) {
                    eprintln!("workspace invalidation delivery failed: {error}");
                }
            }),
        )
    })
    .await
}

#[tauri::command]
pub async fn unmount_workspace(state: Workspace<'_>, root_id: String) -> Result<(), AppError> {
    let service = state.inner().clone();
    run_background("卸载目录", move || service.unmount(&root_id)).await
}

#[tauri::command]
pub async fn list_workspace(
    state: Workspace<'_>,
    root_id: String,
    relative_path: String,
    cursor: Option<PageCursor>,
    request_id: String,
    limit: usize,
) -> Result<WorkspacePage, AppError> {
    let service = state.inner().clone();
    run_background("枚举目录", move || {
        service.page(&root_id, &relative_path, cursor, &request_id, limit)
    })
    .await
}

#[tauri::command]
pub fn cancel_workspace(
    state: Workspace<'_>,
    root_id: String,
    request_id: String,
) -> Result<(), AppError> {
    state.cancel(&root_id, &request_id)
}

#[tauri::command]
pub async fn refresh_workspace(
    state: Workspace<'_>,
    root_id: String,
    relative_path: String,
) -> Result<DirectoryRevision, AppError> {
    let service = state.inner().clone();
    run_background("刷新目录", move || {
        service.refresh(&root_id, &relative_path)
    })
    .await
}

#[tauri::command]
pub async fn collapse_workspace(
    state: Workspace<'_>,
    root_id: String,
    relative_path: String,
) -> Result<(), AppError> {
    let service = state.inner().clone();
    run_background("收起目录", move || {
        service.collapse(&root_id, &relative_path)
    })
    .await
}

#[tauri::command]
pub async fn resolve_workspace_file(
    state: Workspace<'_>,
    root_id: String,
    relative_path: String,
) -> Result<ResourceRef, AppError> {
    let service = state.inner().clone();
    run_background("定位工作区文档", move || {
        service.resolve(&root_id, &relative_path)
    })
    .await
}
