use crate::services::{android_recent_documents, settings_service};
use crate::{
    commands::background::run_background,
    models::{
        app_error::AppError,
        document::{DocumentDto, DocumentOperationDto, FileVersionDto},
        resource::ResourceRef,
    },
};
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tauri_plugin_marklite_mobile::{DocumentWriteRequest, MarkliteMobileExt};

fn remember_recent(uri: &str, title: &str) -> Result<(), AppError> {
    let limit = settings_service::load_settings()?.recent_files_limit;
    android_recent_documents::remember(uri, title, limit)
}

#[tauri::command]
pub async fn get_android_recent_documents(
) -> Result<Vec<android_recent_documents::AndroidRecentDocument>, AppError> {
    run_background("读取 Android 最近文档", android_recent_documents::list).await
}

#[tauri::command]
pub async fn remove_android_recent_document(
    resource: ResourceRef,
) -> Result<Vec<android_recent_documents::AndroidRecentDocument>, AppError> {
    run_background("移除 Android 最近文档", move || {
        android_recent_documents::remove(&resource)
    })
    .await
}

fn document_uri(resource: &ResourceRef) -> Result<&str, AppError> {
    let ResourceRef::AndroidDocument { uri } = resource else {
        return Err(AppError::new(
            "RESOURCE_UNSUPPORTED",
            "需要 Android 文档引用",
        ));
    };
    validate_uri(uri)
}

fn validate_uri(uri: &str) -> Result<&str, AppError> {
    if uri.len() > 4096 || uri.chars().any(char::is_control) {
        return Err(AppError::new("INVALID_RESOURCE_REF", "文档 URI 无效"));
    }
    let parsed =
        url::Url::parse(uri).map_err(|_| AppError::new("INVALID_RESOURCE_REF", "文档 URI 无效"))?;
    if parsed.scheme() != "content" || parsed.host_str().is_none_or(str::is_empty) {
        return Err(AppError::new("INVALID_RESOURCE_REF", "仅接受 content URI"));
    }
    Ok(uri)
}

fn identity(uri: &str) -> String {
    let key = crate::platform::android::document_identity::document_key(uri);
    let identity = key.as_deref().unwrap_or(uri);
    format!("android:{:x}", Sha256::digest(identity.as_bytes()))
}

fn valid_name(name: &str) -> Result<(), AppError> {
    if name.len() > 255
        || name.chars().any(char::is_control)
        || name.contains('/')
        || name.contains('\\')
        || !["md", "markdown", "txt"].iter().any(|extension| {
            name.rsplit_once('.')
                .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case(extension))
        })
    {
        return Err(AppError::new(
            "INVALID_FILE_TYPE",
            "仅支持 .md、.markdown、.txt 文件",
        ));
    }
    Ok(())
}

fn plugin_error(error: impl std::fmt::Display) -> AppError {
    let message = error.to_string();
    let code = if message.contains("FILE_CONTENT_CHANGED") {
        "FILE_CONTENT_CHANGED"
    } else if message.contains("32 MiB") || message.contains("10 MiB") {
        "FILE_TOO_LARGE"
    } else {
        "ANDROID_DOCUMENT_FAILED"
    };
    AppError::new(code, message)
}

#[tauri::command]
pub async fn pick_android_document(app: AppHandle) -> Result<Option<ResourceRef>, AppError> {
    let picked = app
        .marklite_mobile()
        .pick_document()
        .map_err(plugin_error)?;
    picked
        .uri
        .map(|uri| {
            validate_uri(&uri)?;
            Ok(ResourceRef::AndroidDocument { uri })
        })
        .transpose()
}

#[tauri::command]
pub async fn create_android_document(
    app: AppHandle,
    title: String,
) -> Result<Option<ResourceRef>, AppError> {
    valid_name(&title)?;
    let picked = app
        .marklite_mobile()
        .create_document(&title)
        .map_err(plugin_error)?;
    picked
        .uri
        .map(|uri| {
            validate_uri(&uri)?;
            Ok(ResourceRef::AndroidDocument { uri })
        })
        .transpose()
}

#[tauri::command]
pub async fn pick_android_tree(app: AppHandle) -> Result<Option<ResourceRef>, AppError> {
    let picked = app.marklite_mobile().pick_tree().map_err(plugin_error)?;
    picked
        .uri
        .map(|uri| {
            validate_uri(&uri)?;
            Ok(ResourceRef::AndroidTree { uri })
        })
        .transpose()
}

