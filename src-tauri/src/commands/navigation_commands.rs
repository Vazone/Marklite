use crate::{
    commands::background::run_background,
    models::{app_error::AppError, navigation::MarkdownTargetDto},
    services::{
        image_load_service::{self, ImageLoadState},
        local_image_protocol, navigation_service,
    },
};
use tauri::State;

#[tauri::command]
pub async fn resolve_markdown_target(
    document_path: Option<String>,
    target: String,
) -> Result<MarkdownTargetDto, AppError> {
    run_background("链接解析", move || {
        navigation_service::resolve_markdown_target(document_path.as_deref(), &target)
    })
    .await
}

#[tauri::command]
pub async fn open_validated_email_link(address: String) -> Result<(), AppError> {
    let url = navigation_service::validated_email_url(&address)?;
    run_background("打开邮件链接", move || {
        tauri_plugin_opener::open_url(url, None::<&str>).map_err(|error| {
            AppError::new("OPEN_EMAIL_FAILED", format!("打开邮件链接失败：{error}"))
        })
    })
    .await
}

#[tauri::command]
pub async fn load_local_image(
    document_path: Option<String>,
    targets: Vec<String>,
    job_id: String,
    state: State<'_, ImageLoadState>,
) -> Result<tauri::ipc::Response, AppError> {
    let lease = state.acquire(&job_id)?;
    let batch = run_background("本地图片", move || {
        image_load_service::load_local_images_cancellable(
            document_path.as_deref(),
            &targets,
            &lease,
        )
    })
    .await?;
    let body = local_image_protocol::encode_response(&batch)?;
    Ok(tauri::ipc::Response::new(body))
}

#[tauri::command]
pub fn cancel_local_image_job(
    job_id: String,
    state: State<'_, ImageLoadState>,
) -> Result<(), AppError> {
    state.cancel(&job_id)
}
