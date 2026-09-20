use crate::{
    models::{
        app_error::AppError,
        diagram::{DiagramRuntimeAsset, DiagramRuntimeStatus, RenderedDiagram},
    },
    services::{diagram_runtime_service, diagram_service},
    utils::path_utils::app_data_dir,
};

#[tauri::command]
pub async fn validate_diagram_svg(diagram: RenderedDiagram) -> Result<RenderedDiagram, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        diagram_service::validate_rendered_diagram(diagram)
    })
    .await
    .map_err(|error| AppError::background_task_failed("验证 Mermaid SVG", error))?
}

#[tauri::command]
pub async fn get_diagram_runtime_status() -> Result<DiagramRuntimeStatus, AppError> {
    let root = app_data_dir()?;
    tauri::async_runtime::spawn_blocking(move || diagram_runtime_service::status(&root))
        .await
        .map_err(|error| AppError::background_task_failed("读取 Mermaid runtime 状态", error))
}

#[tauri::command]
pub async fn load_diagram_runtime() -> Result<DiagramRuntimeAsset, AppError> {
    let root = app_data_dir()?;
    tauri::async_runtime::spawn_blocking(move || diagram_runtime_service::load_from_root(&root))
        .await
        .map_err(|error| AppError::background_task_failed("加载 Mermaid runtime", error))?
}

#[tauri::command]
pub async fn install_diagram_runtime(pack_path: String) -> Result<DiagramRuntimeStatus, AppError> {
    let root = app_data_dir()?;
    let pack_path = std::path::PathBuf::from(pack_path);
    tauri::async_runtime::spawn_blocking(move || {
        diagram_runtime_service::install_from_archive(&root, &pack_path)
    })
    .await
    .map_err(|error| AppError::background_task_failed("安装 Mermaid runtime", error))?
}

#[tauri::command]
pub async fn uninstall_diagram_runtime() -> Result<DiagramRuntimeStatus, AppError> {
    let root = app_data_dir()?;
    tauri::async_runtime::spawn_blocking(move || {
        diagram_runtime_service::uninstall_from_root(&root)
    })
    .await
    .map_err(|error| AppError::background_task_failed("卸载 Mermaid runtime", error))?
}
