use super::{diagram::RenderedDiagram, export::ExportWarning};
use std::collections::HashMap;

#[derive(Debug)]
pub(crate) struct PreparedDiagrams {
    pub artifacts: HashMap<String, RenderedDiagram>,
    pub print_artifacts: HashMap<String, RenderedDiagram>,
    pub rasters: HashMap<String, RasterDiagram>,
    pub warnings: Vec<ExportWarning>,
}

#[derive(Debug)]
pub(crate) struct RasterDiagram {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl PreparedDiagrams {
    pub fn empty() -> Self {
        Self {
            artifacts: HashMap::new(),
            print_artifacts: HashMap::new(),
            rasters: HashMap::new(),
            warnings: Vec::new(),
        }
    }
}
