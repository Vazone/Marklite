use std::{
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicU64, Ordering},
    sync::Arc,
};

use html_escape::encode_text;
use pulldown_cmark::{html, CowStr, Event, Tag};

use crate::{
    models::diagram::RenderedDiagram,
    models::export::{
        ExportMarginPreset, ExportOrientation, ExportPaperSize, ExportRequest, ExportWarning,
    },
    services::{
        export_resources::{ExportLink, ExportResourceResolver},
        export_semantic::{NodeId, SemanticDocument, SemanticNode},
        math_service, pdf_ready_protocol,
    },
    utils::security::{html_tag_end, sanitize_html, strip_export_raw_images},
};

static RESOURCE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn render_body_with_diagrams(
    document: &SemanticDocument,
    request: &ExportRequest,
    diagrams: &HashMap<String, RenderedDiagram>,
) -> (String, Vec<ExportWarning>) {
    let mut resources = ExportResourceResolver::new(
        request.snapshot.source_path.as_deref(),
        request.options.include_local_images,
    );
    let body = render_roots_with_resources(
        document,
        document.roots(),
        request,
        diagrams,
        &mut resources,
        false,
    );
    (body, resources.into_warnings())
}

/// Render a semantic range while charging resources to the whole export job.
/// Callers keep one resolver across all ranges, so batching cannot reset limits.
pub(crate) fn render_roots_with_resources(
    document: &SemanticDocument,
    roots: &[NodeId],
    request: &ExportRequest,
    diagrams: &HashMap<String, RenderedDiagram>,
    resources: &mut ExportResourceResolver<'_>,
    pdf_part: bool,
) -> String {
    let token = format!(
        "{}-{}-{}",
        std::process::id(),
        RESOURCE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        request.snapshot.content_revision
    );
    let placeholder_prefix = format!("https://marklite.invalid/_export-resource/{token}/");
    let mut events = Vec::new();
    let mut replacements = Vec::new();
    emit_html_nodes(
        document,
        roots,
        resources,
        &placeholder_prefix,
        &mut replacements,
        &mut events,
        diagrams,
    );
    let rendered_math = math_service::render_math_events(events);
    let mut raw = String::new();
    html::push_html(&mut raw, rendered_math.events.iter().cloned());
    let cleaned = sanitize_html(&raw);
    let (cleaned, math_warnings) = rendered_math.substitute(&cleaned);
    for warning in math_warnings {
        resources.warnings_mut().push(warning);
    }
    let body = substitute_resource_placeholders(&cleaned, &placeholder_prefix, &replacements);
    if pdf_part {
        pdf_anchor_links(document, roots, &body)
    } else {
        body
    }
}

pub(crate) const PDF_ANCHOR_URL: &str = "https://marklite.invalid/_pdf-anchor/#";

fn rewrite_pdf_anchor_links(body: &str) -> String {
    let mut output = String::with_capacity(body.len());
    let mut cursor = 0;
    while let Some(relative) = body[cursor..].find("<a") {
        let start = cursor + relative;
        output.push_str(&body[cursor..start]);
        if !body
            .as_bytes()
            .get(start + 2)
            .is_some_and(u8::is_ascii_whitespace)
        {
            output.push_str("<a");
            cursor = start + 2;
            continue;
        }
        let mut quote = None;
        let mut end = body.len();
        for (offset, byte) in body.as_bytes()[start + 2..].iter().copied().enumerate() {
            if quote == Some(byte) {
                quote = None;
            } else if quote.is_none() {
                if byte == b'\"' || byte == b'\'' {
                    quote = Some(byte);
                } else if byte == b'>' {
                    end = start + 2 + offset + 1;
                    break;
                }
            }
        }
        output.push_str(&body[start..end].replace("href=\"#", &format!("href=\"{PDF_ANCHOR_URL}")));
        cursor = end;
    }
    output.push_str(&body[cursor..]);
    output
}

