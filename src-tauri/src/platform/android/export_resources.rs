use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use pulldown_cmark::Tag;
use tauri::Manager;

use crate::{
    models::{app_error::AppError, resource::ResourceRef},
    services::{
        export_resources::ExportResourceResolver,
        export_semantic::{SemanticDocument, SemanticNode},
        image_load_service::ImageLoadState,
    },
};

use super::workspace::AndroidWorkspaceState;

static NEXT_LOAD: AtomicU64 = AtomicU64::new(0);

pub(crate) async fn resolver(
    app: &tauri::AppHandle,
    document: &SemanticDocument,
    source: Option<&ResourceRef>,
    include_images: bool,
) -> Result<ExportResourceResolver<'static>, AppError> {
    let mut resources = ExportResourceResolver::new(None, include_images);
    if !include_images {
        return Ok(resources);
    }
    let mut seen = HashSet::new();
    let mut targets = Vec::new();
    let mut pending = document.roots().to_vec();
    while let Some(id) = pending.pop() {
        match document.node(id) {
            SemanticNode::Element {
                tag: Tag::Image { dest_url, .. },
                children,
            } => {
                if seen.insert(dest_url.to_string()) {
                    targets.push(dest_url.to_string());
                }
                pending.extend(children.iter().copied());
            }
            node => pending.extend(node.children().iter().copied()),
        }
    }
    if targets.is_empty() {
        return Ok(resources);
    }
    let Some(ResourceRef::AndroidDocument { uri }) = source else {
        return Err(AppError::new(
            "ANDROID_TREE_REQUIRED",
            "请先保存文档并载入所在目录，以嵌入相对图片",
        ));
    };
    let source_uri = uri.clone();
    let workspace = app.state::<Arc<AndroidWorkspaceState>>().inner().clone();
    let images = app.state::<ImageLoadState>().inner().clone();
    let app_for_load = app.clone();
    let job = format!(
        "export-images-{}-{}",
        std::process::id(),
        NEXT_LOAD.fetch_add(1, Ordering::Relaxed)
    );
    let batch = tauri::async_runtime::spawn_blocking(move || {
        let lease = images.acquire(&job)?;
        workspace.load_images(&app_for_load, Some(&source_uri), &targets, &lease)
    })
    .await
    .map_err(|_| AppError::new("IMAGE_READ_FAILED", "Android 导出图片任务异常结束"))??;
    resources.install_android_images(batch);
    Ok(resources)
}
