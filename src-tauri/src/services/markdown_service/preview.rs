use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use pulldown_cmark::{Event, Tag, TagEnd};

use crate::models::{
    app_error::AppError,
    diagram::DiagramDiagnostic,
    diagram::DiagramSource,
    markdown::{
        OutlineItem, RenderedMarkdownDto, SourceBlock, VirtualPreviewIndex,
        VirtualPreviewRenderedSegment, VirtualPreviewSegment, VirtualPreviewWindow,
    },
};

use super::{analyze_with_events, render_markdown_html_from};
use crate::services::diagram_service;

const MAX_EVENTS_PER_SEGMENT: usize = 512;
const MAX_BLOCKS_PER_SEGMENT: usize = 64;
const MAX_SOURCE_BYTES_PER_SEGMENT: usize = 32 * 1024;
const OVERSIZED_BLOCK_BYTES: usize = 128 * 1024;
const MAX_NODES_PER_SEGMENT: usize = 4_000;
const MAX_WINDOW_SEGMENTS: usize = 12;

struct IndexedSegment {
    events: Vec<Event<'static>>,
    source_override: Option<SourceBlock>,
    source_block_start: usize,
    source_block_end: usize,
    heading_start: usize,
    heading_end: usize,
    diagram_start: usize,
    diagram_end: usize,
    formula_start: usize,
    estimated_height: usize,
    estimated_nodes: usize,
    oversized: bool,
    oversized_excerpt: Option<String>,
}

pub struct PreviewSession {
    session_id: String,
    segments: Vec<IndexedSegment>,
    source_blocks: Vec<SourceBlock>,
    outline: Vec<OutlineItem>,
    diagrams: Vec<DiagramSource>,
    diagram_diagnostics: Vec<DiagramDiagnostic>,
}

#[derive(Clone, Default)]
pub struct PreviewState {
    inner: Arc<PreviewStateInner>,
}

#[derive(Default)]
struct PreviewStateInner {
    latest_generation: AtomicU64,
    session: Mutex<Option<Arc<PreviewSession>>>,
}

