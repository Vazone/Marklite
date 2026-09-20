use std::{
    fs::{File, Metadata},
    io::{self, Read},
    path::Path,
};

/// An opened file and the metadata obtained from that exact handle.
///
/// Keeping both values together prevents callers from validating one path
/// identity and then reopening a different file for the actual read.
pub struct OpenedFile {
    file: File,
    metadata: Metadata,
}

impl OpenedFile {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        Ok(Self { file, metadata })
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn file(&self) -> &File {
        &self.file
    }

    pub fn read_bounded(self, max_bytes: u64) -> io::Result<Vec<u8>> {
        self.read_bounded_with_cancel(max_bytes, || false)
    }

    pub fn read_bounded_with_cancel(
        mut self,
        max_bytes: u64,
        is_cancelled: impl Fn() -> bool,
    ) -> io::Result<Vec<u8>> {
        let capacity = usize::try_from(self.metadata.len().min(max_bytes)).unwrap_or(0);
        let mut bytes = Vec::with_capacity(capacity);
        let mut remaining = max_bytes.saturating_add(1);
        let mut chunk = [0_u8; 64 * 1024];
        while remaining > 0 {
            if is_cancelled() {
                return Err(io::Error::new(io::ErrorKind::Interrupted, "文件读取已取消"));
            }
            let capacity =
                usize::try_from(remaining.min(chunk.len() as u64)).unwrap_or(chunk.len());
            let read = self.file.read(&mut chunk[..capacity])?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
            remaining = remaining.saturating_sub(read as u64);
        }
        if is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "文件读取已取消"));
        }
        if bytes.len() as u64 > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                format!("文件超过允许的 {max_bytes} 字节"),
            ));
        }
        Ok(bytes)
    }

    pub fn read_to_string_bounded(self, max_bytes: u64) -> io::Result<String> {
        String::from_utf8(self.read_bounded(max_bytes)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::OpenedFile;
    use crate::utils::test_support::TestPath;

    fn test_path(name: &str) -> TestPath {
        TestPath::new("bounded-read", name)
    }

    #[test]
    fn accepts_the_exact_limit_and_rejects_one_extra_byte() {
        let at_limit = test_path("at-limit");
        let over_limit = test_path("over-limit");
        fs::write(&at_limit, b"1234").unwrap();
        fs::write(&over_limit, b"12345").unwrap();

        assert_eq!(
            OpenedFile::open(&at_limit)
                .unwrap()
                .read_bounded(4)
                .unwrap(),
            b"1234"
        );
        assert_eq!(
            OpenedFile::open(&over_limit)
                .unwrap()
                .read_bounded(4)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::FileTooLarge
        );
    }

    #[test]
    fn cancellable_reads_stop_before_returning_bytes() {
        let path = test_path("cancelled");
        fs::write(&path, vec![7_u8; 128 * 1024]).unwrap();
        let checks = std::cell::Cell::new(0);

        let error = OpenedFile::open(&path)
            .unwrap()
            .read_bounded_with_cancel(256 * 1024, || {
                checks.set(checks.get() + 1);
                checks.get() >= 2
            })
            .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    }
}
