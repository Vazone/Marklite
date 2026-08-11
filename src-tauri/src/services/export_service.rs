use std::{
    collections::HashMap,
    io::Cursor,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use docx_rs::{
    AbstractNumbering, BreakType, Docx, Footnote, Hyperlink, HyperlinkType, IndentLevel, Level,
    LevelJc, LevelText, NumberFormat, Numbering, NumberingId, PageMargin, PageOrientationType,
    Paragraph, Pic, Run, Shading, SpecialIndentType, Start, Table, TableCell, TableRow,
};
use html_escape::encode_text;
use pulldown_cmark::{html, CowStr, Event, HeadingLevel, Parser, Tag};
use url::Url;

use crate::{
    models::{
        app_error::AppError,
        export::{
            ExportMarginPreset, ExportOrientation, ExportPaperSize, ExportRequest, ExportResult,
            ExportWarning,
        },
    },
    services::{markdown_service, navigation_service},
    utils::{atomic_write::atomic_write, security::sanitize_html},
};

const MAX_EXPORT_CONTENT_BYTES: usize = 10 * 1024 * 1024;
const MAX_EMBEDDED_RESOURCE_BYTES: usize = 32 * 1024 * 1024;
const MAX_PDF_READY_TOKEN_BYTES: usize = 128;
static RESOURCE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const PDF_READY_SCRIPT_TEMPLATE: &str = r#"<script>
(() => {
  'use strict';
  const token = __MARKLITE_PDF_READY_TOKEN__;
  const stageTimeoutMs = 8000;
  const completed = [];
  let terminalSent = false;

  const send = (kind, stage, code = '', imageCount = 0, imageFailed = 0) => {
    if (terminalSent) return;
    const endpoint = kind === 'ready'
      ? 'marklite-export://ready'
      : 'marklite-export://error';
    const params = new URLSearchParams({
      token,
      stage,
      code,
      completed: completed.join(','),
      images: String(imageCount),
      failed: String(imageFailed)
    });
    window.location.href = `${endpoint}?${params.toString()}`;
    terminalSent = true;
  };

  const stageError = (code) => Object.assign(new Error(code), { code });
  const errorCode = (error, fallback) => error && typeof error.code === 'string'
    ? error.code
    : fallback;
  const bounded = (promise, timeoutCode, failureCode = timeoutCode.replace('_TIMEOUT', '_FAILED')) => new Promise((resolve, reject) => {
    let settled = false;
    const timer = window.setTimeout(() => {
      if (settled) return;
      settled = true;
      reject(stageError(timeoutCode));
    }, stageTimeoutMs);
    Promise.resolve(promise).then(
      (value) => {
        if (settled) return;
        settled = true;
        window.clearTimeout(timer);
        resolve(value);
      },
      () => {
        if (settled) return;
        settled = true;
        window.clearTimeout(timer);
        reject(stageError(failureCode));
      }
    );
  });

  const waitForDom = () => document.readyState === 'loading'
    ? new Promise((resolve) => document.addEventListener('DOMContentLoaded', resolve, { once: true }))
    : Promise.resolve();

  const waitForImage = (image) => {
    if (typeof image.decode === 'function') return image.decode();
    if (image.complete) {
      return image.naturalWidth > 0
        ? Promise.resolve()
        : Promise.reject(stageError('PDF_IMAGE_DECODE_FAILED'));
    }
    return new Promise((resolve, reject) => {
      image.addEventListener('load', resolve, { once: true });
      image.addEventListener('error', () => reject(stageError('PDF_IMAGE_DECODE_FAILED')), { once: true });
    });
  };

  (async () => {
    try {
      await bounded(waitForDom(), 'PDF_DOM_TIMEOUT');
    } catch (error) {
      send('error', 'pageLoaded', errorCode(error, 'PDF_DOM_FAILED'));
      return;
    }
    completed.push('pageLoaded', 'domReady');

    try {
      await bounded(document.fonts ? document.fonts.ready : Promise.resolve(), 'PDF_FONT_TIMEOUT');
    } catch (error) {
      send('error', 'fontsSettled', errorCode(error, 'PDF_FONT_FAILED'));
      return;
    }
    completed.push('fontsSettled');

    const images = Array.from(document.images);
    const imageResults = await Promise.all(images.map(async (image) => {
      try {
        await bounded(waitForImage(image), 'PDF_IMAGE_TIMEOUT', 'PDF_IMAGE_DECODE_FAILED');
        return '';
      } catch (error) {
        return errorCode(error, 'PDF_IMAGE_DECODE_FAILED');
      }
    }));
    const imageFailures = imageResults.filter(Boolean);
    if (imageFailures.length > 0) {
      const code = imageFailures.includes('PDF_IMAGE_TIMEOUT')
        ? 'PDF_IMAGE_TIMEOUT'
        : 'PDF_IMAGE_DECODE_FAILED';
      send('error', 'imagesSettled', code, images.length, imageFailures.length);
      return;
    }
    completed.push('imagesSettled');

    try {
      document.documentElement.getBoundingClientRect();
      if (document.body) document.body.getBoundingClientRect();
      window.getComputedStyle(document.documentElement).getPropertyValue('width');
      await Promise.resolve();
      void document.documentElement.scrollHeight;
    } catch (_) {
      send('error', 'layoutReady', 'PDF_LAYOUT_FAILED', images.length, 0);
      return;
    }
    send('ready', 'layoutReady', '', images.length, 0);
  })().catch(() => send('error', 'layoutReady', 'PDF_LAYOUT_FAILED'));
})();
</script>"#;

#[derive(Debug, Clone)]
enum SemanticNode {
    Element {
        tag: Tag<'static>,
        children: Vec<SemanticNode>,
    },
    Event(Event<'static>),
}

struct SemanticFrame {
    tag: Tag<'static>,
    children: Vec<SemanticNode>,
}

#[derive(Debug, Clone)]
pub struct PreparedPdf {
    pub html: String,
    pub warnings: Vec<ExportWarning>,
}

#[derive(Default, Clone, Copy)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
}

pub fn export_html(request: &ExportRequest) -> Result<ExportResult, AppError> {
    validate_request(request)?;
    let mut warnings = Vec::new();
    let document = parse_document(&request.snapshot.content);
    let body = render_html_body(&document, request, &mut warnings);
    let html = standalone_html(request, &body, None);
    write_export_target(request, html.as_bytes())?;
    Ok(result(request, warnings))
}

pub fn export_docx(request: &ExportRequest) -> Result<ExportResult, AppError> {
    validate_request(request)?;
    let document = parse_document(&request.snapshot.content);
    let mut context = DocxContext::new(request, &document);
    context.render_document(&document);
    let mut bytes = Cursor::new(Vec::new());
    context
        .docx
        .build()
        .pack(&mut bytes)
        .map_err(|error| AppError::new("DOCX_EXPORT_FAILED", format!("生成 DOCX 失败：{error}")))?;
    write_export_target(request, bytes.get_ref())?;
    Ok(result(request, context.warnings))
}

pub fn prepare_pdf(request: &ExportRequest, ready_token: &str) -> Result<PreparedPdf, AppError> {
    validate_request(request)?;
    if ready_token.is_empty()
        || ready_token.len() > MAX_PDF_READY_TOKEN_BYTES
        || !ready_token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(AppError::new(
            "INVALID_PDF_READY_TOKEN",
            "PDF 导出任务缺少有效的内部就绪令牌",
        ));
    }
    let mut warnings = Vec::new();
    let document = parse_document(&request.snapshot.content);
    let body = render_html_body(&document, request, &mut warnings);
    Ok(PreparedPdf {
        html: standalone_html(request, &body, Some(ready_token)),
        warnings,
    })
}