impl PreviewState {
    pub fn reserve_generation(&self) -> u64 {
        self.inner.latest_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn install(&self, generation: u64, session: PreviewSession) -> bool {
        if self.inner.latest_generation.load(Ordering::SeqCst) != generation {
            return false;
        }
        let mut slot = self
            .inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.inner.latest_generation.load(Ordering::SeqCst) != generation {
            return false;
        }
        *slot = Some(Arc::new(session));
        true
    }

    pub fn clear_if_current(&self, generation: u64) {
        let mut slot = self
            .inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.inner.latest_generation.load(Ordering::SeqCst) == generation {
            *slot = None;
        }
    }

    pub fn get(&self, session_id: &str) -> Result<Arc<PreviewSession>, AppError> {
        let slot = self
            .inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        slot.as_ref()
            .filter(|session| session.session_id == session_id)
            .cloned()
            .ok_or_else(|| AppError::new("PREVIEW_SESSION_EXPIRED", "预览文档版本已过期"))
    }

    pub fn release(&self, session_id: &str) {
        let mut slot = self
            .inner
            .session
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if slot
            .as_ref()
            .is_some_and(|session| session.session_id == session_id)
        {
            *slot = None;
        }
    }
}

pub fn prepare_virtual_preview(
    markdown: &str,
    session_id: String,
) -> Result<(RenderedMarkdownDto, PreviewSession), AppError> {
    let (analysis, events, source_blocks, diagrams) = analyze_with_events(markdown, true);
    let segments = partition_events(events, &source_blocks, markdown)?;
    let diagram_diagnostics = diagram_service::validate_sources(&diagrams);
    let directory = segments
        .iter()
        .map(|segment| {
            let first = segment
                .source_override
                .as_ref()
                .unwrap_or(&source_blocks[segment.source_block_start]);
            let last = segment
                .source_override
                .as_ref()
                .unwrap_or(&source_blocks[segment.source_block_end - 1]);
            VirtualPreviewSegment {
                start_utf16: first.start_utf16,
                end_utf16: last.end_utf16,
                start_line: first.start_line,
                end_line: last.end_line,
                estimated_height: segment.estimated_height,
                estimated_nodes: segment.estimated_nodes,
            }
        })
        .collect();
    let dto = RenderedMarkdownDto {
        html: String::new(),
        outline: analysis.outline.clone(),
        stats: analysis.stats,
        source_blocks: Vec::new(),
        diagrams: Vec::new(),
        diagram_diagnostics: Vec::new(),
        virtual_preview: Some(VirtualPreviewIndex {
            session_id: session_id.clone(),
            segments: directory,
        }),
    };
    Ok((
        dto,
        PreviewSession {
            session_id,
            segments,
            source_blocks,
            outline: analysis.outline,
            diagrams,
            diagram_diagnostics,
        },
    ))
}

impl PreviewSession {
    pub fn render_window(
        &self,
        start: usize,
        end: usize,
    ) -> Result<VirtualPreviewWindow, AppError> {
        if start >= end || end > self.segments.len() || end - start > MAX_WINDOW_SEGMENTS {
            return Err(AppError::new("INVALID_PREVIEW_WINDOW", "预览窗口范围无效"));
        }
        let mut rendered = Vec::with_capacity(end - start);
        for index in start..end {
            let segment = &self.segments[index];
            let html = if segment.oversized {
                format!(
                    "<div class=\"preview-oversized-block\" role=\"note\">该块过大，仅显示源码摘录；完整内容可在编辑器中查看。<pre><code>{}</code></pre></div>",
                    segment.oversized_excerpt.as_deref().unwrap_or("")
                )
            } else {
                render_markdown_html_from(
                    segment.events.clone(),
                    true,
                    &self.outline[segment.heading_start..segment.heading_end],
                    segment.formula_start,
                )?
            };
            let diagrams = if segment.oversized {
                Vec::new()
            } else {
                self.diagrams[segment.diagram_start..segment.diagram_end].to_vec()
            };
            let diagram_diagnostics = self
                .diagram_diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagrams
                        .iter()
                        .any(|source| source.diagram_id == diagnostic.diagram_id)
                })
                .cloned()
                .collect();
            rendered.push(VirtualPreviewRenderedSegment {
                index,
                html,
                source_block_start: segment.source_block_start,
                source_blocks: segment
                    .source_override
                    .as_ref()
                    .map(|block| vec![block.clone()])
                    .unwrap_or_else(|| {
                        self.source_blocks[segment.source_block_start..segment.source_block_end]
                            .to_vec()
                    }),
                diagrams,
                diagram_diagnostics,
            });
        }
        Ok(VirtualPreviewWindow {
            session_id: self.session_id.clone(),
            start,
            end,
            segments: rendered,
        })
    }
}

