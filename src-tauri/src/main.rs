#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    if let Some(exit_code) = marklite_lib::cli::try_run_shell_export_from_env() {
        std::process::exit(exit_code);
    }
    if let Some(exit_code) = marklite_lib::cli::try_run_from_env() {
        std::process::exit(exit_code);
    }
    marklite_lib::run();
}