pub fn validate_request(request: &ExportRequest) -> Result<PathBuf, AppError> {
    if request.snapshot.job_id.trim().is_empty()
        || request.snapshot.tab_id.trim().is_empty()
        || request
            .snapshot
            .job_id
            .chars()
            .chain(request.snapshot.tab_id.chars())
            .any(char::is_control)
    {
        return Err(AppError::new(
            "INVALID_EXPORT_REQUEST",
            "导出任务缺少稳定的 jobId 或 tabId",
        ));
    }
    if request.snapshot.content.len() > MAX_EXPORT_CONTENT_BYTES {
        return Err(AppError::new(
            "EXPORT_CONTENT_TOO_LARGE",
            "导出正文超过 10 MiB",
        ));
    }
    let target = PathBuf::from(&request.target_path);
    if !target.is_absolute()
        || !target
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(request.format.extension()))
    {
        return Err(AppError::new(
            "INVALID_EXPORT_TARGET",
            format!(
                "导出目标必须是绝对 .{} 文件路径",
                request.format.extension()
            ),
        ));
    }
    if target.exists() && !target.is_file() {
        return Err(AppError::invalid_file_target(&request.target_path));
    }
    Ok(target)
}

pub fn commit_pdf(request: &ExportRequest, pdf_bytes: &[u8]) -> Result<ExportResult, AppError> {
    validate_request(request)?;
    if !pdf_bytes.starts_with(b"%PDF-") {
        return Err(AppError::new(
            "INVALID_PDF_OUTPUT",
            "平台打印器没有生成有效的 PDF 文件",
        ));
    }
    write_export_target(request, pdf_bytes)?;
    Ok(result(request, Vec::new()))
}

fn result(request: &ExportRequest, warnings: Vec<ExportWarning>) -> ExportResult {
    ExportResult {
        job_id: request.snapshot.job_id.clone(),
        format: request.format,
        path: request.target_path.clone(),
        warnings,
    }
}

fn write_export_target(request: &ExportRequest, bytes: &[u8]) -> Result<(), AppError> {
    let target = validate_request(request)?;
    if let Some(parent) = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| AppError::file_write_failed(&request.target_path, error))?;
    }
    atomic_write(&target, bytes)
        .map_err(|error| AppError::file_write_failed(&request.target_path, error))
}

fn parse_document(markdown: &str) -> Vec<SemanticNode> {
    let mut root = Vec::new();
    let mut stack: Vec<SemanticFrame> = Vec::new();
    for event in Parser::new_ext(markdown, markdown_service::markdown_options()) {
        match event.into_static() {
            Event::Start(tag) => stack.push(SemanticFrame {
                tag,
                children: Vec::new(),
            }),
            Event::End(_) => {
                if let Some(frame) = stack.pop() {
                    push_node(
                        &mut root,
                        &mut stack,
                        SemanticNode::Element {
                            tag: frame.tag,
                            children: frame.children,
                        },
                    );
                }
            }
            event => push_node(&mut root, &mut stack, SemanticNode::Event(event)),
        }
    }
    root
}

fn push_node(root: &mut Vec<SemanticNode>, stack: &mut [impl FrameChildren], node: SemanticNode) {
    if let Some(frame) = stack.last_mut() {
        frame.children_mut().push(node);
    } else {
        root.push(node);
    }
}

trait FrameChildren {
    fn children_mut(&mut self) -> &mut Vec<SemanticNode>;
}

