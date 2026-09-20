#![allow(dead_code)]

use std::{env, fs, path::PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

mod models {
    pub mod app_error {
        include!("../models/app_error.rs");
    }
    pub mod diagram {
        include!("../models/diagram.rs");
    }
}
#[path = "../services/diagram_service.rs"]
mod diagram_service;

use models::diagram::{DiagramSource, RenderedDiagram};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixtureArtifact {
    id: String,
    source: String,
    svg_utf8: String,
    view_box: [f64; 4],
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(
        env::args_os()
            .nth(1)
            .ok_or("missing fixture artifact path")?,
    );
    let artifacts: Vec<FixtureArtifact> = serde_json::from_slice(&fs::read(path)?)?;
    if artifacts.len() != 27 {
        return Err(format!(
            "expected 27 fixture artifacts, received {}",
            artifacts.len()
        )
        .into());
    }
    let mut total_svg_bytes = 0usize;
    for (ordinal, artifact) in artifacts.into_iter().enumerate() {
        let source_sha256 = format!("{:x}", Sha256::digest(artifact.source.as_bytes()));
        let source = DiagramSource {
            diagram_id: format!("diagram-{ordinal}-{}", &source_sha256[..12]),
            ordinal,
            source_utf8: artifact.source.clone(),
            source_sha256: source_sha256.clone(),
            source_start_byte: 0,
            source_end_byte: artifact.source.len(),
        };
        let diagnostics = diagram_service::validate_sources(std::slice::from_ref(&source));
        if !diagnostics.is_empty() {
            return Err(format!("{} failed source policy: {diagnostics:?}", artifact.id).into());
        }
        total_svg_bytes = total_svg_bytes.saturating_add(artifact.svg_utf8.len());
        diagram_service::validate_rendered_diagram(RenderedDiagram {
            diagram_id: source.diagram_id,
            source_sha256,
            cache_key: "b".repeat(64),
            renderer_id: diagram_service::RENDERER_ID.to_string(),
            svg_utf8: artifact.svg_utf8,
            width: artifact.view_box[2],
            height: artifact.view_box[3],
            view_box: artifact.view_box,
            accessible_title: None,
            accessible_description: None,
            warnings: Vec::new(),
        })
        .map_err(|error| {
            format!(
                "{} failed SVG policy: {}: {}",
                artifact.id, error.code, error.message
            )
        })?;
    }
    println!("validated 27 Mermaid fixtures ({total_svg_bytes} SVG bytes)");
    Ok(())
}