fn pdf_anchor_links(document: &SemanticDocument, roots: &[NodeId], body: &str) -> String {
    // Cross-part links must survive even before the target part exists. Native
    // print converts the synthetic self-links into named PDF destinations.
    let mut body = rewrite_pdf_anchor_links(body);
    let mut pending = roots.to_vec();
    let mut known = HashSet::new();
    let mut raw_ids = HashSet::new();
    while let Some(id) = pending.pop() {
        if let SemanticNode::Event(Event::Html(html) | Event::InlineHtml(html)) = document.node(id)
        {
            raw_ids.extend(html_ids(html));
        }
        if let SemanticNode::Element { tag, children } = document.node(id) {
            pending.extend(children.iter().copied());
            let anchor = match tag {
                Tag::Heading { id: Some(id), .. } => Some(id),
                Tag::FootnoteDefinition(id) => Some(id),
                _ => None,
            };
            if let Some(anchor) = anchor {
                known.insert(anchor.to_string());
                append_pdf_target(&mut body, anchor);
            }
        }
    }
    // Sanitized raw HTML can define targets independently of Markdown headings.
    // Inspect actual attributes, not id-like text in code or quoted attributes.
    if !raw_ids.is_empty() {
        for anchor in html_ids(&body) {
            // Generated SVG ids belong to the diagram renderer, not to the
            // source document's HTML anchors. Do not create print links to them.
            if raw_ids.contains(&anchor) && known.insert(anchor.clone()) {
                append_pdf_target(&mut body, &anchor);
            }
        }
    }
    body
}

fn append_pdf_target(body: &mut String, anchor: &str) {
    let escaped = html_escape::encode_double_quoted_attribute(anchor);
    body.push_str(&format!("<a href=\"#{escaped}\" style=\"position:absolute;width:1px;height:1px;font-size:1px;color:transparent\" aria-hidden=\"true\">&#160;</a>"));
}

fn html_ids(html: &str) -> Vec<String> {
    let bytes = html.as_bytes();
    let mut ids = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = html[cursor..].find('<') {
        let start = cursor + offset + 1;
        let end = html_tag_end(bytes, start);
        cursor = end;
        if !bytes.get(start).is_some_and(u8::is_ascii_alphabetic) {
            continue;
        }
        let mut at = start;
        while at < end && !bytes[at].is_ascii_whitespace() && bytes[at] != b'>' {
            at += 1;
        }
        while at < end {
            while at < end && (bytes[at].is_ascii_whitespace() || bytes[at] == b'/') {
                at += 1;
            }
            let name_start = at;
            while at < end && !bytes[at].is_ascii_whitespace() && !b"=>/".contains(&bytes[at]) {
                at += 1;
            }
            let name = &html[name_start..at];
            while at < end && bytes[at].is_ascii_whitespace() {
                at += 1;
            }
            if at >= end || bytes[at] == b'>' {
                break;
            }
            if bytes[at] != b'=' {
                continue;
            }
            at += 1;
            while at < end && bytes[at].is_ascii_whitespace() {
                at += 1;
            }
            let quote = bytes
                .get(at)
                .copied()
                .filter(|byte| matches!(byte, b'\"' | b'\''));
            if quote.is_some() {
                at += 1;
            }
            let value_start = at;
            while at < end
                && match quote {
                    Some(quote) => bytes[at] != quote,
                    None => !bytes[at].is_ascii_whitespace() && bytes[at] != b'>',
                }
            {
                at += 1;
            }
            if name.eq_ignore_ascii_case("id") && at > value_start {
                ids.push(html_escape::decode_html_entities(&html[value_start..at]).into_owned());
            }
            if quote.is_some() {
                at += 1;
            }
        }
    }
    ids
}

