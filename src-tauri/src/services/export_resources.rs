use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use url::Url;

use crate::{
    models::{export::ExportWarning, navigation::MarkdownTargetDto},
    services::{image_load_service, navigation_service},
    utils::path_utils::path_to_utf8,
};

const MAX_EMBEDDED_RESOURCE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Default)]
pub(crate) struct WarningCollector {
    warnings: Vec<ExportWarning>,
    seen: HashSet<(String, Option<String>)>,
}

impl WarningCollector {
    pub(crate) fn push(&mut self, warning: ExportWarning) {
        let identity = (warning.code.clone(), warning.target.clone());
        if self.seen.insert(identity) {
            self.warnings.push(warning);
        }
    }

    pub(crate) fn into_vec(self) -> Vec<ExportWarning> {
        self.warnings
    }

    #[cfg(test)]
    pub(crate) fn as_slice(&self) -> &[ExportWarning] {
        &self.warnings
    }
}

#[derive(Debug, Clone)]
struct CachedImage {
    bytes: Vec<u8>,
    mime: &'static str,
    canonical_path: String,
    data_url: Option<Arc<str>>,
}

#[derive(Debug, Clone)]
enum ResourceOutcome {
    Ready(String),
    Unavailable,
}

/// Per-export resource table. Repeated source spellings are resolved and encoded
/// once. The byte budget is deliberately charged per rendered reference because
/// both HTML data URLs and docx-rs picture nodes duplicate bytes before the final
/// artifact is packed; charging only unique files would leave that peak unbounded.
pub(crate) struct ExportResourceResolver<'a> {
    source_path: Option<&'a str>,
    include_local_images: bool,
    embedded_resource_bytes: usize,
    outcomes_by_target: HashMap<String, ResourceOutcome>,
    images_by_canonical_path: HashMap<String, CachedImage>,
    warnings: WarningCollector,
    #[cfg(test)]
    load_count: usize,
}

impl<'a> ExportResourceResolver<'a> {
    pub(crate) fn new(source_path: Option<&'a str>, include_local_images: bool) -> Self {
        Self {
            source_path,
            include_local_images,
            embedded_resource_bytes: 0,
            outcomes_by_target: HashMap::new(),
            images_by_canonical_path: HashMap::new(),
            warnings: WarningCollector::default(),
            #[cfg(test)]
            load_count: 0,
        }
    }

    pub(crate) fn html_data_url(&mut self, target: &str) -> Option<Arc<str>> {
        let key = self.resolve_image(target)?;
        let image = self.images_by_canonical_path.get_mut(&key)?;
        if image.data_url.is_none() {
            image.data_url = Some(Arc::from(format!(
                "data:{};base64,{}",
                image.mime,
                STANDARD.encode(&image.bytes)
            )));
        }
        image.data_url.clone()
    }

    pub(crate) fn docx_image(&mut self, target: &str) -> Option<(&[u8], &'static str, &str)> {
        let key = self.resolve_image(target)?;
        let image = self.images_by_canonical_path.get(&key)?;
        Some((&image.bytes, image.mime, &image.canonical_path))
    }

