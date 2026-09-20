use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock, Weak,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use crate::utils::file_identity::file_identity;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static TARGET_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();

#[cfg(test)]
type WriteHook = std::sync::Arc<dyn Fn(&Path) + Send + Sync>;

#[cfg(test)]
static WRITE_HOOK: std::sync::OnceLock<std::sync::Mutex<Option<WriteHook>>> =
    std::sync::OnceLock::new();

pub fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    atomic_write_from(path, &mut &*content)
}

/// Atomically publishes a complete file only when the target does not exist.
/// The temporary file is fully written and synced before the no-replace move.
pub fn atomic_write_create_new(path: &Path, content: &[u8]) -> io::Result<()> {
    atomic_write_from_create_new(path, &mut &*content)
}

pub fn atomic_write_from_create_new(path: &Path, source: &mut impl Read) -> io::Result<()> {
    let lock_key = target_lock_key(path)?;
    let target_lock = target_lock(&lock_key);
    let lock_identity = Arc::downgrade(&target_lock);
    let guard = target_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let result = atomic_write_create_new_locked(path, source);
    drop(guard);
    drop(target_lock);
    release_target_lock(&lock_key, &lock_identity);
    result
}

#[derive(Debug)]
pub enum CheckedWriteError<E> {
    Lock(io::Error),
    Check(E),
    Write(io::Error),
}

/// Revalidates caller-owned state and replaces the target while holding the
/// same per-file transaction lock. The check therefore cannot race another
/// MarkLite writer for the same existing file, hard-link alias, or missing
/// target name.
pub fn atomic_write_checked<E>(
    path: &Path,
    content: &[u8],
    check: impl FnOnce(&Path) -> Result<(), E>,
) -> Result<(), CheckedWriteError<E>> {
    let lock_key = target_lock_key(path).map_err(CheckedWriteError::Lock)?;
    let target_lock = target_lock(&lock_key);
    let lock_identity = Arc::downgrade(&target_lock);
    let guard = target_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let result = check(path)
        .map_err(CheckedWriteError::Check)
        .and_then(|()| atomic_write_locked(path, &mut &*content).map_err(CheckedWriteError::Write));
    drop(guard);
    drop(target_lock);
    release_target_lock(&lock_key, &lock_identity);
    result
}

pub fn atomic_write_from(path: &Path, source: &mut impl Read) -> io::Result<()> {
    let lock_key = target_lock_key(path)?;
    let target_lock = target_lock(&lock_key);
    let lock_identity = Arc::downgrade(&target_lock);
    let guard = target_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let result = atomic_write_locked(path, source);
    drop(guard);
    drop(target_lock);
    release_target_lock(&lock_key, &lock_identity);
    result
}

fn atomic_write_locked(path: &Path, source: &mut impl Read) -> io::Result<()> {
    let mut temp = create_temp_file(path)?;
    #[cfg(test)]
    run_write_hook(path);

    io::copy(source, temp.file_mut())?;
    preserve_existing_permissions(temp.file_mut(), path)?;
    temp.file_mut().sync_all()?;
    temp.close();

    replace_file(temp.path(), path)?;
    temp.mark_committed();
    sync_parent_directory(path)?;
    Ok(())
}

fn atomic_write_create_new_locked(path: &Path, source: &mut impl Read) -> io::Result<()> {
    let mut temp = create_temp_file(path)?;
    io::copy(source, temp.file_mut())?;
    temp.file_mut().sync_all()?;
    temp.close();

    move_file_create_new(temp.path(), path)?;
    #[cfg(windows)]
    temp.mark_committed();
    sync_parent_directory(path)?;
    Ok(())
}

fn target_lock_key(path: &Path) -> io::Result<PathBuf> {
    if let Ok(file) = File::open(path) {
        if file.metadata()?.is_file() {
            return file_identity(&file).map(PathBuf::from);
        }
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "写入目标没有父目录"))?;
    let parent = fs::canonicalize(parent)?;
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "写入目标必须包含文件名"))?;
    Ok(parent.join(normalize_missing_file_name(&parent, file_name)?))
}

