use std::{ffi::OsStr, fs};

use pulldown_cmark::{html, Options, Parser};
use serde_json::{json, Value};

use crate::services::markdown_service;

// Observation harness: reuse the production options and renderer without changing
// application behavior. HTML normalization and comparison run in the JS driver.
pub fn run(input: &OsStr, output: &OsStr) {
    let cases: Vec<Value> = serde_json::from_slice(&fs::read(input).unwrap()).unwrap();
    let rows: Vec<_> = cases
        .iter()
        .map(|case| {
            let markdown = case["markdown"].as_str().unwrap();
            let mut commonmark = String::new();
            html::push_html(&mut commonmark, Parser::new_ext(markdown, Options::empty()));
            let mut gfm = String::new();
            html::push_html(
                &mut gfm,
                markdown_service::profile_events(markdown, markdown_service::SyntaxProfile::Gfm)
                    .into_iter(),
            );
            let mut configured = String::new();
            html::push_html(
                &mut configured,
                markdown_service::profile_events(
                    markdown,
                    markdown_service::DEFAULT_SYNTAX_PROFILE,
                )
                .into_iter(),
            );
            let rendered = markdown_service::render_markdown(markdown).unwrap();
            json!({
                "suite": case["suite"], "example": case["example"], "section": case["section"],
                "expected": case["html"], "commonmark": commonmark,
                "gfm": gfm, "configured": configured, "preview": rendered.html
            })
        })
        .collect();
    fs::write(output, serde_json::to_vec(&rows).unwrap()).unwrap();
    println!(
        "Rendered {} official/custom syntax examples through production options and preview",
        rows.len()
    );
}