impl FrameChildren for SemanticFrame {
    fn children_mut(&mut self) -> &mut Vec<SemanticNode> {
        &mut self.children
    }
}

fn render_html_body(
    document: &[SemanticNode],
    request: &ExportRequest,
    warnings: &mut Vec<ExportWarning>,
) -> String {
    let token = format!(
        "{}-{}-{}",
        std::process::id(),
        RESOURCE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        request.snapshot.content_revision
    );
    let mut events = Vec::new();
    let mut replacements = Vec::new();
    let mut embedded_resource_bytes = 0;
    emit_html_nodes(
        document,
        request,
        warnings,
        &token,
        &mut replacements,
        &mut embedded_resource_bytes,
        &mut events,
    );
    let mut raw = String::new();
    html::push_html(&mut raw, events.into_iter());
    let mut cleaned = sanitize_html(&raw);
    for (placeholder, data_url) in replacements {
        cleaned = cleaned.replace(&placeholder, &data_url);
    }
    cleaned
}

fn emit_html_nodes(
    nodes: &[SemanticNode],
    request: &ExportRequest,
    warnings: &mut Vec<ExportWarning>,
    token: &str,
    replacements: &mut Vec<(String, String)>,
    embedded_resource_bytes: &mut usize,
    events: &mut Vec<Event<'static>>,
) {
    for node in nodes {
        match node {
            SemanticNode::Element { tag, children } => match tag {
                Tag::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                } => {
                    if let Some(data_url) =
                        resolve_html_image(request, dest_url, warnings, embedded_resource_bytes)
                    {
                        let placeholder = format!(
                            "https://marklite.invalid/_export-resource/{token}/{}",
                            replacements.len()
                        );
                        replacements.push((placeholder.clone(), data_url));
                        events.push(Event::Start(Tag::Image {
                            link_type: *link_type,
                            dest_url: CowStr::Boxed(placeholder.into_boxed_str()),
                            title: title.clone(),
                            id: id.clone(),
                        }));
                        emit_html_nodes(
                            children,
                            request,
                            warnings,
                            token,
                            replacements,
                            embedded_resource_bytes,
                            events,
                        );
                        events.push(Event::End(tag.to_end()));
                    } else {
                        emit_html_nodes(
                            children,
                            request,
                            warnings,
                            token,
                            replacements,
                            embedded_resource_bytes,
                            events,
                        );
                    }
                }
                Tag::Link { dest_url, .. } if !safe_hyperlink(dest_url) => {
                    warnings.push(ExportWarning::new(
                        "UNSAFE_LINK_SKIPPED",
                        "已移除不安全链接，保留显示文本",
                        Some(dest_url.to_string()),
                    ));
                    emit_html_nodes(
                        children,
                        request,
                        warnings,
                        token,
                        replacements,
                        embedded_resource_bytes,
                        events,
                    );
                }
                _ => {
                    events.push(Event::Start(tag.clone()));
                    emit_html_nodes(
                        children,
                        request,
                        warnings,
                        token,
                        replacements,
                        embedded_resource_bytes,
                        events,
                    );
                    events.push(Event::End(tag.to_end()));
                }
            },
            SemanticNode::Event(event) => events.push(event.clone()),
        }
    }
}

fn resolve_html_image(
    request: &ExportRequest,
    target: &str,
    warnings: &mut Vec<ExportWarning>,
    embedded_resource_bytes: &mut usize,
) -> Option<String> {
    if is_remote_target(target) {
        warnings.push(ExportWarning::new(
            "REMOTE_IMAGE_SKIPPED",
            "导出不会下载远程图片，已保留替代文本",
            Some(target.to_string()),
        ));
        return None;
    }
    if !request.options.include_local_images {
        warnings.push(ExportWarning::new(
            "LOCAL_IMAGE_NOT_EMBEDDED",
            "未选择嵌入本地图片，已保留替代文本",
            Some(target.to_string()),
        ));
        return None;
    }
    match navigation_service::load_local_image_for_export(
        request.snapshot.source_path.as_deref(),
        target,
    ) {
        Ok(image) => {
            if !reserve_export_resource(
                embedded_resource_bytes,
                image.bytes.len(),
                warnings,
                target,
            ) {
                return None;
            }
            Some(format!(
                "data:{};base64,{}",
                image.mime,
                STANDARD.encode(image.bytes)
            ))
        }
        Err(error) => {
            warnings.push(ExportWarning::new(
                error.code,
                error.message,
                Some(target.to_string()),
            ));
            None
        }
    }
}

fn reserve_export_resource(
    embedded_resource_bytes: &mut usize,
    resource_bytes: usize,
    warnings: &mut Vec<ExportWarning>,
    target: &str,
) -> bool {
    let Some(total) = embedded_resource_bytes.checked_add(resource_bytes) else {
        warnings.push(ExportWarning::new(
            "EXPORT_RESOURCE_BUDGET_EXCEEDED",
            "嵌入图片总大小超过 32 MiB，已保留替代文本",
            Some(target.to_string()),
        ));
        return false;
    };
    if total > MAX_EMBEDDED_RESOURCE_BYTES {
        warnings.push(ExportWarning::new(
            "EXPORT_RESOURCE_BUDGET_EXCEEDED",
            "嵌入图片总大小超过 32 MiB，已保留替代文本",
            Some(target.to_string()),
        ));
        return false;
    }
    *embedded_resource_bytes = total;
    true
}

