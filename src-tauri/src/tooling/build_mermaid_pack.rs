use std::{
    env,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const FILES: [&str; 4] = [
    "manifest.json",
    "mermaid.min.js",
    "LICENSE.mermaid",
    "THIRD_PARTY_NOTICES.json",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let source = PathBuf::from(args.next().ok_or("missing staging directory")?);
    let output = PathBuf::from(args.next().ok_or("missing output archive")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    if !source.is_dir() {
        return Err("staging directory does not exist".into());
    }
    let parent = output.parent().ok_or("output has no parent directory")?;
    fs::create_dir_all(parent)?;
    let temporary = output.with_extension("zip.partial");
    write_archive(&source, &temporary)?;
    if output.exists() {
        fs::remove_file(&output)?;
    }
    fs::rename(temporary, output)?;
    Ok(())
}

fn write_archive(source: &Path, output: &Path) -> io::Result<()> {
    let file = File::create(output)?;
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(9))
        .unix_permissions(0o644);
    for name in FILES {
        let bytes = fs::read(source.join(name))?;
        writer.start_file(name, options)?;
        writer.write_all(&bytes)?;
    }
    writer.finish()?;
    Ok(())
}
