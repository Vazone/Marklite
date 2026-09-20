use crate::services::{
    export_progress::{ExportReporter, ExportStage, ProgressStatus},
    export_progress_native,
    export_progress_pump::ProgressPump,
};
use std::{
    ffi::{OsStr, OsString},
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    models::{
        app_error::AppError,
        export::{
            ExportFormat, ExportMarginPreset, ExportOptions, ExportOrientation, ExportPaperSize,
            ExportRequest, ExportSnapshot, ExportWarning,
        },
    },
    services::{
        export_service::{self, ExportCommitPolicy},
        file_service,
    },
    utils::path_utils::path_to_utf8,
};

pub const INTERNAL_CLI_MARKER: &str = "--marklite-internal-cli";
pub const SHELL_EXPORT_MARKER: &str = "--marklite-shell-export";
const SCHEMA_VERSION: u32 = 1;
const EXIT_SUCCESS: i32 = 0;
const EXIT_ARGUMENT: i32 = 2;
const EXIT_INPUT: i32 = 3;
const EXIT_CONFLICT: i32 = 4;
const EXIT_EXPORT: i32 = 5;

const HELP: &str = "MarkLite command-line export\n\nUSAGE:\n  marklite-cli --export <INPUT> --format <html|docx|pdf> [OPTIONS]  (Windows)\n  marklite --export <INPUT> --format <html|docx|pdf> [OPTIONS]      (macOS/Linux)\n\nOPTIONS:\n  --export <PATH>          Export a single input (or use --input)\n  --input <PATH>           Input .md, .markdown, or .txt path\n  --output <PATH>          Output path (default: input with format extension)\n  --format <FORMAT>        Required: html, docx, or pdf\n  --overwrite              Replace an existing output atomically\n  --include-local-images   Embed verified local images\n  --no-title               Omit the document title\n  --timeout-ms <MILLIS>    PDF whole-job timeout (1..300000; default: 240000)\n  --json                   Write exactly one versioned JSON result to stdout\n  -h, --help               Show help without starting the GUI\n  -V, --version            Show version without starting the GUI\n\nUse `--` before a positional INPUT that starts with a hyphen. Paths are passed directly; no shell evaluation is performed.";

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedCommand {
    Help,
    Version,
    Export(CliExportOptions),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliExportOptions {
    input: PathBuf,
    output: Option<PathBuf>,
    format: ExportFormat,
    overwrite: bool,
    json: bool,
    include_local_images: bool,
    include_title: bool,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CliError {
    code: String,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CliResult {
    schema_version: u32,
    ok: bool,
    format: Option<ExportFormat>,
    output_path: Option<String>,
    bytes: Option<u64>,
    warnings: Vec<ExportWarning>,
    error: Option<CliError>,
}

#[derive(Debug, PartialEq, Eq)]
struct CliOutcome {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

pub fn try_run_from_env() -> Option<i32> {
    let remaining = cli_arguments(std::env::args_os().skip(1).collect())?;
    let current_dir = cli_current_dir();
    let outcome = execute(&remaining, &current_dir);
    let stdout = write_output(&mut io::stdout().lock(), &outcome.stdout);
    let stderr = write_output(&mut io::stderr().lock(), &outcome.stderr);
    Some(if stdout.is_err() || stderr.is_err() {
        6
    } else {
        outcome.exit_code
    })
}

fn write_output(writer: &mut impl Write, text: &str) -> io::Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    writer.write_all(text.as_bytes())?;
    writer.flush()
}

fn cli_arguments(mut args: Vec<OsString>) -> Option<Vec<OsString>> {
    if args.first().is_some_and(|arg| arg == INTERNAL_CLI_MARKER) {
        args.remove(0);
        return Some(args);
    }

    #[cfg(not(windows))]
    if args
        .first()
        .and_then(|arg| arg.to_str())
        .is_some_and(|arg| {
            matches!(
                arg,
                "--export" | "export" | "-h" | "--help" | "-V" | "--version"
            )
        })
    {
        return Some(args);
    }

    None
}

pub fn try_run_shell_export_from_env() -> Option<i32> {
    let mut args = std::env::args_os();
    let _program = args.next();
    if args.next().as_deref() != Some(OsStr::new(SHELL_EXPORT_MARKER)) {
        return None;
    }
    let remaining = args.collect::<Vec<_>>();
    let current_dir = cli_current_dir();
    let outcome = execute_shell_export(&remaining, &current_dir);
    show_shell_export_result(&outcome);
    Some(outcome.exit_code)
}

fn execute_shell_export(args: &[OsString], current_dir: &Path) -> CliOutcome {
    let parsed = match parse_shell_export(args) {
        Ok(options) => options,
        Err(error) => return failure_outcome(EXIT_ARGUMENT, false, None, None, error),
    };
    let job_id = format!("shell-{}", std::process::id());
    #[cfg(all(windows, not(test)))]
    let pump = Some(ProgressPump::new(
        &job_id,
        parsed.format,
        export_progress_native::shell_presenter,
    ));
    #[cfg(any(not(windows), test))]
    let pump: Option<ProgressPump> = None;
    let reporter = pump
        .as_ref()
        .map(|pump| pump.reporter.clone())
        .unwrap_or_else(|| ExportReporter::silent(&job_id, parsed.format));
    let outcome = execute_shell_export_reported(parsed, current_dir, &reporter);
    reporter.finish(outcome_progress_status(&outcome));
    drop(pump);
    outcome
}

fn execute_shell_export_reported(
    parsed: CliExportOptions,
    current_dir: &Path,
    reporter: &ExportReporter,
) -> CliOutcome {
    reporter.phase(ExportStage::PreparingTarget);
    let input = absolute_path(&parsed.input, current_dir);

    for suffix in 0..=9_999_u32 {
        let output = shell_output_candidate(&input, parsed.format, suffix);
        // This is only a fast-path. The create-new commit below is still the
        // authority if another process claims the name after this check.
        if output.exists() {
            continue;
        }
        let outcome = run_export_reported(
            CliExportOptions {
                input: input.clone(),
                output: Some(output),
                format: parsed.format,
                overwrite: false,
                json: false,
                include_local_images: false,
                include_title: true,
                timeout_ms: None,
            },
            current_dir,
            reporter,
        );
        if outcome.exit_code != EXIT_CONFLICT {
            return outcome;
        }
    }

    failure_outcome(
        EXIT_CONFLICT,
        false,
        Some(parsed.format),
        None,
        CliError {
            code: "EXPORT_NAME_EXHAUSTED".to_string(),
            message: "could not reserve an unused output name beside the source file".to_string(),
        },
    )
}

fn parse_shell_export(args: &[OsString]) -> Result<CliExportOptions, CliError> {
    if args.len() != 4 || args[0] != OsStr::new("--input") || args[2] != OsStr::new("--format") {
        return Err(argument_error(
            "expected `--input <PATH> --format <html|docx|pdf>`",
        ));
    }
    let format = match args[3].to_str() {
        Some("html") => ExportFormat::Html,
        Some("docx") => ExportFormat::Docx,
        Some("pdf") => ExportFormat::Pdf,
        _ => return Err(argument_error("--format must be `html`, `docx`, or `pdf`")),
    };
    Ok(CliExportOptions {
        input: PathBuf::from(&args[1]),
        output: None,
        format,
        overwrite: false,
        json: false,
        include_local_images: false,
        include_title: true,
        timeout_ms: None,
    })
}

fn shell_output_candidate(input: &Path, format: ExportFormat, suffix: u32) -> PathBuf {
    if suffix == 0 {
        return input.with_extension(format.extension());
    }
    let mut file_name = input
        .file_stem()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("export"));
    file_name.push(format!(" ({suffix}).{}", format.extension()));
    input.with_file_name(file_name)
}

#[cfg(windows)]
fn show_shell_export_result(outcome: &CliOutcome) {
    use windows::{core::HSTRING, Win32::UI::WindowsAndMessaging::*};

    let (message, flags) = if outcome.exit_code == EXIT_SUCCESS {
        (outcome.stdout.trim(), MB_OK | MB_ICONINFORMATION)
    } else {
        (outcome.stderr.trim(), MB_OK | MB_ICONERROR)
    };
    let message = HSTRING::from(if message.is_empty() {
        "MarkLite export finished without a status message."
    } else {
        message
    });
    let title = HSTRING::from("MarkLite Export");
    unsafe {
        let _ = MessageBoxW(None, &message, &title, flags);
    }
}

#[cfg(not(windows))]
fn show_shell_export_result(outcome: &CliOutcome) {
    let message = if outcome.exit_code == EXIT_SUCCESS {
        &outcome.stdout
    } else {
        &outcome.stderr
    };
    eprint!("{message}");
}

fn execute(args: &[OsString], current_dir: &Path) -> CliOutcome {
    let wants_json = requests_json(args);
    match parse(args) {
        Ok(ParsedCommand::Help) => outcome(EXIT_SUCCESS, format!("{HELP}\n"), String::new()),
        Ok(ParsedCommand::Version) => outcome(
            EXIT_SUCCESS,
            format!("marklite-cli {}\n", env!("CARGO_PKG_VERSION")),
            String::new(),
        ),
        Ok(ParsedCommand::Export(options)) => run_export(options, current_dir),
        Err(error) => failure_outcome(EXIT_ARGUMENT, wants_json, None, None, error),
    }
}

fn requests_json(args: &[OsString]) -> bool {
    let mut index = usize::from(
        args.first()
            .is_some_and(|arg| matches_arg(arg, &["export", "--export"])),
    );
    while index < args.len() {
        let arg = &args[index];
        if arg == OsStr::new("--") {
            return false;
        }
        if arg == OsStr::new("--json") {
            return true;
        }
        if matches_arg(arg, &["--input", "--output", "--format", "--timeout-ms"]) {
            index += 1;
        }
        index += 1;
    }
    false
}

fn parse(args: &[OsString]) -> Result<ParsedCommand, CliError> {
    if args.len() == 1 && matches_arg(&args[0], &["-h", "--help"]) {
        return Ok(ParsedCommand::Help);
    }
    if args.len() == 1 && matches_arg(&args[0], &["-V", "--version"]) {
        return Ok(ParsedCommand::Version);
    }
    if !args
        .first()
        .is_some_and(|arg| matches_arg(arg, &["export", "--export"]))
    {
        return Err(argument_error(
            "expected `--export`, `--help`, or `--version` (legacy `export` is also accepted)",
        ));
    }

    let mut input = None;
    let mut output = None;
    let mut format = None;
    let mut overwrite = false;
    let mut json = false;
    let mut include_local_images = false;
    let mut include_title = true;
    let mut timeout_ms = None;
    let mut index = 1;
    let mut positional_only = false;

    while index < args.len() {
        let arg = &args[index];
        if !positional_only && arg == OsStr::new("--") {
            positional_only = true;
            index += 1;
            continue;
        }
        if !positional_only && matches_arg(arg, &["-h", "--help"]) {
            return Ok(ParsedCommand::Help);
        }
        if !positional_only && arg == OsStr::new("--input") {
            if input.is_some() {
                return Err(argument_error("duplicate option: --input"));
            }
            input = Some(take_path(args, &mut index, "--input")?);
        } else if !positional_only && arg == OsStr::new("--output") {
            if output.is_some() {
                return Err(argument_error("duplicate option: --output"));
            }
            output = Some(take_path(args, &mut index, "--output")?);
        } else if !positional_only && arg == OsStr::new("--format") {
            if format.is_some() {
                return Err(argument_error("duplicate option: --format"));
            }
            let value = take_value(args, &mut index, "--format")?;
            format = Some(match value.to_str() {
                Some("html") => ExportFormat::Html,
                Some("docx") => ExportFormat::Docx,
                Some("pdf") => ExportFormat::Pdf,
                _ => return Err(argument_error("--format must be `html`, `docx`, or `pdf`")),
            });
        } else if !positional_only && arg == OsStr::new("--timeout-ms") {
            if timeout_ms.is_some() {
                return Err(argument_error("duplicate option: --timeout-ms"));
            }
            let value = take_value(args, &mut index, "--timeout-ms")?;
            let parsed = value
                .to_str()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| (1..=300_000).contains(value))
                .ok_or_else(|| {
                    argument_error("--timeout-ms must be an integer from 1 to 300000")
                })?;
            timeout_ms = Some(parsed);
        } else if !positional_only && arg == OsStr::new("--overwrite") {
            overwrite = set_once(overwrite, "--overwrite")?;
        } else if !positional_only && arg == OsStr::new("--json") {
            json = set_once(json, "--json")?;
        } else if !positional_only && arg == OsStr::new("--include-local-images") {
            include_local_images = set_once(include_local_images, "--include-local-images")?;
        } else if !positional_only && arg == OsStr::new("--no-title") {
            if !include_title {
                return Err(argument_error("duplicate option: --no-title"));
            }
            include_title = false;
        } else {
            if !positional_only && arg.to_string_lossy().starts_with('-') {
                return Err(argument_error(&format!(
                    "unknown option: {}; use `--` before a path beginning with `-`",
                    arg.to_string_lossy()
                )));
            }
            if input.replace(PathBuf::from(arg)).is_some() {
                return Err(argument_error("only one input file is supported"));
            }
        }
        index += 1;
    }

    Ok(ParsedCommand::Export(CliExportOptions {
        input: input.ok_or_else(|| argument_error("missing input path"))?,
        output,
        format: format.ok_or_else(|| argument_error("missing required --format"))?,
        overwrite,
        json,
        include_local_images,
        include_title,
        timeout_ms,
    }))
}

fn take_value<'a>(
    args: &'a [OsString],
    index: &mut usize,
    option: &str,
) -> Result<&'a OsString, CliError> {
    *index += 1;
    args.get(*index)
        .ok_or_else(|| argument_error(&format!("missing value for {option}")))
}