fn standalone_html(request: &ExportRequest, body: &str, pdf_ready_token: Option<&str>) -> String {
    let safe_title = encode_text(&request.snapshot.title);
    let document_title = if request.options.include_title {
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
        .map(|token| {
            PDF_READY_SCRIPT_TEMPLATE.replace(
                "__MARKLITE_PDF_READY_TOKEN__",
                &serde_json::to_string(token)
                    .expect("serializing an internal PDF ready token cannot fail"),
            )
        })
        .unwrap_or_default();
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
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
    img {{ max-width: 100%; height: auto; }}
    a {{ color: #0969da; }}
    @media print {{ main {{ max-width: none; padding: 0; }} }}
  </style>
</head>
<body><main>{document_title}{body}</main>{ready_script}</body>
</html>"#
    )
}

fn safe_hyperlink(target: &str) -> bool {
    if target.starts_with('#') {
        return true;
    }
    if target.chars().any(char::is_control) || target.trim().is_empty() {
        return false;
    }
    match Url::parse(target) {
        Ok(url) => matches!(url.scheme(), "http" | "https" | "file"),
        Err(_) => !target.contains(':') || is_windows_absolute(target),
    }
}

fn is_remote_target(target: &str) -> bool {
    Url::parse(target)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn is_windows_absolute(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

struct DocxContext<'a> {
    request: &'a ExportRequest,
    docx: Docx,
    warnings: Vec<ExportWarning>,
    footnotes: HashMap<String, Vec<SemanticNode>>,
    next_bookmark_id: usize,
    embedded_resource_bytes: usize,
}

impl<'a> DocxContext<'a> {
    fn new(request: &'a ExportRequest, document: &[SemanticNode]) -> Self {
        let mut footnotes = HashMap::new();
        collect_footnotes(document, &mut footnotes);
        let (width, height) = match request.options.paper_size {
            ExportPaperSize::A4 => (11906, 16838),
            ExportPaperSize::Letter => (12240, 15840),
        };
        let (width, height) = match request.options.orientation {
            ExportOrientation::Portrait => (width, height),
            ExportOrientation::Landscape => (height, width),
        };
        let margin = match request.options.margin {
            ExportMarginPreset::Narrow => 720,
            ExportMarginPreset::Normal => 1440,
            ExportMarginPreset::Wide => 2160,
        };
        let orientation = match request.options.orientation {
            ExportOrientation::Portrait => PageOrientationType::Portrait,
            ExportOrientation::Landscape => PageOrientationType::Landscape,
        };
        let docx = add_numbering_definitions(
            Docx::new()
                .page_size(width, height)
                .page_orient(orientation)
                .page_margin(PageMargin {
                    top: margin,
                    left: margin,
                    bottom: margin,
                    right: margin,
                    header: 720,
                    footer: 720,
                    gutter: 0,
                }),
        );
        Self {
            request,
            docx,
            warnings: Vec::new(),
            footnotes,
            next_bookmark_id: 1,
            embedded_resource_bytes: 0,
        }
    }

    fn render_document(&mut self, nodes: &[SemanticNode]) {
        if self.request.options.include_title {
            let paragraph = Paragraph::new()
                .style("Title")
                .add_run(Run::new().add_text(&self.request.snapshot.title));
            self.push_paragraph(paragraph);
        }
        self.render_blocks(nodes, 0, None);
    }

    fn render_blocks(
        &mut self,
        nodes: &[SemanticNode],
        quote_depth: usize,
        numbering: Option<(usize, usize)>,
    ) {
        for node in nodes {
            match node {
                SemanticNode::Element { tag, children } => match tag {
                    Tag::Paragraph => {
                        let mut paragraph = self.inline_paragraph(children, InlineStyle::default());
                        if quote_depth > 0 {
                            paragraph =
                                paragraph.indent(Some(720 * quote_depth as i32), None, None, None);
                        }
                        if let Some((id, level)) = numbering {
                            paragraph =
                                paragraph.numbering(NumberingId::new(id), IndentLevel::new(level));
                        }
                        self.push_paragraph(paragraph);
                    }
                    Tag::Heading { level, id, .. } => {
                        let mut paragraph = self
                            .inline_paragraph(children, InlineStyle::default())
                            .style(heading_style(*level))
                            .keep_next(true);
                        if let Some(name) = id.as_ref().filter(|value| !value.is_empty()) {
                            let bookmark_id = self.next_bookmark_id;
                            self.next_bookmark_id += 1;
                            paragraph = paragraph
                                .add_bookmark_start(bookmark_id, bookmark_name(name))
                                .add_bookmark_end(bookmark_id);
                        }
                        self.push_paragraph(paragraph);
                    }
                    Tag::BlockQuote(_) => self.render_blocks(children, quote_depth + 1, numbering),
                    Tag::CodeBlock(_) => {
                        let text = plain_text(children);
                        let paragraph = Paragraph::new().add_run(
                            Run::new()
                                .add_text(text)
                                .fonts(docx_rs::RunFonts::new().ascii("Consolas"))
                                .shading(Shading::new().fill("F6F8FA")),
                        );
                        self.push_paragraph(paragraph);
                    }
                    Tag::List(start) => self.render_list(children, quote_depth, 0, start.is_some()),
                    Tag::Table(_) => self.render_table(children),
                    Tag::FootnoteDefinition(_) => {}
                    Tag::HtmlBlock => {
                        self.warnings.push(ExportWarning::new(
                            "RAW_HTML_DEGRADED",
                            "DOCX 不执行原始 HTML，已按纯文本导出",
                            None,
                        ));
                        self.push_paragraph(
                            Paragraph::new().add_run(Run::new().add_text(plain_text(children))),
                        );
                    }
                    Tag::Item | Tag::TableHead | Tag::TableRow | Tag::TableCell => {
                        self.render_blocks(children, quote_depth, numbering)
                    }
                    _ => {
                        let paragraph = self.inline_paragraph(children, InlineStyle::default());
                        self.push_paragraph(paragraph);
                    }
                },
                SemanticNode::Event(Event::Rule) => self.push_paragraph(
                    Paragraph::new().add_run(Run::new().add_text("────────────────────────")),
                ),
                SemanticNode::Event(Event::Html(value) | Event::InlineHtml(value)) => {
                    self.warnings.push(ExportWarning::new(
                        "RAW_HTML_DEGRADED",
                        "DOCX 不执行原始 HTML，已按纯文本导出",
                        None,
                    ));
                    self.push_paragraph(
                        Paragraph::new().add_run(Run::new().add_text(value.as_ref())),
                    );
                }
                SemanticNode::Event(event) => {
                    let text = event_text(event);
                    if !text.is_empty() {
                        self.push_paragraph(Paragraph::new().add_run(Run::new().add_text(text)));
                    }
                }
            }
        }
    }

    fn render_list(
        &mut self,
        nodes: &[SemanticNode],
        quote_depth: usize,
        level: usize,
        ordered: bool,
    ) {
        for node in nodes {
            let SemanticNode::Element {
                tag: Tag::Item,
                children,
            } = node
            else {
                continue;
            };
            let numbering_id = if ordered { 2 } else { 3 };
            let mut rendered_primary = false;
            for child in children {
                match child {
                    SemanticNode::Element {
                        tag: Tag::List(start),
                        children,
                    } => {
                        self.render_list(children, quote_depth, level + 1, start.is_some());
                    }
                    SemanticNode::Element {
                        tag: Tag::Paragraph,
                        children,
                    } if !rendered_primary => {
                        rendered_primary = true;
                        let mut paragraph = self.inline_paragraph(children, InlineStyle::default());
                        if quote_depth > 0 {
                            paragraph =
                                paragraph.indent(Some(720 * quote_depth as i32), None, None, None);
                        }
                        paragraph = paragraph
                            .numbering(NumberingId::new(numbering_id), IndentLevel::new(level));
                        self.push_paragraph(paragraph);
                    }
                    other if !rendered_primary => {
                        rendered_primary = true;
                        let paragraph = self
                            .inline_paragraph(std::slice::from_ref(other), InlineStyle::default())
                            .numbering(NumberingId::new(numbering_id), IndentLevel::new(level));
                        self.push_paragraph(paragraph);
                    }
                    SemanticNode::Element { children, .. } => {
                        self.render_blocks(children, quote_depth, None)
                    }
                    _ => {}
                }
            }
            if !rendered_primary {
                self.push_paragraph(
                    Paragraph::new()
                        .numbering(NumberingId::new(numbering_id), IndentLevel::new(level)),
                );
            }
        }
    }

    fn render_table(&mut self, nodes: &[SemanticNode]) {
        let mut rows = Vec::new();
        collect_table_rows(nodes, &mut rows);
        if rows.is_empty() {
            return;
        }
        let rows = rows
            .into_iter()
            .map(|cells| {
                TableRow::new(
                    cells
                        .into_iter()
                        .map(|cell| {
                            TableCell::new()
                                .add_paragraph(self.inline_paragraph(cell, InlineStyle::default()))
                        })
                        .collect(),
                )
            })
            .collect();
        self.docx = std::mem::take(&mut self.docx).add_table(Table::new(rows));
    }

    fn inline_paragraph(&mut self, nodes: &[SemanticNode], style: InlineStyle) -> Paragraph {
        let mut paragraph = Paragraph::new();
        for node in nodes {
            paragraph = self.add_inline(paragraph, node, style);
        }
        paragraph
    }

    fn add_inline(
        &mut self,
        paragraph: Paragraph,
        node: &SemanticNode,
        style: InlineStyle,
    ) -> Paragraph {
        match node {
            SemanticNode::Element { tag, children } => match tag {
                Tag::Strong => self.add_inline_children(
                    paragraph,
                    children,
                    InlineStyle {
                        bold: true,
                        ..style
                    },
                ),
                Tag::Emphasis => self.add_inline_children(
                    paragraph,
                    children,
                    InlineStyle {
                        italic: true,
                        ..style
                    },
                ),
                Tag::Strikethrough => self.add_inline_children(
                    paragraph,
                    children,
                    InlineStyle {
                        strike: true,
                        ..style
                    },
                ),
                Tag::Link { dest_url, .. } => {
                    let label = plain_text(children);
                    if safe_hyperlink(dest_url) {
                        let kind = if dest_url.starts_with('#') {
                            HyperlinkType::Anchor
                        } else {
                            HyperlinkType::External
                        };
                        let value = dest_url.strip_prefix('#').unwrap_or(dest_url);
                        let run = styled_run(Run::new().add_text(label), style)
                            .color("0969DA")
                            .underline("single");
                        paragraph.add_hyperlink(Hyperlink::new(value, kind).add_run(run))
                    } else {
                        self.warnings.push(ExportWarning::new(
                            "UNSAFE_LINK_SKIPPED",
                            "已移除不安全链接，保留显示文本",
                            Some(dest_url.to_string()),
                        ));
                        paragraph.add_run(styled_run(Run::new().add_text(label), style))
                    }
                }
                Tag::Image { dest_url, .. } => {
                    let alt = plain_text(children);
                    if !self.request.options.include_local_images {
                        self.warnings.push(ExportWarning::new(
                            "LOCAL_IMAGE_NOT_EMBEDDED",
                            "未选择嵌入本地图片，已保留替代文本",
                            Some(dest_url.to_string()),
                        ));
                        return paragraph.add_run(styled_run(Run::new().add_text(alt), style));
                    }
                    if is_remote_target(dest_url) {
                        self.warnings.push(ExportWarning::new(
                            "REMOTE_IMAGE_SKIPPED",
                            "导出不会下载远程图片，已保留替代文本",
                            Some(dest_url.to_string()),
                        ));
                        return paragraph.add_run(styled_run(Run::new().add_text(alt), style));
                    }
                    match navigation_service::load_local_image_for_export(
                        self.request.snapshot.source_path.as_deref(),
                        dest_url,
                    ) {
                        Ok(image) if matches!(image.mime, "image/png" | "image/jpeg") => {
                            if reserve_export_resource(
                                &mut self.embedded_resource_bytes,
                                image.bytes.len(),
                                &mut self.warnings,
                                dest_url,
                            ) {
                                paragraph.add_run(Run::new().add_image(Pic::new(&image.bytes)))
                            } else {
                                paragraph.add_run(styled_run(Run::new().add_text(alt), style))
                            }
                        }
                        Ok(image) => {
                            self.warnings.push(ExportWarning::new(
                                "DOCX_IMAGE_FORMAT_DEGRADED",
                                "DOCX 当前只嵌入 PNG/JPEG；GIF/WebP 已保留替代文本",
                                Some(image.path),
                            ));
                            paragraph.add_run(styled_run(Run::new().add_text(alt), style))
                        }
                        Err(error) => {
                            self.warnings.push(ExportWarning::new(
                                error.code,
                                error.message,
                                Some(dest_url.to_string()),
                            ));
                            paragraph.add_run(styled_run(Run::new().add_text(alt), style))
                        }
                    }
                }
                _ => self.add_inline_children(paragraph, children, style),
            },
            SemanticNode::Event(Event::Text(value)) => {
                paragraph.add_run(styled_run(Run::new().add_text(value.as_ref()), style))
            }
            SemanticNode::Event(Event::Code(value)) => paragraph.add_run(styled_run(
                Run::new()
                    .add_text(value.as_ref())
                    .fonts(docx_rs::RunFonts::new().ascii("Consolas"))
                    .shading(Shading::new().fill("F6F8FA")),
                InlineStyle {
                    code: true,
                    ..style
                },
            )),
            SemanticNode::Event(Event::SoftBreak | Event::HardBreak) => {
                paragraph.add_run(Run::new().add_break(BreakType::TextWrapping))
            }
            SemanticNode::Event(Event::TaskListMarker(done)) => {
                paragraph.add_run(Run::new().add_text(if *done { "☒ " } else { "☐ " }))
            }
            SemanticNode::Event(Event::FootnoteReference(label)) => {
                if let Some(nodes) = self.footnotes.get(label.as_ref()).cloned() {
                    let content = plain_text(&nodes);
                    let footnote = Footnote::new()
                        .add_content(Paragraph::new().add_run(Run::new().add_text(content)));
                    paragraph.add_run(Run::new().add_footnote_reference(footnote))
                } else {
                    self.warnings.push(ExportWarning::new(
                        "MISSING_FOOTNOTE_DEFINITION",
                        "脚注引用没有对应定义，已按文本导出",
                        Some(label.to_string()),
                    ));
                    paragraph.add_run(Run::new().add_text(format!("[^{label}]")))
                }
            }
            SemanticNode::Event(Event::InlineMath(value) | Event::DisplayMath(value)) => {
                self.warnings.push(ExportWarning::new(
                    "MATH_DEGRADED",
                    "数学内容已按纯文本导出",
                    None,
                ));
                paragraph.add_run(styled_run(Run::new().add_text(value.as_ref()), style))
            }
            SemanticNode::Event(Event::Html(value) | Event::InlineHtml(value)) => {
                self.warnings.push(ExportWarning::new(
                    "RAW_HTML_DEGRADED",
                    "DOCX 不执行原始 HTML，已按纯文本导出",
                    None,
                ));
                paragraph.add_run(styled_run(Run::new().add_text(value.as_ref()), style))
            }
            SemanticNode::Event(Event::Rule) => paragraph.add_run(Run::new().add_text("────────")),
            SemanticNode::Event(Event::Start(_) | Event::End(_)) => paragraph,
        }
    }

    fn add_inline_children(
        &mut self,
        mut paragraph: Paragraph,
        nodes: &[SemanticNode],
        style: InlineStyle,
    ) -> Paragraph {
        for node in nodes {
            paragraph = self.add_inline(paragraph, node, style);
        }
        paragraph
    }

    fn push_paragraph(&mut self, paragraph: Paragraph) {
        self.docx = std::mem::take(&mut self.docx).add_paragraph(paragraph);
    }
}

fn styled_run(mut run: Run, style: InlineStyle) -> Run {
    if style.bold {
        run = run.bold();
    }
    if style.italic {
        run = run.italic();
    }
    if style.strike {
        run = run.strike();
    }
    if style.code {
        run = run.fonts(docx_rs::RunFonts::new().ascii("Consolas"));
    }
    run
}

fn heading_style(level: HeadingLevel) -> &'static str {
    match level {
        HeadingLevel::H1 => "Heading1",
        HeadingLevel::H2 => "Heading2",
        HeadingLevel::H3 => "Heading3",
        HeadingLevel::H4 => "Heading4",
        HeadingLevel::H5 => "Heading5",
        HeadingLevel::H6 => "Heading6",
    }
}

fn bookmark_name(value: &str) -> String {
    let mut result = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if result.is_empty() || !result.starts_with(|character: char| character.is_ascii_alphabetic()) {
        result.insert_str(0, "marklite_");
    }
    result.truncate(40);
    result
}

fn add_numbering_definitions(mut docx: Docx) -> Docx {
    let mut ordered = AbstractNumbering::new(2);
    let mut bullets = AbstractNumbering::new(3);
    for level in 0..=8 {
        ordered = ordered.add_level(
            Level::new(
                level,
                Start::new(1),
                NumberFormat::new("decimal"),
                LevelText::new(format!("%{}.", level + 1)),
                LevelJc::new("left"),
            )
            .indent(
                Some((720 + level * 360) as i32),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        );
        bullets = bullets.add_level(
            Level::new(
                level,
                Start::new(1),
                NumberFormat::new("bullet"),
                LevelText::new("•"),
                LevelJc::new("left"),
            )
            .indent(
                Some((720 + level * 360) as i32),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        );
    }
    docx = docx
        .add_abstract_numbering(ordered)
        .add_numbering(Numbering::new(2, 2))
        .add_abstract_numbering(bullets)
        .add_numbering(Numbering::new(3, 3));
    docx
}

fn collect_footnotes(nodes: &[SemanticNode], footnotes: &mut HashMap<String, Vec<SemanticNode>>) {
    for node in nodes {
        if let SemanticNode::Element { tag, children } = node {
            if let Tag::FootnoteDefinition(label) = tag {
                footnotes.insert(label.to_string(), children.clone());
            } else {
                collect_footnotes(children, footnotes);
            }
        }
    }
}

fn collect_table_rows<'a>(nodes: &'a [SemanticNode], rows: &mut Vec<Vec<&'a [SemanticNode]>>) {
    for node in nodes {
        if let SemanticNode::Element { tag, children } = node {
            match tag {
                Tag::TableHead | Tag::TableRow => {
                    let cells = children
                        .iter()
                        .filter_map(|child| match child {
                            SemanticNode::Element {
                                tag: Tag::TableCell,
                                children,
                            } => Some(children.as_slice()),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    if !cells.is_empty() {
                        rows.push(cells);
                    }
                }
                _ => collect_table_rows(children, rows),
            }
        }
    }
}

fn plain_text(nodes: &[SemanticNode]) -> String {
    let mut text = String::new();
    for node in nodes {
        match node {
            SemanticNode::Element { children, .. } => text.push_str(&plain_text(children)),
            SemanticNode::Event(event) => text.push_str(&event_text(event)),
        }
    }
    text
}

fn event_text(event: &Event<'_>) -> String {
    match event {
        Event::Text(value)
        | Event::Code(value)
        | Event::InlineMath(value)
        | Event::DisplayMath(value)
        | Event::Html(value)
        | Event::InlineHtml(value) => value.to_string(),
        Event::FootnoteReference(value) => format!("[^{value}]"),
        Event::SoftBreak | Event::HardBreak => "\n".to_string(),
        Event::Rule => "────────────────".to_string(),
        Event::TaskListMarker(done) => {
            if *done {
                "☒ ".to_string()
            } else {
                "☐ ".to_string()
            }
        }
        Event::Start(_) | Event::End(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use base64::{engine::general_purpose::STANDARD, Engine};
    use docx_rs::read_docx;

    use super::{
        commit_pdf, export_docx, export_html, prepare_pdf, reserve_export_resource,
        validate_request, MAX_EMBEDDED_RESOURCE_BYTES,
    };

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn unique_path(extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "marklite-export-test-{}-{}.{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            extension
        ))
    }
    use crate::models::export::{
        ExportFormat, ExportMarginPreset, ExportOptions, ExportOrientation, ExportPaperSize,
        ExportRequest, ExportSnapshot,
    };

    fn request(format: ExportFormat, path: &Path, content: &str) -> ExportRequest {
        ExportRequest {
            snapshot: ExportSnapshot {
                job_id: "job-1".to_string(),
                tab_id: "tab-1".to_string(),
                content_revision: 7,
                source_path: None,
                title: "中文 Title".to_string(),
                content: content.to_string(),
            },
            target_path: path.to_string_lossy().to_string(),
            format,
            options: ExportOptions {
                paper_size: ExportPaperSize::A4,
                orientation: ExportOrientation::Portrait,
                margin: ExportMarginPreset::Normal,
                include_title: true,
                include_local_images: false,
            },
        }
    }

    #[test]
    fn validates_absolute_matching_export_targets() {
        let relative_request = request(ExportFormat::Html, Path::new("relative.html"), "body");
        assert_eq!(
            validate_request(&relative_request).unwrap_err().code,
            "INVALID_EXPORT_TARGET"
        );

        let mismatched = request(ExportFormat::Html, &unique_path("pdf"), "body");
        assert_eq!(
            validate_request(&mismatched).unwrap_err().code,
            "INVALID_EXPORT_TARGET"
        );

        let directory = unique_path("html");
        fs::create_dir(&directory).unwrap();
        let directory_request = request(ExportFormat::Html, &directory, "body");
        assert_eq!(
            validate_request(&directory_request).unwrap_err().code,
            "INVALID_FILE_TARGET"
        );
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn exports_sanitized_standalone_html_without_remote_image_fetches() {
        let path = unique_path("html");
        let request = request(
            ExportFormat::Html,
            &path,
            "# Heading\n\n<script>alert(1)</script>\n\n![remote](https://example.com/x.png)",
        );
        let result = export_html(&request).unwrap();
        let html = fs::read_to_string(&path).unwrap();
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("Heading"));
        assert!(!html.contains("<script>alert"));
        assert!(!html.contains("https://example.com/x.png"));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.code == "REMOTE_IMAGE_SKIPPED"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn embeds_only_explicitly_enabled_verified_local_images_in_html() {
        let directory = unique_path("resources");
        fs::create_dir(&directory).unwrap();
        let image_path = directory.join("像素.png");
        let image_bytes = STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        fs::write(&image_path, image_bytes).unwrap();
        let source_path = directory.join("fixture.md");
        fs::write(&source_path, b"fixture").unwrap();
        let output = directory.join("portable.html");
        let mut request = request(
            ExportFormat::Html,
            &output,
            "# 中文 😀\n\n![像素](像素.png)\n\n[site](https://example.com)",
        );
        request.snapshot.source_path = Some(source_path.to_string_lossy().into());
        request.options.include_local_images = true;

        let result = export_html(&request).unwrap();
        let html = fs::read_to_string(&output).unwrap();
        assert!(
            html.contains("data:image/png;base64,"),
            "warnings: {:?}",
            result.warnings
        );
        assert!(!html.contains(&image_path.to_string_lossy().to_string()));
        assert!(html.contains("https://example.com"));
        assert!(result.warnings.is_empty());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn exports_and_reopens_structured_docx() {
        let path = unique_path("docx");
        let request = request(
            ExportFormat::Docx,
            &path,
            "# Heading {#anchor}\n\n- item\n- [x] done\n\n| A | B |\n| - | - |\n| 1 | 2 |\n\n[site](https://example.com) note[^1]\n\n[^1]: footnote",
        );
        export_docx(&request).unwrap();
        let bytes = fs::read(&path).unwrap();
        let reopened = read_docx(&bytes).unwrap().json();
        assert!(reopened.contains("Heading"));
        assert!(reopened.contains("footnote"));
        assert!(reopened.contains("https://example.com"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn prepares_a_bounded_pdf_ready_surface_without_application_ui() {
        let path = unique_path("pdf");
        let request = request(ExportFormat::Pdf, &path, "# Heading\n\nbody");
        let prepared = prepare_pdf(&request, "test-ready-token").unwrap();
        assert!(prepared.html.contains("document.fonts.ready"));
        assert!(prepared.html.contains("image.decode"));
        assert!(!prepared.html.contains("requestAnimationFrame"));
        assert!(prepared.html.contains("PDF_FONT_TIMEOUT"));
        assert!(prepared.html.contains("PDF_IMAGE_TIMEOUT"));
        assert!(prepared.html.contains("PDF_IMAGE_DECODE_FAILED"));
        assert!(!prepared.html.contains("PDF_IMAGE_FAILED"));
        assert!(prepared.html.contains("marklite-export://ready"));
        assert!(prepared.html.contains("marklite-export://error"));
        assert!(!prepared.html.contains("marklite-export://progress"));
        assert!(prepared.html.contains("completed: completed.join(',')"));
        assert!(prepared.html.contains("let terminalSent = false"));
        assert!(prepared.html.contains("test-ready-token"));
        assert!(!prepared.html.contains("sidebar-resize-separator"));
        assert!(!prepared.html.contains("markdown-toolbar"));
    }

    #[test]
    fn rejects_invalid_pdf_ready_tokens() {
        let path = unique_path("pdf");
        let request = request(ExportFormat::Pdf, &path, "body");
        assert_eq!(
            prepare_pdf(&request, "token with spaces").unwrap_err().code,
            "INVALID_PDF_READY_TOKEN"
        );
    }

    #[test]
    fn maps_pdf_page_options_into_the_standalone_surface() {
        let path = unique_path("pdf");
        let mut request = request(ExportFormat::Pdf, &path, "body");
        request.options.paper_size = ExportPaperSize::A4;
        request.options.orientation = ExportOrientation::Landscape;
        request.options.margin = ExportMarginPreset::Wide;
        let a4 = prepare_pdf(&request, "a4-landscape").unwrap();
        assert!(a4
            .html
            .contains("@page { size: 297mm 210mm; margin: 38.1mm; }"));

        request.options.paper_size = ExportPaperSize::Letter;
        request.options.orientation = ExportOrientation::Portrait;
        request.options.margin = ExportMarginPreset::Narrow;
        let letter = prepare_pdf(&request, "letter-portrait").unwrap();
        assert!(letter
            .html
            .contains("@page { size: 8.5in 11in; margin: 12.7mm; }"));
    }

    #[test]
    fn rejects_invalid_pdf_bytes_without_replacing_the_existing_target() {
        let path = unique_path("pdf");
        fs::write(&path, b"existing-pdf").unwrap();
        let request = request(ExportFormat::Pdf, &path, "body");
        assert_eq!(
            commit_pdf(&request, b"not-a-pdf").unwrap_err().code,
            "INVALID_PDF_OUTPUT"
        );
        assert_eq!(fs::read(&path).unwrap(), b"existing-pdf");

        let valid = b"%PDF-1.4\n%%EOF\n";
        let result = commit_pdf(&request, valid).unwrap();
        assert_eq!(result.job_id, "job-1");
        assert_eq!(fs::read(&path).unwrap(), valid);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn enforces_total_embedded_resource_budget() {
        let mut total = MAX_EMBEDDED_RESOURCE_BYTES - 1;
        let mut warnings = Vec::new();
        assert!(reserve_export_resource(
            &mut total,
            1,
            &mut warnings,
            "first.png"
        ));
        assert!(!reserve_export_resource(
            &mut total,
            1,
            &mut warnings,
            "second.png"
        ));
        assert_eq!(total, MAX_EMBEDDED_RESOURCE_BYTES);
        assert_eq!(warnings[0].code, "EXPORT_RESOURCE_BUDGET_EXCEEDED");
    }
}
