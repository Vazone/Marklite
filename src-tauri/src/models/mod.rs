pub mod app_error;
pub mod diagram;
pub mod document;
pub mod export;
pub mod markdown;
pub(crate) mod markdown_event;
pub mod navigation;
pub mod recent;
pub mod recovery;
pub mod resource;
pub mod session;
pub mod settings;
pub mod startup;
pub mod workspace;

#[cfg(test)]
mod contract_tests;

pub(crate) mod diagram_assets;
