use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::models::{
    app_error::AppError,
    diagram::{DiagramRuntimeAsset, DiagramRuntimeStatus},
};

use super::diagram_service::RENDERER_ID;

const VERSION: &str = "11.17.2";
const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 5 * 1024 * 1024;
const MAX_SCRIPT_BYTES: u64 = 4 * 1024 * 1024;
const ARCHIVE_FILES: [&str; 4] = [
    "manifest.json",
    "mermaid.min.js",
    "LICENSE.mermaid",
    "THIRD_PARTY_NOTICES.json",
];
const PAYLOAD_FILES: [&str; 3] = [
    "mermaid.min.js",
    "LICENSE.mermaid",
    "THIRD_PARTY_NOTICES.json",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackManifest {
    schema_version: u32,
    generator: String,
    renderer_id: String,
    mermaid_version: String,
    files: Vec<PackFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackFile {
    name: String,
    bytes: u64,
    sha256: String,
}

pub fn status(root: &Path) -> DiagramRuntimeStatus {
    DiagramRuntimeStatus {
        renderer_id: RENDERER_ID.to_string(),
        installed: load_from_root(root).is_ok(),
    }
}

pub fn load_from_root(root: &Path) -> Result<DiagramRuntimeAsset, AppError> {
    let directory = version_directory(root);
    if !directory.is_dir() {
        return Err(unavailable());
    }
    let manifest_bytes = read_file_bounded(&directory.join("manifest.json"), 128 * 1024)?;
    let manifest = parse_manifest(&manifest_bytes)?;
    let payload = validate_directory(&directory, &manifest)?;
    let script = payload.get("mermaid.min.js").ok_or_else(invalid)?;
    let script_utf8 = String::from_utf8(script.clone()).map_err(|_| invalid())?;
    Ok(DiagramRuntimeAsset {
        renderer_id: RENDERER_ID.to_string(),
        script_utf8,
    })
}

pub fn install_from_archive(
    root: &Path,
    archive_path: &Path,
) -> Result<DiagramRuntimeStatus, AppError> {
    if !archive_path.is_absolute() || !archive_path.is_file() {
        return Err(AppError::new(
            "DIAGRAM_RUNTIME_INVALID",
            "Mermaid pack 必须是存在的绝对文件路径",
        ));
    }
    let metadata = fs::metadata(archive_path).map_err(|_| invalid())?;
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(invalid());
    }
    let archive_bytes = read_file_bounded(archive_path, MAX_ARCHIVE_BYTES)?;
    let files = read_archive(&archive_bytes)?;
    let manifest = parse_manifest(files.get("manifest.json").ok_or_else(invalid)?)?;
    validate_payload_map(&files, &manifest)?;

    let runtime_root = root.join("diagram-runtime");
    fs::create_dir_all(&runtime_root).map_err(|_| invalid())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = runtime_root.join(format!(".{VERSION}-install-{}-{nonce}", std::process::id()));
    fs::create_dir(&temporary).map_err(|_| invalid())?;
    let write_result = (|| {
        for name in ARCHIVE_FILES {
            let bytes = files.get(name).ok_or_else(invalid)?;
            let mut output = File::options()
                .create_new(true)
                .write(true)
                .open(temporary.join(name))
                .map_err(|_| invalid())?;
            output.write_all(bytes).map_err(|_| invalid())?;
            output.sync_all().map_err(|_| invalid())?;
        }
        let target = version_directory(root);
        if target.exists() {
            load_from_root(root)?;
            let backup =
                runtime_root.join(format!(".{VERSION}-backup-{}-{nonce}", std::process::id()));
            fs::rename(&target, &backup).map_err(|_| invalid())?;
            if fs::rename(&temporary, &target).is_err() {
                let _ = fs::rename(&backup, &target);
                return Err(invalid());
            }
            remove_known_directory(&backup)?;
        } else {
            fs::rename(&temporary, &target).map_err(|_| invalid())?;
        }
        Ok(())
    })();
    if write_result.is_err() && temporary.exists() {
        let _ = remove_known_directory(&temporary);
    }
    write_result?;
    Ok(status(root))
}

pub fn uninstall_from_root(root: &Path) -> Result<DiagramRuntimeStatus, AppError> {
    let directory = version_directory(root);
    if !directory.exists() {
        return Ok(status(root));
    }
    load_from_root(root)?;
    remove_known_directory(&directory)?;
    Ok(status(root))
}

fn version_directory(root: &Path) -> PathBuf {
    root.join("diagram-runtime").join(VERSION)
}

fn read_archive(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>, AppError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid())?;
    let mut files = HashMap::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|_| invalid())?;
        let name = entry.name().to_string();
        if !ARCHIVE_FILES.contains(&name.as_str())
            || entry
                .enclosed_name()
                .is_none_or(|path| path != Path::new(&name))
            || entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            || files.contains_key(&name)
        {
            return Err(invalid());
        }
        total = total.checked_add(entry.size()).ok_or_else(invalid)?;
        if total > MAX_UNCOMPRESSED_BYTES
            || (name == "mermaid.min.js" && entry.size() > MAX_SCRIPT_BYTES)
        {
            return Err(invalid());
        }
        let entry_size = entry.size();
        let mut data = Vec::with_capacity(usize::try_from(entry_size).map_err(|_| invalid())?);
        entry
            .by_ref()
            .take(entry_size.saturating_add(1))
            .read_to_end(&mut data)
            .map_err(|_| invalid())?;
        if data.len() as u64 != entry_size {
            return Err(invalid());
        }
        files.insert(name, data);
    }
    if files.len() != ARCHIVE_FILES.len() {
        return Err(invalid());
    }
    Ok(files)
}