fn take_path(args: &[OsString], index: &mut usize, option: &str) -> Result<PathBuf, CliError> {
    take_value(args, index, option).map(PathBuf::from)
}

fn set_once(current: bool, option: &str) -> Result<bool, CliError> {
    if current {
        Err(argument_error(&format!("duplicate option: {option}")))
    } else {
        Ok(true)
    }
}

fn matches_arg(value: &OsStr, expected: &[&str]) -> bool {
    expected
        .iter()
        .any(|candidate| value == OsStr::new(candidate))
}

fn terminal_progress_enabled(json: bool) -> bool {
    if json {
        return false;
    }
    #[cfg(windows)]
    {
        std::env::var_os("MARKLITE_CLI_PROGRESS").as_deref() == Some(OsStr::new("1"))
    }
    #[cfg(not(windows))]
    {
        use std::io::IsTerminal;
        io::stdout().is_terminal() && io::stderr().is_terminal()
    }
}

fn outcome_progress_status(outcome: &CliOutcome) -> ProgressStatus {
    match outcome.exit_code {
        EXIT_SUCCESS => ProgressStatus::Succeeded,
        130 => ProgressStatus::Cancelled,
        _ => ProgressStatus::Failed,
    }
}

fn run_export(options: CliExportOptions, current_dir: &Path) -> CliOutcome {
    let job_id = format!("cli-{}", std::process::id());
    let pump = terminal_progress_enabled(options.json).then(|| {
        ProgressPump::new(&job_id, options.format, || {
            export_progress_native::terminal_event
        })
    });
    let reporter = pump
        .as_ref()
        .map(|pump| pump.reporter.clone())
        .unwrap_or_else(|| ExportReporter::silent(&job_id, options.format));
    let outcome = run_export_reported(options, current_dir, &reporter);
    reporter.finish(outcome_progress_status(&outcome));
    drop(pump);
    outcome
}

