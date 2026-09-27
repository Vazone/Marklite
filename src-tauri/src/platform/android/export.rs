use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

use crate::models::{app_error::AppError, export::ExportFormat};

static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);
const MAX_STAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PDF_BYTES: u64 = 512 * 1024 * 1024;

/// A private, single-job file. The provider commit is awaited before this owner drops.
pub(crate) struct StagedArtifact {
    directory: PathBuf,
    output: PathBuf,
    max_bytes: u64,
}

impl StagedArtifact {
    pub(crate) fn create(format: ExportFormat) -> Result<Self, AppError> {
        let parent = super::app_data_dir()?.join("export-staging");
        fs::create_dir_all(&parent)
            .map_err(|error| AppError::new("EXPORT_TEMP_FAILED", error.to_string()))?;
        for _ in 0..32 {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let sequence = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
            let directory = parent.join(format!("{}-{nonce}-{sequence}", std::process::id()));
            match fs::create_dir(&directory) {
                Ok(()) => {
                    let output = directory.join(format!("output.{}", format.extension()));
                    let max_bytes = if format == ExportFormat::Pdf {
                        MAX_PDF_BYTES
                    } else {
                        MAX_STAGE_BYTES
                    };
                    return Ok(Self {
                        directory,
                        output,
                        max_bytes,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(AppError::new("EXPORT_TEMP_FAILED", error.to_string()));
                }
            }
        }
        Err(AppError::new(
            "EXPORT_TEMP_FAILED",
            "无法创建 Android 导出暂存目录",
        ))
    }

    pub(crate) fn output(&self) -> &Path {
        &self.output
    }

    pub(crate) fn part_path(&self) -> PathBuf {
        self.directory.join("part.pdf")
    }

    pub(crate) fn image_path(&self, name: &str) -> Result<PathBuf, AppError> {
        if name.is_empty()
            || name.len() > 255
            || name == "."
            || name == ".."
            || name
                .chars()
                .any(|ch| ch.is_control() || ch == '/' || ch == '\\')
            || !name.ends_with(".png")
        {
            return Err(AppError::new(
                "PNG_INVALID_FILENAME",
                "Android 图片文件名无效",
            ));
        }
        Ok(self.directory.join(name))
    }

    pub(crate) fn output_string(&self) -> Result<String, AppError> {
        self.output
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| AppError::new("EXPORT_TEMP_FAILED", "Android 暂存路径不是 UTF-8"))
    }

    pub(crate) fn fingerprint(&self) -> Result<(u64, String), AppError> {
        let mut file = File::open(&self.output)
            .map_err(|error| AppError::new("EXPORT_TEMP_FAILED", error.to_string()))?;
        let mut hash = Sha256::new();
        let mut bytes = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|error| AppError::new("EXPORT_TEMP_FAILED", error.to_string()))?;
            if count == 0 {
                break;
            }
            bytes = bytes.saturating_add(count as u64);
            if bytes > self.max_bytes {
                return Err(AppError::new(
                    "EXPORT_OUTPUT_TOO_LARGE",
                    format!("Android 导出产物超过 {} MiB", self.max_bytes / 1024 / 1024),
                ));
            }
            hash.update(&buffer[..count]);
        }
        if bytes == 0 {
            return Err(AppError::new(
                "EXPORT_INVALID_ARTIFACT",
                "Android 导出产物为空",
            ));
        }
        Ok((bytes, format!("{:x}", hash.finalize())))
    }
}

impl Drop for StagedArtifact {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}