    fn resolve_image(&mut self, target: &str) -> Option<String> {
        if let Some(outcome) = self.outcomes_by_target.get(target) {
            return match outcome {
                ResourceOutcome::Ready(key) => {
                    let key = key.clone();
                    let bytes = self.images_by_canonical_path.get(&key)?.bytes.len();
                    self.reserve(bytes, target).then_some(key)
                }
                ResourceOutcome::Unavailable => None,
            };
        }

        if is_remote_target(target) {
            self.warn(
                "REMOTE_IMAGE_SKIPPED",
                "导出不会下载远程图片，已保留替代文本",
                target,
            );
            self.outcomes_by_target
                .insert(target.to_string(), ResourceOutcome::Unavailable);
            return None;
        }
        if !self.include_local_images {
            self.warn(
                "LOCAL_IMAGE_NOT_EMBEDDED",
                "未选择嵌入本地图片，已保留替代文本",
                target,
            );
            self.outcomes_by_target
                .insert(target.to_string(), ResourceOutcome::Unavailable);
            return None;
        }

        let unwrapped = match navigation_service::unwrap_marklite_target(target) {
            Ok(value) => value,
            Err(error) => return self.cache_error(target, error),
        };
        if let Err(error) = navigation_service::validate_raw_target(&unwrapped) {
            return self.cache_error(target, error);
        }
        let canonical_path =
            match navigation_service::resolve_local_path(self.source_path, &unwrapped, false) {
                Ok(path) => match path_to_utf8(&path) {
                    Ok(path) => path.to_string(),
                    Err(error) => return self.cache_error(target, error),
                },
                Err(error) => return self.cache_error(target, error),
            };

        if let Some(image) = self.images_by_canonical_path.get(&canonical_path) {
            let bytes = image.bytes.len();
            if !self.reserve(bytes, target) {
                self.outcomes_by_target
                    .insert(target.to_string(), ResourceOutcome::Unavailable);
                return None;
            }
            self.outcomes_by_target.insert(
                target.to_string(),
                ResourceOutcome::Ready(canonical_path.clone()),
            );
            return Some(canonical_path);
        }

        #[cfg(test)]
        {
            self.load_count += 1;
        }
        let image = match image_load_service::load_local_image_for_export(self.source_path, target)
        {
            Ok(image) => image,
            Err(error) => {
                return self.cache_error(target, error);
            }
        };

        if !self.reserve(image.bytes.len(), target) {
            self.outcomes_by_target
                .insert(target.to_string(), ResourceOutcome::Unavailable);
            return None;
        }
        if !self.images_by_canonical_path.contains_key(&canonical_path) {
            self.images_by_canonical_path.insert(
                canonical_path.clone(),
                CachedImage {
                    bytes: image.bytes,
                    mime: image.mime,
                    canonical_path: canonical_path.clone(),
                    data_url: None,
                },
            );
        }
        self.outcomes_by_target.insert(
            target.to_string(),
            ResourceOutcome::Ready(canonical_path.clone()),
        );
        Some(canonical_path)
    }

    fn cache_error(
        &mut self,
        target: &str,
        error: crate::models::app_error::AppError,
    ) -> Option<String> {
        self.warnings.push(ExportWarning::new(
            error.code,
            error.message,
            Some(target.to_string()),
        ));
        self.outcomes_by_target
            .insert(target.to_string(), ResourceOutcome::Unavailable);
        None
    }

    fn reserve(&mut self, resource_bytes: usize, target: &str) -> bool {
        let Some(total) = self.embedded_resource_bytes.checked_add(resource_bytes) else {
            self.warn_budget(target);
            return false;
        };
        if total > MAX_EMBEDDED_RESOURCE_BYTES {
            self.warn_budget(target);
            return false;
        }
        self.embedded_resource_bytes = total;
        true
    }

    fn warn_budget(&mut self, target: &str) {
        self.warn(
            "EXPORT_RESOURCE_BUDGET_EXCEEDED",
            "嵌入图片总大小超过 32 MiB，已保留替代文本",
            target,
        );
    }

    fn warn(&mut self, code: &str, message: &str, target: &str) {
        self.warnings
            .push(ExportWarning::new(code, message, Some(target.to_string())));
    }

    pub(crate) fn warnings_mut(&mut self) -> &mut WarningCollector {
        &mut self.warnings
    }

    pub(crate) fn into_warnings(self) -> Vec<ExportWarning> {
        self.warnings.into_vec()
    }