fn emit_html_nodes(
    document: &SemanticDocument,
    roots: &[NodeId],
    resources: &mut ExportResourceResolver<'_>,
    placeholder_prefix: &str,
    replacements: &mut Vec<Arc<str>>,
    events: &mut Vec<Event<'static>>,
    diagrams: &HashMap<String, RenderedDiagram>,
) {
    enum Work {
        Node(NodeId),
        End(pulldown_cmark::TagEnd),
        CloseFootnote,
    }

    let mut pending = roots
        .iter()
        .rev()
        .copied()
        .map(Work::Node)
        .collect::<Vec<_>>();
    while let Some(work) = pending.pop() {
        let node_id = match work {
            Work::Node(id) => id,
            Work::End(end) => {
                events.push(Event::End(end));
                continue;
            }
            Work::CloseFootnote => {
                events.push(Event::Html("</div>\n".into()));
                continue;
            }
        };
        match document.node(node_id) {
            SemanticNode::Element { tag, children } => match tag {
                Tag::FootnoteDefinition(label) => {
                    let escaped = html_escape::encode_double_quoted_attribute(label);
                    let number = document.footnote_number(label);
                    events.push(Event::Html(format!("<div class=\"footnote-definition\" id=\"{escaped}\"><sup class=\"footnote-definition-label\">{number}</sup>").into()));
                    pending.push(Work::CloseFootnote);
                    pending.extend(children.iter().rev().copied().map(Work::Node));
                }
                Tag::CodeBlock(_) if document.diagram_source(node_id).is_some() => {
                    let source = document
                        .diagram_source(node_id)
                        .expect("checked Mermaid node");
                    if let Some(diagram) = diagrams.get(&source.diagram_id) {
                        let placeholder = format!("{placeholder_prefix}{}", replacements.len());
                        let title = diagram
                            .accessible_title
                            .as_deref()
                            .unwrap_or("Mermaid diagram");
                        let title = encode_text(title);
                        replacements.push(Arc::from(format!(
                            "<figure class=\"mermaid-diagram\" role=\"img\" aria-label=\"{title}\">{}<figcaption>{title}</figcaption></figure>",
                            diagram.svg_utf8
                        )));
                        events.push(Event::Html(CowStr::Boxed(placeholder.into_boxed_str())));
                    } else {
                        events.push(Event::Start(tag.clone()));
                        pending.push(Work::End(tag.to_end()));
                        pending.extend(children.iter().rev().copied().map(Work::Node));
                    }
                }
                Tag::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                } => {
                    if let Some(data_url) = resources.html_data_url(dest_url) {
                        let placeholder = format!("{placeholder_prefix}{}", replacements.len());
                        replacements.push(data_url);
                        events.push(Event::Start(Tag::Image {
                            link_type: *link_type,
                            dest_url: CowStr::Boxed(placeholder.into_boxed_str()),
                            title: title.clone(),
                            id: id.clone(),
                        }));
                        pending.push(Work::End(tag.to_end()));
                    }
                    pending.extend(children.iter().rev().copied().map(Work::Node));
                }
                Tag::Link {
                    link_type,
                    title,
                    id,
                    ..
                } => {
                    let resolved = document
                        .link_target(node_id)
                        .expect("link node must have a shared target");
                    if let Ok(resolved) = resolved {
                        let (destination, requires_placeholder) = match resolved {
                            ExportLink::Anchor(fragment) => (format!("#{fragment}"), false),
                            ExportLink::External(url) => {
                                let is_local_file = url.starts_with("file:");
                                (url.clone(), is_local_file)
                            }
                            ExportLink::Email(address) => (format!("mailto:{address}"), true),
                        };
                        let destination = if requires_placeholder {
                            let placeholder = format!("{placeholder_prefix}{}", replacements.len());
                            replacements.push(Arc::from(destination));
                            placeholder
                        } else {
                            destination
                        };
                        events.push(Event::Start(Tag::Link {
                            link_type: *link_type,
                            dest_url: CowStr::Boxed(destination.into_boxed_str()),
                            title: title.clone(),
                            id: id.clone(),
                        }));
                        pending.push(Work::End(tag.to_end()));
                    } else if let Err(warning) = resolved {
                        resources.warnings_mut().push(warning.clone());
                    }
                    pending.extend(children.iter().rev().copied().map(Work::Node));
                }
                _ => {
                    events.push(Event::Start(tag.clone()));
                    pending.push(Work::End(tag.to_end()));
                    pending.extend(children.iter().rev().copied().map(Work::Node));
                }
            },
            SemanticNode::Event(Event::FootnoteReference(label)) => {
                let escaped = html_escape::encode_double_quoted_attribute(label);
                let number = document.footnote_number(label);
                events.push(Event::Html(format!("<sup class=\"footnote-reference\"><a href=\"#{escaped}\">{number}</a></sup>").into()));
            }
            SemanticNode::Event(Event::Html(value)) => {
                let sanitized = sanitize_raw_html_for_export(value, resources);
                events.push(Event::Html(CowStr::Boxed(sanitized.into_boxed_str())));
            }
            SemanticNode::Event(Event::InlineHtml(value)) => {
                let sanitized = sanitize_raw_html_for_export(value, resources);
                events.push(Event::InlineHtml(CowStr::Boxed(sanitized.into_boxed_str())));
            }
            SemanticNode::Event(event) => events.push(event.clone()),
        }
    }
}