fn parse_manifest(bytes: &[u8]) -> Result<PackManifest, AppError> {
    let manifest: PackManifest = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if manifest.schema_version != 1
        || manifest.renderer_id != RENDERER_ID
        || manifest.mermaid_version != VERSION
        || manifest.generator != "marklite-build-mermaid-pack-v1"
    {
        return Err(invalid());
    }
    Ok(manifest)
}

fn validate_payload_map(
    files: &HashMap<String, Vec<u8>>,
    manifest: &PackManifest,
) -> Result<(), AppError> {
    let payload = PAYLOAD_FILES
        .iter()
        .map(|name| {
            (
                (*name).to_string(),
                files.get(*name).cloned().ok_or_else(invalid),
            )
        })
        .map(|(name, result)| result.map(|bytes| (name, bytes)))
        .collect::<Result<HashMap<_, _>, _>>()?;
    validate_payload(&payload, manifest)
}

fn validate_directory(
    directory: &Path,
    manifest: &PackManifest,
) -> Result<HashMap<String, Vec<u8>>, AppError> {
    let actual_names = fs::read_dir(directory)
        .map_err(|_| invalid())?
        .map(|entry| {
            let entry = entry.map_err(|_| invalid())?;
            if !entry.file_type().map_err(|_| invalid())?.is_file() {
                return Err(invalid());
            }
            entry.file_name().into_string().map_err(|_| invalid())
        })
        .collect::<Result<HashSet<_>, _>>()?;
    if actual_names
        != ARCHIVE_FILES
            .iter()
            .map(|name| (*name).to_string())
            .collect()
    {
        return Err(invalid());
    }
    let mut payload = HashMap::new();
    for file in &manifest.files {
        let maximum = if file.name == "mermaid.min.js" {
            MAX_SCRIPT_BYTES
        } else {
            512 * 1024
        };
        payload.insert(
            file.name.clone(),
            read_file_bounded(&directory.join(&file.name), maximum)?,
        );
    }
    validate_payload(&payload, manifest)?;
    Ok(payload)
}

fn validate_payload(
    payload: &HashMap<String, Vec<u8>>,
    manifest: &PackManifest,
) -> Result<(), AppError> {
    let mut names = HashSet::new();
    if manifest.files.len() != PAYLOAD_FILES.len() {
        return Err(invalid());
    }
    for file in &manifest.files {
        if !PAYLOAD_FILES.contains(&file.name.as_str()) || !names.insert(file.name.as_str()) {
            return Err(invalid());
        }
        let bytes = payload.get(&file.name).ok_or_else(invalid)?;
        let hash = format!("{:x}", Sha256::digest(bytes));
        if file.bytes != bytes.len() as u64 || file.sha256 != hash {
            return Err(invalid());
        }
    }
    serde_json::from_slice::<serde_json::Value>(
        payload
            .get("THIRD_PARTY_NOTICES.json")
            .ok_or_else(invalid)?,
    )
    .map_err(|_| invalid())?;
    Ok(())
}

