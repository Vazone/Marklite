use crate::models::markdown_event::{self as html, Event};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use pulldown_cmark::{CowStr, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};
use sha2::{Digest, Sha256};

use crate::{
    models::{
        app_error::AppError,
        diagram::DiagramSource,
        markdown::{
            DocumentStats, DocumentToc, MarkdownAnalysisDto, OutlineItem, RenderedMarkdownDto,
            SourceBlock,
        },
    },
    services::{diagram_service, math_service},
    utils::security::sanitize_html,
};

#[path = "markdown_service/front_matter.rs"]
mod front_matter;
#[path = "markdown_service/inline.rs"]
mod inline;
#[path = "markdown_service/marks.rs"]
mod marks;
#[path = "markdown_service/preview.rs"]
mod preview;
#[cfg(test)]
pub use preview::prepare_virtual_preview;
pub use preview::{prepare_preview, PreviewState};

#[cfg(test)]
fn render_markdown_to_html(markdown: &str) -> Result<String, AppError> {
    let (analysis, events, _, _) = analyze_with_events(markdown, false);
    render_markdown_html(events, true, &analysis.outline)
}

fn render_markdown_html(
    events: Vec<Event<'static>>,
    encode_targets: bool,
    outline: &[OutlineItem],
) -> Result<String, AppError> {
    render_markdown_html_from(events, encode_targets, outline, 0)
}

fn render_markdown_html_from(
    events: Vec<Event<'static>>,
    encode_targets: bool,
    outline: &[OutlineItem],
    prior_formulas: usize,
) -> Result<String, AppError> {
    let parser = apply_heading_ids(events, outline)
        .into_iter()
        .flat_map(|event| {
            let (direct, expanded) = match event {
                Event::Toc(toc) => (None, toc.events().collect::<Vec<_>>()),
                event => (Some(event), Vec::new()),
            };
            direct.into_iter().chain(expanded)
        })
        .map(move |event| {
            if encode_targets {
                encode_navigation_target(event)
            } else {
                event
            }
        })
        .collect();
    let rendered_math = math_service::render_math_events_from(parser, prior_formulas);
    let mut raw_html = String::new();
    html::push_html(&mut raw_html, rendered_math.events.iter().cloned());
    let sanitized = sanitize_html(&raw_html);
    Ok(rendered_math.substitute(&sanitized).0)
}

fn encode_navigation_target(event: Event<'_>) -> Event<'_> {
    match event {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let dest_url = if link_type == LinkType::Email && !dest_url.starts_with("mailto:") {
                format!("mailto:{dest_url}").into()
            } else {
                dest_url
            };
            Event::Start(Tag::Link {
                link_type: if link_type == LinkType::Email {
                    LinkType::Inline
                } else {
                    link_type
                },
                dest_url: encoded_target(dest_url),
                title,
                id,
            })
        }
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: encoded_target(dest_url),
            title,
            id,
        }),
        event => event,
    }
}

fn encoded_target(target: CowStr<'_>) -> CowStr<'_> {
    format!(
        "marklite:{}",
        utf8_percent_encode(target.as_ref(), NON_ALPHANUMERIC)
    )
    .into()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SyntaxProfile {
    CommonMark,
    // Constructed by the standalone official-corpus probe, which includes this
    // module but is intentionally not part of the application binary.
    #[allow(dead_code)]
    Gfm,
    GfmWithMarkLiteExtensions,
}

pub(crate) const DEFAULT_SYNTAX_PROFILE: SyntaxProfile = SyntaxProfile::GfmWithMarkLiteExtensions;

pub(crate) fn markdown_options(profile: SyntaxProfile) -> Options {
    let mut options = Options::empty();
    if profile != SyntaxProfile::CommonMark {
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
    }
    if profile == SyntaxProfile::GfmWithMarkLiteExtensions {
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
        options.insert(Options::ENABLE_GFM);
        options.insert(Options::ENABLE_DEFINITION_LIST);
        options.insert(Options::ENABLE_MATH);
    }
    options
}

#[cfg(test)]
std::thread_local! {
    static SOURCE_PARSE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn source_events(
    markdown: &str,
    profile: SyntaxProfile,
) -> impl Iterator<Item = (Event<'_>, std::ops::Range<usize>)> {
    let offset = if profile == SyntaxProfile::GfmWithMarkLiteExtensions {
        front_matter::inspect(markdown)
            .filter(|metadata| metadata.diagnostic.is_none())
            .map_or(0, |metadata| metadata.range.end)
    } else {
        0
    };
    source_events_at(markdown, profile, offset)
}

fn source_events_at(
    markdown: &str,
    profile: SyntaxProfile,
    offset: usize,
) -> impl Iterator<Item = (Event<'_>, std::ops::Range<usize>)> {
    #[cfg(test)]
    SOURCE_PARSE_COUNT.with(|count| count.set(count.get() + 1));
    inline::emoji_events(
        markdown,
        marks::events(
            markdown,
            Parser::new_ext(&markdown[offset..], markdown_options(profile))
                .into_offset_iter()
                .map(move |(event, range)| {
                    (event.into(), range.start + offset..range.end + offset)
                }),
            profile == SyntaxProfile::GfmWithMarkLiteExtensions,
        ),
        profile == SyntaxProfile::GfmWithMarkLiteExtensions,
    )
}

fn parser_source(markdown: &str) -> Cow<'_, str> {
    let bytes = markdown.as_bytes();
    if !bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| *byte == b'\r' && bytes.get(index + 1) != Some(&b'\n'))
    {
        return Cow::Borrowed(markdown);
    }
    // Replace lone CR only: equal byte/UTF-16 width preserves all original source offsets.
    let mut normalized = bytes.to_vec();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\r' && bytes.get(index + 1) != Some(&b'\n') {
            normalized[index] = b'\n';
        }
    }
    Cow::Owned(String::from_utf8(normalized).expect("ASCII newline replacement preserves UTF-8"))
}

