use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    models::{
        app_error::AppError,
        recovery::{
            RecoveryEntry, RecoveryInventory, RecoveryIssue, RecoveryReceipt, RecoverySnapshot,
        },
        resource::ResourceRef,
    },
    utils::{atomic_write::atomic_write, bounded_read::OpenedFile, path_utils::app_data_dir},
};

const MAX_CONTENT_BYTES: usize = 10 * 1024 * 1024;
const MAX_RECORD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RECORDS: usize = 4096;
const MAX_COPIES: usize = 128;
const RETENTION_DAYS: i64 = 30;
const RESOLVED_RETENTION_MINUTES: i64 = 5;
const MAX_TOMBSTONE_BYTES: u64 = 1024;
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    version: u32,
    receipt: RecoveryReceipt,
    // None is a resolved tombstone. It rejects late writes for the old id.
    snapshot: Option<RecoverySnapshot>,
    resolved_at: Option<String>,
}

pub fn save(snapshot: RecoverySnapshot) -> Result<RecoveryReceipt, AppError> {
    with_store(|store| store.save(snapshot))
}

pub fn list() -> Result<RecoveryInventory, AppError> {
    with_store(|store| store.list())
}

pub fn read(id: &str) -> Result<RecoverySnapshot, AppError> {
    with_store(|store| store.load(id)?.snapshot.ok_or_else(closed))
}

pub fn resolve(receipt: RecoveryReceipt) -> Result<(), AppError> {
    with_store(|store| store.resolve(receipt))
}

fn with_store<T>(operation: impl FnOnce(&Store) -> Result<T, AppError>) -> Result<T, AppError> {
    let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let directory = app_data_dir()?.join("recovery");
    fs::create_dir_all(&directory).map_err(io_error)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(io_error)?;
    }
    operation(&Store {
        directory,
        quota: MAX_TOTAL_BYTES,
    })
}

struct Store {
    directory: PathBuf,
    quota: u64,
}

