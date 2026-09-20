#![cfg(windows)]

use std::{
    collections::HashSet,
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    thread,
    time::{Duration, Instant},
};

fn workspace_names(parent: &Path) -> HashSet<OsString> {
    fs::read_dir(parent)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".marklite-pdf-work-")
        })
        .map(|entry| entry.file_name())
        .collect()
}

fn run() -> Result<i32, String> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.len() < 5 {
        return Err("usage: cancel-helper <launcher> <workspace-parent> <stdout> <stderr> <cli-args...>".to_string());
    }
    let launcher = PathBuf::from(&args[0]);
    let workspace_parent = PathBuf::from(&args[1]);
    let stdout = File::create(PathBuf::from(&args[2])).map_err(|error| error.to_string())?;
    let stderr = File::create(PathBuf::from(&args[3])).map_err(|error| error.to_string())?;
    let existing = workspace_names(&workspace_parent);
    let mut child = Command::new(launcher)
        .args(&args[4..])
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| error.to_string())?;

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Err(format!(
                "CLI exited before its PDF workspace could be cancelled: {status}"
            ));
        }
        if workspace_names(&workspace_parent)
            .iter()
            .any(|name| !existing.contains(name))
        {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            return Err("timed out waiting for the PDF workspace".to_string());
        }
        thread::sleep(Duration::from_millis(1));
    }

    let cancel_file = std::env::temp_dir().join(format!("marklite-cli-cancel-{}", child.id()));
    if let Err(error) = fs::write(&cancel_file, b"cancel") {
        let _ = child.kill();
        return Err(format!("cannot trigger the launcher cancellation channel: {error}"));
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    Ok(status.code().unwrap_or(130))
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