#[cfg(windows)]
fn normalize_missing_file_name(
    parent: &Path,
    file_name: &std::ffi::OsStr,
) -> io::Result<std::ffi::OsString> {
    use std::{
        mem::size_of,
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    };

    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FileCaseSensitiveInfo, GetFileInformationByHandleEx, FILE_CASE_SENSITIVE_INFO,
        },
    };

    let directory = OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2 | 0x4)
        .custom_flags(0x0200_0000)
        .open(parent)?;
    let mut information = FILE_CASE_SENSITIVE_INFO::default();
    // SAFETY: `directory` owns a valid handle for this call and `information`
    // is writable storage whose byte size matches the requested structure.
    let query = unsafe {
        GetFileInformationByHandleEx(
            HANDLE(directory.as_raw_handle()),
            FileCaseSensitiveInfo,
            (&mut information as *mut FILE_CASE_SENSITIVE_INFO).cast(),
            size_of::<FILE_CASE_SENSITIVE_INFO>() as u32,
        )
    };
    let case_sensitive = match query {
        Ok(()) => information.Flags & 1 != 0,
        Err(error) if matches!((error.code().0 as u32) & 0xffff, 50 | 87) => false,
        Err(error) => return Err(io::Error::other(error)),
    };
    if !case_sensitive {
        Ok(lowercase_windows_file_name(file_name))
    } else {
        Ok(file_name.to_os_string())
    }
}

#[cfg(windows)]
fn lowercase_windows_file_name(file_name: &std::ffi::OsStr) -> std::ffi::OsString {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let mut normalized = Vec::new();
    for decoded in char::decode_utf16(file_name.encode_wide()) {
        match decoded {
            Ok(value) => {
                for lowercase in value.to_lowercase() {
                    let mut buffer = [0_u16; 2];
                    normalized.extend_from_slice(lowercase.encode_utf16(&mut buffer));
                }
            }
            Err(error) => normalized.push(error.unpaired_surrogate()),
        }
    }
    std::ffi::OsString::from_wide(&normalized)
}

#[cfg(not(windows))]
fn normalize_missing_file_name(
    _parent: &Path,
    file_name: &std::ffi::OsStr,
) -> io::Result<std::ffi::OsString> {
    Ok(file_name.to_os_string())
}

fn target_lock(key: &Path) -> Arc<Mutex<()>> {
    let mut locks = TARGET_LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(lock) = locks.get(key).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(key.to_path_buf(), Arc::downgrade(&lock));
    lock
}

fn release_target_lock(key: &Path, lock_identity: &Weak<Mutex<()>>) {
    let mut locks = TARGET_LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if lock_identity.strong_count() == 0
        && locks
            .get(key)
            .is_some_and(|registered| registered.ptr_eq(lock_identity))
    {
        locks.remove(key);
    }
}

struct OwnedTempFile {
    path: PathBuf,
    file: Option<File>,
    committed: bool,
}

impl OwnedTempFile {
    fn file_mut(&mut self) -> &mut File {
        self.file
            .as_mut()
            .expect("owned temporary file must be open before replacement")
    }