fn sanitize_raw_html_for_export(value: &str, resources: &mut ExportResourceResolver<'_>) -> String {
    let (without_raw_images, removed_image) = strip_export_raw_images(value);
    if removed_image {
        resources.warnings_mut().push(ExportWarning::new(
            "RAW_HTML_IMAGE_SKIPPED",
            "原始 HTML 图片不参与资源解析，已移除图片元素",
            None,
        ));
    }
    without_raw_images
}

fn substitute_resource_placeholders(
    html: &str,
    placeholder_prefix: &str,
    replacements: &[Arc<str>],
) -> String {
    let mut rendered = String::with_capacity(
        html.len() + replacements.iter().map(|value| value.len()).sum::<usize>(),
    );
    let mut cursor = 0;
    while let Some(relative) = html[cursor..].find(placeholder_prefix) {
        let start = cursor + relative;
        rendered.push_str(&html[cursor..start]);
        let digits_start = start + placeholder_prefix.len();
        let digits_end = html[digits_start..]
            .find(|character: char| !character.is_ascii_digit())
            .map(|offset| digits_start + offset)
            .unwrap_or(html.len());
        let replacement = html[digits_start..digits_end]
            .parse::<usize>()
            .ok()
            .and_then(|index| replacements.get(index));
        if let Some(replacement) = replacement {
            rendered.push_str(replacement);
            cursor = digits_end;
        } else {
            rendered.push_str(placeholder_prefix);
            cursor = digits_start;
        }
    }
    rendered.push_str(&html[cursor..]);
    rendered
}

pub(crate) fn standalone(
    request: &ExportRequest,
    body: &str,
    pdf_ready_token: Option<&str>,
) -> String {
    standalone_with_title(
        request,
        body,
        pdf_ready_token,
        request.options.include_title,
    )
}

