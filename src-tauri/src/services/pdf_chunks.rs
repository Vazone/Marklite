use crate::models::markdown_event::Event;
use crate::{
    models::app_error::AppError,
    services::export_semantic::{SemanticDocument, SemanticNode},
};
use pulldown_cmark::Tag;
use std::ops::Range;

pub(crate) const MAX_PART_NODES: usize = 32_000;
pub(crate) const MAX_PART_TEXT_BYTES: usize = 512 * 1024;
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    nodes: usize,
    text_bytes: usize,
}

impl Limits {
    #[cfg(any(test, target_os = "android"))]
    pub(crate) const ANDROID: Self = Self {
        nodes: 2_048,
        text_bytes: 16 * 1024,
    };
    const DESKTOP: Self = Self {
        nodes: MAX_PART_NODES,
        text_bytes: MAX_PART_TEXT_BYTES,
    };
}

/// Long plain paragraphs and fenced code can be continued at print boundaries
/// without truncating their text. Retain arena identities for all other blocks.
pub(crate) fn split_large_text_blocks(document: &mut SemanticDocument) {
    split_large_text_blocks_with_limits(document, Limits::DESKTOP);
}

pub(crate) fn split_large_text_blocks_with_limits(document: &mut SemanticDocument, limits: Limits) {
    let continuation_nodes = limits.nodes / 2;
    let continuation_bytes = limits.text_bytes / 2;
    let mut roots = Vec::new();
    for root in document.roots().to_vec() {
        if let SemanticNode::Event(Event::Toc(toc)) = document.node(root) {
            let pieces: Vec<_> = toc
                .chunks(continuation_nodes / 3, continuation_bytes)
                .collect();
            if pieces.len() > 1 {
                for part in pieces {
                    roots
                        .push(document.append_fragment_node(SemanticNode::Event(Event::Toc(part))));
                }
            } else {
                roots.push(root);
            }
            continue;
        }
        // Diagram artifacts are bound to the original node identity. A diagram
        // is indivisible and must be checked by the planner, never text-split.
        if document.diagram_source(root).is_some() {
            roots.push(root);
            continue;
        }
        let (tag, children) = match document.node(root) {
            SemanticNode::Element { tag, children }
                if matches!(tag, Tag::Paragraph | Tag::CodeBlock(_)) =>
            {
                (tag.clone(), children.clone())
            }
            _ => {
                roots.push(root);
                continue;
            }
        };
        let plain = children.iter().all(|id| {
            matches!(
                document.node(*id),
                SemanticNode::Event(Event::Text(_) | Event::SoftBreak | Event::HardBreak)
            )
        });
        if !plain {
            roots.push(root);
            continue;
        }
        let total_bytes: usize = children
            .iter()
            .map(|id| match document.node(*id) {
                SemanticNode::Event(Event::Text(text)) => text.len(),
                _ => 0,
            })
            .sum();
        if children.len() < continuation_nodes && total_bytes <= continuation_bytes {
            roots.push(root);
            continue;
        }
        let mut pieces = Vec::new();
        for child in children {
            let text = match document.node(child) {
                SemanticNode::Event(Event::Text(text)) if text.len() > continuation_bytes => {
                    Some(text.clone())
                }
                _ => None,
            };
            if let Some(text) = text {
                let mut cursor = 0;
                while cursor < text.len() {
                    let mut end = (cursor + continuation_bytes).min(text.len());
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    if end < text.len() {
                        if let Some(newline) = text[cursor..end].rfind('\n') {
                            if newline > continuation_bytes / 2 {
                                end = cursor + newline + 1;
                            }
                        }
                    }
                    pieces.push(
                        document.append_fragment_node(SemanticNode::Event(Event::Text(
                            text[cursor..end].to_owned().into(),
                        ))),
                    );
                    cursor = end;
                }
            } else {
                pieces.push(child);
            }
        }
        let mut group = Vec::new();
        let mut bytes = 0;
        for piece in pieces {
            let size = match document.node(piece) {
                SemanticNode::Event(Event::Text(text)) => text.len(),
                _ => 0,
            };
            if !group.is_empty()
                && (group.len() + 2 > continuation_nodes || bytes + size > continuation_bytes)
            {
                roots.push(document.append_fragment_node(SemanticNode::Element {
                    tag: tag.clone(),
                    children: std::mem::take(&mut group),
                }));
                bytes = 0;
            }
            group.push(piece);
            bytes += size;
        }
        if !group.is_empty() {
            roots.push(document.append_fragment_node(SemanticNode::Element {
                tag,
                children: group,
            }));
        }
    }
    document.replace_roots(roots);
}