    fn close(&mut self) {
        self.file.take();
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn mark_committed(&mut self) {
        self.committed = true;
    }
}

impl Drop for OwnedTempFile {
    fn drop(&mut self) {
        self.file.take();
        if !self.committed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn create_temp_file(path: &Path) -> io::Result<OwnedTempFile> {
    for _ in 0..100 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let suffix = format!(
            ".marklite-tmp-{}-{timestamp}-{sequence}",
            std::process::id()
        );
        let candidate = sibling_path(path, &suffix)?;
        #[cfg(windows)]
        let opened = create_private_windows_temp_file(path, &candidate);
        #[cfg(not(windows))]
        let opened = {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options.open(&candidate)
        };
        match opened {
            Ok(file) => {
                return Ok(OwnedTempFile {
                    path: candidate,
                    file: Some(file),
                    committed: false,
                })
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "无法分配 MarkLite 临时写入文件",
    ))
}

#[cfg(windows)]
fn create_private_windows_temp_file(target: &Path, candidate: &Path) -> io::Result<File> {
    use std::{
        mem::size_of,
        os::windows::{ffi::OsStrExt, io::FromRawHandle},
    };
    use windows::{
        core::{w, PCWSTR},
        Win32::{
            Foundation::{LocalFree, GENERIC_WRITE, HLOCAL},
            Security::{
                Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
                GetFileSecurityW, GetSecurityDescriptorDacl, DACL_SECURITY_INFORMATION,
                PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
            },
            Storage::FileSystem::{
                CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_MODE,
            },
        },
    };

    enum Descriptor {
        Existing(Vec<u8>),
        Private(PSECURITY_DESCRIPTOR),
    }
    impl Descriptor {
        fn pointer(&mut self) -> *mut std::ffi::c_void {
            match self {
                Self::Existing(bytes) => bytes.as_mut_ptr().cast(),
                Self::Private(descriptor) => descriptor.0,
            }
        }
    }
    impl Drop for Descriptor {
        fn drop(&mut self) {
            if let Self::Private(descriptor) = self {
                // SAFETY: ConvertStringSecurityDescriptorToSecurityDescriptorW allocated this buffer.
                unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
            }
        }
    }

    let mut descriptor = match fs::metadata(target) {
        Ok(_) => {
            let wide = target
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>();
            let mut required = 0u32;
            // SAFETY: the NUL-terminated path and output length remain valid for the call.
            let _ = unsafe {
                GetFileSecurityW(
                    PCWSTR(wide.as_ptr()),
                    DACL_SECURITY_INFORMATION.0,
                    None,
                    0,
                    &mut required,
                )
            };
            if required == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut bytes = vec![0u8; required as usize];
            // SAFETY: the buffer has the size requested by GetFileSecurityW.
            if !unsafe {
                GetFileSecurityW(
                    PCWSTR(wide.as_ptr()),
                    DACL_SECURITY_INFORMATION.0,
                    Some(PSECURITY_DESCRIPTOR(bytes.as_mut_ptr().cast())),
                    required,
                    &mut required,
                )
            }
            .as_bool()
            {
                return Err(io::Error::last_os_error());
            }
            let mut present = windows::core::BOOL(0);
            let mut dacl = std::ptr::null_mut();
            let mut defaulted = windows::core::BOOL(0);
            // SAFETY: bytes contains the descriptor returned by GetFileSecurityW.
            unsafe {
                GetSecurityDescriptorDacl(
                    PSECURITY_DESCRIPTOR(bytes.as_mut_ptr().cast()),
                    &mut present,
                    &mut dacl,
                    &mut defaulted,
                )
            }
            .map_err(io::Error::other)?;
            if !present.as_bool() || dacl.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "目标文件没有可安全复制的 DACL",
                ));
            }
            Descriptor::Existing(bytes)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut pointer = PSECURITY_DESCRIPTOR(std::ptr::null_mut());
            // SAFETY: the API owns the returned self-relative descriptor until LocalFree.
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    w!("D:P(A;;FA;;;OW)(A;;FA;;;SY)"),
                    1,
                    &mut pointer,
                    None,
                )
            }
            .map_err(io::Error::other)?;
            Descriptor::Private(pointer)
        }
        Err(error) => return Err(error),
    };
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.pointer(),
        bInheritHandle: false.into(),
    };
    let wide = candidate
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: the descriptor, attributes and NUL-terminated path remain live for this call.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            Some(&attributes),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(io::Error::other)?;
    // SAFETY: CreateFileW returned an owned, valid handle and File takes ownership exactly once.
    Ok(unsafe { File::from_raw_handle(handle.0) })
}