pub(crate) fn standalone_with_title(
    request: &ExportRequest,
    body: &str,
    pdf_ready_token: Option<&str>,
    include_title: bool,
) -> String {
    let safe_title = encode_text(&request.snapshot.title);
    let document_title = if include_title {
        format!(r#"<h1 class="document-title">{safe_title}</h1>"#)
    } else {
        String::new()
    };
    let (page_width, page_height) = match request.options.paper_size {
        ExportPaperSize::A4 => ("210mm", "297mm"),
        ExportPaperSize::Letter => ("8.5in", "11in"),
    };
    let (page_width, page_height) = match request.options.orientation {
        ExportOrientation::Portrait => (page_width, page_height),
        ExportOrientation::Landscape => (page_height, page_width),
    };
    let margin = match request.options.margin {
        ExportMarginPreset::Narrow => "12.7mm",
        ExportMarginPreset::Normal => "25.4mm",
        ExportMarginPreset::Wide => "38.1mm",
    };
    let ready_script = pdf_ready_token
        .map(pdf_ready_protocol::render_ready_script)
        .unwrap_or_default();
    let script_source = if pdf_ready_token.is_some() {
        "'unsafe-inline'"
    } else {
        "'none'"
    };
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'; script-src {script_source}; font-src 'none'; connect-src 'none'; media-src 'none'; object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'">
  <title>{safe_title}</title>
  <style>
    @page {{ size: {page_width} {page_height}; margin: {margin}; }}
    * {{ box-sizing: border-box; }}
    body {{ margin: 0; color: #1f2328; background: #fff; font: 16px/1.7 "Segoe UI", "PingFang SC", system-ui, sans-serif; overflow-wrap: anywhere; }}
    main {{ max-width: 860px; margin: 0 auto; padding: 48px 28px; }}
    .document-title {{ margin-top: 0; padding-bottom: .35em; border-bottom: 1px solid #d0d7de; }}
    h1, h2, h3, h4, h5, h6 {{ break-after: avoid; line-height: 1.3; }}
    pre {{ overflow: auto; padding: 16px; border-radius: 8px; background: #f6f8fa; white-space: pre-wrap; }}
    code {{ font-family: "Cascadia Code", Consolas, monospace; }}
    table {{ border-collapse: collapse; width: 100%; break-inside: avoid; }}
    th, td {{ border: 1px solid #d0d7de; padding: 8px 10px; }}
    blockquote {{ margin-left: 0; padding-left: 16px; color: #57606a; border-left: 4px solid #d0d7de; }}
    math[display="block"] {{ display: block; width: fit-content; max-width: 100%; margin: 1em auto; overflow-x: auto; overflow-y: hidden; }}
    .math-error {{ color: #b42318; border: 1px solid #f1aeb5; border-radius: 4px; padding: .1em .35em; background: #fff5f5; }}
    .mermaid-diagram {{ max-width: 100%; overflow-x: auto; margin: 1.5em 0; break-inside: avoid; }}
    .mermaid-diagram svg {{ display: block; height: auto; max-width: none; margin: auto; }}
    .mermaid-diagram figcaption {{ position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; }}
    img {{ max-width: 100%; height: auto; }}
    a {{ color: #0969da; }}
    @media print {{ main {{ max-width: none; padding: 0; }} }}
  </style>
</head>
<body><main>{document_title}{body}</main>{ready_script}</body>
</html>"#
    )
}

#[cfg(test)]
mod range_tests {
    use super::*;
    #[test]
    fn custom_html_targets_survive_without_promoting_code_literals_to_anchors() {
        let html = "<p id=\"custom&amp;name\" title=\"&gt; id=&quot;fake&quot;\">target</p><code> id=\"literal\"</code>";
        assert_eq!(html_ids(html), vec!["custom&name"]);
        let document = SemanticDocument::parse("<p id=\"custom&amp;name\">target</p>", None);
        let output = pdf_anchor_links(&document, document.roots(), html);
        assert!(output.contains("href=\"#custom&amp;name\""));
        assert!(!output.contains("href=\"#literal\""));
        let with_diagram = pdf_anchor_links(
            &document,
            document.roots(),
            &format!("{html}<svg id=\"generated\"></svg>"),
        );
        assert!(!with_diagram.contains("href=\"#generated\""));
        assert_eq!(html_ids("<p ID=unquoted>text</p>"), vec!["unquoted"]);
    }
    #[test]
    fn rewrites_only_anchor_attributes_and_preserves_code_literals() {
        let html = "<pre><code>href=\"#literal\"</code></pre><a title=\"x &gt; y\" href=\"#target\">go</a>";
        let rewritten = rewrite_pdf_anchor_links(html);
        assert!(rewritten.contains("<code>href=\"#literal\"</code>"));
        assert!(rewritten.contains("href=\"https://marklite.invalid/_pdf-anchor/#target\""));
    }

    #[test]
    fn ranges_share_footnote_numbers_and_resource_warnings() {
        let content = "# First\n\nOne[^a].\n\n# Second\n\nTwo[^b] and ![image](https://example.com/a.png).\n\n[^a]: First note.\n\n[^b]: Second note.";
        let request: ExportRequest = serde_json::from_value(serde_json::json!({"snapshot":{"jobId":"range","tabId":"tab","contentRevision":1,"sourcePath":null,"title":"Range","content":content},"targetPath":"/tmp/range.pdf","format":"pdf","options":{"paperSize":"a4","orientation":"portrait","margin":"normal","includeTitle":true,"includeLocalImages":false}})).unwrap();
        let document = SemanticDocument::parse(content, None);
        let mut resources = ExportResourceResolver::new(None, false);
        let mut parts = Vec::new();
        for root in document.roots() {
            parts.push(render_roots_with_resources(
                &document,
                &[*root],
                &request,
                &HashMap::new(),
                &mut resources,
                true,
            ));
        }
        let html = parts.join("");
        assert!(html
            .split_once("_pdf-anchor/#a\"")
            .unwrap()
            .1
            .split_once('>')
            .unwrap()
            .1
            .starts_with("1</a>"));
        assert!(html
            .split_once("_pdf-anchor/#b\"")
            .unwrap()
            .1
            .split_once('>')
            .unwrap()
            .1
            .starts_with("2</a>"));
        assert!(html.contains("footnote-definition-label\">2</sup>"));
        assert!(html.contains("href=\"#second\""));
        assert_eq!(
            resources
                .into_warnings()
                .iter()
                .filter(|warning| warning.code == "REMOTE_IMAGE_SKIPPED")
                .count(),
            1
        );
    }
}
