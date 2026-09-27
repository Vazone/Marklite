#[cfg(target_os = "android")]
pub mod android_document_commands;
#[cfg(target_os = "android")]
pub mod android_export_commands;
#[cfg(target_os = "android")]
pub mod android_lifecycle_commands;
#[cfg(target_os = "android")]
pub mod android_workspace_commands;
mod background;
pub mod diagram_commands;
pub mod export_commands;
pub mod file_commands;
pub mod markdown_commands;
pub mod navigation_commands;
pub mod open_request_commands;
pub mod platform_commands;
pub mod recent_commands;
pub mod recovery_commands;
pub mod session_commands;
pub mod settings_commands;
pub mod startup_commands;
#[cfg(desktop)]
pub mod workspace_commands;