fn partition_events(
    events: Vec<Event<'static>>,
    source_blocks: &[SourceBlock],
    markdown: &str,
) -> Result<Vec<IndexedSegment>, AppError> {
    let mut result = Vec::new();
    let mut current = Vec::new();
    let mut block_start = 0;
    let mut block_end = 0;
    let mut heading_start = 0;
    let mut heading_end = 0;
    let mut diagram_start = 0;
    let mut diagram_end = 0;
    let mut formula_start = 0;
    let mut formula_end = 0;
    let mut source_bytes = 0;
    let mut estimated_nodes = 0;
    let mut current_block_bytes = 0;
    let mut current_block_nodes = 0;
    let mut current_block_event_start = 0;
    let mut current_block_heading_start = 0;
    let mut current_block_diagram_start = 0;
    let mut current_block_formula_start = 0;
    let mut oversized = false;

    for event in events {
        if let Some(block_index) = marker_index(&event) {
            if current_block_bytes > OVERSIZED_BLOCK_BYTES
                || current_block_nodes > MAX_NODES_PER_SEGMENT
            {
                if block_end - block_start > 1 {
                    let large_block = current.split_off(current_block_event_start);
                    result.push(finish_segment(
                        std::mem::take(&mut current),
                        source_blocks,
                        block_start,
                        block_end - 1,
                        heading_start,
                        current_block_heading_start,
                        diagram_start,
                        current_block_diagram_start,
                        formula_start,
                        false,
                    ));
                    result.push(finish_segment(
                        large_block,
                        source_blocks,
                        block_end - 1,
                        block_end,
                        current_block_heading_start,
                        heading_end,
                        current_block_diagram_start,
                        diagram_end,
                        current_block_formula_start,
                        true,
                    ));
                    block_start = block_end;
                    heading_start = heading_end;
                    diagram_start = diagram_end;
                    formula_start = formula_end;
                    source_bytes = 0;
                    estimated_nodes = 0;
                    oversized = false;
                } else {
                    oversized = true;
                }
            }
            if !current.is_empty()
                && block_end > block_start
                && (current.len() >= MAX_EVENTS_PER_SEGMENT
                    || block_end - block_start >= MAX_BLOCKS_PER_SEGMENT
                    || source_bytes >= MAX_SOURCE_BYTES_PER_SEGMENT
                    || estimated_nodes >= MAX_NODES_PER_SEGMENT)
            {
                result.push(finish_segment(
                    std::mem::take(&mut current),
                    source_blocks,
                    block_start,
                    block_end,
                    heading_start,
                    heading_end,
                    diagram_start,
                    diagram_end,
                    formula_start,
                    oversized,
                ));
                block_start = block_index;
                heading_start = heading_end;
                diagram_start = diagram_end;
                formula_start = formula_end;
                source_bytes = 0;
                estimated_nodes = 0;
                oversized = false;
            }
            if block_index != block_end {
                return Err(AppError::new(
                    "PREVIEW_INDEX_MISMATCH",
                    "预览来源块顺序不一致",
                ));
            }
            current_block_bytes = 0;
            current_block_nodes = 0;
            current_block_event_start = current.len();
            current_block_heading_start = heading_end;
            current_block_diagram_start = diagram_end;
            current_block_formula_start = formula_end;
            block_end += 1;
        }
        if matches!(event, Event::Start(Tag::Heading { .. })) {
            heading_end += 1;
        }
        if matches!(event, Event::InlineMath(_) | Event::DisplayMath(_)) {
            formula_end += 1;
        }
        if matches!(&event, Event::Start(Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(info))) if info.trim().eq_ignore_ascii_case("mermaid"))
        {
            diagram_end += 1;
        }
        let weight = event_weight(&event);
        let nodes = event_node_weight(&event);
        source_bytes += weight;
        estimated_nodes += nodes;
        current_block_bytes += weight;
        current_block_nodes += nodes;
        current.push(event);
    }
    if current_block_bytes > OVERSIZED_BLOCK_BYTES || current_block_nodes > MAX_NODES_PER_SEGMENT {
        if block_end - block_start > 1 {
            let large_block = current.split_off(current_block_event_start);
            result.push(finish_segment(
                std::mem::take(&mut current),
                source_blocks,
                block_start,
                block_end - 1,
                heading_start,
                current_block_heading_start,
                diagram_start,
                current_block_diagram_start,
                formula_start,
                false,
            ));
            current = large_block;
            block_start = block_end - 1;
            heading_start = current_block_heading_start;
            diagram_start = current_block_diagram_start;
            formula_start = current_block_formula_start;
        }
        oversized = true;
    }
    if block_end != source_blocks.len() {
        return Err(AppError::new(
            "PREVIEW_INDEX_MISMATCH",
            "预览来源块数量不一致",
        ));
    }
    if !current.is_empty() && block_end > block_start {
        result.push(finish_segment(
            current,
            source_blocks,
            block_start,
            block_end,
            heading_start,
            heading_end,
            diagram_start,
            diagram_end,
            formula_start,
            oversized,
        ));
    }
    Ok(result
        .into_iter()
        .flat_map(|segment| expand_oversized_segment(segment, source_blocks, markdown))
        .collect())
}