// The standalone conformance probe includes this module outside the application crate.
#[allow(dead_code)]
pub(crate) fn profile_events(markdown: &str, profile: SyntaxProfile) -> Vec<Event<'static>> {
    if profile == DEFAULT_SYNTAX_PROFILE {
        return normalized_document(markdown).events;
    }
    let source = parser_source(markdown);
    normalize_profile_events(
        source_events(&source, profile).map(|(event, _)| event.into_static()),
        profile,
    )
}

fn normalize_profile_events(
    events: impl Iterator<Item = Event<'static>>,
    profile: SyntaxProfile,
) -> Vec<Event<'static>> {
    if profile == SyntaxProfile::CommonMark {
        return events.collect();
    }
    let events = inject_gfm_autolinks(
        events
            .map(filter_gfm_tag_event)
            .map(preserve_literal_dollars)
            .map(normalize_email_link),
    );
    if profile == SyntaxProfile::GfmWithMarkLiteExtensions {
        promote_fenced_math(events)
    } else {
        events
    }
}

fn normalize_email_link(event: Event<'static>) -> Event<'static> {
    match event {
        Event::Start(Tag::Link {
            link_type: LinkType::Email,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type: LinkType::Inline,
            dest_url: if dest_url.starts_with("mailto:") {
                dest_url
            } else {
                format!("mailto:{dest_url}").into()
            },
            title,
            id,
        }),
        event => event,
    }
}

fn preserve_literal_dollars(event: Event<'static>) -> Event<'static> {
    let Event::InlineMath(source) = event else {
        return event;
    };
    let trimmed = source.trim();
    let currency_like = trimmed.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && trimmed.chars().any(char::is_whitespace);
    if currency_like || source.contains('`') {
        Event::Text(format!("${source}$").into())
    } else {
        Event::InlineMath(source)
    }
}

fn promote_fenced_math(events: Vec<Event<'static>>) -> Vec<Event<'static>> {
    let mut output = Vec::with_capacity(events.len());
    let mut formula: Option<String> = None;
    for event in events {
        match (&mut formula, event) {
            (None, Event::Start(Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(info))))
                if info.trim().eq_ignore_ascii_case("math") =>
            {
                formula = Some(String::new());
            }
            (Some(source), Event::Text(value) | Event::Code(value)) => {
                source.push_str(value.as_ref());
            }
            (Some(_), Event::End(TagEnd::CodeBlock)) => {
                output.push(Event::DisplayMath(
                    formula.take().expect("fenced math state is present").into(),
                ));
            }
            (Some(source), Event::SoftBreak | Event::HardBreak) => source.push('\n'),
            (Some(source), other) => source.push_str(&event_text_for_math_fence(&other)),
            (None, other) => output.push(other),
        }
    }
    if let Some(source) = formula {
        output.push(Event::DisplayMath(source.into()));
    }
    output
}

fn event_text_for_math_fence(event: &Event<'_>) -> String {
    match event {
        Event::Text(value)
        | Event::Code(value)
        | Event::InlineMath(value)
        | Event::DisplayMath(value)
        | Event::Html(value)
        | Event::InlineHtml(value) => value.to_string(),
        Event::FootnoteReference(value) => format!("[^{value}]"),
        Event::TaskListMarker(done) => format!("[{}]", if *done { 'x' } else { ' ' }),
        Event::SoftBreak | Event::HardBreak => "\n".to_string(),
        Event::Rule
        | Event::Start(_)
        | Event::End(_)
        | Event::StartHighlight
        | Event::EndHighlight
        | Event::Toc(_) => String::new(),
    }
}

fn filter_gfm_tag_event(event: Event<'_>) -> Event<'static> {
    match event {
        Event::Html(value) => Event::Html(filter_disallowed_raw_html(value.as_ref()).into()),
        Event::InlineHtml(value) => {
            Event::InlineHtml(filter_disallowed_raw_html(value.as_ref()).into())
        }
        event => event.into_static(),
    }
}

fn filter_disallowed_raw_html(value: &str) -> String {
    const DISALLOWED: [&str; 9] = [
        "title",
        "textarea",
        "style",
        "xmp",
        "iframe",
        "noembed",
        "noframes",
        "script",
        "plaintext",
    ];
    let mut output = String::with_capacity(value.len());
    let mut copied_until = 0;
    for (index, _) in value.match_indices('<') {
        let suffix = &value[index + 1..];
        let disallowed = DISALLOWED.iter().any(|tag| {
            suffix
                .get(..tag.len())
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(tag))
                && suffix[tag.len()..]
                    .chars()
                    .next()
                    .is_some_and(|next| next.is_ascii_whitespace() || matches!(next, '/' | '>'))
        });
        if disallowed {
            output.push_str(&value[copied_until..index]);
            output.push_str("&lt;");
            copied_until = index + 1;
        }
    }
    output.push_str(&value[copied_until..]);
    output
}

pub(crate) struct NormalizedMarkdown {
    pub diagnostics: Vec<crate::models::markdown::MarkdownDiagnostic>,
    pub events: Vec<Event<'static>>,
    pub event_blocks: Vec<Option<usize>>,
    pub source_blocks: Vec<SourceBlock>,
    pub diagrams: Vec<DiagramSource>,
}

pub(crate) fn normalized_document(markdown: &str) -> NormalizedMarkdown {
    let (analysis, mut document) = analyze_source(markdown, false, true);
    document.events = apply_heading_ids(document.events, &analysis.outline);
    document.diagnostics = analysis.markdown_diagnostics;
    document
}

fn apply_heading_ids(events: Vec<Event<'static>>, outline: &[OutlineItem]) -> Vec<Event<'static>> {
    let mut heading_index = 0;
    events
        .into_iter()
        .map(|event| match event {
            Event::Start(Tag::Heading {
                level,
                classes,
                attrs,
                ..
            }) => {
                let id = outline
                    .get(heading_index)
                    .map(|item| item.slug.clone().into());
                heading_index += 1;
                Event::Start(Tag::Heading {
                    level,
                    id,
                    classes,
                    attrs,
                })
            }
            event => event,
        })
        .collect()
}

fn inject_gfm_autolinks<'a>(events: impl Iterator<Item = Event<'a>>) -> Vec<Event<'static>> {
    let mut output = Vec::new();
    let mut protected_depth = 0usize;
    let mut pending_text = String::new();
    for event in events {
        match event {
            Event::Start(Tag::Link { .. } | Tag::Image { .. } | Tag::CodeBlock(_)) => {
                flush_gfm_text(&mut pending_text, &mut output);
                protected_depth += 1;
                output.push(event.into_static());
            }
            Event::End(TagEnd::Link | TagEnd::Image | TagEnd::CodeBlock) => {
                flush_gfm_text(&mut pending_text, &mut output);
                protected_depth = protected_depth.saturating_sub(1);
                output.push(event.into_static());
            }
            Event::Text(text) if protected_depth == 0 => pending_text.push_str(text.as_ref()),
            event => {
                flush_gfm_text(&mut pending_text, &mut output);
                output.push(event.into_static());
            }
        }
    }
    flush_gfm_text(&mut pending_text, &mut output);
    output
}

fn flush_gfm_text(pending: &mut String, output: &mut Vec<Event<'static>>) {
    if !pending.is_empty() {
        split_gfm_autolinks(pending, output);
        pending.clear();
    }
}

fn split_gfm_autolinks(text: &str, output: &mut Vec<Event<'static>>) {
    let mut emitted_until = 0;
    let mut previous = None;
    for (byte_index, character) in text.char_indices() {
        if byte_index < emitted_until {
            previous = Some(character);
            continue;
        }
        let may_start = byte_index == 0 || previous.is_some_and(gfm_autolinks::check_prev);
        if may_start {
            let suffix = &text[byte_index..];
            if let Some((destination, skip_chars)) = gfm_autolinks::match_start(suffix) {
                let byte_length = suffix
                    .char_indices()
                    .nth(skip_chars)
                    .map(|(index, _)| index)
                    .unwrap_or(suffix.len());
                if byte_index > emitted_until {
                    output.push(Event::Text(
                        text[emitted_until..byte_index].to_string().into(),
                    ));
                }
                let label = &suffix[..byte_length];
                output.push(Event::Start(Tag::Link {
                    link_type: LinkType::Inline,
                    dest_url: destination.into(),
                    title: CowStr::Borrowed(""),
                    id: CowStr::Borrowed(""),
                }));
                output.push(Event::Text(label.to_string().into()));
                output.push(Event::End(TagEnd::Link));
                emitted_until = byte_index + byte_length;
            }
        }
        previous = Some(character);
    }
    if emitted_until < text.len() {
        output.push(Event::Text(text[emitted_until..].to_string().into()));
    }
}

// The standalone conformance probe exercises the full preview serializer.
#[allow(dead_code)]
pub fn render_markdown(markdown: &str) -> Result<RenderedMarkdownDto, AppError> {
    render_analyzed(analyze_with_events(markdown, true))
}

fn render_analyzed(
    (analysis, events, source_blocks, diagrams): AnalyzedMarkdown,
) -> Result<RenderedMarkdownDto, AppError> {
    let diagram_diagnostics = diagram_service::validate_sources(&diagrams);
    let html = render_markdown_html(events, true, &analysis.outline)?;
    Ok(RenderedMarkdownDto {
        markdown_diagnostics: analysis.markdown_diagnostics,
        html,
        outline: analysis.outline,
        stats: analysis.stats,
        source_blocks,
        diagrams,
        diagram_diagnostics,
        virtual_preview: None,
    })
}

#[cfg(test)]
fn extract_outline(markdown: &str) -> Vec<OutlineItem> {
    analyze_markdown(markdown).outline
}

#[cfg(test)]
fn calculate_stats(markdown: &str) -> DocumentStats {
    analyze_markdown(markdown).stats
}

struct PendingHeading {
    level: u8,
    line: usize,
    title: String,
    explicit_id: Option<String>,
}

pub fn analyze_markdown(markdown: &str) -> MarkdownAnalysisDto {
    analyze_with_events(markdown, false).0
}

type AnalyzedMarkdown = (
    MarkdownAnalysisDto,
    Vec<Event<'static>>,
    Vec<SourceBlock>,
    Vec<DiagramSource>,
);

fn analyze_with_events(markdown: &str, mark_blocks: bool) -> AnalyzedMarkdown {
    let (analysis, document) = analyze_source(markdown, mark_blocks, false);
    (
        analysis,
        document.events,
        document.source_blocks,
        document.diagrams,
    )
}

fn analyze_source(
    markdown: &str,
    mark_blocks: bool,
    track_origins: bool,
) -> (MarkdownAnalysisDto, NormalizedMarkdown) {
    let mut outline = Vec::new();
    let mut current_heading: Option<PendingHeading> = None;
    let mut used_heading_ids = HashSet::new();
    let mut next_heading_suffix = HashMap::new();
    let mut link_count = 0;
    let mut image_count = 0;
    let mut line_cursor = LineCursor::new(markdown);
    let mut raw_events = Vec::new();
    let mut block_event_ends = Vec::new();
    let mut block_ranges = Vec::new();
    let mut block_start = None;
    let mut depth = 0usize;
    let mut pending_diagram: Option<PendingDiagram> = None;
    let mut diagrams = Vec::new();

    let source = parser_source(markdown);
    let metadata = front_matter::inspect(&source);
    let offset = metadata
        .as_ref()
        .filter(|metadata| metadata.diagnostic.is_none())
        .map_or(0, |metadata| metadata.range.end);
    let markdown_diagnostics = metadata
        .and_then(|metadata| metadata.diagnostic)
        .into_iter()
        .collect();
    for (event, range) in source_events_at(&source, DEFAULT_SYNTAX_PROFILE, offset) {
        match &event {
            Event::Start(Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(info)))
                if info.trim().eq_ignore_ascii_case("mermaid") =>
            {
                pending_diagram = Some(PendingDiagram {
                    source: String::new(),
                    start_byte: None,
                    end_byte: None,
                });
            }
            Event::Text(value) | Event::Code(value) if pending_diagram.is_some() => {
                let pending = pending_diagram.as_mut().expect("diagram presence checked");
                pending.source.push_str(value.as_ref());
                pending.start_byte.get_or_insert(range.start);
                pending.end_byte = Some(range.end);
            }
            Event::SoftBreak | Event::HardBreak if pending_diagram.is_some() => {
                let pending = pending_diagram.as_mut().expect("diagram presence checked");
                pending.source.push('\n');
                pending.start_byte.get_or_insert(range.start);
                pending.end_byte = Some(range.end);
            }
            Event::End(TagEnd::CodeBlock) if pending_diagram.is_some() => {
                let pending = pending_diagram.take().expect("diagram presence checked");
                let ordinal = diagrams.len();
                let source_sha256 = format!("{:x}", Sha256::digest(pending.source.as_bytes()));
                diagrams.push(DiagramSource {
                    diagram_id: format!("diagram-{ordinal}-{}", &source_sha256[..12]),
                    ordinal,
                    source_utf8: pending.source,
                    source_sha256,
                    source_start_byte: pending.start_byte.unwrap_or(range.start),
                    source_end_byte: pending.end_byte.unwrap_or(range.start),
                });
            }
            _ => {}
        }
        if depth == 0 {
            match &event {
                Event::Start(tag) if is_source_block_tag(tag) => {
                    block_start = Some(range.start);
                    if mark_blocks {
                        raw_events
                            .push(Event::Text(source_block_marker(block_ranges.len()).into()));
                    }
                }
                Event::Rule | Event::Html(_) | Event::DisplayMath(_) => {
                    if mark_blocks {
                        raw_events
                            .push(Event::Text(source_block_marker(block_ranges.len()).into()));
                    }
                    block_ranges.push((range.start, range.end));
                }
                _ => {}
            }
        }
        match &event {
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    if let Some(start) = block_start.take() {
                        block_ranges.push((start, range.end));
                    }
                }
            }
            _ => {}
        }
        raw_events.push(event.clone().into_static());
        if block_event_ends.len() < block_ranges.len() {
            block_event_ends.push(raw_events.len());
        }
        match event {
            Event::Start(Tag::Heading { level, id, .. }) => {
                current_heading = Some(PendingHeading {
                    level: heading_level(level),
                    line: line_cursor.line_at(range.start),
                    title: String::new(),
                    explicit_id: id.map(|value| value.into_string()),
                });
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(heading) = current_heading.take() {
                    let title = heading.title.trim().to_string();
                    let base_id = heading
                        .explicit_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| slugify(&title));
                    outline.push(OutlineItem {
                        level: heading.level,
                        slug: unique_heading_id(
                            base_id,
                            &mut used_heading_ids,
                            &mut next_heading_suffix,
                        ),
                        title,
                        line: heading.line,
                    });
                }
            }
            Event::Text(text)
            | Event::Code(text)
            | Event::InlineMath(text)
            | Event::DisplayMath(text)
                if current_heading.is_some() =>
            {
                current_heading
                    .as_mut()
                    .expect("heading presence checked")
                    .title
                    .push_str(text.as_ref());
            }
            Event::SoftBreak | Event::HardBreak if current_heading.is_some() => {
                current_heading
                    .as_mut()
                    .expect("heading presence checked")
                    .title
                    .push(' ');
            }
            _ => {}
        }
    }

    // Normalize complete source blocks so synthetic autolink/math events inherit
    // an explicit origin, rather than guessing offsets from their rendered text.
    let mut raw = raw_events.into_iter();
    let mut events = Vec::new();
    let mut event_blocks = Vec::new();
    let mut previous_end = 0;
    let mut shared_toc = None;
    for (block, end) in block_event_ends.into_iter().enumerate() {
        let block_events: Vec<_> = raw.by_ref().take(end - previous_end).collect();
        let (start_byte, end_byte) = block_ranges[block];
        let body = if mark_blocks {
            &block_events[1..]
        } else {
            &block_events[..]
        };
        let is_toc = markdown[start_byte..end_byte].trim() == "[TOC]"
            && matches!(body, [Event::Start(Tag::Paragraph), middle @ .., Event::End(TagEnd::Paragraph)]
                if middle.iter().all(|event| matches!(event, Event::Text(_))));
        let normalized = if is_toc {
            let toc = shared_toc
                .get_or_insert_with(|| DocumentToc::new(outline.clone()))
                .clone();
            let mut replacement = Vec::with_capacity(2);
            if mark_blocks {
                replacement.push(block_events[0].clone());
            }
            replacement.push(Event::Toc(toc));
            replacement
        } else {
            normalize_profile_events(block_events.into_iter(), DEFAULT_SYNTAX_PROFILE)
        };
        if track_origins {
            event_blocks.extend(std::iter::repeat_n(Some(block), normalized.len()));
        }
        events.extend(normalized);
        previous_end = end;
    }
    let remaining = normalize_profile_events(raw, DEFAULT_SYNTAX_PROFILE);
    if track_origins {
        event_blocks.extend(std::iter::repeat_n(None, remaining.len()));
    }
    events.extend(remaining);
    for event in &events {
        match event {
            Event::Start(Tag::Link { .. }) => link_count += 1,
            Event::Start(Tag::Image { .. }) => image_count += 1,
            _ => {}
        }
    }

    let analysis = MarkdownAnalysisDto {
        markdown_diagnostics,
        stats: DocumentStats {
            word_count: markdown.split_whitespace().count(),
            character_count: markdown.chars().count(),
            line_count: LineCursor::new(markdown).line_at(markdown.len()),
            heading_count: outline.len(),
            link_count,
            image_count,
        },
        outline,
    };
    let mut line_cursor = LineCursor::new(markdown);
    let mut utf16_cursor = Utf16Cursor::new(markdown);
    let source_blocks = block_ranges
        .into_iter()
        .map(|(start, end)| SourceBlock {
            start_utf16: utf16_cursor.at(start),
            end_utf16: utf16_cursor.at(end),
            start_line: line_cursor.line_at(start),
            end_line: line_cursor.line_at(end),
        })
        .collect();
    (
        analysis,
        NormalizedMarkdown {
            diagnostics: Vec::new(),
            events,
            event_blocks,
            source_blocks,
            diagrams,
        },
    )
}

