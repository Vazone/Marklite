use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::{
    models::app_error::AppError,
    services::export_service::ExportCommitPolicy,
    utils::{
        atomic_write::{atomic_write_from, atomic_write_from_create_new},
        path_utils::path_to_utf8,
    },
};

const PDF_TAIL_BYTES: u64 = 16 * 1024;
const PDF_XREF_PROBE_BYTES: usize = 8 * 1024;
const PDF_SCAN_CHUNK_BYTES: usize = 16 * 1024;
const PDF_SCAN_OVERLAP_BYTES: usize = 128;
const MIN_PDF_BYTES: u64 = 128;
static WORKSPACE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PENDING_WEBVIEW_CLEANUP: OnceLock<Mutex<Vec<PathBuf>>> = OnceLock::new();

pub struct PdfWorkspace {
    root: PathBuf,
    html_path: PathBuf,
    pdf_path: PathBuf,
    merged_pdf_path: PathBuf,
    webview_data_path: PathBuf,
}

impl PdfWorkspace {
    pub fn create(target: &Path) -> Result<Self, AppError> {
        let parent = target
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| AppError::new("INVALID_EXPORT_TARGET", "导出目标没有父目录"))?;
        let target_display = path_to_utf8(target)?;
        fs::create_dir_all(parent)
            .map_err(|error| AppError::file_write_failed(target_display, error))?;
        for _ in 0..100 {
            let sequence = WORKSPACE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let root = parent.join(format!(
                ".marklite-pdf-work-{}-{timestamp}-{sequence}",
                std::process::id()
            ));
            match create_private_directory(&root) {
                Ok(()) => {
                    return Ok(Self {
                        html_path: root.join("source.html"),
                        pdf_path: root.join("output.pdf"),
                        merged_pdf_path: root.join("merged.pdf"),
                        webview_data_path: root.join("webview-data"),
                        root,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(AppError::new(
                        "EXPORT_TEMP_FAILED",
                        format!("无法创建 PDF 私有临时目录：{error}"),
                    ));
                }
            }
        }
        Err(AppError::new(
            "EXPORT_TEMP_FAILED",
            "无法为 PDF 分配私有临时目录",
        ))
    }

    pub fn write_html(&self, html: &str) -> Result<(), AppError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.html_path)
            .map_err(|error| AppError::new("EXPORT_TEMP_FAILED", error.to_string()))?;
        file.write_all(html.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|error| AppError::new("EXPORT_TEMP_FAILED", error.to_string()))
    }

    pub fn html_path(&self) -> &Path {
        &self.html_path
    }

    pub fn pdf_path(&self) -> &Path {
        &self.pdf_path
    }

    pub fn merged_pdf_path(&self) -> &Path {
        &self.merged_pdf_path
    }

    pub fn clear_part(&self) -> Result<(), AppError> {
        for path in [&self.html_path, &self.pdf_path] {
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(AppError::new("EXPORT_TEMP_FAILED", error.to_string())),
            }
        }
        Ok(())
    }

    pub fn webview_data_path(&self) -> &Path {
        &self.webview_data_path
    }

    pub fn register_post_runtime_cleanup(&self) {
        pending_webview_cleanup()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(self.root.clone());
    }

    #[cfg(test)]
    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for PdfWorkspace {
    fn drop(&mut self) {
        // Only remove the two names allocated inside this workspace. A recursive
        // delete could erase an unrelated tree if the directory path were swapped.
        let _ = fs::remove_file(&self.html_path);
        let _ = fs::remove_file(&self.pdf_path);
        let _ = fs::remove_file(&self.merged_pdf_path);
        // This child is exclusively assigned to the hidden export WebView. It
        // may contain an engine-owned tree, unlike arbitrary workspace names.
        let _ = fs::remove_dir_all(&self.webview_data_path);
        let _ = fs::remove_dir(&self.root);
    }
}

fn pending_webview_cleanup() -> &'static Mutex<Vec<PathBuf>> {
    PENDING_WEBVIEW_CLEANUP.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg_attr(test, allow(dead_code))]
