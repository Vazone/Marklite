use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use base64::{engine::general_purpose::STANDARD, Engine};
use percent_encoding::percent_decode_str;
use tauri::AppHandle;
use tauri_plugin_marklite_mobile::MarkliteMobileExt;
use url::Url;

use crate::models::{
    app_error::AppError,
    navigation::MarkdownTargetDto,
    resource::ResourceRef,
    workspace::{
        DirectoryRevision, EntryKind, PageCursor, WorkspaceEntry, WorkspacePage, WorkspaceRoot,
    },
};
use crate::platform::android::document_identity::document_key;
use crate::services::{
    image_load_service::{self, ImageLoadLease, PreviewImageBatch, PreviewImageEntry},
    settings_service,
};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

struct Root {
    dto: WorkspaceRoot,
    generations: Mutex<HashMap<String, u64>>,
    cancelled: Mutex<HashSet<String>>,
    locations: Mutex<HashMap<String, String>>,
}

#[derive(Default)]
pub struct AndroidWorkspaceState {
    current: Mutex<Option<Arc<Root>>>,
}

fn stale() -> AppError {
    AppError::new("WORKSPACE_STALE", "工作区或目录已变化，请刷新")
}
fn invalid() -> AppError {
    AppError::new("WORKSPACE_INVALID_PATH", "工作区相对路径无效")
}

fn valid_uri(uri: &str) -> bool {
    uri.len() <= 4096
        && !uri.chars().any(char::is_control)
        && url::Url::parse(uri)
            .is_ok_and(|url| url.scheme() == "content" && url.host_str().is_some())
}

fn valid_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment.len() <= 1024
        && !segment
            .chars()
            .any(|character| character.is_control() || character == '/' || character == '\\')
}

fn valid_relative(relative: &str) -> bool {
    relative.len() <= 16_384
        && (relative.is_empty()
            || (relative.split('/').count() <= 64 && relative.split('/').all(valid_segment)))
}

fn valid_request(request_id: &str, limit: usize) -> bool {
    (1..=256).contains(&limit)
        && !request_id.is_empty()
        && request_id.len() <= 64
        && request_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn tree_uri(resource: &ResourceRef) -> Result<&str, AppError> {
    match resource {
        ResourceRef::AndroidTree { uri } if valid_uri(uri) => Ok(uri),
        _ => Err(AppError::new(
            "RESOURCE_UNSUPPORTED",
            "需要 Android 目录授权",
        )),
    }
}

fn relative_target(source: &str, target: &str) -> Result<(String, Option<String>), AppError> {
    if target.len() > 4096
        || target.contains('?')
        || target.starts_with('/')
        || target.starts_with('\\')
        || target.contains('\\')
        || target.starts_with("//")
        || target.contains(':')
    {
        return Err(AppError::invalid_markdown_target(
            "Android 目录仅支持相对路径",
        ));
    }
    let (path, fragment) = match target.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (target, None),
    };
    let decoded = percent_decode_str(path)
        .decode_utf8()
        .map_err(|_| AppError::invalid_markdown_target("URL 编码不是有效 UTF-8"))?;
    let mut parts: Vec<&str> = source.split('/').collect();
    parts.pop();
    for segment in decoded.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                if parts.pop().is_none() {
                    return Err(AppError::invalid_markdown_target("相对路径超出授权目录"));
                }
            }
            _ if valid_segment(segment) => parts.push(segment),
            _ => return Err(AppError::invalid_markdown_target("相对路径包含无效段")),
        }
    }
    let resolved = parts.join("/");
    if resolved.is_empty() || !valid_relative(&resolved) {
        return Err(AppError::invalid_markdown_target("相对路径无效"));
    }
    let fragment = fragment
        .map(|value| {
            percent_decode_str(value)
                .decode_utf8()
                .map(|value| value.into_owned())
                .map_err(|_| AppError::invalid_markdown_target("URL 编码不是有效 UTF-8"))
        })
        .transpose()?;
    Ok((resolved, fragment))
}