struct PendingDiagram {
    source: String,
    start_byte: Option<usize>,
    end_byte: Option<usize>,
}

fn source_block_marker(index: usize) -> String {
    format!("\u{e000}MARKLITE_BLOCK_{index}\u{e001}")
}

fn is_source_block_tag(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::List(_)
            | Tag::Table(_)
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::HtmlBlock
    )
}

struct Utf16Cursor<'a> {
    source: &'a str,
    byte_offset: usize,
    utf16_offset: usize,
}

impl<'a> Utf16Cursor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            byte_offset: 0,
            utf16_offset: 0,
        }
    }

    fn at(&mut self, byte_offset: usize) -> usize {
        debug_assert!(byte_offset >= self.byte_offset && self.source.is_char_boundary(byte_offset));
        self.utf16_offset += self.source[self.byte_offset..byte_offset]
            .chars()
            .map(char::len_utf16)
            .sum::<usize>();
        self.byte_offset = byte_offset;
        self.utf16_offset
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

struct LineCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
    line: usize,
}

impl<'a> LineCursor<'a> {
    fn new(markdown: &'a str) -> Self {
        Self {
            bytes: markdown.as_bytes(),
            offset: 0,
            line: 1,
        }
    }

    fn line_at(&mut self, byte_offset: usize) -> usize {
        debug_assert!(
            byte_offset >= self.offset,
            "parser offsets must be monotonic"
        );
        for index in self.offset..byte_offset {
            if self.bytes[index] == b'\n'
                || (self.bytes[index] == b'\r' && self.bytes.get(index + 1) != Some(&b'\n'))
            {
                self.line += 1;
            }
        }
        self.offset = byte_offset;
        self.line
    }
}