fn read_file_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, AppError> {
    let file = File::open(path).map_err(|_| invalid())?;
    let metadata = file.metadata().map_err(|_| invalid())?;
    if !metadata.is_file() || metadata.len() > maximum {
        return Err(invalid());
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).map_err(|_| invalid())?);
    file.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() as u64 != metadata.len() {
        return Err(invalid());
    }
    Ok(bytes)
}

fn remove_known_directory(directory: &Path) -> Result<(), AppError> {
    let names = fs::read_dir(directory)
        .map_err(|_| invalid())?
        .map(|entry| entry.map_err(|_| invalid()))
        .collect::<Result<Vec<_>, _>>()?;
    if names.iter().any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_none_or(|name| !ARCHIVE_FILES.contains(&name))
            || entry.file_type().map_or(true, |kind| !kind.is_file())
    }) {
        return Err(invalid());
    }
    for entry in names {
        fs::remove_file(entry.path()).map_err(|_| invalid())?;
    }
    fs::remove_dir(directory).map_err(|_| invalid())
}

fn unavailable() -> AppError {
    AppError::new(
        "DIAGRAM_RUNTIME_UNAVAILABLE",
        "尚未安装兼容的 Mermaid 离线 pack",
    )
}

fn invalid() -> AppError {
    AppError::new("DIAGRAM_RUNTIME_INVALID", "Mermaid 离线 pack 无效或已损坏")
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write};

    use serde_json::json;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;
    use zip::{write::SimpleFileOptions, ZipWriter};

    use super::{install_from_archive, load_from_root, status, uninstall_from_root, RENDERER_ID};

    fn write_pack(path: &std::path::Path, corrupt_hash: bool, extra_file: bool) {
        let payload = [
            ("mermaid.min.js", b"globalThis.mermaid={};".as_slice()),
            ("LICENSE.mermaid", b"MIT".as_slice()),
            ("THIRD_PARTY_NOTICES.json", b"{\"packages\":[]}".as_slice()),
        ];
        let files = payload
            .iter()
            .map(|(name, bytes)| {
                json!({
                    "name": name,
                    "bytes": bytes.len(),
                    "sha256": if corrupt_hash { "0".repeat(64) } else { format!("{:x}", Sha256::digest(bytes)) }
                })
            })
            .collect::<Vec<_>>();
        let manifest = serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "generator": "marklite-build-mermaid-pack-v1",
            "rendererId": RENDERER_ID,
            "mermaidVersion": "11.17.2",
            "files": files
        }))
        .unwrap();
        let file = fs::File::create(path).unwrap();
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        archive.start_file("manifest.json", options).unwrap();
        archive.write_all(&manifest).unwrap();
        for (name, bytes) in payload {
            archive.start_file(name, options).unwrap();
            archive.write_all(bytes).unwrap();
        }
        if extra_file {
            archive.start_file("unexpected.txt", options).unwrap();
            archive.write_all(b"unexpected").unwrap();
        }
        archive.finish().unwrap();
    }

    #[test]
    fn installs_loads_and_uninstalls_a_validated_pack() {
        let directory = tempdir().unwrap();
        let pack = directory.path().join("runtime.zip");
        write_pack(&pack, false, false);

        assert!(!status(directory.path()).installed);
        assert!(
            install_from_archive(directory.path(), &pack)
                .unwrap()
                .installed
        );
        let asset = load_from_root(directory.path()).unwrap();
        assert_eq!(asset.renderer_id, RENDERER_ID);
        assert_eq!(asset.script_utf8, "globalThis.mermaid={};");
        assert!(!uninstall_from_root(directory.path()).unwrap().installed);
    }

    #[test]
    fn rejects_hash_mismatch_and_unlisted_archive_entries() {
        for (name, corrupt_hash, extra_file) in [
            ("bad-hash.zip", true, false),
            ("extra-file.zip", false, true),
        ] {
            let directory = tempdir().unwrap();
            let pack = directory.path().join(name);
            write_pack(&pack, corrupt_hash, extra_file);
            let error = install_from_archive(directory.path(), &pack).unwrap_err();
            assert_eq!(error.code, "DIAGRAM_RUNTIME_INVALID");
            assert!(!status(directory.path()).installed);
        }
    }
}