    #[cfg(test)]
    pub(crate) fn load_count(&self) -> usize {
        self.load_count
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExportLink {
    Anchor(String),
    External(String),
    Email(String),
}

pub(crate) fn resolve_export_link(
    source_path: Option<&str>,
    target: &str,
) -> Result<ExportLink, ExportWarning> {
    match navigation_service::resolve_markdown_target(source_path, target) {
        Ok(MarkdownTargetDto::Anchor { fragment }) => Ok(ExportLink::Anchor(fragment)),
        Ok(MarkdownTargetDto::External { url }) => Ok(ExportLink::External(url)),
        Ok(MarkdownTargetDto::Email { address }) => Ok(ExportLink::Email(address)),
        Ok(MarkdownTargetDto::LocalDocument { path, fragment }) => {
            let mut url = Url::from_file_path(&path).map_err(|_| {
                ExportWarning::new(
                    "INVALID_MARKDOWN_TARGET",
                    "本地链接无法转换为 file URL，已保留显示文本",
                    Some(target.to_string()),
                )
            })?;
            url.set_fragment(fragment.as_deref());
            Ok(ExportLink::External(url.to_string()))
        }
        Err(error) => Err(ExportWarning::new(
            error.code,
            error.message,
            Some(target.to_string()),
        )),
    }
}

fn is_remote_target(target: &str) -> bool {
    Url::parse(target)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use base64::{engine::general_purpose::STANDARD, Engine};

    use super::{resolve_export_link, ExportLink, ExportResourceResolver};
    use crate::utils::test_support::TestDirectory;

    #[test]
    fn repeated_image_target_loads_and_encodes_one_resource() {
        let directory = TestDirectory::new("export-resource-cache");
        let image_path = directory.join("像素 空格.png");
        let bytes = STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        fs::write(&image_path, bytes).unwrap();
        let source = directory.join("source.md");
        fs::write(&source, b"fixture").unwrap();
        let mut resolver = ExportResourceResolver::new(source.to_str(), true);
        let first = resolver.html_data_url("像素%20空格.png").unwrap();
        for index in 0..999 {
            let target = if index % 2 == 0 {
                "像素%20空格.png"
            } else {
                "./像素%20空格.png"
            };
            assert_eq!(resolver.html_data_url(target).unwrap(), first);
        }
        assert_eq!(resolver.load_count(), 1);
        assert!(resolver.warnings.as_slice().is_empty());
    }

    #[test]
    fn resolves_relative_local_links_from_the_markdown_source() {
        let directory = TestDirectory::new("export-resource-links");
        let source_directory = directory.join("源 文档");
        let output_directory = directory.join("exports");
        fs::create_dir_all(&source_directory).unwrap();
        fs::create_dir_all(&output_directory).unwrap();
        let source = source_directory.join("index.md");
        let target = directory.join("中文 目标.md");
        fs::write(&source, b"fixture").unwrap();
        fs::write(&target, b"target").unwrap();

        let resolved = resolve_export_link(
            source.to_str(),
            "../%E4%B8%AD%E6%96%87%20%E7%9B%AE%E6%A0%87.md#section",
        )
        .unwrap();
        let ExportLink::External(value) = resolved else {
            panic!("local document must become an external file URL")
        };
        let url = url::Url::parse(&value).unwrap();
        assert_eq!(
            url.to_file_path().unwrap().canonicalize().unwrap(),
            target.canonicalize().unwrap()
        );
        assert_eq!(url.fragment(), Some("section"));
    }

    #[test]
    fn counts_unique_resources_and_deduplicates_budget_warnings() {
        let mut resolver = ExportResourceResolver::new(None, true);
        assert!(resolver.reserve(super::MAX_EMBEDDED_RESOURCE_BYTES, "first.png"));
        assert!(!resolver.reserve(1, "second.png"));
        assert!(!resolver.reserve(1, "second.png"));
        assert_eq!(
            resolver.embedded_resource_bytes,
            super::MAX_EMBEDDED_RESOURCE_BYTES
        );
        assert_eq!(resolver.warnings.as_slice().len(), 1);
        assert_eq!(
            resolver.warnings.as_slice()[0].code,
            "EXPORT_RESOURCE_BUDGET_EXCEEDED"
        );
    }
}
