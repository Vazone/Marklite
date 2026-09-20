use std::{
    ops::Deref,
    path::{Path, PathBuf},
};

pub struct TestDirectory {
    inner: tempfile::TempDir,
}

impl TestDirectory {
    pub fn new(label: &str) -> Self {
        Self {
            inner: tempfile::Builder::new()
                .prefix(&format!("marklite-{label}-"))
                .tempdir()
                .expect("create isolated test directory"),
        }
    }

    pub fn path(&self) -> &Path {
        self.inner.path()
    }
}

impl AsRef<Path> for TestDirectory {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl Deref for TestDirectory {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

pub struct TestPath {
    _directory: TestDirectory,
    path: PathBuf,
}

impl TestPath {
    pub fn new(label: &str, file_name: impl AsRef<Path>) -> Self {
        let directory = TestDirectory::new(label);
        let path = directory.path().join(file_name);
        Self {
            _directory: directory,
            path,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TestPath {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl Deref for TestPath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        panic::{catch_unwind, AssertUnwindSafe},
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    use super::{TestDirectory, TestPath};

    #[test]
    fn removes_the_directory_while_unwinding() {
        let observed_path = Arc::new(Mutex::new(None::<PathBuf>));
        let captured_path = Arc::clone(&observed_path);

        let result = catch_unwind(AssertUnwindSafe(move || {
            let directory = TestDirectory::new("raii-panic");
            let path = directory.path().to_path_buf();
            *captured_path.lock().unwrap() = Some(path.clone());
            assert!(path.exists());
            panic!("exercise RAII cleanup");
        }));

        assert!(result.is_err());
        let path = observed_path.lock().unwrap().clone().unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_path_keeps_its_parent_alive_and_cleans_it_on_drop() {
        let path = {
            let fixture = TestPath::new("owned-path", "nested/file.md");
            std::fs::create_dir_all(fixture.path().parent().unwrap()).unwrap();
            std::fs::write(fixture.path(), b"fixture").unwrap();
            let path = fixture.path().to_path_buf();
            assert!(path.exists());
            path
        };

        assert!(!path.exists());
        assert!(!path.parent().unwrap().exists());
    }
}