impl Store {
    fn path(&self, id: &str) -> Result<PathBuf, AppError> {
        if id.is_empty()
            || id.len() > 80
            || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(invalid());
        }
        Ok(self.directory.join(format!("recovery-{id}.json")))
    }

    fn load(&self, id: &str) -> Result<Record, AppError> {
        let path = self.path(id)?;
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_RECORD_BYTES {
            return Err(invalid());
        }
        let bytes = OpenedFile::open(&path)
            .and_then(|file| file.read_bounded(MAX_RECORD_BYTES))
            .map_err(io_error)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if value.get("version").and_then(|v| v.as_u64()) != Some(1) {
            return Err(AppError::new(
                "RECOVERY_VERSION_UNSUPPORTED",
                "恢复副本版本不受支持，原副本已保留",
            ));
        }
        let record: Record = serde_json::from_value(value).map_err(|_| invalid())?;
        if record.receipt.id != id
            || DateTime::parse_from_rfc3339(&record.receipt.updated_at).is_err()
        {
            return Err(invalid());
        }
        if record.receipt.revision > 9_007_199_254_740_991
            || !record
                .receipt
                .checksum
                .strip_prefix("sha256:")
                .is_some_and(|digest| {
                    digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit())
                })
            || record.snapshot.is_some() != record.resolved_at.is_none()
            || record
                .resolved_at
                .as_ref()
                .is_some_and(|time| DateTime::parse_from_rfc3339(time).is_err())
        {
            return Err(invalid());
        }
        if let Some(snapshot) = &record.snapshot {
            validate(snapshot)?;
            if snapshot.id != id
                || snapshot.revision != record.receipt.revision
                || checksum(snapshot)? != record.receipt.checksum
            {
                return Err(AppError::new(
                    "RECOVERY_CHECKSUM_MISMATCH",
                    "恢复副本校验失败，原副本已保留",
                ));
            }
        }
        Ok(record)
    }

    fn save(&self, snapshot: RecoverySnapshot) -> Result<RecoveryReceipt, AppError> {
        self.save_with(snapshot, atomic_write)
    }

    fn save_with(
        &self,
        snapshot: RecoverySnapshot,
        persist: impl FnOnce(&Path, &[u8]) -> std::io::Result<()>,
    ) -> Result<RecoveryReceipt, AppError> {
        validate(&snapshot)?;
        let path = self.path(&snapshot.id)?;
        let digest = checksum(&snapshot)?;
        if path.try_exists().map_err(io_error)? {
            let old = self.load(&snapshot.id)?;
            if old.snapshot.is_none() {
                return Err(closed());
            }
            if old.receipt.revision >= snapshot.revision {
                return if old.receipt.revision == snapshot.revision
                    && old.receipt.checksum == digest
                {
                    Ok(old.receipt)
                } else {
                    Err(stale())
                };
            }
        }
        let record = Record {
            version: 1,
            receipt: RecoveryReceipt {
                id: snapshot.id.clone(),
                revision: snapshot.revision,
                checksum: digest,
                updated_at: Utc::now().to_rfc3339(),
            },
            snapshot: Some(snapshot),
            resolved_at: None,
        };
        let bytes = serde_json::to_vec(&record).map_err(|_| invalid())?;
        self.check_quota(&path, bytes.len() as u64)?;
        persist(&path, &bytes).map_err(io_error)?;
        Ok(record.receipt)
    }

    fn resolve(&self, receipt: RecoveryReceipt) -> Result<(), AppError> {
        self.resolve_with(receipt, atomic_write)
    }

    fn resolve_with(
        &self,
        receipt: RecoveryReceipt,
        persist: impl FnOnce(&Path, &[u8]) -> std::io::Result<()>,
    ) -> Result<(), AppError> {
        let mut record = self.load(&receipt.id)?;
        if record.receipt != receipt {
            return Err(stale());
        }
        if record.snapshot.is_none() {
            return Ok(());
        }
        record.snapshot = None;
        record.resolved_at = Some(Utc::now().to_rfc3339());
        // Retain the exact receipt: repeat resolution is idempotent.
        let bytes = serde_json::to_vec(&record).map_err(|_| invalid())?;
        persist(&self.path(&receipt.id)?, &bytes).map_err(io_error)
    }

    fn paths(&self) -> Result<Vec<PathBuf>, AppError> {
        let mut paths = Vec::new();
        for entry in fs::read_dir(&self.directory).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            paths.push(entry.path());
            if paths.len() > MAX_RECORDS {
                return Err(quota());
            }
        }
        Ok(paths)
    }

    fn check_quota(&self, replacing: &Path, bytes: u64) -> Result<(), AppError> {
        let mut total = bytes;
        let mut copies = 1;
        let mut records = 1;
        for path in self.paths()? {
            if path == replacing {
                continue;
            }
            let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
            if !metadata.file_type().is_file() {
                return Err(invalid());
            }
            // A valid resolved record has no body and fits in 1 KiB. Large
            // records count as copies without re-reading/hash-checking their text.
            let record = if metadata.len() <= MAX_TOMBSTONE_BYTES {
                let id = path
                    .file_stem()
                    .and_then(|v| v.to_str())
                    .and_then(|v| v.strip_prefix("recovery-"))
                    .unwrap_or_default();
                self.load(id).ok()
            } else {
                None
            };
            if record.as_ref().is_some_and(expired) {
                fs::remove_file(&path).map_err(io_error)?;
                continue;
            }
            records += 1;
            total = total.checked_add(metadata.len()).ok_or_else(quota)?;
            // Unknown, damaged and future records count against quota; never delete them.
            if record.is_none_or(|r| r.snapshot.is_some()) {
                copies += 1;
            }
            if total > self.quota || copies > MAX_COPIES || records > MAX_RECORDS {
                return Err(quota());
            }
        }
        if total > self.quota {
            return Err(quota());
        }
        Ok(())
    }

    fn list(&self) -> Result<RecoveryInventory, AppError> {
        let mut inventory = RecoveryInventory::default();
        let cutoff = Utc::now() - Duration::days(RETENTION_DAYS);
        for path in self.paths()? {
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|v| v.to_str())
                .and_then(|v| v.strip_prefix("recovery-"))
                .unwrap_or_default();
            match self.load(id) {
                Ok(record) => {
                    let updated = DateTime::parse_from_rfc3339(&record.receipt.updated_at)
                        .map_err(|_| invalid())?;
                    if let Some(snapshot) = &record.snapshot {
                        inventory.entries.push(RecoveryEntry {
                            receipt: record.receipt,
                            title: snapshot.title.clone(),
                            resource: snapshot.resource.clone(),
                            content_bytes: snapshot.content.len(),
                            stale: updated < cutoff,
                        });
                    } else if expired(&record) {
                        // Only a validated, explicitly resolved record may expire.
                        fs::remove_file(&path).map_err(io_error)?;
                    }
                }
                Err(error) => inventory.issues.push(RecoveryIssue {
                    id: id.to_owned(),
                    error,
                }),
            }
        }
        inventory
            .entries
            .sort_by(|a, b| b.receipt.updated_at.cmp(&a.receipt.updated_at));
        Ok(inventory)
    }
}

fn expired(record: &Record) -> bool {
    record.snapshot.is_none()
        && record
            .resolved_at
            .as_ref()
            .and_then(|time| DateTime::parse_from_rfc3339(time).ok())
            .is_some_and(|time| time < Utc::now() - Duration::minutes(RESOLVED_RETENTION_MINUTES))
}