fn run_export_reported(
    options: CliExportOptions,
    current_dir: &Path,
    reporter: &ExportReporter,
) -> CliOutcome {
    reporter.phase(ExportStage::PreparingTarget);
    if options.format != ExportFormat::Pdf && options.timeout_ms.is_some() {
        return failure_outcome(
            EXIT_ARGUMENT,
            options.json,
            Some(options.format),
            None,
            argument_error("--timeout-ms is only valid for PDF export"),
        );
    }
    let format = Some(options.format);
    let input = absolute_path(&options.input, current_dir);
    let output = absolute_path(
        options.output.as_deref().unwrap_or_else(|| Path::new("")),
        current_dir,
    );
    let output = if options.output.is_some() {
        output
    } else {
        input.with_extension(options.format.extension())
    };
    let output_string = match path_to_utf8(&output) {
        Ok(path) => path.to_string(),
        Err(error) => {
            return failure_outcome(EXIT_ARGUMENT, options.json, format, None, app_error(error))
        }
    };
    if !output
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case(options.format.extension()))
    {
        return failure_outcome(
            EXIT_ARGUMENT,
            options.json,
            format,
            Some(output_string),
            argument_error(&format!(
                "output extension must be .{}",
                options.format.extension()
            )),
        );
    }
    let input_string = match path_to_utf8(&input) {
        Ok(path) => path.to_string(),
        Err(error) => {
            return failure_outcome(
                EXIT_INPUT,
                options.json,
                format,
                Some(output_string),
                app_error(error),
            )
        }
    };
    reporter.phase(ExportStage::Loading);
    let document = match file_service::read_markdown_file(&input_string) {
        Ok(document) => document,
        Err(error) => {
            return failure_outcome(
                EXIT_INPUT,
                options.json,
                format,
                Some(output_string),
                app_error(error),
            )
        }
    };
    reporter.phase(ExportStage::Snapshot);
    let request = ExportRequest {
        snapshot: ExportSnapshot {
            job_id: reporter.job_id().to_owned(),
            tab_id: "cli".to_string(),
            content_revision: 0,
            source_path: document.path,
            title: document.title,
            content: document.content,
        },
        target_path: output_string.clone(),
        target_kind: Default::default(),
        format: options.format,
        options: ExportOptions {
            paper_size: ExportPaperSize::A4,
            orientation: ExportOrientation::Portrait,
            margin: ExportMarginPreset::Normal,
            include_title: options.include_title,
            include_local_images: options.include_local_images,
        },
        mind_map_svg: None,
    };
    let policy = if options.overwrite {
        ExportCommitPolicy::Replace
    } else {
        ExportCommitPolicy::CreateNew
    };
    let result =
        export_service::preflight_commit(&request, policy).and_then(|()| match options.format {
            ExportFormat::Html => {
                reporter.phase(ExportStage::Parsing);
                let document = crate::services::export_semantic::SemanticDocument::parse(
                    &request.snapshot.content,
                    request.snapshot.source_path.as_deref(),
                );
                if document.diagram_sources().is_empty() {
                    export_service::export_html_with_document(&request, policy, &document, reporter)
                } else {
                    #[cfg(not(test))]
                    {
                        crate::cli_pdf_runtime::export_document_with_diagrams(
                            request.clone(),
                            policy,
                            document,
                            reporter.clone(),
                        )
                    }
                    #[cfg(test)]
                    {
                        export_service::export_html_with_policy(&request, policy)
                    }
                }
            }
            ExportFormat::Docx => {
                reporter.phase(ExportStage::Parsing);
                let document = crate::services::export_semantic::SemanticDocument::parse(
                    &request.snapshot.content,
                    request.snapshot.source_path.as_deref(),
                );
                if document.diagram_sources().is_empty() {
                    export_service::export_docx_with_document(&request, policy, &document, reporter)
                } else {
                    #[cfg(not(test))]
                    {
                        crate::cli_pdf_runtime::export_document_with_diagrams(
                            request.clone(),
                            policy,
                            document,
                            reporter.clone(),
                        )
                    }
                    #[cfg(test)]
                    {
                        export_service::export_docx_with_policy(&request, policy)
                    }
                }
            }
            ExportFormat::Pdf => crate::cli_pdf_runtime::export_pdf(
                request.clone(),
                policy,
                std::time::Duration::from_millis(
                    options
                        .timeout_ms
                        .unwrap_or(crate::services::pdf_export_service::DEFAULT_PDF_TIMEOUT_MS),
                ),
                reporter.clone(),
            ),
            ExportFormat::Svg | ExportFormat::Png => {
                unreachable!("parser does not expose GUI-only export formats")
            }
        });
    reporter.phase(ExportStage::Finalizing);
    match result {
        Ok(result) => match std::fs::metadata(&output) {
            Ok(metadata) => success_outcome(
                options.json,
                options.format,
                output_string,
                metadata.len(),
                result.warnings,
            ),
            Err(error) => failure_outcome(
                EXIT_EXPORT,
                options.json,
                format,
                Some(output_string.clone()),
                app_error(AppError::file_read_failed(&output_string, error)),
            ),
        },
        Err(error) => {
            let exit_code = match error.code.as_str() {
                "EXPORT_TARGET_EXISTS" => EXIT_CONFLICT,
                "PDF_PLATFORM_UNAVAILABLE"
                | "PDF_PLATFORM_UNSUPPORTED"
                | "PDF_CANCEL_HANDLER_FAILED" => 6,
                "PDF_EXPORT_TIMEOUT" | "DIAGRAM_TIMEOUT" => 124,
                "PDF_EXPORT_CANCELLED" | "DIAGRAM_CANCELLED" => 130,
                _ => EXIT_EXPORT,
            };
            failure_outcome(
                exit_code,
                options.json,
                format,
                Some(output_string),
                app_error(error),
            )
        }
    }
}