pub fn cleanup_pending_webview_data(timeout: Duration) -> usize {
    let deadline = Instant::now().checked_add(timeout);
    let roots = {
        let mut pending = pending_webview_cleanup()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut roots = std::mem::take(&mut *pending);
        roots.sort_unstable();
        roots.dedup();
        roots
    };
    if roots.is_empty() {
        return 0;
    }
    let mut stable_absence_checks = 0_u8;
    loop {
        let mut any_profile_exists = false;
        for root in &roots {
            let webview_data = root.join("webview-data");
            let _ = fs::remove_dir_all(&webview_data);
            let _ = fs::remove_dir(root);
            if webview_data.exists() {
                any_profile_exists = true;
            }
        }
        if any_profile_exists {
            stable_absence_checks = 0;
        } else {
            stable_absence_checks += 1;
        }
        if stable_absence_checks >= 10 {
            return 0;
        }
        if deadline.is_none_or(|deadline| Instant::now() >= deadline) {
            let remaining: Vec<_> = roots
                .into_iter()
                .filter(|root| root.join("webview-data").exists())
                .collect();
            let count = remaining.len();
            pending_webview_cleanup()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .extend(remaining);
            return count;
        }
        thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

#[cfg(test)]
pub fn commit_pdf_file(
    source: &mut File,
    target: &Path,
    target_display: &str,
) -> Result<(), AppError> {
    commit_pdf_file_with_policy(source, target, target_display, ExportCommitPolicy::Replace)
}

pub fn commit_pdf_file_with_policy(
    source: &mut File,
    target: &Path,
    target_display: &str,
    policy: ExportCommitPolicy,
) -> Result<(), AppError> {
    validate_pdf_file(source)?;
    source
        .seek(SeekFrom::Start(0))
        .map_err(|error| AppError::file_read_failed("PDF 平台产物", error))?;
    let write = match policy {
        ExportCommitPolicy::Replace => atomic_write_from(target, source),
        ExportCommitPolicy::CreateNew => atomic_write_from_create_new(target, source),
    };
    write.map_err(|error| {
        if policy == ExportCommitPolicy::CreateNew
            && error.kind() == std::io::ErrorKind::AlreadyExists
        {
            AppError::new(
                "EXPORT_TARGET_EXISTS",
                format!("导出目标已存在，使用 --overwrite 才可覆盖：{target_display}"),
            )
        } else {
            AppError::file_write_failed(target_display, error)
        }
    })
}

fn validate_pdf_file(file: &mut File) -> Result<(), AppError> {
    let metadata = file
        .metadata()
        .map_err(|error| AppError::file_read_failed("PDF 平台产物", error))?;
    if !metadata.is_file() || metadata.len() < MIN_PDF_BYTES {
        return Err(invalid_pdf_output());
    }
    let length = metadata.len();

    let mut header = [0_u8; 8];
    file.seek(SeekFrom::Start(0))
        .and_then(|_| file.read_exact(&mut header))
        .map_err(|error| AppError::file_read_failed("PDF 平台产物", error))?;
    if !matches!(&header[..7], b"%PDF-1." | b"%PDF-2.") || !header[7].is_ascii_digit() {
        return Err(invalid_pdf_output());
    }

    let tail_start = length.saturating_sub(PDF_TAIL_BYTES);
    file.seek(SeekFrom::Start(tail_start))
        .map_err(|error| AppError::file_read_failed("PDF 平台产物", error))?;
    let mut tail = Vec::with_capacity((length - tail_start) as usize);
    file.take(PDF_TAIL_BYTES)
        .read_to_end(&mut tail)
        .map_err(|error| AppError::file_read_failed("PDF 平台产物", error))?;
    let eof = find_last(&tail, b"%%EOF").ok_or_else(invalid_pdf_output)?;
    if !tail[eof + 5..].iter().all(|byte| is_pdf_whitespace(*byte)) {
        return Err(invalid_pdf_output());
    }
    let startxref = find_last(&tail[..eof], b"startxref").ok_or_else(invalid_pdf_output)?;
    let xref_offset = parse_startxref(&tail[startxref + b"startxref".len()..eof])
        .ok_or_else(invalid_pdf_output)?;
    if xref_offset >= length {
        return Err(invalid_pdf_output());
    }

    file.seek(SeekFrom::Start(xref_offset))
        .map_err(|error| AppError::file_read_failed("PDF 平台产物", error))?;
    let mut xref_probe = vec![0_u8; PDF_XREF_PROBE_BYTES];
    let probe_len = file
        .read(&mut xref_probe)
        .map_err(|error| AppError::file_read_failed("PDF 平台产物", error))?;
    xref_probe.truncate(probe_len);
    let xref_probe = trim_pdf_whitespace(&xref_probe);
    let table_xref = starts_with_keyword(xref_probe, b"xref");
    let stream_xref =
        contains_name_pair(xref_probe, b"/Type", b"/XRef") && contains_keyword(xref_probe, b"obj");
    if !table_xref && !stream_xref {
        return Err(invalid_pdf_output());
    }

    let markers = scan_pdf_markers(file)?;
    if !markers.root
        || !markers.catalog
        || !markers.page
        || (table_xref && !markers.trailer)
        || (stream_xref && !markers.xref_stream)
    {
        return Err(invalid_pdf_output());
    }
    Ok(())
}

fn invalid_pdf_output() -> AppError {
    AppError::new(
        "INVALID_PDF_OUTPUT",
        "平台打印器没有生成结构完整且包含页面的 PDF 文件",
    )
}

fn parse_startxref(bytes: &[u8]) -> Option<u64> {
    let mut index = 0;
    while bytes
        .get(index)
        .is_some_and(|byte| is_pdf_whitespace(*byte))
    {
        index += 1;
    }
    let start = index;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index == start || !bytes[index..].iter().all(|byte| is_pdf_whitespace(*byte)) {
        return None;
    }
    std::str::from_utf8(&bytes[start..index]).ok()?.parse().ok()
}

#[derive(Default)]
struct PdfMarkers {
    root: bool,
    catalog: bool,
    page: bool,
    trailer: bool,
    xref_stream: bool,
}

fn scan_pdf_markers(file: &mut File) -> Result<PdfMarkers, AppError> {
    file.seek(SeekFrom::Start(0))
        .map_err(|error| AppError::file_read_failed("PDF 平台产物", error))?;
    let mut markers = PdfMarkers::default();
    let mut buffer = vec![0_u8; PDF_SCAN_CHUNK_BYTES + PDF_SCAN_OVERLAP_BYTES];
    let mut carry_len = 0;
    loop {
        let read = file
            .read(&mut buffer[carry_len..carry_len + PDF_SCAN_CHUNK_BYTES])
            .map_err(|error| AppError::file_read_failed("PDF 平台产物", error))?;
        if read == 0 {
            break;
        }
        let window_len = carry_len + read;
        let window = &buffer[..window_len];
        markers.root |= contains_name(window, b"/Root");
        markers.catalog |= contains_name_pair(window, b"/Type", b"/Catalog");
        markers.page |= contains_name_pair(window, b"/Type", b"/Page");
        markers.trailer |= contains_keyword(window, b"trailer");
        markers.xref_stream |= contains_name_pair(window, b"/Type", b"/XRef");
        carry_len = window_len.min(PDF_SCAN_OVERLAP_BYTES);
        buffer.copy_within(window_len - carry_len..window_len, 0);
    }
    Ok(markers)
}

fn contains_name(bytes: &[u8], name: &[u8]) -> bool {
    bytes
        .windows(name.len())
        .enumerate()
        .any(|(index, window)| {
            window == name
                && bytes
                    .get(index + name.len())
                    .is_none_or(|byte| is_pdf_delimiter(*byte))
        })
}

fn contains_name_pair(bytes: &[u8], first: &[u8], second: &[u8]) -> bool {
    bytes
        .windows(first.len())
        .enumerate()
        .any(|(index, window)| {
            if window != first {
                return false;
            }
            let mut next = index + first.len();
            if bytes.get(next).is_some_and(|byte| !is_pdf_delimiter(*byte)) {
                return false;
            }
            while bytes.get(next).is_some_and(|byte| is_pdf_whitespace(*byte)) {
                next += 1;
            }
            bytes.get(next..next + second.len()) == Some(second)
                && bytes
                    .get(next + second.len())
                    .is_none_or(|byte| is_pdf_delimiter(*byte))
        })
}

fn contains_keyword(bytes: &[u8], keyword: &[u8]) -> bool {
    bytes
        .windows(keyword.len())
        .enumerate()
        .any(|(index, window)| {
            window == keyword
                && index
                    .checked_sub(1)
                    .and_then(|before| bytes.get(before))
                    .is_none_or(|byte| is_pdf_delimiter(*byte))
                && bytes
                    .get(index + keyword.len())
                    .is_none_or(|byte| is_pdf_delimiter(*byte))
        })
}

fn starts_with_keyword(bytes: &[u8], keyword: &[u8]) -> bool {
    bytes.starts_with(keyword)
        && bytes
            .get(keyword.len())
            .is_none_or(|byte| is_pdf_delimiter(*byte))
}

fn trim_pdf_whitespace(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(|byte| is_pdf_whitespace(*byte)) {
        bytes = &bytes[1..];
    }
    bytes
}

fn is_pdf_whitespace(byte: u8) -> bool {
    matches!(byte, 0 | b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

fn is_pdf_delimiter(byte: u8) -> bool {
    is_pdf_whitespace(byte)
        || matches!(
            byte,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
        )
}

fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use super::{
        commit_pdf_file, commit_pdf_file_with_policy, scan_pdf_markers, validate_pdf_file,
        PdfWorkspace, PDF_SCAN_CHUNK_BYTES,
    };
    use crate::services::export_service::ExportCommitPolicy;
    use crate::utils::test_support::TestPath;

    fn test_path(name: &str, extension: &str) -> TestPath {
        TestPath::new("pdf-artifact", format!("{name}.{extension}"))
    }

    fn minimal_pdf() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for object in [
            b"<< /Type /Catalog /Pages 2 0 R >>".as_slice(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".as_slice(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 72 72] /Contents 4 0 R >>".as_slice(),
            b"<< /Length 0 >>\nstream\n\nendstream".as_slice(),
        ] {
            offsets.push(pdf.len());
            let number = offsets.len();
            writeln!(&mut pdf, "{number} 0 obj").unwrap();
            pdf.extend_from_slice(object);
            pdf.extend_from_slice(b"\nendobj\n");
        }
        let xref = pdf.len();
        pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
        for offset in offsets {
            writeln!(&mut pdf, "{offset:010} 00000 n ").unwrap();
        }
        write!(
            &mut pdf,
            "trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        )
        .unwrap();
        pdf
    }

    fn open_fixture(name: &str, bytes: &[u8]) -> (TestPath, fs::File) {
        let path = test_path(name, "pdf");
        fs::write(&path, bytes).unwrap();
        let file = fs::File::open(&path).unwrap();
        (path, file)
    }

    #[test]
    fn accepts_a_structural_single_page_pdf() {
        let (_fixture, mut file) = open_fixture("valid", &minimal_pdf());

        validate_pdf_file(&mut file).unwrap();
    }

    #[test]
    fn marker_scan_preserves_tokens_split_across_chunk_boundaries() {
        let mut bytes = vec![b' '; PDF_SCAN_CHUNK_BYTES - 3];
        bytes.extend_from_slice(b"/Root /Type /Catalog /Type /Page trailer /Type /XRef");
        let (_fixture, mut file) = open_fixture("boundary-markers", &bytes);

        let markers = scan_pdf_markers(&mut file).unwrap();

        assert!(markers.root);
        assert!(markers.catalog);
        assert!(markers.page);
        assert!(markers.trailer);
        assert!(markers.xref_stream);
    }

    #[test]
    fn rejects_header_only_truncated_missing_trailer_and_random_tail() {
        let valid = minimal_pdf();
        let cases = [
            ("header-only", b"%PDF-1.4\n%%EOF\n".to_vec()),
            ("zero", Vec::new()),
            ("truncated", valid[..valid.len() - 16].to_vec()),
            (
                "missing-trailer",
                valid
                    .windows(b"trailer".len())
                    .position(|window| window == b"trailer")
                    .map(|position| {
                        let mut bytes = valid.clone();
                        bytes[position..position + b"trailer".len()].fill(b'x');
                        bytes
                    })
                    .unwrap(),
            ),
            ("random-tail", {
                let mut bytes = valid.clone();
                bytes.extend_from_slice(b"not-whitespace");
                bytes
            }),
        ];

        for (name, bytes) in cases {
            let (_fixture, mut file) = open_fixture(name, &bytes);
            assert_eq!(
                validate_pdf_file(&mut file).unwrap_err().code,
                "INVALID_PDF_OUTPUT",
                "case {name}"
            );
        }
    }

    #[test]
    fn invalid_pdf_never_replaces_the_existing_target() {
        let target = test_path("preserve-target", "pdf");
        fs::write(&target, b"existing-pdf").unwrap();
        let (_source_guard, mut source) = open_fixture("invalid-source", b"%PDF-1.4\n%%EOF\n");

        let error = commit_pdf_file(&mut source, &target, &target.to_string_lossy()).unwrap_err();

        assert_eq!(error.code, "INVALID_PDF_OUTPUT");
        assert_eq!(fs::read(&target).unwrap(), b"existing-pdf");
    }

    #[test]
    fn commit_streams_the_validated_file_into_the_target() {
        let pdf = minimal_pdf();
        let target = test_path("commit", "pdf");
        let (_source_guard, mut source) = open_fixture("commit-source", &pdf);

        commit_pdf_file(&mut source, &target, &target.to_string_lossy()).unwrap();

        assert_eq!(fs::read(&target).unwrap(), pdf);
    }

    #[test]
    fn create_new_commit_preserves_an_existing_target() {
        let target = test_path("create-new-preserve", "pdf");
        fs::write(&target, b"keep-existing").unwrap();
        let (_source_guard, mut source) = open_fixture("create-new-source", &minimal_pdf());

        let error = commit_pdf_file_with_policy(
            &mut source,
            &target,
            &target.to_string_lossy(),
            ExportCommitPolicy::CreateNew,
        )
        .unwrap_err();

        assert_eq!(error.code, "EXPORT_TARGET_EXISTS");
        assert_eq!(fs::read(&target).unwrap(), b"keep-existing");
    }

    #[test]
    fn workspace_reserves_its_directory_and_html_name_atomically() {
        let target = test_path("workspace", "pdf");
        let first = PdfWorkspace::create(&target).unwrap();
        let second = PdfWorkspace::create(&target).unwrap();
        assert_ne!(first.root(), second.root());
        fs::write(first.html_path(), "foreign-sentinel").unwrap();

        let error = first.write_html("replacement").unwrap_err();

        assert_eq!(error.code, "EXPORT_TEMP_FAILED");
        assert_eq!(
            fs::read_to_string(first.html_path()).unwrap(),
            "foreign-sentinel"
        );
        drop(first);
        drop(second);
    }

    #[test]
    fn workspace_cleanup_never_recursively_deletes_unknown_entries() {
        let target = test_path("owned-cleanup", "pdf");
        let workspace = PdfWorkspace::create(&target).unwrap();
        let root = workspace.root().to_path_buf();
        let unknown = root.join("unknown.txt");
        fs::write(&unknown, "preserve").unwrap();

        drop(workspace);

        assert_eq!(fs::read_to_string(&unknown).unwrap(), "preserve");
    }
}