#[tauri::command]
pub async fn open_android_document(
    app: AppHandle,
    resource: ResourceRef,
) -> Result<DocumentOperationDto, AppError> {
    let uri = document_uri(&resource)?.to_owned();
    run_background("读取 Android 文档", move || {
        let response = app
            .marklite_mobile()
            .read_document(&uri)
            .map_err(plugin_error)?;
        if response.uri != uri {
            return Err(AppError::new("INVALID_RESPONSE", "Android 文档 URI 不匹配"));
        }
        valid_name(&response.name)?;
        if response.size > crate::services::file_service::MAX_FILE_SIZE {
            return Err(AppError::file_too_large(&response.name, "打开"));
        }
        let auxiliary_error = remember_recent(&uri, &response.name).err();
        Ok(DocumentOperationDto {
            document: DocumentDto {
                resource: Some(ResourceRef::AndroidDocument { uri: uri.clone() }),
                path: None,
                file_identity: Some(identity(&uri)),
                content_version: Some(response.version),
                title: response.name,
                content: response.content,
                is_dirty: false,
                last_saved_at: None,
                file_size: Some(response.size),
            },
            auxiliary_error,
        })
    })
    .await
}

#[tauri::command]
pub async fn android_document_name(
    app: AppHandle,
    resource: ResourceRef,
) -> Result<Option<String>, AppError> {
    let uri = document_uri(&resource)?.to_owned();
    run_background("读取 Android 文档名称", move || {
        let response = app
            .marklite_mobile()
            .document_name(&uri)
            .map_err(plugin_error)?;
        if let Some(name) = response.name.as_deref() {
            valid_name(name)?;
        }
        Ok(response.name)
    })
    .await
}

#[tauri::command]
pub async fn android_document_version(
    app: AppHandle,
    resource: ResourceRef,
) -> Result<FileVersionDto, AppError> {
    let uri = document_uri(&resource)?.to_owned();
    run_background("检查 Android 文档版本", move || {
        let response = app
            .marklite_mobile()
            .document_version(&uri)
            .map_err(plugin_error)?;
        Ok(FileVersionDto {
            file_identity: identity(&uri),
            content_version: response.version,
        })
    })
    .await
}

#[tauri::command]
pub async fn save_android_document(
    app: AppHandle,
    resource: ResourceRef,
    content: String,
    expected_file_identity: Option<String>,
    expected_content_version: Option<String>,
    overwrite_content_conflict: bool,
) -> Result<DocumentOperationDto, AppError> {
    let uri = document_uri(&resource)?.to_owned();
    if content.len() as u64 > crate::services::file_service::MAX_FILE_SIZE {
        return Err(AppError::file_too_large(&uri, "保存"));
    }
    if expected_file_identity
        .as_deref()
        .is_some_and(|expected| expected != identity(&uri))
    {
        return Err(AppError::file_target_changed(&uri));
    }
    run_background("保存 Android 文档", move || {
        let response = app
            .marklite_mobile()
            .write_document(DocumentWriteRequest {
                uri: &uri,
                content: &content,
                expected_version: if overwrite_content_conflict {
                    None
                } else {
                    expected_content_version.as_deref()
                },
            })
            .map_err(plugin_error)?;
        if response.uri != uri {
            return Err(AppError::new("INVALID_RESPONSE", "Android 文档 URI 不匹配"));
        }
        valid_name(&response.name)?;
        let auxiliary_error = remember_recent(&uri, &response.name).err();
        Ok(DocumentOperationDto {
            document: DocumentDto {
                resource: Some(ResourceRef::AndroidDocument { uri: uri.clone() }),
                path: None,
                file_identity: Some(identity(&uri)),
                content_version: Some(response.version),
                title: response.name,
                content,
                is_dirty: false,
                last_saved_at: None,
                file_size: Some(response.size),
            },
            auxiliary_error,
        })
    })
    .await
}

#[tauri::command]
pub async fn drain_android_open_requests(app: AppHandle) -> Result<Vec<ResourceRef>, AppError> {
    let response = app
        .marklite_mobile()
        .drain_intents()
        .map_err(plugin_error)?;
    response
        .uris
        .into_iter()
        .map(|uri| {
            validate_uri(&uri)?;
            Ok(ResourceRef::AndroidDocument { uri })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_content_references_and_markdown_names() {
        assert!(validate_uri("content://provider/document/abc").is_ok());
        assert!(validate_uri("file:///tmp/note.md").is_err());
        assert!(valid_name("中文 文件.MD").is_ok());
        assert!(valid_name("archive.zip").is_err());
        assert_ne!(identity("content://a/1"), identity("content://a/2"));
        let direct = "content://provider/document/root%2Fsame.md";
        let tree = "content://provider/tree/root/document/root%2Fsame.md";
        assert_eq!(identity(direct), identity(tree));
        assert_ne!(
            identity(direct),
            identity("content://provider/document/other%2Fsame.md")
        );
    }
}