fn absolute_path(path: &Path, current_dir: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir.join(path)
    }
}

fn cli_current_dir() -> PathBuf {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("APPIMAGE").is_some() {
            // linuxdeploy's AppRun changes the process directory to AppDir/usr
            // while leaving PWD at the caller's directory.
            let app_dir = std::env::var_os("APPDIR").map(PathBuf::from);
            let original_dir = std::env::var_os("PWD").map(PathBuf::from);
            if let (Some(app_dir), Some(original_dir)) = (app_dir, original_dir) {
                if current_dir.starts_with(app_dir)
                    && original_dir.is_absolute()
                    && original_dir.is_dir()
                {
                    return original_dir;
                }
            }
        }
    }

    current_dir
}

fn success_outcome(
    json: bool,
    format: ExportFormat,
    output_path: String,
    bytes: u64,
    warnings: Vec<ExportWarning>,
) -> CliOutcome {
    if json {
        json_outcome(
            EXIT_SUCCESS,
            CliResult {
                schema_version: SCHEMA_VERSION,
                ok: true,
                format: Some(format),
                output_path: Some(output_path),
                bytes: Some(bytes),
                warnings,
                error: None,
            },
        )
    } else {
        outcome(
            EXIT_SUCCESS,
            format!("Exported {bytes} bytes to {output_path}\n"),
            String::new(),
        )
    }
}