/// Stable root ranges over a single parsed document. No Markdown reparsing or
/// cloned arena per part; each print surface ends at a semantic block boundary.
pub(crate) fn plan(document: &SemanticDocument) -> Result<Vec<Range<usize>>, AppError> {
    plan_with_limits(document, Limits::DESKTOP)
}

pub(crate) fn plan_with_limits(
    document: &SemanticDocument,
    limits: Limits,
) -> Result<Vec<Range<usize>>, AppError> {
    let roots = document.roots();
    let mut ranges = Vec::new();
    let (mut start, mut nodes, mut bytes) = (0, 0, 0);
    let mut trailing_headings: Option<(usize, usize, usize)> = None;
    for (index, root) in roots.iter().enumerate() {
        let (mut block_nodes, mut block_bytes) = (0, 0);
        let mut pending = vec![*root];
        while let Some(id) = pending.pop() {
            block_nodes += 1;
            match document.node(id) {
                SemanticNode::Highlight { children } => pending.extend(children.iter().copied()),
                SemanticNode::Element { tag, children } => {
                    if let Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(info)) = tag {
                        block_nodes += super::code_highlight::node_budget(
                            &document.plain_text(children),
                            info,
                        );
                    }
                    pending.extend(children.iter().copied());
                    if let Tag::Image { dest_url, .. } | Tag::Link { dest_url, .. } = tag {
                        block_bytes += dest_url.len();
                    }
                }
                SemanticNode::Event(Event::Toc(toc)) => {
                    block_nodes += 1 + toc.headings().len() * 3;
                    block_bytes += toc.text_bytes();
                }
                SemanticNode::Event(event) => {
                    block_bytes += match event {
                        Event::Text(s)
                        | Event::Code(s)
                        | Event::Html(s)
                        | Event::InlineHtml(s)
                        | Event::InlineMath(s)
                        | Event::DisplayMath(s) => s.len(),
                        _ => 0,
                    }
                }
            }
            if block_nodes > limits.nodes || block_bytes > limits.text_bytes {
                return Err(AppError::new(
                    "PDF_BLOCK_TOO_LARGE",
                    format!("第 {} 个内容块超过单批 PDF 渲染预算", index + 1),
                ));
            }
        }
        if index > start
            && (nodes + block_nodes > limits.nodes || bytes + block_bytes > limits.text_bytes)
        {
            let (boundary, carried_nodes, carried_bytes) =
                trailing_headings.unwrap_or((index, 0, 0));
            if boundary == start
                || carried_nodes + block_nodes > limits.nodes
                || carried_bytes + block_bytes > limits.text_bytes
            {
                return Err(AppError::new(
                    "PDF_BLOCK_TOO_LARGE",
                    format!("第 {} 个内容块及其标题超过单批 PDF 渲染预算", index + 1),
                ));
            }
            ranges.push(start..boundary);
            start = boundary;
            nodes = carried_nodes;
            bytes = carried_bytes;
        }
        nodes += block_nodes;
        bytes += block_bytes;
        if matches!(
            document.node(*root),
            SemanticNode::Element {
                tag: Tag::Heading { .. },
                ..
            }
        ) {
            let (first, count, text) = trailing_headings.unwrap_or((index, 0, 0));
            trailing_headings = Some((first, count + block_nodes, text + block_bytes));
        } else {
            trailing_headings = None;
        }
    }
    // Empty documents still produce the configured title / blank page.
    ranges.push(start..roots.len());
    Ok(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_batches_keep_text_and_bound_plain_blocks() {
        let content = format!("# Start\n\n{}", "Long paragraph. ".repeat(5_000));
        let mut document = SemanticDocument::parse(&content, None);
        let before = document.plain_text(document.roots());
        split_large_text_blocks_with_limits(&mut document, Limits::ANDROID);
        let ranges = plan_with_limits(&document, Limits::ANDROID).unwrap();
        assert!(ranges.len() > 1);
        assert_eq!(document.plain_text(document.roots()), before);
        assert_eq!(
            ranges.iter().map(|range| range.len()).sum::<usize>(),
            document.roots().len()
        );
    }
    #[test]
    fn math_and_footnote_links_survive_pdf_parts() {
        use crate::services::{export_html_writer, export_resources::ExportResourceResolver};
        use std::collections::HashMap;

        let fixture = include_str!("../../../scripts/fixtures/math-export.md");
        let content = fixture.replace(
            "# Matrices and links",
            &format!(
                "{}\n\n# Matrices and links",
                "Cross-batch paragraph. ".repeat(30_000)
            ),
        );
        let mut document = SemanticDocument::parse(&content, None);
        split_large_text_blocks(&mut document);
        let ranges = plan(&document).unwrap();
        assert_eq!(ranges.len(), 2);
        let request = serde_json::from_value(serde_json::json!({
            "snapshot": { "jobId":"math-parts", "tabId":"tab", "contentRevision":1, "sourcePath":null, "title":"Math", "content":content },
            "targetPath":"math.pdf", "format":"pdf", "options":{"paperSize":"a4", "orientation":"portrait", "margin":"normal", "includeTitle":false, "includeLocalImages":false}
        })).unwrap();
        let mut resources = ExportResourceResolver::new(None, false);
        let parts: Vec<_> = ranges
            .iter()
            .map(|range| {
                export_html_writer::render_roots_with_resources(
                    &document,
                    &document.roots()[range.clone()],
                    &request,
                    &HashMap::new(),
                    &mut resources,
                    true,
                )
            })
            .collect();
        assert!(parts[0].contains("id=\"formula\""));
        assert!(!parts[1].contains("id=\"formula\""));
        assert!(parts[1].contains("https://marklite.invalid/_pdf-anchor/#formula"));
        assert!(parts[0].contains("<mfrac>"));
        assert!(parts[0].contains("𝒜"));
        assert!(parts[0].contains("<mtable"));
        assert!(parts[1].matches("<mtable").count() >= 2);
        assert_eq!(
            parts
                .iter()
                .map(|html| html.matches("<math ").count())
                .sum::<usize>(),
            10
        );
        assert!(parts[1].contains("mailto:reader@example.com"));
        assert!(parts[1].contains("https://example.com/?a=1&amp;b=2"));
        assert!(!parts.iter().any(|html| html.contains("math-error\"")));
    }

    #[test]
    fn large_toc_parts_preserve_global_links_and_target_markers() {
        use crate::services::{export_html_writer, export_resources::ExportResourceResolver};
        use std::collections::HashMap;
        let content = format!("[TOC]\n\n{}", "# 中文\n\n".repeat(10_000));
        let mut document = SemanticDocument::parse(&content, None);
        split_large_text_blocks(&mut document);
        let ranges = plan(&document).unwrap();
        assert!(ranges.len() > 1);
        let toc_ranges: Vec<_> = document
            .roots()
            .iter()
            .filter_map(|root| {
                if let SemanticNode::Event(Event::Toc(toc)) = document.node(*root) {
                    Some(toc)
                } else {
                    None
                }
            })
            .collect();
        assert!(toc_ranges.len() > 1);
        assert_eq!(
            toc_ranges
                .iter()
                .map(|toc| toc.headings().len())
                .sum::<usize>(),
            10_000
        );
        let last_slug = toc_ranges
            .last()
            .unwrap()
            .headings()
            .last()
            .unwrap()
            .slug
            .clone();
        let request = serde_json::from_value(serde_json::json!({
            "snapshot": { "jobId":"toc-parts", "tabId":"tab", "contentRevision":1, "sourcePath":null, "title":"TOC", "content":content },
            "targetPath":"toc.pdf", "format":"pdf", "options":{"paperSize":"a4", "orientation":"portrait", "margin":"normal", "includeTitle":false, "includeLocalImages":false}
        })).unwrap();
        let mut resources = ExportResourceResolver::new(None, false);
        let mut links = 0;
        let mut targets = 0;
        let mut link_part = None;
        let mut target_part = None;
        for (index, range) in ranges.iter().enumerate() {
            let html = export_html_writer::render_roots_with_resources(
                &document,
                &document.roots()[range.clone()],
                &request,
                &HashMap::new(),
                &mut resources,
                true,
            );
            let html = percent_encoding::percent_decode_str(&html)
                .decode_utf8()
                .unwrap();
            links += html
                .matches("href=\"https://marklite.invalid/_pdf-anchor/#")
                .count();
            targets += html.matches("<h1 id=").count();
            if html.contains(&format!(
                "href=\"https://marklite.invalid/_pdf-anchor/#{last_slug}\""
            )) {
                link_part = Some(index);
            }
            if html.contains(&format!("id=\"{last_slug}\"")) {
                target_part = Some(index);
                assert!(html.contains(&format!("href=\"#{last_slug}\"")));
            }
        }
        assert_eq!(links, 10_000);
        assert_eq!(targets, 10_000);
        assert!(link_part.unwrap() < target_part.unwrap());
        assert!(resources.into_warnings().is_empty());
    }

    #[test]
    fn covers_roots_once_without_splitting_normal_blocks() {
        let content = "# Heading\n\nA paragraph with **strong** text.\n\n".repeat(10000);
        let document = SemanticDocument::parse(&content, None);
        let ranges = plan(&document).unwrap();
        assert!(ranges.len() > 1);
        let covered = ranges
            .iter()
            .flat_map(|range| range.clone())
            .collect::<Vec<_>>();
        assert_eq!(covered, (0..document.roots().len()).collect::<Vec<_>>());
    }
    #[test]
    fn moves_a_trailing_heading_with_its_following_block() {
        let content = format!(
            "{}\n\n# Title\n\n{}",
            "x".repeat(MAX_PART_TEXT_BYTES - 16),
            "y".repeat(30)
        );
        let document = SemanticDocument::parse(&content, None);
        assert_eq!(plan(&document).unwrap(), vec![0..1, 1..3]);
    }

    #[test]
    fn continues_long_plain_paragraphs_and_code_without_losing_unicode_or_lines() {
        for content in [
            "中文 line\n".repeat(100000),
            format!("```text\n{}\n```", "中文x".repeat(200000)),
        ] {
            let mut document = SemanticDocument::parse(&content, None);
            let before = document.plain_text(document.roots());
            split_large_text_blocks(&mut document);
            assert!(plan(&document).unwrap().len() > 1);
            assert_eq!(document.plain_text(document.roots()), before);
        }
    }

    #[test]
    fn refuses_an_unbounded_single_block_instead_of_silently_truncating() {
        let document = SemanticDocument::parse(&"x".repeat(MAX_PART_TEXT_BYTES + 1), None);
        assert_eq!(plan(&document).unwrap_err().code, "PDF_BLOCK_TOO_LARGE");
    }

    #[test]
    fn heading_and_long_continuations_fit_without_changing_text() {
        for body in [
            "字".repeat(MAX_PART_TEXT_BYTES),
            "x".repeat(MAX_PART_TEXT_BYTES),
            format!("```rust\n{}\n```", "x".repeat(MAX_PART_TEXT_BYTES * 2)),
        ] {
            let mut document = SemanticDocument::parse(&format!("# Title\n\n{body}"), None);
            let before = document.plain_text(document.roots());
            split_large_text_blocks(&mut document);
            let ranges = plan(&document).unwrap();
            assert!(ranges.len() > 1);
            assert!(ranges[0].end >= 2);
            assert_eq!(document.plain_text(document.roots()), before);
        }
    }

    #[test]
    fn oversized_mermaid_keeps_identity_and_is_rejected_before_rendering() {
        let mut document = SemanticDocument::parse(
            &format!(
                "```mermaid\nflowchart TD\n{}\n```",
                "%% comment\n".repeat(60000)
            ),
            None,
        );
        let roots = document.roots().to_vec();
        assert!(document.diagram_source(roots[0]).is_some());
        split_large_text_blocks(&mut document);
        assert_eq!(document.roots(), roots);
        assert!(document.diagram_source(roots[0]).is_some());
        assert_eq!(plan(&document).unwrap_err().code, "PDF_BLOCK_TOO_LARGE");
    }
}
