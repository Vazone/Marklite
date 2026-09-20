use std::path::PathBuf;

use tauri::{AppHandle, Emitter, Manager};

use crate::models::app_error::AppError;
use crate::services::open_request_service::OpenRequestState;
use crate::services::startup_diagnostics_service::StartupState;

const OPEN_FILE_EVENT: &str = "single-instance-open-file";

pub(crate) fn handle(app: &AppHandle, args: Vec<String>, cwd: String) {
    match markdown_arg_from_args(&args, &cwd) {
        Ok(Some(path)) => {
            app.state::<OpenRequestState>().enqueue(path);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.emit(OPEN_FILE_EVENT, ());
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        Err(_) => {
            let _ = app.state::<StartupState>().record_native(
                "singleInstanceOpen",
                "failed",
                Some("unsupportedPathEncoding"),
            );
        }
        Ok(None) => {}
    }
}

fn markdown_arg_from_args(args: &[String], cwd: &str) -> Result<Option<String>, AppError> {
    for arg in args.iter().skip(1) {
        let mut path = PathBuf::from(arg);
        if path.is_relative() {
            path = PathBuf::from(cwd).join(path);
        }

        if path.is_file() && crate::services::file_service::ensure_allowed_file(&path).is_ok() {
            if let Ok(canonical) = crate::utils::path_utils::canonicalize_path(&path) {
                return Ok(Some(
                    crate::utils::path_utils::path_to_utf8(&canonical)?.to_string(),
                ));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::markdown_arg_from_args;
    use crate::utils::test_support::TestDirectory;

    #[test]
    fn arguments_resolve_relative_markdown_to_canonical_files() {
        let dir = TestDirectory::new("single-instance");
        let markdown = dir.join("note.md");
        fs::write(&markdown, "note").unwrap();

        let args = vec!["marklite.exe".to_string(), "note.md".to_string()];
        assert_eq!(
            markdown_arg_from_args(&args, dir.to_str().unwrap()).unwrap(),
            Some(
                crate::utils::path_utils::canonicalize_path(&markdown)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string()
            )
        );

        let rejected = vec!["marklite.exe".to_string(), "missing.md".to_string()];
        assert_eq!(
            markdown_arg_from_args(&rejected, dir.to_str().unwrap()).unwrap(),
            None
        );
    }
}