#[cfg(unix)]
fn preserve_existing_permissions(temp: &File, target: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    match fs::metadata(target) {
        Ok(metadata) if metadata.is_file() => temp.set_permissions(fs::Permissions::from_mode(
            metadata.permissions().mode() & 0o7777,
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(not(unix))]
fn preserve_existing_permissions(_temp: &File, _target: &Path) -> io::Result<()> {
    Ok(())
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

#[cfg(windows)]
fn replace_file(temp: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, target: *const u16, flags: u32) -> i32;
    }

    let existing = temp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: Both pointers reference live, NUL-terminated UTF-16 buffers for the
    // duration of the call. The paths are same-directory files allocated here.
    let moved = unsafe {
        MoveFileExW(
            existing.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(temp: &Path, target: &Path) -> io::Result<()> {
    fs::rename(temp, target)
}

#[cfg(windows)]
fn move_file_create_new(temp: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, target: *const u16, flags: u32) -> i32;
    }

    let existing = temp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: Both pointers are live NUL-terminated UTF-16 buffers. Omitting
    // MOVEFILE_REPLACE_EXISTING makes publication fail atomically on conflict.
    let moved = unsafe { MoveFileExW(existing.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH) };
    if moved == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn move_file_create_new(temp: &Path, target: &Path) -> io::Result<()> {
    // Same-directory hard-link creation is atomic and fails when `target`
    // exists. The temporary name is removed by `OwnedTempFile::drop`.
    fs::hard_link(temp, target)
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "写入目标没有父目录"))?;
    File::open(parent)?.sync_all()
}

#[cfg(windows)]
fn sync_parent_directory(_path: &Path) -> io::Result<()> {
    // MoveFileExW(MOVEFILE_WRITE_THROUGH) is the available durability barrier for
    // the replacement; std does not expose opening directory handles on Windows.
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn sync_parent_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
fn run_write_hook(path: &Path) {
    let hook = WRITE_HOOK
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(hook) = hook {
        hook(path);
    }
}

#[cfg(test)]
fn set_write_hook(hook: Option<WriteHook>) {
    *WRITE_HOOK
        .get_or_init(|| std::sync::Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = hook;
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Condvar, Mutex,
        },
        thread,
        time::Duration,
    };

    use super::{
        atomic_write, atomic_write_create_new, atomic_write_from, set_write_hook, target_lock_key,
        TARGET_LOCKS,
    };
    use crate::utils::test_support::TestPath;

    fn test_path(name: &str) -> TestPath {
        TestPath::new("atomic-write", format!("{name}.txt"))
    }

    #[cfg(unix)]
    #[test]
    fn unix_temp_starts_private_and_existing_mode_survives_replacement() {
        use std::os::unix::fs::PermissionsExt;

        let new_path = test_path("unix-private-new");
        let temp = super::create_temp_file(&new_path).unwrap();
        assert_eq!(
            fs::metadata(temp.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(temp);
        assert!(!new_path.exists());

        atomic_write(&new_path, b"new").unwrap();
        assert_eq!(
            fs::metadata(&new_path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        for mode in [0o600, 0o640] {
            fs::set_permissions(&new_path, fs::Permissions::from_mode(mode)).unwrap();
            atomic_write(&new_path, b"replacement").unwrap();
            assert_eq!(
                fs::metadata(&new_path).unwrap().permissions().mode() & 0o777,
                mode
            );
            assert_eq!(fs::read(&new_path).unwrap(), b"replacement");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_replacement_keeps_an_explicit_dacl_and_new_temp_is_protected() {
        use std::{path::Path, process::Command};

        fn powershell(path: &Path, script: &str) -> String {
            let modules = std::env::var_os("WINDIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
                .join("System32/WindowsPowerShell/v1.0/Modules");
            let output = Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &format!("$ErrorActionPreference='Stop'; {script}"),
                ])
                .env("MARKLITE_ACL_TEST_PATH", path)
                .env("PSModulePath", modules)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap().trim().to_string()
        }

        let path = test_path("windows-explicit-acl");
        fs::write(&path, b"old").unwrap();
        powershell(
            &path,
            r#"
            $acl = Get-Acl -LiteralPath $env:MARKLITE_ACL_TEST_PATH
            $acl.SetAccessRuleProtection($true, $false)
            $owner = [System.Security.AccessControl.FileSystemAccessRule]::new($acl.Owner, 'FullControl', 'Allow')
            $users = [System.Security.AccessControl.FileSystemAccessRule]::new(
              [System.Security.Principal.SecurityIdentifier]::new('S-1-5-32-545'), 'ReadData', 'Allow')
            $acl.SetAccessRule($owner)
            $acl.AddAccessRule($users)
            Set-Acl -LiteralPath $env:MARKLITE_ACL_TEST_PATH -AclObject $acl
        "#,
        );
        let before = powershell(
            &path,
            "(Get-Acl -LiteralPath $env:MARKLITE_ACL_TEST_PATH).Sddl",
        );
        let temp = super::create_temp_file(&path).unwrap();
        let temporary = powershell(
            temp.path(),
            "(Get-Acl -LiteralPath $env:MARKLITE_ACL_TEST_PATH).Sddl",
        );
        assert_eq!(
            temporary.replace("D:PAI", "D:P"),
            before.replace("D:PAI", "D:P")
        );
        drop(temp);

        atomic_write(&path, b"replacement").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"replacement");
        let after = powershell(
            &path,
            "(Get-Acl -LiteralPath $env:MARKLITE_ACL_TEST_PATH).Sddl",
        );
        assert_eq!(
            after.replace("D:PAI", "D:P"),
            before.replace("D:PAI", "D:P")
        );

        let new_path = test_path("windows-private-temp");
        let temp = super::create_temp_file(&new_path).unwrap();
        let access = powershell(
            temp.path(),
            r#"
            $acl = Get-Acl -LiteralPath $env:MARKLITE_ACL_TEST_PATH
            "$($acl.AreAccessRulesProtected)|$(($acl.Access | ForEach-Object { $_.IdentityReference.Value }) -join ',')"
        "#,
        );
        assert!(access.starts_with("True|"), "{access}");
        assert!(!access.contains("BUILTIN\\Users"), "{access}");
        drop(temp);
    }

    #[cfg(windows)]
    #[test]
    fn windows_missing_target_lock_normalization_preserves_unpaired_surrogates() {
        use std::{
            ffi::OsString,
            os::windows::ffi::{OsStrExt, OsStringExt},
        };

        let original = OsString::from_wide(&[b'A' as u16, 0xD800]);
        let normalized = super::lowercase_windows_file_name(&original);

        assert_eq!(
            normalized.encode_wide().collect::<Vec<_>>(),
            [b'a' as u16, 0xD800]
        );
    }

    #[test]
    fn replaces_an_existing_file_without_partial_content() {
        let path = test_path("replace");
        fs::write(&path, "old").unwrap();

        atomic_write(&path, b"new content").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new content");
    }

    #[test]
    fn create_new_publishes_complete_content_and_preserves_conflicts() {
        let path = test_path("create-new");

        atomic_write_create_new(&path, b"first").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"first");

        let error = atomic_write_create_new(&path, b"second").unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&path).unwrap(), b"first");
        let temp_prefix = format!("{}.", path.file_name().unwrap().to_string_lossy());
        assert!(!path.parent().unwrap().read_dir().unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(&temp_prefix)
        }));
    }

    #[test]
    fn streams_reader_content_without_requiring_a_source_buffer() {
        struct RepeatingReader {
            remaining: usize,
        }

        impl std::io::Read for RepeatingReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let read = self.remaining.min(buffer.len());
                buffer[..read].fill(b'x');
                self.remaining -= read;
                Ok(read)
            }
        }

        let path = test_path("stream");
        let length = 2 * 1024 * 1024;
        let mut source = RepeatingReader { remaining: length };

        atomic_write_from(&path, &mut source).unwrap();

        assert_eq!(fs::metadata(&path).unwrap().len(), length as u64);
    }

    #[test]
    fn source_failure_preserves_the_target_and_removes_the_owned_temp() {
        struct FailingReader {
            first_read: bool,
        }

        impl std::io::Read for FailingReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if self.first_read {
                    return Err(std::io::Error::other("fixture read failure"));
                }
                self.first_read = true;
                let bytes = b"partial";
                buffer[..bytes.len()].copy_from_slice(bytes);
                Ok(bytes.len())
            }
        }

        let path = test_path("stream-failure");
        fs::write(&path, "original").unwrap();
        let mut source = FailingReader { first_read: false };

        assert!(atomic_write_from(&path, &mut source).is_err());

        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
        let parent = path.parent().unwrap();
        let file_name = path.file_name().unwrap().to_string_lossy();
        let leftovers = fs::read_dir(parent)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{file_name}.marklite-tmp-"))
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn never_deletes_an_unrelated_legacy_backup_name() {
        let path = test_path("unrelated-backup");
        let mut unrelated_name = path.file_name().unwrap().to_os_string();
        unrelated_name.push(".marklite-backup");
        let unrelated = path.with_file_name(unrelated_name);
        fs::write(&path, "old").unwrap();
        fs::write(&unrelated, "user-owned").unwrap();

        atomic_write(&path, b"new").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(fs::read_to_string(&unrelated).unwrap(), "user-owned");
    }

    #[test]
    fn failed_replacement_cleans_only_its_owned_temporary_file() {
        let parent = test_path("cleanup-parent");
        fs::create_dir(&parent).unwrap();
        let target = parent.join("target.md");
        fs::create_dir(&target).unwrap();

        assert!(atomic_write(&target, b"content").is_err());

        assert!(target.is_dir());
        let leftovers = fs::read_dir(&parent)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path() != target)
            .collect::<Vec<_>>();
        assert!(leftovers.is_empty(), "owned temp was not removed");
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
        let key = target_lock_key(path.as_ref()).unwrap();
        let locks = TARGET_LOCKS
            .get()
            .unwrap()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(!locks.contains_key(&key));
        drop(locks);
    }

    #[test]
    fn releases_per_target_lock_entries_after_writes_complete() {
        let paths = (0..16)
            .map(|index| test_path(&format!("lock-cleanup-{index}")))
            .collect::<Vec<_>>();
        let keys = paths
            .iter()
            .map(|path| target_lock_key(path).unwrap())
            .collect::<Vec<_>>();

        for path in &paths {
            atomic_write(path, b"content").unwrap();
        }

        let locks = TARGET_LOCKS
            .get()
            .unwrap()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(keys.iter().all(|key| !locks.contains_key(key)));
        drop(locks);
    }

    #[test]
    fn independent_targets_can_enter_the_write_phase_concurrently() {
        let first = Arc::new(test_path("parallel-a"));
        let second = Arc::new(test_path("parallel-b"));
        let targets = Arc::new([first.path().to_path_buf(), second.path().to_path_buf()]);
        let rendezvous = Arc::new((Mutex::new(0_usize), Condvar::new()));
        let timed_out = Arc::new(AtomicBool::new(false));

        set_write_hook(Some(Arc::new({
            let targets = Arc::clone(&targets);
            let rendezvous = Arc::clone(&rendezvous);
            let timed_out = Arc::clone(&timed_out);
            move |path| {
                if !targets.iter().any(|target| target == path) {
                    return;
                }
                let (lock, ready) = &*rendezvous;
                let mut count = lock.lock().unwrap();
                *count += 1;
                ready.notify_all();
                while *count < 2 {
                    let (next, wait) = ready.wait_timeout(count, Duration::from_secs(2)).unwrap();
                    count = next;
                    if wait.timed_out() {
                        timed_out.store(true, Ordering::SeqCst);
                        break;
                    }
                }
            }
        })));

        let first_writer = {
            let path = Arc::clone(&first);
            thread::spawn(move || atomic_write(path.as_ref(), b"first"))
        };
        let second_writer = {
            let path = Arc::clone(&second);
            thread::spawn(move || atomic_write(path.as_ref(), b"second"))
        };
        first_writer.join().unwrap().unwrap();
        second_writer.join().unwrap().unwrap();
        set_write_hook(None);

        assert!(
            !timed_out.load(Ordering::SeqCst),
            "unrelated targets were serialized by a process-wide lock"
        );
        assert_eq!(fs::read_to_string(first.as_ref()).unwrap(), "first");
        assert_eq!(fs::read_to_string(second.as_ref()).unwrap(), "second");
    }
}