impl AndroidWorkspaceState {
    fn source_context(
        &self,
        app: &AppHandle,
        source_uri: Option<&str>,
    ) -> Result<(Arc<Root>, String), AppError> {
        let source_uri = source_uri.ok_or_else(|| {
            AppError::new(
                "ANDROID_TREE_REQUIRED",
                "请先保存文档并载入所在目录，以访问相对资源",
            )
        })?;
        if !valid_uri(source_uri) {
            return Err(invalid());
        }
        let root = self.current.lock().unwrap().clone().ok_or_else(|| {
            AppError::new(
                "ANDROID_TREE_REQUIRED",
                "请先载入文档所在目录，以访问相对资源",
            )
        })?;
        let source_key = document_key(source_uri).ok_or_else(invalid)?;
        let tree = tree_uri(&root.dto.resource)?;
        let known = root.locations.lock().unwrap().get(&source_key).cloned();
        let source = match known {
            Some(relative) => relative,
            None => app
                .marklite_mobile()
                .locate_tree_document(tree, source_uri)
                .map_err(|error| AppError::new("WORKSPACE_FILE_UNAVAILABLE", error.to_string()))?
                .relative_path
                .filter(|relative| !relative.is_empty() && valid_relative(relative))
                .ok_or_else(|| {
                    AppError::new(
                        "ANDROID_TREE_REQUIRED",
                        "请在 MarkLite 中打开文档所在目录，以访问相对资源",
                    )
                })?,
        };
        let current = app
            .marklite_mobile()
            .resolve_tree_file(tree, &source)
            .map_err(|error| AppError::new("WORKSPACE_FILE_UNAVAILABLE", error.to_string()))?;
        if current.uri.as_deref().and_then(document_key).as_deref() != Some(source_key.as_str())
            || !self.still_current(&root)
        {
            return Err(stale());
        }
        root.locations
            .lock()
            .unwrap()
            .insert(source_key, source.clone());
        Ok((root, source))
    }
    pub fn mount(&self, app: &AppHandle, resource: ResourceRef) -> Result<WorkspaceRoot, AppError> {
        let uri = tree_uri(&resource)?;
        let name = app
            .marklite_mobile()
            .inspect_tree(uri)
            .map_err(|error| AppError::new("WORKSPACE_PERMISSION_DENIED", error.to_string()))?
            .name;
        let dto = WorkspaceRoot {
            root_id: format!(
                "android-workspace-{}",
                NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
            ),
            resource,
            name: if name.trim().is_empty() {
                "Documents".into()
            } else {
                name
            },
        };
        let root = Arc::new(Root {
            dto: dto.clone(),
            generations: Mutex::new(HashMap::new()),
            cancelled: Mutex::new(HashSet::new()),
            locations: Mutex::new(HashMap::new()),
        });
        *self.current.lock().unwrap() = Some(root);
        Ok(dto)
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

    fn still_current(&self, root: &Arc<Root>) -> bool {
        self.current
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, root))
    }

    pub fn unmount(&self, root_id: &str) -> Result<(), AppError> {
        let mut current = self.current.lock().unwrap();
        if current
            .as_ref()
            .is_none_or(|root| root.dto.root_id != root_id)
        {
            return Err(stale());
        }
        *current = None;
        Ok(())
    }

    pub fn cancel(&self, root_id: &str, request_id: &str) -> Result<(), AppError> {
        if !valid_request(request_id, 1) {
            return Err(invalid());
        }
        let root = self.root(root_id)?;
        let mut cancelled = root.cancelled.lock().unwrap();
        if cancelled.len() >= 64 {
            cancelled.clear();
        }
        cancelled.insert(request_id.to_owned());
        Ok(())
    }

    pub fn page(
        &self,
        app: &AppHandle,
        root_id: &str,
        relative: &str,
        cursor: Option<PageCursor>,
        request_id: &str,
        limit: usize,
    ) -> Result<WorkspacePage, AppError> {
        if !valid_request(request_id, limit) || !valid_relative(relative) {
            return Err(invalid());
        }
        let root = self.root(root_id)?;
        let generation = *root
            .generations
            .lock()
            .unwrap()
            .entry(relative.to_owned())
            .or_insert(1);
        if cursor
            .as_ref()
            .is_some_and(|cursor| cursor.generation != generation)
        {
            return Err(stale());
        }
        if root.cancelled.lock().unwrap().remove(request_id) {
            return Err(AppError::new("WORKSPACE_CANCELLED", "目录请求已取消"));
        }
        let uri = tree_uri(&root.dto.resource)?;
        let response = app
            .marklite_mobile()
            .list_tree(uri, relative)
            .map_err(|error| AppError::new("WORKSPACE_LIST_FAILED", error.to_string()))?;
        if response.entries.len() > 4096 {
            return Err(invalid());
        }
        let mut entries = Vec::with_capacity(response.entries.len());
        for child in response.entries {
            if !valid_segment(&child.name) || !valid_uri(&child.uri) {
                return Err(invalid());
            }
            let kind = match child.kind.as_str() {
                "directory" => EntryKind::Directory,
                "markdown" => EntryKind::Markdown,
                _ => return Err(invalid()),
            };
            let relative_path = if relative.is_empty() {
                child.name.clone()
            } else {
                format!("{relative}/{}", child.name)
            };
            entries.push(WorkspaceEntry {
                name: child.name,
                relative_path,
                kind: kind.clone(),
                resource: if kind == EntryKind::Markdown {
                    Some(ResourceRef::AndroidDocument { uri: child.uri })
                } else {
                    None
                },
                size: child.size,
                issue: None,
            });
        }
        entries.sort_by(|left, right| {
            let left_kind = if left.kind == EntryKind::Directory {
                0
            } else {
                1
            };
            let right_kind = if right.kind == EntryKind::Directory {
                0
            } else {
                1
            };
            left_kind
                .cmp(&right_kind)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.name.cmp(&right.name))
        });
        if !self.still_current(&root)
            || root.generations.lock().unwrap().get(relative).copied() != Some(generation)
        {
            return Err(stale());
        }
        if root.cancelled.lock().unwrap().remove(request_id) {
            return Err(AppError::new("WORKSPACE_CANCELLED", "目录请求已取消"));
        }
        let offset = cursor.map_or(0, |cursor| cursor.offset);
        if offset > entries.len() {
            return Err(AppError::new(
                "WORKSPACE_INVALID_CURSOR",
                "目录分页游标无效",
            ));
        }
        let end = offset.saturating_add(limit).min(entries.len());
        {
            let mut locations = root.locations.lock().unwrap();
            if locations.len() > 8192 {
                locations.clear();
            }
            for entry in &entries[offset..end] {
                if let Some(ResourceRef::AndroidDocument { uri }) = &entry.resource {
                    if let Some(key) = document_key(uri) {
                        locations.insert(key, entry.relative_path.clone());
                    }
                }
            }
        }
        Ok(WorkspacePage {
            root_id: root_id.to_owned(),
            relative_path: relative.to_owned(),
            generation,
            entries: entries[offset..end].to_vec(),
            next: (end < entries.len()).then_some(PageCursor {
                generation,
                offset: end,
            }),
            watch_issue: Some(AppError::new(
                "WORKSPACE_WATCH_UNAVAILABLE",
                "此目录需要手动刷新",
            )),
        })
    }

    pub fn refresh(&self, root_id: &str, relative: &str) -> Result<DirectoryRevision, AppError> {
        if !valid_relative(relative) {
            return Err(invalid());
        }
        let root = self.root(root_id)?;
        let mut generations = root.generations.lock().unwrap();
        let generation = generations.entry(relative.to_owned()).or_insert(1);
        *generation = generation.saturating_add(1);
        Ok(DirectoryRevision {
            relative_path: relative.to_owned(),
            generation: *generation,
        })
    }

    pub fn collapse(&self, root_id: &str, relative: &str) -> Result<(), AppError> {
        if !valid_relative(relative) {
            return Err(invalid());
        }
        let root = self.root(root_id)?;
        let mut generations = root.generations.lock().unwrap();
        generations
            .retain(|path, _| !(path == relative || path.starts_with(&format!("{relative}/"))));
        Ok(())
    }

    pub fn resolve(
        &self,
        app: &AppHandle,
        root_id: &str,
        relative: &str,
    ) -> Result<ResourceRef, AppError> {
        if relative.is_empty() || !valid_relative(relative) {
            return Err(invalid());
        }
        let root = self.root(root_id)?;
        let uri = tree_uri(&root.dto.resource)?;
        let result = app
            .marklite_mobile()
            .resolve_tree_file(uri, relative)
            .map_err(|error| AppError::new("WORKSPACE_FILE_UNAVAILABLE", error.to_string()))?;
        let uri = result
            .uri
            .ok_or_else(|| AppError::new("WORKSPACE_FILE_UNAVAILABLE", "目录文档不存在"))?;
        if !valid_uri(&uri) || !self.still_current(&root) {
            return Err(stale());
        }
        {
            let mut locations = root.locations.lock().unwrap();
            if locations.len() > 8192 {
                locations.clear();
            }
            if let Some(key) = document_key(&uri) {
                locations.insert(key, relative.to_owned());
            }
        }
        Ok(ResourceRef::AndroidDocument { uri })
    }

    pub fn resolve_link(
        &self,
        app: &AppHandle,
        source_uri: Option<&str>,
        target: &str,
    ) -> Result<MarkdownTargetDto, AppError> {
        let target = crate::services::navigation_service::unwrap_marklite_target(target)?;
        crate::services::navigation_service::validate_raw_target(&target)?;
        if target.starts_with('#') {
            return crate::services::navigation_service::resolve_markdown_target(None, &target);
        }
        if let Ok(url) = Url::parse(&target) {
            return if matches!(url.scheme(), "https" | "http" | "mailto") {
                crate::services::navigation_service::resolve_markdown_target(None, &target)
            } else {
                Err(AppError::invalid_markdown_target(
                    "Android 不允许本机路径或此链接协议",
                ))
            };
        }
        let (root, source) = self.source_context(app, source_uri)?;
        let (relative, fragment) = relative_target(&source, &target)?;
        let resource = self.resolve(app, &root.dto.root_id, &relative)?;
        Ok(MarkdownTargetDto::LocalDocument {
            resource,
            path: None,
            fragment,
        })
    }

    pub fn load_images(
        &self,
        app: &AppHandle,
        source_uri: Option<&str>,
        targets: &[String],
        lease: &ImageLoadLease,
    ) -> Result<PreviewImageBatch, AppError> {
        if targets.len() > image_load_service::MAX_PREVIEW_IMAGE_REFERENCES {
            return Err(AppError::preview_image_budget_exceeded());
        }
        if !settings_service::load_settings()?.allow_local_images {
            return Err(AppError::local_images_disabled());
        }
        let (root, source) = self.source_context(app, source_uri)?;
        let tree = tree_uri(&root.dto.resource)?;
        let mut entries = Vec::with_capacity(targets.len());
        let mut resources = Vec::new();
        let mut seen = HashMap::<String, Result<usize, AppError>>::new();
        let mut encoded_total = 0_u64;
        let mut decoded_total = 0_u64;
        let mut exhausted = false;
        for target in targets {
            if lease.is_cancelled() {
                return Err(AppError::image_load_cancelled());
            }
            let result = (|| {
                if exhausted {
                    return Err(AppError::preview_image_budget_exceeded());
                }
                let unwrapped =
                    crate::services::navigation_service::unwrap_marklite_target(target)?;
                crate::services::navigation_service::validate_raw_target(&unwrapped)?;
                let (relative, fragment) = relative_target(&source, &unwrapped)?;
                if fragment.is_some() {
                    return Err(AppError::invalid_markdown_target(
                        "图片目标不能包含 fragment",
                    ));
                }
                if let Some(cached) = seen.get(&relative) {
                    return cached.clone();
                }
                let loaded = (|| {
                    let uri = app
                        .marklite_mobile()
                        .resolve_tree_image(tree, &relative)
                        .map_err(|error| {
                            AppError::new("WORKSPACE_FILE_UNAVAILABLE", error.to_string())
                        })?
                        .uri
                        .ok_or_else(|| AppError::new("WORKSPACE_FILE_UNAVAILABLE", "图片不存在"))?;
                    if !valid_uri(&uri) {
                        return Err(invalid());
                    }
                    let encoded = app
                        .marklite_mobile()
                        .read_image(&uri)
                        .map_err(|error| AppError::new("IMAGE_READ_FAILED", error.to_string()))?
                        .base64;
                    if encoded.len() > 14 * 1024 * 1024 {
                        return Err(AppError::preview_image_budget_exceeded());
                    }
                    let bytes = STANDARD
                        .decode(encoded)
                        .map_err(|_| AppError::new("IMAGE_READ_FAILED", "图片编码无效"))?;
                    let resource =
                        image_load_service::preview_resource_from_bytes(relative.clone(), bytes)?;
                    let image_bytes = resource.image.bytes.len() as u64;
                    if encoded_total.saturating_add(image_bytes)
                        > image_load_service::MAX_PREVIEW_ENCODED_BYTES
                        || decoded_total.saturating_add(resource.decoded_bytes)
                            > image_load_service::MAX_PREVIEW_DECODED_BYTES
                    {
                        return Err(AppError::preview_image_budget_exceeded());
                    }
                    encoded_total += image_bytes;
                    decoded_total += resource.decoded_bytes;
                    let index = resources.len();
                    resources.push(resource);
                    Ok(index)
                })();
                seen.insert(relative, loaded.clone());
                loaded
            })();
            if result
                .as_ref()
                .is_err_and(|error| error.code == "PREVIEW_IMAGE_BUDGET_EXCEEDED")
            {
                exhausted = true;
            }
            entries.push(match result {
                Ok(resource_index) => PreviewImageEntry {
                    target: target.clone(),
                    resource_index: Some(resource_index),
                    error: None,
                },
                Err(error) => PreviewImageEntry {
                    target: target.clone(),
                    resource_index: None,
                    error: Some(error),
                },
            });
        }
        if !self.still_current(&root) {
            return Err(stale());
        }
        Ok(PreviewImageBatch { entries, resources })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_tree_traversal_and_wrong_resource_kind() {
        assert!(valid_relative("chapters/中文 文件.md"));
        for path in ["../private", "/root", "a//b", "a\\b", "a/./b"] {
            assert!(!valid_relative(path));
        }
        assert!(tree_uri(&ResourceRef::DesktopDirectory {
            path: "/tmp".into()
        })
        .is_err());
    }

    #[test]
    fn relative_links_stay_inside_tree_and_decode_once() {
        assert_eq!(
            relative_target("chapters/a.md", "../图%20片.md#one%20two").unwrap(),
            ("图 片.md".into(), Some("one two".into()))
        );
        for target in [
            "../../secret.md",
            "/secret.md",
            "file:///tmp/a.md",
            "a?b.md",
            "a%2F..%2F..%2F..%2Fx.md",
        ] {
            assert!(
                relative_target("chapters/a.md", target).is_err(),
                "{target}"
            );
        }
    }
}