fn slugify(title: &str) -> String {
    let mut slug = String::new();
    let mut pending_separator = false;
    for character in title.trim().chars() {
        if character.is_alphanumeric() {
            if pending_separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.extend(character.to_lowercase());
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }
    if slug.is_empty() {
        "section".to_string()
    } else {
        slug
    }
}

fn unique_heading_id(
    base_id: String,
    used: &mut HashSet<String>,
    next_suffix: &mut HashMap<String, usize>,
) -> String {
    if used.insert(base_id.clone()) {
        next_suffix.entry(base_id.clone()).or_insert(2);
        return base_id;
    }

    let suffix = next_suffix.entry(base_id.clone()).or_insert(2);
    loop {
        let candidate = format!("{base_id}-{}", *suffix);
        *suffix += 1;
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        analyze_markdown, apply_heading_ids, calculate_stats, encode_navigation_target,
        extract_outline, profile_events, render_markdown, render_markdown_to_html, LineCursor,
        SyntaxProfile, DEFAULT_SYNTAX_PROFILE,
    };
    use crate::models::markdown_event::{self as html, Event};
    use crate::services::math_service;

    #[test]
    fn analysis_render_and_export_each_parse_source_once() {
        let source = "# Heading\n\nA bare https://example.org and [link](note.md).\n";
        super::SOURCE_PARSE_COUNT.with(|count| {
            count.set(0);
            let analysis = analyze_markdown(source);
            assert_eq!(analysis.outline[0].slug, "heading");
            assert_eq!(analysis.stats.link_count, 2);
            assert_eq!(count.get(), 1);

            count.set(0);
            let rendered = render_markdown(source).expect("render should succeed");
            assert_eq!(rendered.outline[0].slug, analysis.outline[0].slug);
            assert_eq!(rendered.stats.link_count, analysis.stats.link_count);
            assert_eq!(count.get(), 1);

            count.set(0);
            let semantic = crate::services::export_semantic::SemanticDocument::parse(source, None);
            assert!(!semantic.roots().is_empty());
            assert!(semantic.diagram_sources().is_empty());
            assert_eq!(count.get(), 1);
        });
    }

    #[test]
    fn extracts_mermaid_sources_with_stable_hashes_and_utf8_byte_ranges() {
        let source = "前言\n\n```mermaid\nflowchart TD\nA-->B\n```\n\n```rust\nfn main() {}\n```\n\n```MERMAID\nsequenceDiagram\nA->>B: 你好\n```";
        let rendered = render_markdown(source).expect("render should succeed");
        assert_eq!(rendered.diagrams.len(), 2);

        let first = &rendered.diagrams[0];
        assert_eq!(first.ordinal, 0);
        assert_eq!(first.source_utf8, "flowchart TD\nA-->B\n");
        assert_eq!(
            &source.as_bytes()[first.source_start_byte..first.source_end_byte],
            first.source_utf8.as_bytes()
        );
        assert_eq!(
            first.source_sha256,
            "bd82be55b98e9030585603754372f5e90ae47e48336f0f05284d4290f0cd52f8"
        );
        assert_eq!(first.diagram_id, "diagram-0-bd82be55b98e");

        let second = &rendered.diagrams[1];
        assert_eq!(second.ordinal, 1);
        assert_eq!(second.source_utf8, "sequenceDiagram\nA->>B: 你好\n");
        assert_eq!(
            &source.as_bytes()[second.source_start_byte..second.source_end_byte],
            second.source_utf8.as_bytes()
        );
        assert!(rendered.html.contains("language-mermaid"));
        assert!(rendered.html.contains("language-rust"));
    }

    #[test]
    #[ignore = "native release timing probe; run explicitly"]
    fn measure_live_split_markdown_stages() {
        use std::{fs, path::Path, time::Instant};

        use crate::models::markdown_event as html;
        use serde_json::json;

        let samples = std::env::var("MARKLITE_BENCHMARK_SAMPLES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|count| *count > 0)
            .unwrap_or(30);
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("tmp")
            .join("performance-corpus");
        for name in [
            "representative-10-kib.md",
            "representative-100-kib.md",
            "representative-500-kib.md",
            "representative-1-mib.md",
            "representative-5-mib.md",
        ] {
            let source =
                fs::read_to_string(corpus.join(name)).expect("generated corpus must exist");
            let mut stages = Vec::with_capacity(samples);
            for _ in 0..samples {
                let started = Instant::now();
                let analysis = analyze_markdown(&source);
                let analyzed = started.elapsed().as_secs_f64() * 1000.0;

                let started = Instant::now();
                let events = profile_events(&source, DEFAULT_SYNTAX_PROFILE);
                let parsed = started.elapsed().as_secs_f64() * 1000.0;

                let started = Instant::now();
                let encoded = apply_heading_ids(events, &analysis.outline)
                    .into_iter()
                    .map(encode_navigation_target)
                    .collect();
                let prepared = started.elapsed().as_secs_f64() * 1000.0;

                let started = Instant::now();
                let rendered_math = math_service::render_math_events(encoded);
                let math = started.elapsed().as_secs_f64() * 1000.0;

                let started = Instant::now();
                let mut raw_html = String::new();
                html::push_html(&mut raw_html, rendered_math.events.iter().cloned());
                let html = started.elapsed().as_secs_f64() * 1000.0;

                let started = Instant::now();
                let sanitized = crate::utils::security::sanitize_html(&raw_html);
                let sanitize = started.elapsed().as_secs_f64() * 1000.0;

                let started = Instant::now();
                let (output, _) = rendered_math.substitute(&sanitized);
                let substitute = started.elapsed().as_secs_f64() * 1000.0;
                assert!(!output.is_empty());
                stages.push(json!({
                    "analysisMs": analyzed, "parseMs": parsed, "prepareMs": prepared,
                    "mathMs": math, "htmlMs": html, "sanitizeMs": sanitize,
                    "substituteMs": substitute, "outputBytes": output.len()
                }));
            }
            println!(
                "MARKLITE_STAGE_RESULT {}",
                json!({
                    "name": name, "sourceBytes": source.len(), "samples": samples, "stages": stages
                })
            );
        }
    }

    #[test]
    fn renders_tables_and_task_lists() {
        let html =
            render_markdown_to_html("- [x] Done\n\n| A | B |\n| - | - |\n| 1 | 2 |").unwrap();
        assert!(html.contains("<table>"));
        assert!(html.contains("checkbox"));
        let blank_template = render_markdown_to_html("|  |  |\n| --- | --- |\n|  |  |").unwrap();
        assert!(blank_template.contains("<table>"));
    }

    #[test]
    fn renders_registered_lightweight_extensions() {
        let html = render_markdown_to_html("> [!NOTE]\n> A note\n\nTerm\n: Definition").unwrap();

        assert!(html.contains("markdown-alert-note"));
        assert!(html.contains("<dl>"));
        assert!(html.contains("<dt>Term</dt>"));
        assert!(html.contains("<dd>Definition</dd>"));
    }

    #[test]
    fn renders_inline_display_and_fenced_math_but_preserves_literal_dollars_and_code() {
        let html = render_markdown_to_html(concat!(
            "Inline $x^2 + y^2$ and price \\$5.\n\n",
            "$$\\frac{1}{2}$$\n\n",
            "```math\n\\sqrt{x}\n```\n\n",
            "`$not_math$` and incomplete $x"
        ))
        .unwrap();

        assert_eq!(html.matches("<math").count(), 3, "{html}");
        assert!(html.contains("<mfrac>"), "{html}");
        assert!(html.contains("<msqrt>"), "{html}");
        assert!(html.contains("$5"), "{html}");
        assert!(html.contains("<code>$not_math$</code>"), "{html}");
        assert!(html.contains("incomplete $x"), "{html}");
    }

    #[test]
    fn ordinary_profiles_and_documents_do_not_dispatch_formula_rendering() {
        let ordinary = math_service::render_math_events(profile_events(
            "Price is $5 and code is `$x$`.",
            SyntaxProfile::GfmWithMarkLiteExtensions,
        ));
        assert_eq!(ordinary.formula_count, 0);
        assert!(profile_events("$x$", SyntaxProfile::CommonMark)
            .iter()
            .all(|event| !matches!(event, Event::InlineMath(_) | Event::DisplayMath(_))));
        assert!(profile_events("$x$", SyntaxProfile::Gfm)
            .iter()
            .all(|event| !matches!(event, Event::InlineMath(_) | Event::DisplayMath(_))));
    }

    #[test]
    fn removes_script_tags() {
        let html = render_markdown_to_html("<script>alert(1)</script>\n\n# Safe").unwrap();
        assert!(!html.contains("<script>"));
        assert!(html.contains("<h1"));
    }

    #[test]
    fn wraps_markdown_link_and_image_targets_for_typed_resolution() {
        let html = render_markdown_to_html(
            "[Local](active/card.md#part) ![Image](images/a%20b.png) [Bad](javascript:alert(1))",
        )
        .unwrap();

        assert!(html.contains("href=\"marklite:active%2Fcard%2Emd%23part\""));
        assert!(html.contains("src=\"marklite:images%2Fa%2520b%2Epng\""));
        assert!(html.contains("href=\"marklite:javascript%3Aalert%281%29\""));
    }

    #[test]
    fn extracts_outline() {
        let outline = extract_outline("# A\nText\n### B");
        assert_eq!(outline.len(), 2);
        assert_eq!(outline[1].level, 3);
    }

    #[test]
    fn outline_uses_parser_events_for_setext_formatting_and_fenced_code() {
        let outline = extract_outline(
            "Setext **title**\n===\n\n```md\n# Not a heading\n```\n\n## Inline `code`",
        );

        assert_eq!(outline.len(), 2);
        assert_eq!(outline[0].title, "Setext title");
        assert_eq!(outline[0].line, 1);
        assert_eq!(outline[1].title, "Inline code");
        assert_eq!(outline[1].line, 8);
    }

    #[test]
    fn rendered_heading_ids_are_unique_and_identical_to_outline_slugs() {
        let rendered =
            render_markdown("# 中文 标题\n\n# 中文 标题\n\n## Explicit {#中文-标题-2}\n\n# !!!")
                .unwrap();

        let slugs = rendered
            .outline
            .iter()
            .map(|item| item.slug.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            slugs,
            ["中文-标题", "中文-标题-2", "中文-标题-2-2", "section"]
        );
        for slug in slugs {
            assert!(rendered.html.contains(&format!("id=\"{slug}\"")));
        }
    }

    #[test]
    fn empty_headings_keep_outline_and_rendered_heading_identity_aligned() {
        let rendered = render_markdown("#\n\n# Named\n\n[go](#named)").unwrap();

        let headings = rendered
            .outline
            .iter()
            .map(|item| (item.title.as_str(), item.slug.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(headings, [("", "section"), ("Named", "named")]);
        assert!(rendered.html.contains("<h1 id=\"section\"></h1>"));
        assert!(rendered.html.contains("<h1 id=\"named\">Named</h1>"));
    }

    #[test]
    fn default_profile_preserves_standard_punctuation() {
        let html = render_markdown_to_html("\"plain\" -- ...").unwrap();
        assert!(html.contains("\"plain\" -- ..."));
    }

    #[test]
    fn gfm_autolinks_bare_targets_without_touching_code_or_existing_links() {
        let html = render_markdown_to_html(
            "https://example.org/a(b). www.example.org/test. writer@example.org `https://code.example` [label](https://linked.example) ![https://image-alt.example](image.png)",
        )
        .unwrap();

        assert!(
            html.contains("href=\"marklite:https%3A%2F%2Fexample%2Eorg%2Fa%28b%29\""),
            "{html}"
        );
        assert!(html.contains("href=\"marklite:http%3A%2F%2Fwww%2Eexample%2Eorg%2Ftest\""));
        assert!(html.contains("href=\"marklite:mailto%3Awriter%40example%2Eorg\""));
        assert!(html.contains("<code>https://code.example</code>"));
        assert!(html.contains("alt=\"https://image-alt.example\""));
        assert!(!html.contains("marklite:https%3A%2F%2Fimage%2Dalt%2Eexample"));
        assert_eq!(
            html.matches("marklite:https%3A%2F%2Flinked%2Eexample")
                .count(),
            1
        );
    }

    #[test]
    fn gfm_autolinks_leave_block_code_literal_and_keep_body_links() {
        for (code, expected_links) in [
            ("```text\nhttps://code.example writer@code.example\n```", 1),
            ("    https://code.example writer@code.example", 1),
            (
                "- item\n\n      https://code.example writer@code.example",
                1,
            ),
            ("```text\nhttps://code.example writer@code.example", 0),
        ] {
            let source = format!("{code}\n\nhttps://body.example `writer@inline.example`");
            let rendered = render_markdown(&source).unwrap();
            assert_eq!(rendered.stats.link_count, expected_links, "{source}");
            assert_eq!(
                rendered.html.matches("<a ").count(),
                expected_links,
                "{source}"
            );
            assert!(rendered
                .html
                .contains("https://code.example writer@code.example"));
            if expected_links > 0 {
                assert!(rendered.html.contains("<code>writer@inline.example</code>"));
            }
        }
    }

    #[test]
    fn gfm_spec_www_example_links_in_prose_only() {
        // GFM 0.29-gfm example 622 uses this www address in ordinary prose.
        let source = "www.commonmark.org\n\n```\nwww.commonmark.org\n```";
        let mut html = String::new();
        html::push_html(
            &mut html,
            profile_events(source, SyntaxProfile::Gfm).into_iter(),
        );
        assert!(html.contains("<a href=\"http://www.commonmark.org\">www.commonmark.org</a>"));
        assert!(html.contains("<code>www.commonmark.org\n</code>"));
        assert_eq!(html.matches("<a ").count(), 1);
    }

    #[test]
    fn all_email_syntaxes_share_typed_preview_targets() {
        let rendered =
            render_markdown("<one@example.org> two@example.org [three](mailto:three@example.org)")
                .unwrap();
        assert_eq!(rendered.stats.link_count, 3);
        for address in ["one", "two", "three"] {
            assert!(rendered.html.contains(&format!(
                "href=\"marklite:mailto%3A{address}%40example%2Eorg\""
            )));
        }
    }

    #[test]
    fn preserves_fixed_table_alignment_through_sanitization() {
        let html = render_markdown_to_html(
            "| Left | Center | Right |\n| :--- | :---: | ---: |\n| A | B | C |",
        )
        .unwrap();

        assert!(html.contains("<th style=\"text-align: left\">"));
        assert!(html.contains("<th style=\"text-align: center\">"));
        assert!(html.contains("<th style=\"text-align: right\">"));
    }

    #[test]
    fn calculates_stats() {
        let stats = calculate_stats("# Title\n![img](x.png)\n[site](https://example.com)");
        assert_eq!(stats.heading_count, 1);
        assert_eq!(stats.image_count, 1);
        assert_eq!(stats.link_count, 1);
    }

    #[test]
    fn counts_unicode_scalar_values_and_combining_marks_without_utf16_drift() {
        let stats = calculate_stats("😀e\u{301}\n");
        assert_eq!(stats.character_count, 4);
        assert_eq!(stats.line_count, 2);
    }

    #[test]
    fn counts_a_trailing_newline_as_an_empty_final_line() {
        assert_eq!(calculate_stats("first\n").line_count, 2);
        assert_eq!(calculate_stats("").line_count, 1);
    }

    #[test]
    fn counts_only_parsed_link_and_image_events() {
        let markdown = r#"
[inline **label**](https://a.example)
[reference][site]
<https://b.example>
[![nested](image.png)](https://c.example)
![reference image][logo]
`[code](https://ignored.example)`
\[escaped](https://ignored.example)

[site]: https://reference.example
[logo]: logo.png
"#;

        let stats = calculate_stats(markdown);

        assert_eq!(stats.link_count, 5);
        assert_eq!(stats.image_count, 2);
    }

    #[test]
    fn source_blocks_keep_utf16_and_line_ranges_across_unicode_crlf_and_nested_blocks() {
        let source = "# 中文😀\r\n\r\nParagraph\r\n\r\n- a\r\n  - b\r\n\r\n| A | B |\r\n| --- | --- |\r\n| 1 | 2 |\r\n\r\n```math\r\nx+1\r\n```\r\n";
        let rendered = render_markdown(source).expect("render should succeed");
        assert_eq!(rendered.html.matches("\u{e000}MARKLITE_BLOCK_").count(), 5);
        let blocks = &rendered.source_blocks;
        assert_eq!(blocks.len(), 5);
        for (block, (needle, line)) in blocks.iter().zip([
            ("# 中文😀", 1),
            ("Paragraph", 3),
            ("- a", 5),
            ("| A | B |", 8),
            ("```math", 12),
        ]) {
            let byte_start = source.find(needle).expect("fixture block exists");
            assert_eq!(
                block.start_utf16,
                source[..byte_start].encode_utf16().count()
            );
            assert_eq!(block.start_line, line);
            assert!(block.end_utf16 > block.start_utf16);
            assert!(block.end_line >= block.start_line);
        }
        assert!(blocks
            .windows(2)
            .all(|pair| pair[0].end_utf16 <= pair[1].start_utf16));
    }

    #[test]
    fn shared_boundary_fixtures_match_full_and_window_source_blocks() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../../src/shared/markdown-boundary-fixtures.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let source = case["source"].as_str().unwrap();
            let full = render_markdown(source).unwrap();
            for needle in case["htmlContains"].as_array().unwrap() {
                assert!(
                    full.html.contains(needle.as_str().unwrap()),
                    "{} missing {needle}: {}",
                    case["id"],
                    full.html
                );
            }
            for needle in case["htmlExcludes"].as_array().unwrap() {
                assert!(
                    !full.html.contains(needle.as_str().unwrap()),
                    "{} unexpectedly renders {needle}",
                    case["id"]
                );
            }
            let blocks = case["blocks"].as_array().unwrap();
            assert_eq!(full.source_blocks.len(), blocks.len(), "{}", case["id"]);
            for (actual, expected) in full.source_blocks.iter().zip(blocks) {
                let byte = source.find(expected["text"].as_str().unwrap()).unwrap();
                assert_eq!(
                    actual.start_utf16,
                    source[..byte].encode_utf16().count(),
                    "{}",
                    case["id"]
                );
                assert_eq!(
                    actual.start_line,
                    expected["line"].as_u64().unwrap() as usize
                );
            }
            let (_, session) = super::prepare_virtual_preview(source, "boundary".into()).unwrap();
            let window = session.render_window(0, 1).unwrap();
            assert_eq!(window.segments[0].html, full.html, "{}", case["id"]);
            assert_eq!(
                serde_json::to_value(&window.segments[0].source_blocks).unwrap(),
                serde_json::to_value(&full.source_blocks).unwrap()
            );
        }
    }

    #[test]
    fn source_lines_follow_commonmark_lf_crlf_and_cr_without_changing_offsets() {
        for ending in ["\n", "\r\n", "\r"] {
            let source = ["# 中文😀", "", "Body", "", "## Second", ""].join(ending);
            let full = render_markdown(&source).unwrap();
            assert_eq!(
                full.outline
                    .iter()
                    .map(|item| item.line)
                    .collect::<Vec<_>>(),
                [1, 5],
                "{ending:?}"
            );
            assert_eq!(full.stats.line_count, 6, "{ending:?}");
            assert_eq!(
                full.source_blocks
                    .iter()
                    .map(|block| block.start_line)
                    .collect::<Vec<_>>(),
                [1, 3, 5],
                "{ending:?}"
            );
            let (_, session) =
                super::prepare_virtual_preview(&source, "line-endings".into()).unwrap();
            let window = session.render_window(0, 1).unwrap();
            assert_eq!(
                serde_json::to_value(&full.source_blocks).unwrap(),
                serde_json::to_value(&window.segments[0].source_blocks).unwrap()
            );
            let byte = source.find("## Second").unwrap();
            assert_eq!(
                full.source_blocks[2].start_utf16,
                source[..byte].encode_utf16().count()
            );
        }
    }

    #[test]
    fn line_cursor_advances_once_across_dense_heading_offsets() {
        let markdown = (0..10_000)
            .map(|index| format!("# Heading {index}\n"))
            .collect::<String>();
        let mut cursor = LineCursor::new(&markdown);
        let mut search_from = 0;
        for expected_line in 1..=10_000 {
            let offset = markdown[search_from..]
                .find('#')
                .map(|relative| search_from + relative)
                .unwrap();
            assert_eq!(cursor.line_at(offset), expected_line);
            search_from = offset + 1;
        }
        assert_eq!(cursor.offset, markdown.rfind('#').unwrap());
    }
}

#[cfg(test)]
mod document_toc_tests {
    use super::*;

    #[test]
    fn toc_uses_full_outline_and_original_source_positions() {
        let source = "[TOC]\n\n# 中文\n\n[TOC]\n\n## 中文\n\n[TOC]";
        let rendered = render_markdown(source).unwrap();
        assert_eq!(rendered.outline.len(), 2);
        assert!(rendered.html.contains("class=\"toc-level-1\""));
        assert!(rendered.html.contains("class=\"toc-level-2\""));
        assert_eq!(
            rendered.html.matches("<ul class=\"document-toc\">").count(),
            3
        );
        assert!(!rendered.html.contains("[TOC]"));
        let document = normalized_document(source);
        let tocs: Vec<_> = document
            .events
            .iter()
            .enumerate()
            .filter_map(|(index, event)| {
                if let Event::Toc(toc) = event {
                    Some((index, toc))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(tocs.len(), 3);
        for (index, toc) in tocs {
            assert_eq!(toc.headings(), rendered.outline.as_slice());
            let block = &document.source_blocks[document.event_blocks[index].unwrap()];
            assert!(matches!(block.start_line, 1 | 5 | 9));
        }
        assert_ne!(rendered.outline[0].slug, rendered.outline[1].slug);
    }

    #[test]
    fn toc_does_not_capture_code_escapes_nested_or_reference_links() {
        for source in [
            "`[TOC]`",
            "```md\n[TOC]\n```",
            "\\[TOC]",
            "> [TOC]",
            "- [TOC]",
            "Text [TOC]",
            "[TOC]\n\n[TOC]: https://example.com",
        ] {
            let document = normalized_document(source);
            assert!(
                !document
                    .events
                    .iter()
                    .any(|event| matches!(event, Event::Toc(_))),
                "{source}"
            );
        }
        for profile in [SyntaxProfile::CommonMark, SyntaxProfile::Gfm] {
            assert!(!profile_events("[TOC]\n\n# Heading", profile)
                .iter()
                .any(|event| matches!(event, Event::Toc(_))));
        }
    }
}
