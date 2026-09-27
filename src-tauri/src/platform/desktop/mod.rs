#[cfg(desktop)]
pub(crate) mod export_resources;
pub mod resources;
#[cfg(desktop)]
pub mod workspace;
#[cfg(desktop)]
pub mod workspace_watch;

#[cfg(desktop)]
pub mod export;

#[cfg(desktop)]
pub(crate) mod chapter;

#[cfg(desktop)]
pub(crate) mod pdf;

#[cfg(desktop)]
pub(crate) mod pdf_native;