fn validate(snapshot: &RecoverySnapshot) -> Result<(), AppError> {
    if snapshot.content.len() > MAX_CONTENT_BYTES
        || snapshot.title.len() > 1024
        || snapshot.revision > 9_007_199_254_740_991
        || snapshot
            .base_file_identity
            .as_ref()
            .is_some_and(|s| s.len() > 512)
        || snapshot
            .base_content_version
            .as_ref()
            .is_some_and(|s| s.len() > 128)
    {
        return Err(invalid());
    }
    if let Some(resource) = &snapshot.resource {
        let value = match resource {
            ResourceRef::DesktopFile { path } => path,
            ResourceRef::DesktopDirectory { .. } => return Err(invalid()),
            ResourceRef::AndroidDocument { uri } | ResourceRef::AndroidTree { uri } => uri,
        };
        if value.is_empty() || value.len() > 32_768 || value.contains('\0') {
            return Err(invalid());
        }
    }
    Ok(())
}

fn checksum(snapshot: &RecoverySnapshot) -> Result<String, AppError> {
    let bytes = serde_json::to_vec(snapshot).map_err(|_| invalid())?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn invalid() -> AppError {
    AppError::new("RECOVERY_INVALID", "恢复副本无效，未修改原文件")
}
fn stale() -> AppError {
    AppError::new(
        "RECOVERY_REVISION_CONFLICT",
        "恢复修订已变化，未覆盖或删除较新的副本",
    )
}
fn closed() -> AppError {
    AppError::new("RECOVERY_RESOLVED", "恢复副本已处理，旧任务不能再次写入")
}
fn quota() -> AppError {
    AppError::new(
        "RECOVERY_QUOTA_EXCEEDED",
        "恢复存储额度已满，请保存或处理已有恢复副本",
    )
}
fn io_error(error: std::io::Error) -> AppError {
    AppError::new(
        "RECOVERY_IO_FAILED",
        format!("恢复副本读写失败：{}", error.kind()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::test_support::TestDirectory;

    fn snapshot(revision: u64) -> RecoverySnapshot {
        RecoverySnapshot {
            id: "recovery-test".into(),
            revision,
            title: "Untitled.md".into(),
            content: format!("unsaved revision {revision}"),
            resource: None,
            base_file_identity: None,
            base_content_version: None,
        }
    }

    fn store(directory: &TestDirectory) -> Store {
        Store {
            directory: directory.path().to_owned(),
            quota: MAX_TOTAL_BYTES,
        }
    }

    #[test]
    fn shared_contract_preserves_source_version_and_checksum() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../src/shared/recovery-contract-fixtures.json"
        ))
        .unwrap();
        let snapshot: RecoverySnapshot =
            serde_json::from_value(fixture["snapshot"].clone()).unwrap();
        let receipt: RecoveryReceipt = serde_json::from_value(fixture["receipt"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&snapshot).unwrap(),
            fixture["snapshot"]
        );
        assert_eq!(checksum(&snapshot).unwrap(), receipt.checksum);
        assert_eq!(serde_json::to_value(receipt).unwrap(), fixture["receipt"]);
    }

    #[test]
    fn acknowledged_untitled_revision_survives_reopening() {
        let directory = TestDirectory::new("recovery-reopen");
        let receipt = store(&directory).save(snapshot(1)).unwrap();
        let reopened = store(&directory);
        assert_eq!(
            reopened.load(&receipt.id).unwrap().snapshot.unwrap(),
            snapshot(1)
        );
        assert_eq!(reopened.list().unwrap().entries[0].receipt, receipt);
        assert_eq!(reopened.save(snapshot(1)).unwrap(), receipt);
    }

    #[test]
    fn older_write_or_resolution_cannot_destroy_newer_revision() {
        let directory = TestDirectory::new("recovery-revisions");
        let store = store(&directory);
        let first = store.save(snapshot(1)).unwrap();
        let second = store.save(snapshot(2)).unwrap();
        assert_eq!(
            store.resolve(first).unwrap_err().code,
            "RECOVERY_REVISION_CONFLICT"
        );
        assert_eq!(
            store.save(snapshot(1)).unwrap_err().code,
            "RECOVERY_REVISION_CONFLICT"
        );
        store.resolve(second.clone()).unwrap();
        store.resolve(second).unwrap();
        assert_eq!(
            store.save(snapshot(3)).unwrap_err().code,
            "RECOVERY_RESOLVED"
        );
        assert!(store.list().unwrap().entries.is_empty());
    }

    #[test]
    fn quota_failure_retains_last_acknowledged_content() {
        let directory = TestDirectory::new("recovery-quota");
        let mut store = store(&directory);
        let first = store.save(snapshot(1)).unwrap();
        store.quota = 1;
        assert_eq!(
            store.save(snapshot(2)).unwrap_err().code,
            "RECOVERY_QUOTA_EXCEEDED"
        );
        assert_eq!(store.load(&first.id).unwrap().receipt, first);
        assert_eq!(
            store.load(&first.id).unwrap().snapshot.unwrap(),
            snapshot(1)
        );
    }

    #[test]
    fn disk_full_during_save_or_resolution_keeps_the_acknowledged_copy() {
        let directory = TestDirectory::new("recovery-disk-full");
        let store = store(&directory);
        let first = store.save(snapshot(1)).unwrap();
        let path = store.path(&first.id).unwrap();
        let original = fs::read(&path).unwrap();
        let full = |_: &Path, _: &[u8]| Err(std::io::Error::from(std::io::ErrorKind::StorageFull));
        assert_eq!(
            store.save_with(snapshot(2), full).unwrap_err().code,
            "RECOVERY_IO_FAILED"
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(
            store.resolve_with(first.clone(), full).unwrap_err().code,
            "RECOVERY_IO_FAILED"
        );
        assert_eq!(
            store.load(&first.id).unwrap().snapshot.unwrap(),
            snapshot(1)
        );
        assert_eq!(store.load(&first.id).unwrap().receipt, first);
    }

    #[test]
    fn future_and_damaged_records_are_reported_and_never_overwritten() {
        let directory = TestDirectory::new("recovery-invalid");
        let store = store(&directory);
        let receipt = store.save(snapshot(1)).unwrap();
        let path = store.path(&receipt.id).unwrap();
        for raw in [r#"{"version":2}"#, "broken json"] {
            fs::write(&path, raw).unwrap();
            assert!(store.save(snapshot(2)).is_err());
            assert!(store.resolve(receipt.clone()).is_err());
            assert_eq!(store.list().unwrap().issues.len(), 1);
            assert_eq!(fs::read_to_string(&path).unwrap(), raw);
        }
    }

    #[test]
    fn checksum_mismatch_and_traversal_are_rejected() {
        let directory = TestDirectory::new("recovery-integrity");
        let store = store(&directory);
        let receipt = store.save(snapshot(1)).unwrap();
        let path = store.path(&receipt.id).unwrap();
        let mut record = store.load(&receipt.id).unwrap();
        record.snapshot.as_mut().unwrap().content = "tampered".into();
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(
            store.load(&receipt.id).err().unwrap().code,
            "RECOVERY_CHECKSUM_MISMATCH"
        );
        assert!(store.path("../outside").is_err());
        assert!(store.path("C:\\outside").is_err());
    }

    #[test]
    fn expiration_removes_only_resolved_records() {
        let directory = TestDirectory::new("recovery-retention");
        let store = store(&directory);
        let receipt = store.save(snapshot(1)).unwrap();
        let mut record = store.load(&receipt.id).unwrap();
        record.receipt.updated_at = (Utc::now() - Duration::days(31)).to_rfc3339();
        let path = store.path(&receipt.id).unwrap();
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(store.list().unwrap().entries[0].stale);
        assert!(path.exists());
        store.resolve(record.receipt).unwrap();
        assert!(store.list().unwrap().entries.is_empty());
        assert!(path.exists()); // retention starts when explicitly resolved
        let mut closed = store.load(&receipt.id).unwrap();
        closed.resolved_at = Some((Utc::now() - Duration::days(31)).to_rfc3339());
        fs::write(&path, serde_json::to_vec(&closed).unwrap()).unwrap();
        assert!(store.list().unwrap().entries.is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn writes_reclaim_expired_resolutions_without_removing_unresolved_text() {
        let directory = TestDirectory::new("recovery-resolved-quota");
        let store = store(&directory);
        let pending = store.save(snapshot(1)).unwrap();
        for index in 0..20 {
            let mut copy = snapshot(1);
            copy.id = format!("retired-{index}");
            let receipt = store.save(copy).unwrap();
            store.resolve(receipt.clone()).unwrap();
            let mut record = store.load(&receipt.id).unwrap();
            record.resolved_at = Some((Utc::now() - Duration::minutes(6)).to_rfc3339());
            fs::write(
                store.path(&receipt.id).unwrap(),
                serde_json::to_vec(&record).unwrap(),
            )
            .unwrap();
        }
        store.save(snapshot(2)).unwrap();
        assert_eq!(store.paths().unwrap().len(), 1);
        assert_eq!(
            store.load(&pending.id).unwrap().snapshot.unwrap(),
            snapshot(2)
        );
    }
}
