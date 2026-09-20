use crate::{
    models::app_error::AppError,
    services::export_semantic::{SemanticDocument, SemanticNode},
};
use pulldown_cmark::{Event, Tag};
use std::ops::Range;

pub(crate) const MAX_PART_NODES: usize = 32_000;
pub(crate) const MAX_PART_TEXT_BYTES: usize = 512 * 1024;
// Leave room for the heading carried with the first continuation.
const MAX_CONTINUATION_BYTES: usize = MAX_PART_TEXT_BYTES / 2;
const MAX_CONTINUATION_NODES: usize = MAX_PART_NODES / 2;

/// Long plain paragraphs and fenced code can be continued at print boundaries
/// without truncating their text. Retain arena identities for all other blocks.
pub(crate) fn split_large_text_blocks(document: &mut SemanticDocument) {
    let mut roots = Vec::new();
    for root in document.roots().to_vec() {
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
        if children.len() < MAX_CONTINUATION_NODES && total_bytes <= MAX_CONTINUATION_BYTES {
            roots.push(root);
            continue;
        }
        let mut pieces = Vec::new();
        for child in children {
            let text = match document.node(child) {
                SemanticNode::Event(Event::Text(text)) if text.len() > MAX_CONTINUATION_BYTES => {
                    Some(text.clone())
                }
                _ => None,
            };
            if let Some(text) = text {
                let mut cursor = 0;
                while cursor < text.len() {
                    let mut end = (cursor + MAX_CONTINUATION_BYTES).min(text.len());
                    while !text.is_char_boundary(end) {
                        end -= 1;
                    }
                    if end < text.len() {
                        if let Some(newline) = text[cursor..end].rfind('\n') {
                            if newline > MAX_CONTINUATION_BYTES / 2 {
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
                && (group.len() + 2 > MAX_CONTINUATION_NODES
                    || bytes + size > MAX_CONTINUATION_BYTES)
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
                SemanticNode::Element { tag, children } => {
                    pending.extend(children.iter().copied());
                    if let Tag::Image { dest_url, .. } | Tag::Link { dest_url, .. } = tag {
                        block_bytes += dest_url.len();
                    }
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
            if block_nodes > MAX_PART_NODES || block_bytes > MAX_PART_TEXT_BYTES {
                return Err(AppError::new(
                    "PDF_BLOCK_TOO_LARGE",
                    format!("第 {} 个内容块超过单批 PDF 渲染预算", index + 1),
                ));
            }
        }
        if index > start
            && (nodes + block_nodes > MAX_PART_NODES || bytes + block_bytes > MAX_PART_TEXT_BYTES)
        {
            let (boundary, carried_nodes, carried_bytes) =
                trailing_headings.unwrap_or((index, 0, 0));
            if boundary == start
                || carried_nodes + block_nodes > MAX_PART_NODES
                || carried_bytes + block_bytes > MAX_PART_TEXT_BYTES
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