fn failure_outcome(
    exit_code: i32,
    json: bool,
    format: Option<ExportFormat>,
    output_path: Option<String>,
    error: CliError,
) -> CliOutcome {
    if json {
        json_outcome(
            exit_code,
            CliResult {
                schema_version: SCHEMA_VERSION,
                ok: false,
                format,
                output_path,
                bytes: None,
                warnings: Vec::new(),
                error: Some(error),
            },
        )
    } else {
        outcome(
            exit_code,
            String::new(),
            format!("{}: {}\n", error.code, error.message),
        )
    }
}

fn json_outcome(exit_code: i32, result: CliResult) -> CliOutcome {
    let stdout = serde_json::to_string(&result)
        .map(|json| format!("{json}\n"))
        .unwrap_or_else(|_| {
            "{\"schemaVersion\":1,\"ok\":false,\"format\":null,\"outputPath\":null,\"bytes\":null,\"warnings\":[],\"error\":{\"code\":\"JSON_SERIALIZATION_FAILED\",\"message\":\"failed to serialize CLI result\"}}\n".to_string()
        });
    outcome(exit_code, stdout, String::new())
}

fn outcome(exit_code: i32, stdout: String, stderr: String) -> CliOutcome {
    CliOutcome {
        exit_code,
        stdout,
        stderr,
    }
}

