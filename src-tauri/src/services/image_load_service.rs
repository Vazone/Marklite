use std::{
    collections::HashMap,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use crate::{
    models::app_error::AppError,
    services::{navigation_service, settings_service},
    utils::{bounded_read::OpenedFile, path_utils::path_to_utf8},
};

pub(crate) const MAX_IMAGE_SIZE: u64 = 10 * 1024 * 1024;
pub(crate) const MAX_PREVIEW_IMAGE_REFERENCES: usize = 256;
pub(crate) const MAX_PREVIEW_ENCODED_BYTES: u64 = 32 * 1024 * 1024;
pub(crate) const MAX_PREVIEW_DECODED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_PREVIEW_IMAGE_TARGET_BYTES: usize = 4 * 1024;
const MAX_IMAGE_JOB_ID_LENGTH: usize = 128;

#[derive(Clone, Default)]
pub struct ImageLoadState {
    jobs: Arc<Mutex<HashMap<String, ImageJobEntry>>>,
}

struct ImageJobEntry {
    cancelled: Arc<AtomicBool>,
    active: usize,
}

pub struct ImageLoadLease {
    job_id: String,
    cancelled: Arc<AtomicBool>,
    jobs: Arc<Mutex<HashMap<String, ImageJobEntry>>>,
}

impl ImageLoadState {
    pub fn acquire(&self, job_id: &str) -> Result<ImageLoadLease, AppError> {
        if job_id.is_empty()
            || job_id.len() > MAX_IMAGE_JOB_ID_LENGTH
            || !job_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(AppError::invalid_image_job());
        }
        let mut jobs = self
            .jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = jobs
            .entry(job_id.to_string())
            .or_insert_with(|| ImageJobEntry {
                cancelled: Arc::new(AtomicBool::new(false)),
                active: 0,
            });
        entry.active += 1;
        Ok(ImageLoadLease {
            job_id: job_id.to_string(),
            cancelled: entry.cancelled.clone(),
            jobs: self.jobs.clone(),
        })
    }

    pub fn cancel(&self, job_id: &str) -> Result<(), AppError> {
        if job_id.is_empty() || job_id.len() > MAX_IMAGE_JOB_ID_LENGTH {
            return Err(AppError::invalid_image_job());
        }
        if let Some(entry) = self
            .jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(job_id)
        {
            entry.cancelled.store(true, Ordering::Release);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn active_jobs(&self) -> usize {
        self.jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

impl ImageLoadLease {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

impl Drop for ImageLoadLease {
    fn drop(&mut self) {
        let mut jobs = self
            .jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let remove = if let Some(entry) = jobs.get_mut(&self.job_id) {
            entry.active = entry.active.saturating_sub(1);
            entry.active == 0
        } else {
            false
        };
        if remove {
            jobs.remove(&self.job_id);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportImage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub path: String,
}

#[derive(Debug)]
pub struct PreviewImageResource {
    pub image: ExportImage,
    pub width: usize,
    pub height: usize,
    pub decoded_bytes: u64,
}

#[derive(Debug)]
pub struct PreviewImageEntry {
    pub target: String,
    pub resource_index: Option<usize>,
    pub error: Option<AppError>,
}

#[derive(Debug)]
pub struct PreviewImageBatch {
    pub entries: Vec<PreviewImageEntry>,
    pub resources: Vec<PreviewImageResource>,
}

pub fn load_local_images_cancellable(
    document_path: Option<&str>,
    targets: &[String],
    lease: &ImageLoadLease,
) -> Result<PreviewImageBatch, AppError> {
    load_local_images(
        document_path,
        targets,
        settings_service::load_settings()?.allow_local_images,
        lease,
    )
}

fn load_local_images(
    document_path: Option<&str>,
    targets: &[String],
    allow_local_images: bool,
    lease: &ImageLoadLease,
) -> Result<PreviewImageBatch, AppError> {
    if targets.len() > MAX_PREVIEW_IMAGE_REFERENCES {
        return Err(AppError::preview_image_budget_exceeded());
    }
    if !allow_local_images {
        return Err(AppError::local_images_disabled());
    }

    let mut entries = Vec::with_capacity(targets.len());
    let mut resources = Vec::new();
    let mut canonical = HashMap::<std::path::PathBuf, Result<usize, AppError>>::new();
    let mut encoded_bytes = 0_u64;
    let mut decoded_bytes = 0_u64;
    let mut budget_exhausted = false;

    for target in targets {
        if lease.is_cancelled() {
            return Err(AppError::image_load_cancelled());
        }
        if budget_exhausted {
            entries.push(PreviewImageEntry {
                target: target.clone(),
                resource_index: None,
                error: Some(AppError::preview_image_budget_exceeded()),
            });
            continue;
        }
        if target.len() > MAX_PREVIEW_IMAGE_TARGET_BYTES {
            entries.push(PreviewImageEntry {
                target: target.clone(),
                resource_index: None,
                error: Some(AppError::invalid_markdown_target("图片目标超过 4 KiB 上限")),
            });
            continue;
        }
        let result = (|| {
            let unwrapped = navigation_service::unwrap_marklite_target(target)?;
            navigation_service::validate_raw_target(&unwrapped)?;
            let path = navigation_service::resolve_local_path(document_path, &unwrapped, false)?;
            if let Some(cached) = canonical.get(&path) {
                return cached.clone();
            }

            let display_path = path_to_utf8(&path)?.to_string();
            let loaded =
                read_local_image_bytes(&path, "加载图片", Some(lease)).and_then(|(bytes, mime)| {
                    let dimensions = imagesize::blob_size(&bytes)
                        .map_err(|_| AppError::unsupported_image_type(&display_path))?;
                    let encoded = u64::try_from(bytes.len())
                        .map_err(|_| AppError::preview_image_budget_exceeded())?;
                    let decoded = u64::try_from(dimensions.width)
                        .ok()
                        .and_then(|width| {
                            u64::try_from(dimensions.height)
                                .ok()
                                .and_then(|height| width.checked_mul(height))
                        })
                        .and_then(|pixels| pixels.checked_mul(4))
                        .ok_or_else(AppError::preview_image_budget_exceeded)?;
                    if encoded_bytes.saturating_add(encoded) > MAX_PREVIEW_ENCODED_BYTES
                        || decoded_bytes.saturating_add(decoded) > MAX_PREVIEW_DECODED_BYTES
                    {
                        return Err(AppError::preview_image_budget_exceeded());
                    }
                    encoded_bytes += encoded;
                    decoded_bytes += decoded;
                    let index = resources.len();
                    resources.push(PreviewImageResource {
                        image: ExportImage {
                            bytes,
                            mime,
                            path: display_path,
                        },
                        width: dimensions.width,
                        height: dimensions.height,
                        decoded_bytes: decoded,
                    });
                    Ok(index)
                });
            canonical.insert(path, loaded.clone());
            loaded
        })();

        match result {
            Ok(resource_index) => entries.push(PreviewImageEntry {
                target: target.clone(),
                resource_index: Some(resource_index),
                error: None,
            }),
            Err(error) => {
                if error.code == "PREVIEW_IMAGE_BUDGET_EXCEEDED" {
                    budget_exhausted = true;
                }
                entries.push(PreviewImageEntry {
                    target: target.clone(),
                    resource_index: None,
                    error: Some(error),
                });
            }
        }
    }

    Ok(PreviewImageBatch { entries, resources })
}

#[cfg(test)]
pub(crate) fn load_local_images_with_permission(
    document_path: Option<&str>,
    targets: &[String],
    allow_local_images: bool,
    lease: &ImageLoadLease,
) -> Result<PreviewImageBatch, AppError> {
    load_local_images(document_path, targets, allow_local_images, lease)
}

#[cfg(test)]
pub(crate) fn load_local_image_with_permission(
    document_path: Option<&str>,
    target: &str,
    allow_local_images: bool,
) -> Result<ExportImage, AppError> {
    load_local_image(document_path, target, allow_local_images, None)
}

#[cfg(test)]
pub(crate) fn load_local_image_with_permission_and_cancel(
    document_path: Option<&str>,
    target: &str,
    allow_local_images: bool,
    lease: Option<&ImageLoadLease>,
) -> Result<ExportImage, AppError> {
    load_local_image(document_path, target, allow_local_images, lease)
}

#[cfg(test)]
fn load_local_image(
    document_path: Option<&str>,
    target: &str,
    allow_local_images: bool,
    lease: Option<&ImageLoadLease>,
) -> Result<ExportImage, AppError> {
    if !allow_local_images {
        return Err(AppError::local_images_disabled());
    }
    let target = navigation_service::unwrap_marklite_target(target)?;
    navigation_service::validate_raw_target(&target)?;
    let path = navigation_service::resolve_local_path(document_path, &target, false)?;
    let (bytes, mime) = read_local_image_bytes(&path, "加载图片", lease)?;
    Ok(ExportImage {
        bytes,
        mime,
        path: path_to_utf8(&path)?.to_string(),
    })
}

pub fn load_local_image_for_export(
    document_path: Option<&str>,
    target: &str,
) -> Result<ExportImage, AppError> {
    let target = navigation_service::unwrap_marklite_target(target)?;
    navigation_service::validate_raw_target(&target)?;
    let path = navigation_service::resolve_local_path(document_path, &target, false)?;
    let (bytes, mime) = read_local_image_bytes(&path, "导出图片", None)?;
    Ok(ExportImage {
        bytes,
        mime,
        path: path_to_utf8(&path)?.to_string(),
    })
}

fn read_local_image_bytes(
    path: &Path,
    operation: &str,
    lease: Option<&ImageLoadLease>,
) -> Result<(Vec<u8>, &'static str), AppError> {
    let display_path = path_to_utf8(path)?;
    let opened =
        OpenedFile::open(path).map_err(|error| AppError::file_read_failed(display_path, error))?;
    if !opened.metadata().is_file() {
        return Err(AppError::invalid_file_target(display_path));
    }
    if opened.metadata().len() > MAX_IMAGE_SIZE {
        return Err(AppError::file_too_large(display_path, operation));
    }
    #[cfg(test)]
    run_image_read_hook(path);
    let bytes = opened
        .read_bounded_with_cancel(MAX_IMAGE_SIZE, || {
            lease.is_some_and(ImageLoadLease::is_cancelled)
        })
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::Interrupted
                && lease.is_some_and(ImageLoadLease::is_cancelled)
            {
                AppError::image_load_cancelled()
            } else if error.kind() == std::io::ErrorKind::FileTooLarge {
                AppError::file_too_large(display_path, operation)
            } else {
                AppError::file_read_failed(display_path, error)
            }
        })?;
    let mime = image_mime(path, &bytes)?;
    Ok((bytes, mime))
}

fn image_mime(path: &Path, bytes: &[u8]) -> Result<&'static str, AppError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mime = match extension.as_str() {
        "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => "image/png",
        "jpg" | "jpeg" if bytes.starts_with(&[0xff, 0xd8, 0xff]) => "image/jpeg",
        "gif" if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") => "image/gif",
        "webp" if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" => {
            "image/webp"
        }
        _ => return Err(AppError::unsupported_image_type(path_to_utf8(path)?)),
    };
    Ok(mime)
}

#[cfg(test)]
type ImageReadHook = std::sync::Arc<dyn Fn(&Path) + Send + Sync>;

#[cfg(test)]
static IMAGE_READ_HOOK: std::sync::OnceLock<std::sync::Mutex<Option<ImageReadHook>>> =
    std::sync::OnceLock::new();

#[cfg(test)]
fn run_image_read_hook(path: &Path) {
    let hook = IMAGE_READ_HOOK
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(hook) = hook {
        hook(path);
    }
}

#[cfg(test)]
pub(crate) fn set_image_read_hook(hook: Option<ImageReadHook>) {
    *IMAGE_READ_HOOK
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = hook;
}

#[cfg(test)]
mod preview_batch_tests {
    use std::{fs, fs::OpenOptions};

    use tempfile::tempdir;

    use super::*;

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    #[test]
    fn canonical_aliases_share_one_read_and_one_resource() {
        let directory = tempdir().unwrap();
        let document = directory.path().join("note.md");
        let image = directory.path().join("pixel.png");
        fs::write(&document, "# Note").unwrap();
        fs::write(&image, png_header(32, 16)).unwrap();
        let state = ImageLoadState::default();
        let lease = state.acquire("aliases").unwrap();
        let targets = (0..64)
            .map(|index| format!("{}pixel.png", "./".repeat(index + 1)))
            .collect::<Vec<_>>();

        let batch = load_local_images_with_permission(
            Some(document.to_str().unwrap()),
            &targets,
            true,
            &lease,
        )
        .unwrap();
        assert_eq!(batch.resources.len(), 1);
        assert_eq!(batch.entries.len(), 64);
        assert!(batch
            .entries
            .iter()
            .all(|entry| entry.resource_index == Some(0)));
        assert_eq!(batch.resources[0].decoded_bytes, 32 * 16 * 4);
    }

    #[test]
    fn rejects_a_resource_that_exceeds_the_decoded_surface_budget() {
        let directory = tempdir().unwrap();
        let document = directory.path().join("note.md");
        fs::write(&document, "# Note").unwrap();
        fs::write(directory.path().join("huge.png"), png_header(6_000, 6_000)).unwrap();
        fs::write(directory.path().join("later.png"), png_header(1, 1)).unwrap();
        let state = ImageLoadState::default();
        let lease = state.acquire("decoded-budget").unwrap();

        let batch = load_local_images_with_permission(
            Some(document.to_str().unwrap()),
            &["huge.png".to_string(), "later.png".to_string()],
            true,
            &lease,
        )
        .unwrap();

        assert!(batch.resources.is_empty());
        assert!(batch.entries.iter().all(|entry| entry
            .error
            .as_ref()
            .is_some_and(|error| error.code == "PREVIEW_IMAGE_BUDGET_EXCEEDED")));
    }

    #[test]
    fn stops_loading_after_the_cumulative_encoded_budget_is_exhausted() {
        let directory = tempdir().unwrap();
        let document = directory.path().join("note.md");
        fs::write(&document, "# Note").unwrap();
        let mut targets = Vec::new();
        for index in 0..4 {
            let name = format!("encoded-{index}.png");
            let path = directory.path().join(&name);
            fs::write(&path, png_header(1, 1)).unwrap();
            OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .set_len(9 * 1024 * 1024)
                .unwrap();
            targets.push(name);
        }
        targets.push("not-read.png".to_string());
        let state = ImageLoadState::default();
        let lease = state.acquire("encoded-budget").unwrap();

        let batch = load_local_images_with_permission(
            Some(document.to_str().unwrap()),
            &targets,
            true,
            &lease,
        )
        .unwrap();

        assert_eq!(batch.resources.len(), 3);
        assert!(batch.entries[0..3]
            .iter()
            .all(|entry| entry.error.is_none()));
        assert!(batch.entries[3..].iter().all(|entry| entry
            .error
            .as_ref()
            .is_some_and(|error| error.code == "PREVIEW_IMAGE_BUDGET_EXCEEDED")));
    }

    #[test]
    fn releases_one_hundred_completed_job_owners() {
        let state = ImageLoadState::default();
        for index in 0..100 {
            let lease = state.acquire(&format!("cycle-{index}")).unwrap();
            let batch = load_local_images_with_permission(None, &[], true, &lease).unwrap();
            assert!(batch.entries.is_empty());
            drop(lease);
            assert_eq!(state.active_jobs(), 0);
        }
    }

    #[test]
    fn rejects_more_than_the_bounded_reference_count_before_loading() {
        let state = ImageLoadState::default();
        let lease = state.acquire("reference-budget").unwrap();
        let targets = vec!["pixel.png".to_string(); MAX_PREVIEW_IMAGE_REFERENCES + 1];

        let error = load_local_images_with_permission(None, &targets, true, &lease).unwrap_err();

        assert_eq!(error.code, "PREVIEW_IMAGE_BUDGET_EXCEEDED");
    }
}