#[allow(clippy::too_many_arguments)]
fn finish_segment(
    events: Vec<Event<'static>>,
    blocks: &[SourceBlock],
    block_start: usize,
    block_end: usize,
    heading_start: usize,
    heading_end: usize,
    diagram_start: usize,
    diagram_end: usize,
    formula_start: usize,
    oversized: bool,
) -> IndexedSegment {
    let first = &blocks[block_start];
    let last = &blocks[block_end - 1];
    let line_count = last.end_line.saturating_sub(first.start_line) + 1;
    let oversized_excerpt = oversized.then(|| {
        let mut excerpt = String::new();
        let mut remaining = 4096;
        for event in &events {
            let source = match event {
                Event::Text(value)
                | Event::Code(value)
                | Event::Html(value)
                | Event::InlineHtml(value)
                    if marker_index(event).is_none() =>
                {
                    value.as_ref()
                }
                _ => continue,
            };
            for character in source.chars() {
                if remaining == 0 {
                    break;
                }
                excerpt.push(character);
                remaining -= 1;
            }
            if remaining == 0 {
                break;
            }
        }
        excerpt
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    });
    let estimated_height = if oversized {
        64
    } else {
        (line_count * 20 + (block_end - block_start) * 18).max(32)
    };
    IndexedSegment {
        estimated_nodes: if oversized {
            1
        } else {
            events.iter().map(event_node_weight).sum()
        },
        estimated_height,
        events,
        source_override: None,
        source_block_start: block_start,
        source_block_end: block_end,
        heading_start,
        heading_end,
        diagram_start,
        diagram_end,
        formula_start,
        oversized,
        oversized_excerpt,
    }
}

fn expand_oversized_segment(
    mut segment: IndexedSegment,
    blocks: &[SourceBlock],
    markdown: &str,
) -> Vec<IndexedSegment> {
    if !segment.oversized || segment.source_block_end - segment.source_block_start != 1 {
        return vec![segment];
    }
    if let [_, Event::Start(Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(info))), middle @ .., Event::End(TagEnd::CodeBlock)] =
        segment.events.as_slice()
    {
        if info.trim().eq_ignore_ascii_case("mermaid") {
            let source_bytes = middle.iter().map(event_weight).sum::<usize>();
            let source_lines = middle
                .iter()
                .map(|event| match event {
                    Event::Text(value) | Event::Code(value) => {
                        value.bytes().filter(|byte| *byte == b'\n').count()
                    }
                    _ => 0,
                })
                .sum::<usize>()
                + 1;
            if source_bytes <= diagram_service::MAX_DIAGRAM_SOURCE_BYTES
                && source_lines <= diagram_service::MAX_DIAGRAM_SOURCE_LINES
            {
                segment.oversized = false;
                segment.oversized_excerpt = None;
                segment.estimated_nodes = segment.events.len();
                segment.estimated_height = source_lines * 20 + 32;
                return vec![segment];
            }
        }
    }
    let source = &blocks[segment.source_block_start];
    let pieces = split_large_code(&segment.events)
        .map(|chunks| (chunks, 1))
        .or_else(|| split_large_table(&segment.events).map(|chunks| (chunks, 2)));
    let Some((chunks, prefix_lines)) = pieces else {
        segment.events.clear();
        return vec![segment];
    };
    if chunks.len() < 2 {
        segment.events.clear();
        return vec![segment];
    }
    let boundaries = source_line_boundaries(markdown, source);
    let total_lines = chunks.iter().map(|(_, lines)| *lines).sum::<usize>();
    if boundaries.len() <= prefix_lines + total_lines {
        segment.events.clear();
        return vec![segment];
    }
    let mut consumed_lines = 0;
    let mut consumed_formulas = 0;
    let chunk_count = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, (events, line_count))| {
            let start_line_index = if index == 0 {
                0
            } else {
                prefix_lines + consumed_lines
            };
            consumed_lines += line_count;
            let end_line_index = prefix_lines + consumed_lines;
            let source_override = SourceBlock {
                start_utf16: boundaries[start_line_index],
                end_utf16: if index + 1 == chunk_count {
                    source.end_utf16
                } else {
                    boundaries[end_line_index]
                },
                start_line: source.start_line + start_line_index,
                end_line: if index + 1 == chunk_count {
                    source.end_line
                } else {
                    source.start_line + end_line_index
                },
            };
            let formula_start = segment.formula_start + consumed_formulas;
            consumed_formulas += events
                .iter()
                .filter(|event| matches!(event, Event::InlineMath(_) | Event::DisplayMath(_)))
                .count();
            IndexedSegment {
                estimated_height: (line_count * 20 + 32).max(32),
                estimated_nodes: events.len(),
                events,
                source_override: Some(source_override),
                source_block_start: segment.source_block_start,
                source_block_end: segment.source_block_end,
                heading_start: segment.heading_start,
                heading_end: segment.heading_end,
                diagram_start: segment.diagram_start,
                diagram_end: segment.diagram_end,
                formula_start,
                oversized: false,
                oversized_excerpt: None,
            }
        })
        .collect()
}