fn argument_error(message: &str) -> CliError {
    CliError {
        code: "INVALID_ARGUMENT".to_string(),
        message: message.to_string(),
    }
}

fn app_error(error: AppError) -> CliError {
    CliError {
        code: error.code,
        message: error.message,
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, fs};

    use serde_json::Value;

    use super::{
        cli_arguments, execute, execute_shell_export, parse, CliExportOptions, ParsedCommand,
    };
    use crate::{models::export::ExportFormat, utils::test_support::TestDirectory};

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn internal_marker_is_removed_before_cli_parsing() {
        assert_eq!(
            cli_arguments(args(&[super::INTERNAL_CLI_MARKER, "--version"])),
            Some(args(&["--version"]))
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_binary_accepts_only_unambiguous_direct_cli_commands() {
        assert_eq!(
            cli_arguments(args(&["--export", "note.md", "--format", "html"])),
            Some(args(&["--export", "note.md", "--format", "html"]))
        );
        assert_eq!(
            cli_arguments(args(&["export", "note.md", "--format", "html"])),
            Some(args(&["export", "note.md", "--format", "html"]))
        );
        assert_eq!(cli_arguments(args(&["--help"])), Some(args(&["--help"])));
        assert_eq!(cli_arguments(args(&["note.md"])), None);
    }

    #[test]
    fn parses_finite_options_without_a_cli_dependency() {
        assert_eq!(
            parse(&args(&[
                "export",
                "input.md",
                "--format",
                "html",
                "--output",
                "out.html",
                "--overwrite",
                "--json",
                "--include-local-images",
                "--no-title",
            ]))
            .unwrap(),
            ParsedCommand::Export(CliExportOptions {
                input: "input.md".into(),
                output: Some("out.html".into()),
                format: ExportFormat::Html,
                overwrite: true,
                json: true,
                include_local_images: true,
                include_title: false,
                timeout_ms: None,
            })
        );
    }

    #[test]
    fn export_alias_keeps_options_errors_and_json_scope_identical() {
        let directory = TestDirectory::new("cli-alias");
        for tail in [
            vec!["--input", "missing.md", "--format", "html", "--json"],
            vec!["missing.md", "--format", "docx", "--json"],
            vec!["--format", "pdf", "--timeout-ms", "1", "--", "-稿件.md"],
            vec!["a.md", "b.md", "--format", "html", "--json"],
            vec!["--input", "a.md", "--input", "b.md", "--json"],
            vec!["a.md", "--format", "html", "--format", "pdf", "--json"],
            vec!["a.md", "--export", "b.md", "--json"],
            vec!["a.md", "--unknown", "--json"],
            vec!["--format", "html", "--", "--json"],
        ] {
            let old = args(&[vec!["export"], tail.clone()].concat());
            let new = args(&[vec!["--export"], tail].concat());
            assert_eq!(
                execute(&new, directory.path()),
                execute(&old, directory.path())
            );
        }
    }

    #[test]
    fn treats_a_leading_hyphen_as_a_literal_path_after_separator() {
        let parsed = parse(&args(&["export", "--format", "docx", "--", "-稿件.md"])).unwrap();
        assert!(matches!(
            parsed,
            ParsedCommand::Export(CliExportOptions { input, .. }) if input == std::path::PathBuf::from("-稿件.md")
        ));
    }

    #[test]
    fn parses_pdf_with_a_bounded_whole_job_timeout() {
        let parsed = parse(&args(&[
            "export",
            "input.md",
            "--format",
            "pdf",
            "--timeout-ms",
            "45000",
        ]))
        .unwrap();

        assert!(matches!(
            parsed,
            ParsedCommand::Export(CliExportOptions {
                format: ExportFormat::Pdf,
                timeout_ms: Some(45_000),
                ..
            })
        ));
        assert!(parse(&args(&[
            "export",
            "input.md",
            "--format",
            "pdf",
            "--timeout-ms",
            "0",
        ]))
        .is_err());
    }

    #[test]
    fn rejects_pdf_only_timeout_for_pure_export_formats() {
        let directory = TestDirectory::new("cli-timeout-scope");
        let input = directory.path().join("input.md");
        fs::write(&input, "body").unwrap();

        let outcome = execute(
            &[
                OsString::from("export"),
                input.into_os_string(),
                OsString::from("--format"),
                OsString::from("html"),
                OsString::from("--timeout-ms"),
                OsString::from("1000"),
                OsString::from("--json"),
            ],
            directory.path(),
        );

        assert_eq!(outcome.exit_code, 2);
        assert_eq!(
            serde_json::from_str::<Value>(&outcome.stdout).unwrap()["error"]["code"],
            "INVALID_ARGUMENT"
        );
    }

    #[test]
    fn does_not_treat_a_json_named_path_after_separator_as_the_json_flag() {
        let directory = TestDirectory::new("cli-json-path");
        fs::write(directory.path().join("--json"), "body").unwrap();
        let outcome = execute(
            &args(&["export", "--format", "html", "--", "--json"]),
            directory.path(),
        );

        assert_eq!(outcome.exit_code, 3);
        assert!(outcome.stdout.is_empty());
        assert!(outcome.stderr.starts_with("INVALID_FILE_TYPE:"));
    }

    #[test]
    fn json_success_is_single_document_and_output_is_complete() {
        let directory = TestDirectory::new("cli-success");
        let input = directory.path().join("输入 (draft)&.md");
        let output = directory.path().join("输出 (final)&.html");
        fs::write(&input, "# 标题\n\nbody").unwrap();

        let outcome = execute(
            &[
                OsString::from("export"),
                input.into_os_string(),
                OsString::from("--format"),
                OsString::from("html"),
                OsString::from("--output"),
                output.clone().into_os_string(),
                OsString::from("--json"),
            ],
            directory.path(),
        );

        assert_eq!(outcome.exit_code, 0);
        assert!(outcome.stderr.is_empty());
        assert_eq!(outcome.stdout.lines().count(), 1);
        let result: Value = serde_json::from_str(&outcome.stdout).unwrap();
        assert_eq!(result["schemaVersion"], 1);
        assert_eq!(result["ok"], true);
        assert_eq!(result["format"], "html");
        assert_eq!(result["bytes"], fs::metadata(&output).unwrap().len());
        assert!(fs::read_to_string(output).unwrap().ends_with("</html>"));
    }

    #[test]
    fn existing_target_fails_closed_until_overwrite_is_explicit() {
        let directory = TestDirectory::new("cli-conflict");
        let input = directory.path().join("input.md");
        let output = directory.path().join("output.docx");
        fs::write(&input, "body").unwrap();
        fs::write(&output, "keep").unwrap();
        let base = vec![
            OsString::from("export"),
            input.into_os_string(),
            OsString::from("--format"),
            OsString::from("docx"),
            OsString::from("--output"),
            output.clone().into_os_string(),
            OsString::from("--json"),
        ];

        let conflict = execute(&base, directory.path());
        assert_eq!(conflict.exit_code, 4);
        assert_eq!(fs::read(&output).unwrap(), b"keep");
        assert_eq!(
            serde_json::from_str::<Value>(&conflict.stdout).unwrap()["error"]["code"],
            "EXPORT_TARGET_EXISTS"
        );

        let mut overwrite = base;
        overwrite.push(OsString::from("--overwrite"));
        let success = execute(&overwrite, directory.path());
        assert_eq!(success.exit_code, 0);
        assert!(fs::read(&output).unwrap().starts_with(b"PK"));
    }

    #[test]
    fn existing_pdf_target_fails_before_starting_the_platform_runtime() {
        let directory = TestDirectory::new("cli-pdf-conflict");
        let input = directory.path().join("input.md");
        let output = directory.path().join("output.pdf");
        fs::write(&input, "body").unwrap();
        fs::write(&output, "keep").unwrap();

        let outcome = execute(
            &[
                OsString::from("export"),
                input.into_os_string(),
                OsString::from("--format"),
                OsString::from("pdf"),
                OsString::from("--output"),
                output.clone().into_os_string(),
                OsString::from("--json"),
            ],
            directory.path(),
        );

        assert_eq!(outcome.exit_code, 4);
        assert_eq!(fs::read(&output).unwrap(), b"keep");
        assert_eq!(
            serde_json::from_str::<Value>(&outcome.stdout).unwrap()["error"]["code"],
            "EXPORT_TARGET_EXISTS"
        );
    }

    #[test]
    fn shell_export_reserves_a_collision_free_name_without_overwriting() {
        let directory = TestDirectory::new("shell-export-collision");
        let input = directory.path().join("输入 (draft)&.md");
        let first = directory.path().join("输入 (draft)&.html");
        let second = directory.path().join("输入 (draft)& (1).html");
        fs::write(&input, "# 标题\n\nbody").unwrap();
        fs::write(&first, "keep").unwrap();

        let outcome = execute_shell_export(
            &[
                OsString::from("--input"),
                input.into_os_string(),
                OsString::from("--format"),
                OsString::from("html"),
            ],
            directory.path(),
        );

        assert_eq!(outcome.exit_code, 0);
        assert_eq!(fs::read_to_string(first).unwrap(), "keep");
        assert!(fs::read_to_string(second).unwrap().ends_with("</html>"));
    }

    #[test]
    fn shell_export_rejects_extra_options_instead_of_exposing_overwrite() {
        let directory = TestDirectory::new("shell-export-options");
        let outcome = execute_shell_export(
            &args(&["--input", "input.md", "--format", "html", "--overwrite"]),
            directory.path(),
        );

        assert_eq!(outcome.exit_code, 2);
        assert!(outcome.stderr.starts_with("INVALID_ARGUMENT:"));
    }

    #[test]
    fn invalid_arguments_and_missing_inputs_have_stable_exit_codes() {
        let directory = TestDirectory::new("cli-errors");
        let invalid = execute(&args(&["export", "input.md", "--json"]), directory.path());
        assert_eq!(invalid.exit_code, 2);
        assert!(invalid.stderr.is_empty());
        let invalid_json: Value = serde_json::from_str(&invalid.stdout).unwrap();
        assert_eq!(invalid_json["error"]["code"], "INVALID_ARGUMENT");

        let missing = execute(
            &args(&["export", "missing.md", "--format", "html", "--json"]),
            directory.path(),
        );
        assert_eq!(missing.exit_code, 3);
        let missing_json: Value = serde_json::from_str(&missing.stdout).unwrap();
        assert_eq!(missing_json["error"]["code"], "FILE_NOT_FOUND");
    }
}
