use crate::{
    commands::background::run_background,
    models::{
        app_error::AppError,
        navigation::MarkdownTargetDto,
        resource::ResourceRef,
        workspace::{
            DirectoryRevision, PageCursor, WorkspacePage, WorkspacePreferences, WorkspaceRoot,
        },
    },
    platform::android::workspace::AndroidWorkspaceState,
    services::{image_load_service::ImageLoadState, local_image_protocol, workspace_preferences},
};
use std::sync::Arc;
use tauri::{AppHandle, State};

type Workspace<'a> = State<'a, Arc<AndroidWorkspaceState>>;

#[tauri::command]
pub async fn get_android_workspace_preferences() -> Result<WorkspacePreferences, AppError> {
    run_background("读取 Android 工作区位置", workspace_preferences::load).await
}

#[tauri::command]
pub async fn save_android_workspace_preferences(
    preferences: WorkspacePreferences,
) -> Result<WorkspacePreferences, AppError> {
    run_background("保存 Android 工作区位置", move || {
        workspace_preferences::save(preferences)
    })
    .await
}

#[tauri::command]
pub async fn mount_android_workspace(
    app: AppHandle,
    state: Workspace<'_>,
    resource: ResourceRef,
) -> Result<WorkspaceRoot, AppError> {
    let service = state.inner().clone();
    run_background("挂载 Android 目录", move || {
        service.mount(&app, resource)
    })
    .await
}

#[tauri::command]
pub async fn unmount_android_workspace(
    state: Workspace<'_>,
    root_id: String,
) -> Result<(), AppError> {
    state.unmount(&root_id)
}

#[tauri::command]
pub async fn list_android_workspace(
    app: AppHandle,
    state: Workspace<'_>,
    root_id: String,
    relative_path: String,
    cursor: Option<PageCursor>,
    request_id: String,
    limit: usize,
) -> Result<WorkspacePage, AppError> {
    let service = state.inner().clone();
    run_background("枚举 Android 目录", move || {
        service.page(&app, &root_id, &relative_path, cursor, &request_id, limit)
    })
    .await
}

#[tauri::command]
pub fn cancel_android_workspace(
    state: Workspace<'_>,
    root_id: String,
    request_id: String,
) -> Result<(), AppError> {
    state.cancel(&root_id, &request_id)
}

#[tauri::command]
pub fn refresh_android_workspace(
    state: Workspace<'_>,
    root_id: String,
    relative_path: String,
) -> Result<DirectoryRevision, AppError> {
    state.refresh(&root_id, &relative_path)
}

#[tauri::command]
pub fn collapse_android_workspace(
    state: Workspace<'_>,
    root_id: String,
    relative_path: String,
) -> Result<(), AppError> {
    state.collapse(&root_id, &relative_path)
}

#[tauri::command]
pub async fn resolve_android_workspace_file(
    app: AppHandle,
    state: Workspace<'_>,
    root_id: String,
    relative_path: String,
) -> Result<ResourceRef, AppError> {
    let service = state.inner().clone();
    run_background("定位 Android 目录文档", move || {
        service.resolve(&app, &root_id, &relative_path)
    })
    .await
}

#[tauri::command]
pub async fn resolve_android_markdown_target(
    app: AppHandle,
    state: Workspace<'_>,
    source: Option<ResourceRef>,
    target: String,
) -> Result<MarkdownTargetDto, AppError> {
    let uri = match source {
        Some(ResourceRef::AndroidDocument { uri }) => Some(uri),
        None => None,
        _ => return Err(AppError::new("RESOURCE_UNSUPPORTED", "需要 Android 文档")),
    };
    let service = state.inner().clone();
    run_background("解析 Android 文档链接", move || {
        service.resolve_link(&app, uri.as_deref(), &target)
    })
    .await
}

#[tauri::command]
pub async fn load_android_images(
    app: AppHandle,
    state: Workspace<'_>,
    image_state: State<'_, ImageLoadState>,
    source: Option<ResourceRef>,
    targets: Vec<String>,
    job_id: String,
) -> Result<tauri::ipc::Response, AppError> {
    let uri = match source {
        Some(ResourceRef::AndroidDocument { uri }) => Some(uri),
        None => None,
        _ => return Err(AppError::new("RESOURCE_UNSUPPORTED", "需要 Android 文档")),
    };
    let lease = image_state.acquire(&job_id)?;
    let service = state.inner().clone();
    let batch = run_background("加载 Android 图片", move || {
        service.load_images(&app, uri.as_deref(), &targets, &lease)
    })
    .await?;
    Ok(tauri::ipc::Response::new(
        local_image_protocol::encode_response(&batch)?,
    ))
}