fn split_large_code(events: &[Event<'static>]) -> Option<Vec<(Vec<Event<'static>>, usize)>> {
    let [marker, Event::Start(Tag::CodeBlock(kind)), middle @ .., Event::End(TagEnd::CodeBlock)] =
        events
    else {
        return None;
    };
    if matches!(kind, pulldown_cmark::CodeBlockKind::Fenced(info) if info.trim().eq_ignore_ascii_case("mermaid"))
    {
        return None;
    }
    let mut code = String::new();
    for event in middle {
        match event {
            Event::Text(value) | Event::Code(value) => code.push_str(value),
            Event::SoftBreak | Event::HardBreak => code.push('\n'),
            _ => return None,
        }
    }
    let mut pages = Vec::new();
    let mut page = String::new();
    let mut lines = 0;
    for line in code.split_inclusive('\n') {
        if line.len() > MAX_SOURCE_BYTES_PER_SEGMENT {
            return None;
        }
        if lines > 0 && (lines >= 128 || page.len() + line.len() > MAX_SOURCE_BYTES_PER_SEGMENT) {
            pages.push((std::mem::take(&mut page), lines));
            lines = 0;
        }
        page.push_str(line);
        lines += 1;
    }
    if !page.is_empty() {
        pages.push((page, lines));
    }
    Some(
        pages
            .into_iter()
            .map(|(page, lines)| {
                (
                    vec![
                        marker.clone(),
                        Event::Start(Tag::CodeBlock(kind.clone())),
                        Event::Text(page.into()),
                        Event::End(TagEnd::CodeBlock),
                    ],
                    lines,
                )
            })
            .collect(),
    )
}

fn split_large_table(events: &[Event<'static>]) -> Option<Vec<(Vec<Event<'static>>, usize)>> {
    let [marker, table_start @ Event::Start(Tag::Table(_)), rest @ ..] = events else {
        return None;
    };
    let (table_end, body) = rest.split_last()?;
    if !matches!(table_end, Event::End(TagEnd::Table)) {
        return None;
    }
    let head_end = body
        .iter()
        .position(|event| matches!(event, Event::End(TagEnd::TableHead)))?
        + 1;
    let head = &body[..head_end];
    if head.iter().any(|event| {
        matches!(
            event,
            Event::InlineMath(_) | Event::DisplayMath(_) | Event::FootnoteReference(_)
        )
    }) {
        return None;
    }
    let mut rows = Vec::new();
    let mut cursor = head_end;
    while cursor < body.len() {
        if !matches!(body[cursor], Event::Start(Tag::TableRow)) {
            return None;
        }
        let end = body[cursor..]
            .iter()
            .position(|event| matches!(event, Event::End(TagEnd::TableRow)))?
            + cursor
            + 1;
        let row = &body[cursor..end];
        if row.iter().map(event_weight).sum::<usize>() > MAX_SOURCE_BYTES_PER_SEGMENT {
            return None;
        }
        rows.push(row);
        cursor = end;
    }
    let mut pages = Vec::new();
    let mut page = Vec::new();
    let mut weight = head.iter().map(event_weight).sum::<usize>();
    let mut row_count = 0;
    for row in rows {
        let row_weight = row.iter().map(event_weight).sum::<usize>();
        if row_count > 0 && (row_count >= 128 || weight + row_weight > MAX_SOURCE_BYTES_PER_SEGMENT)
        {
            pages.push((std::mem::take(&mut page), row_count));
            weight = head.iter().map(event_weight).sum();
            row_count = 0;
        }
        page.extend_from_slice(row);
        weight += row_weight;
        row_count += 1;
    }
    if !page.is_empty() {
        pages.push((page, row_count));
    }
    Some(
        pages
            .into_iter()
            .map(|(page, rows)| {
                let mut output = vec![marker.clone(), table_start.clone()];
                output.extend_from_slice(head);
                output.extend(page);
                output.push(table_end.clone());
                (output, rows)
            })
            .collect(),
    )
}

fn source_line_boundaries(markdown: &str, block: &SourceBlock) -> Vec<usize> {
    let mut utf16 = 0;
    let mut start = None;
    let mut end = markdown.len();
    for (byte, character) in markdown.char_indices() {
        if utf16 == block.start_utf16 {
            start = Some(byte);
        }
        if utf16 == block.end_utf16 {
            end = byte;
            break;
        }
        utf16 += character.len_utf16();
    }
    let Some(start) = start else {
        return Vec::new();
    };
    let mut boundaries = vec![block.start_utf16];
    let mut offset = block.start_utf16;
    for character in markdown[start..end].chars() {
        offset += character.len_utf16();
        if character == '\n' {
            boundaries.push(offset);
        }
    }
    boundaries.push(block.end_utf16);
    boundaries
}

fn marker_index(event: &Event<'_>) -> Option<usize> {
    let Event::Text(value) = event else {
        return None;
    };
    value
        .strip_prefix('\u{e000}')?
        .strip_prefix("MARKLITE_BLOCK_")?
        .strip_suffix('\u{e001}')?
        .parse()
        .ok()
}

fn event_weight(event: &Event<'_>) -> usize {
    match event {
        Event::Text(value)
        | Event::Code(value)
        | Event::Html(value)
        | Event::InlineHtml(value)
        | Event::InlineMath(value)
        | Event::DisplayMath(value) => value.len(),
        _ => 24,
    }
}

fn event_node_weight(event: &Event<'_>) -> usize {
    match event {
        Event::Html(value) | Event::InlineHtml(value) => {
            value.bytes().filter(|byte| *byte == b'<').count().max(1)
        }
        Event::Start(_) | Event::Rule => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{prepare_virtual_preview, PreviewState};
    use crate::services::markdown_service::render_markdown;

    #[test]
    fn windows_keep_reference_links_and_global_heading_ids() {
        let mut source = String::new();
        for index in 0..130 {
            source.push_str("# Repeated\n\n");
            source.push_str(&format!("Paragraph {index} with [ref][target].\n\n"));
        }
        source.push_str("[target]: https://example.com/path\n");
        let (dto, session) = prepare_virtual_preview(&source, "test-1".into()).unwrap();
        let directory = dto.virtual_preview.unwrap();
        assert!(directory.segments.len() > 1);
        let first = session.render_window(0, 1).unwrap();
        let last = session
            .render_window(directory.segments.len() - 1, directory.segments.len())
            .unwrap();
        assert!(first.segments[0].html.contains("id=\"repeated\""));
        assert!(last.segments[0].html.contains("id=\"repeated-129\""));
        let full = render_markdown(&source).unwrap();
        let link_start = full.html.find("href=\"").unwrap();
        let link_end = full.html[link_start + 6..].find('"').unwrap() + link_start + 7;
        assert!(first.segments[0]
            .html
            .contains(&full.html[link_start..link_end]));
        assert_eq!(dto.outline.len(), 130);
    }

    #[test]
    fn ordinary_window_matches_full_render_in_document_order() {
        let source = "# One\n\nA [link](https://example.com).\n\n- item\n- other\n\n| A | B |\n| - | - |\n| 1 | 2 |\n";
        let full = render_markdown(source).unwrap();
        let (_, session) = prepare_virtual_preview(source, "test-2".into()).unwrap();
        let window = session.render_window(0, 1).unwrap();
        assert_eq!(window.segments[0].html, full.html);
        assert_eq!(
            serde_json::to_value(&window.segments[0].source_blocks).unwrap(),
            serde_json::to_value(&full.source_blocks).unwrap()
        );
    }

    #[test]
    fn single_huge_paragraph_is_bounded_and_explicit() {
        let source = "x".repeat(200_000);
        let (dto, session) = prepare_virtual_preview(&source, "test-3".into()).unwrap();
        let window = session.render_window(0, 1).unwrap();
        assert!(dto.virtual_preview.unwrap().segments[0].estimated_nodes <= 1);
        assert!(window.segments[0].html.contains("该块过大"));
        assert!(window.segments[0].html.contains("<pre><code>"));
        assert!(window.segments[0].html.len() < 20_000);
    }

    #[test]
    fn huge_middle_block_does_not_hide_neighboring_blocks() {
        let source = format!("# Before\n\n{}\n\n# After\n", "x".repeat(200_000));
        let (dto, session) = prepare_virtual_preview(&source, "test-4".into()).unwrap();
        let count = dto.virtual_preview.unwrap().segments.len();
        assert_eq!(count, 3);
        assert!(session.render_window(0, 1).unwrap().segments[0]
            .html
            .contains("Before"));
        assert!(session.render_window(1, 2).unwrap().segments[0]
            .html
            .contains("该块过大"));
        assert!(session.render_window(2, 3).unwrap().segments[0]
            .html
            .contains("After"));
    }

    #[test]
    fn split_windows_preserve_footnotes_and_raw_html_sanitization() {
        let mut source = String::new();
        for index in 0..90 {
            source.push_str(&format!("Paragraph {index} with note[^shared].\n\n"));
        }
        source.push_str("[^shared]: Footnote **body**.\n\n<div>safe <em>HTML</em></div>\n\n<script>unsafe()</script>\n");
        let full = render_markdown(&source).unwrap();
        let (dto, session) = prepare_virtual_preview(&source, "test-5".into()).unwrap();
        let count = dto.virtual_preview.unwrap().segments.len();
        assert!(count > 1);
        let mut combined = String::new();
        for index in 0..count {
            combined.push_str(&session.render_window(index, index + 1).unwrap().segments[0].html);
        }
        assert_eq!(combined, full.html);
    }

    #[test]
    fn long_code_is_windowed_by_lines_without_losing_code_semantics() {
        let mut source = String::from("```rust\n");
        for index in 0..12000 {
            source.push_str(&format!("let row_{index} = \"中😀\";\n"));
        }
        source.push_str("```\n");
        let (dto, session) = prepare_virtual_preview(&source, "code".into()).unwrap();
        let directory = dto.virtual_preview.unwrap();
        assert!(directory.segments.len() > 1);
        assert!(directory
            .segments
            .windows(2)
            .all(|pair| pair[0].end_utf16 == pair[1].start_utf16));
        assert_eq!(
            directory.segments.last().unwrap().end_utf16,
            render_markdown(&source).unwrap().source_blocks[0].end_utf16
        );
        let first = session.render_window(0, 1).unwrap();
        let last = session
            .render_window(directory.segments.len() - 1, directory.segments.len())
            .unwrap();
        assert!(first.segments[0].html.contains("<pre><code"));
        assert!(first.segments[0].html.contains("row_0"));
        assert!(last.segments[0].html.contains("row_11999"));
        assert!(first.segments[0].html.len() < 40_000);
    }

    #[test]
    fn large_table_is_windowed_by_rows_and_repeats_header() {
        let mut source = String::from("| Name | Value |\n| --- | --- |\n");
        for index in 0..6000 {
            source.push_str(&format!("| row_{index} | {index} |\n"));
        }
        let (dto, session) = prepare_virtual_preview(&source, "table".into()).unwrap();
        let directory = dto.virtual_preview.unwrap();
        assert!(directory.segments.len() > 1);
        assert!(directory
            .segments
            .windows(2)
            .all(|pair| pair[0].end_utf16 <= pair[1].start_utf16));
        let first = session.render_window(0, 1).unwrap();
        let last = session
            .render_window(directory.segments.len() - 1, directory.segments.len())
            .unwrap();
        assert!(first.segments[0].html.contains("<table>"));
        assert!(first.segments[0].html.contains("row_0"));
        assert!(last.segments[0].html.contains("Name"));
        assert!(last.segments[0].html.contains("row_5999"));
        assert!(first.segments[0].html.len() < 60_000);
    }

    #[test]
    fn dense_raw_html_falls_back_before_it_can_mount_unbounded_nodes() {
        let source = format!("<div>\n{}\n</div>\n", "<b>x</b>".repeat(5_000));
        let (_, session) = prepare_virtual_preview(&source, "html".into()).unwrap();
        let html = &session.render_window(0, 1).unwrap().segments[0].html;
        assert!(html.contains("该块过大"));
        assert!(html.len() < 20_000);
    }

    #[test]
    fn formula_budget_is_global_across_windows() {
        let source = "$x$\n\n".repeat(300);
        let full = render_markdown(&source).unwrap();
        let (dto, session) = prepare_virtual_preview(&source, "math".into()).unwrap();
        let count = dto.virtual_preview.unwrap().segments.len();
        assert!(count > 1);
        let mut combined = String::new();
        for index in 0..count {
            combined.push_str(&session.render_window(index, index + 1).unwrap().segments[0].html);
        }
        assert_eq!(combined.matches("<math").count(), 256);
        assert_eq!(combined, full.html);
    }

    #[test]
    fn replacing_a_session_rejects_old_windows_and_old_release_cannot_remove_new_one() {
        let state = PreviewState::default();
        let first_generation = state.reserve_generation();
        let (_, first) =
            prepare_virtual_preview("# First\n", first_generation.to_string()).unwrap();
        assert!(state.install(first_generation, first));
        let second_generation = state.reserve_generation();
        let (_, second) =
            prepare_virtual_preview("# Second\n", second_generation.to_string()).unwrap();
        assert!(state.install(second_generation, second));
        assert_eq!(
            state.get(&first_generation.to_string()).err().unwrap().code,
            "PREVIEW_SESSION_EXPIRED"
        );
        state.release(&first_generation.to_string());
        assert!(state.get(&second_generation.to_string()).is_ok());
        state.release(&second_generation.to_string());
        assert_eq!(
            state
                .get(&second_generation.to_string())
                .err()
                .unwrap()
                .code,
            "PREVIEW_SESSION_EXPIRED"
        );
    }

    #[test]
    fn valid_large_mermaid_keeps_its_diagram_identity() {
        let mut source = String::from("```mermaid\nflowchart TD\n");
        for index in 0..3000 {
            source.push_str(&format!("A{index}-->|{}|B{index}\n", "x".repeat(40)));
        }
        source.push_str("```\n");
        let (_, session) = prepare_virtual_preview(&source, "mermaid".into()).unwrap();
        let window = session.render_window(0, 1).unwrap();
        assert_eq!(window.segments[0].diagrams.len(), 1);
        assert!(window.segments[0].html.contains("language-mermaid"));
        assert!(!window.segments[0].html.contains("该块过大"));
    }
}
