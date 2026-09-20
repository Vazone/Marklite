use std::{fs::File, io};

/// Returns an opaque identity for the file referenced by this exact open handle.
///
/// The value is only suitable for equality comparisons inside MarkLite. It must
/// never be displayed as a path or parsed by callers.
#[cfg(windows)]
pub fn file_identity(file: &File) -> io::Result<String> {
    use std::{mem::MaybeUninit, os::windows::io::AsRawHandle};

    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION},
    };

    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: `file` owns a valid handle for the duration of the call and
    // `information` points to writable storage of the required type.
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), information.as_mut_ptr())
            .map_err(io::Error::other)?;
        let information = information.assume_init();
        let file_index =
            (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow);
        Ok(format!(
            "marklite-file:v1:windows:{:08x}:{file_index:016x}",
            information.dwVolumeSerialNumber
        ))
    }
}

#[cfg(unix)]
pub fn file_identity(file: &File) -> io::Result<String> {
    use std::os::unix::fs::MetadataExt;

    let metadata = file.metadata()?;
    Ok(format!(
        "marklite-file:v1:unix:{:016x}:{:016x}",
        metadata.dev(),
        metadata.ino()
    ))
}

#[cfg(not(any(windows, unix)))]
compile_error!("MarkLite file identity is not implemented for this target");
