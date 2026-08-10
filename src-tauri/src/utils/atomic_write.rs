use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
};

static WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const BACKUP_SUFFIX: &str = ".marklite-backup";

pub fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    let _guard = write_lock();
    recover_atomic_write_inner(path)?;

    let (temp_path, mut file) = create_temp_file(path)?;
    let write_result = (|| {
        file.write_all(content)?;
        file.sync_all()?;
        Ok::<(), io::Error>(())
    })();
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    let backup_path = sibling_path(path, BACKUP_SUFFIX)?;
    let had_original = path.exists();
    if had_original {
        if backup_path.exists() {
            fs::remove_file(&backup_path)?;
        }
        if let Err(error) = fs::rename(path, &backup_path) {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }
    }

    if let Err(error) = fs::rename(&temp_path, path) {
        let rollback = if had_original {
            fs::rename(&backup_path, path)
        } else {
            Ok(())
        };
        let _ = fs::remove_file(&temp_path);
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(io::Error::new(
                error.kind(),
                format!(
                    "替换失败：{error}；旧文件保留在 {}，回滚失败：{rollback_error}",
                    backup_path.display()
                ),
            )),
        };
    }

    if had_original {
        let _ = fs::remove_file(backup_path);
    }
    Ok(())
}

pub fn recover_atomic_write(path: &Path) -> io::Result<()> {
    let _guard = write_lock();
    recover_atomic_write_inner(path)
}

fn recover_atomic_write_inner(path: &Path) -> io::Result<()> {
    let backup_path = sibling_path(path, BACKUP_SUFFIX)?;
    match (path.exists(), backup_path.exists()) {
        (false, true) => fs::rename(backup_path, path),
        (true, true) => {
            let _ = fs::remove_file(backup_path);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn create_temp_file(path: &Path) -> io::Result<(PathBuf, File)> {
    for _ in 0..100 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let suffix = format!(".marklite-tmp-{}-{sequence}", std::process::id());
        let candidate = sibling_path(path, &suffix)?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "无法分配 MarkLite 临时写入文件",
    ))
}

fn sibling_path(path: &Path, suffix: &str) -> io::Result<PathBuf> {
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "写入目标必须包含文件名"))?;
    let mut sibling_name = file_name.to_os_string();
    sibling_name.push(suffix);
    Ok(path.with_file_name(sibling_name))
}

fn write_lock() -> std::sync::MutexGuard<'static, ()> {
    WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, sync::Arc, thread};

    use super::{atomic_write, recover_atomic_write, sibling_path, BACKUP_SUFFIX};

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "marklite-atomic-{}-{}-{name}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn replaces_an_existing_file_without_partial_content() {
        let path = test_path("replace");
        fs::write(&path, "old").unwrap();

        atomic_write(&path, b"new content").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
        assert!(!sibling_path(&path, BACKUP_SUFFIX).unwrap().exists());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn restores_an_interrupted_replacement_backup() {
        let path = test_path("recover");
        let backup = sibling_path(&path, BACKUP_SUFFIX).unwrap();
        fs::write(&backup, "preserved").unwrap();

        recover_atomic_write(&path).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "preserved");
        assert!(!backup.exists());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn serializes_concurrent_writers_without_mixing_content() {
        let path = Arc::new(test_path("concurrent"));
        let values = (0..8)
            .map(|index| format!("writer-{index}"))
            .collect::<Vec<_>>();
        let handles = values
            .iter()
            .cloned()
            .map(|value| {
                let path = Arc::clone(&path);
                thread::spawn(move || atomic_write(path.as_ref(), value.as_bytes()).unwrap())
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }

        let content = fs::read_to_string(path.as_ref()).unwrap();
        assert!(values.contains(&content));
        fs::remove_file(path.as_ref()).unwrap();
    }
}
