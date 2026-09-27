#[path = "export_docx_writer/footnote_assets.rs"]
mod footnote_assets;
use super::export_progress::{ExportReporter, ExportStage};
use crate::models::diagram_assets::RasterDiagram;
use crate::models::markdown_event::Event;
use std::{
    collections::HashMap,
    io::{Cursor, Read, Write},
};

use docx_rs::{
    AbstractNumbering, AlignmentType, BreakType, Docx, Footnote, Hyperlink, HyperlinkType,
    IndentLevel, Level, LevelJc, LevelOverride, LevelText, NumberFormat, Numbering, NumberingId,
    PageMargin, PageOrientationType, Paragraph, Pic, Run, Shading, SpecialIndentType, Start, Table,
    TableCell, TableRow,
};
use pulldown_cmark::{Alignment, BlockQuoteKind, HeadingLevel, Tag};
use quick_xml::{events::Event as XmlEvent, Reader as XmlReader};
use sha2::{Digest, Sha256};

fn code_run(text: &str) -> Run {
    let mut run = Run::new()
        .fonts(docx_rs::RunFonts::new().ascii("Consolas"))
        .shading(Shading::new().fill("F6F8FA"));
    let mut start = 0;
    for (at, ch) in text.char_indices() {
        if ch == '\n' || ch == '\t' {
            if at > start {
                run = run.add_text(&text[start..at]);
            }
            run = if ch == '\n' {
                run.add_break(BreakType::TextWrapping)
            } else {
                run.add_tab()
            };
            start = at + 1;
        }
    }
    if start < text.len() {
        run = run.add_text(&text[start..]);
    }
    run
}

use crate::{
    models::{
        app_error::AppError,
        export::{
            ExportMarginPreset, ExportOrientation, ExportPaperSize, ExportRequest, ExportWarning,
        },
    },
    services::{
        export_resources::{ExportLink, ExportResourceResolver},
        export_semantic::{event_text, NodeId, SemanticDocument, SemanticNode},
        math_service,
    },
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

const EMU_PER_TWIP: u32 = 635;

#[derive(Default, Clone, Copy)]
struct InlineStyle {
    highlight: bool,
    bold: bool,
    italic: bool,
    strike: bool,
    vertical: Option<docx_rs::VertAlignType>,
    code: bool,
    in_footnote: bool,
}

impl InlineStyle {
    fn for_tag(mut self, tag: &Tag<'_>) -> Option<Self> {
        match tag {
            Tag::Strong => self.bold = true,
            Tag::Emphasis => self.italic = true,
            Tag::Strikethrough => self.strike = true,
            Tag::Superscript => self.vertical = Some(docx_rs::VertAlignType::SuperScript),
            Tag::Subscript => self.vertical = Some(docx_rs::VertAlignType::SubScript),
            _ => return None,
        }
        Some(self)
    }
}

#[cfg(test)]
pub(crate) fn render(
    request: &ExportRequest,
    document: &SemanticDocument,
    diagrams: &HashMap<String, RasterDiagram>,
    reporter: &ExportReporter,
) -> Result<(Vec<u8>, Vec<ExportWarning>), AppError> {
    let resources = ExportResourceResolver::new(
        request.snapshot.source_path.as_deref(),
        request.options.include_local_images,
    );
    render_with_resources(request, document, diagrams, reporter, resources)
}

pub(crate) fn render_with_resources(
    request: &ExportRequest,
    document: &SemanticDocument,
    diagrams: &HashMap<String, RasterDiagram>,
    reporter: &ExportReporter,
    resources: ExportResourceResolver<'_>,
) -> Result<(Vec<u8>, Vec<ExportWarning>), AppError> {
    let mut context = DocxContext::new(request, document, diagrams, resources);
    for warning in document.warnings() {
        context.resources.warnings_mut().push(warning.clone());
    }
    context.render_document(document);
    reporter.phase(ExportStage::Encoding);
    let mut bytes = Cursor::new(Vec::new());
    let mut package = context.docx.build();
    let footnote_rels = footnote_assets::pack(
        &mut package,
        context.footnote_images,
        context.footnote_links,
    );
    package
        .pack(&mut bytes)
        .map_err(|error| AppError::new("DOCX_EXPORT_FAILED", format!("生成 DOCX 失败：{error}")))?;
    reporter.phase(ExportStage::Validating);
    let bytes = inject_omml(
        bytes.into_inner(),
        &context.math_replacements,
        &context.diagram_picture_alts,
        footnote_rels.as_deref(),
    )?;
    Ok((bytes, context.resources.into_warnings()))
}

struct DocxContext<'a> {
    request: &'a ExportRequest,
    document: &'a SemanticDocument,
    docx: Docx,
    resources: ExportResourceResolver<'a>,
    diagram_rasters: &'a HashMap<String, RasterDiagram>,
    diagram_picture_alts: Vec<DiagramPictureAlt>,
    footnote_images: Vec<Pic>,
    footnote_links: Vec<docx_rs::HyperlinkData>,
    footnotes: HashMap<String, Vec<NodeId>>,
    heading_bookmarks: HashMap<String, String>,
    next_bookmark_id: usize,
    next_numbering_id: usize,
    content_width_emu: u32,
    content_height_emu: u32,
    math_replacements: Vec<MathReplacement>,
    math_slot_prefix: String,
    math_count: usize,
}

struct DiagramPictureAlt {
    bookmark_id: usize,
    part: MathPart,
    description: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MathPart {
    Document,
    Footnotes,
}

impl MathPart {
    fn zip_name(self) -> &'static str {
        match self {
            Self::Document => "word/document.xml",
            Self::Footnotes => "word/footnotes.xml",
        }
    }

    fn root_tag(self) -> &'static str {
        match self {
            Self::Document => "<w:document ",
            Self::Footnotes => "<w:footnotes ",
        }
    }
}

struct MathReplacement {
    part: MathPart,
    token: String,
    omml: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DocxPageLayout {
    width_twips: u32,
    height_twips: u32,
    margin_twips: u32,
    orientation: PageOrientationType,
}

impl DocxPageLayout {
    pub(crate) fn from_request(request: &ExportRequest) -> Self {
        let (width_twips, height_twips) = match request.options.paper_size {
            ExportPaperSize::A4 => (11906, 16838),
            ExportPaperSize::Letter => (12240, 15840),
        };
        let (width_twips, height_twips, orientation) = match request.options.orientation {
            ExportOrientation::Portrait => {
                (width_twips, height_twips, PageOrientationType::Portrait)
            }
            ExportOrientation::Landscape => {
                (height_twips, width_twips, PageOrientationType::Landscape)
            }
        };
        let margin_twips = match request.options.margin {
            ExportMarginPreset::Narrow => 720,
            ExportMarginPreset::Normal => 1440,
            ExportMarginPreset::Wide => 2160,
        };
        Self {
            width_twips,
            height_twips,
            margin_twips,
            orientation,
        }
    }

    pub(crate) fn content_width_emu(self) -> u32 {
        self.width_twips
            .saturating_sub(self.margin_twips.saturating_mul(2))
            .saturating_mul(EMU_PER_TWIP)
            .max(1)
    }

    fn content_height_emu(self) -> u32 {
        self.height_twips
            .saturating_sub(self.margin_twips.saturating_mul(2))
            .saturating_mul(EMU_PER_TWIP)
            .max(1)
    }

    fn page_margin(self) -> PageMargin {
        let margin = i32::try_from(self.margin_twips).unwrap_or(i32::MAX);
        PageMargin {
            top: margin,
            left: margin,
            bottom: margin,
            right: margin,
            header: 720,
            footer: 720,
            gutter: 0,
        }
    }
}

pub(crate) fn fit_docx_picture_to_content_width(picture: Pic, max_width_emu: u32) -> Pic {
    let (width_emu, height_emu) = picture.size;
    if width_emu == 0 || height_emu == 0 || width_emu <= max_width_emu {
        return picture;
    }

    let scaled_height = (u64::from(height_emu)
        .saturating_mul(u64::from(max_width_emu))
        .saturating_add(u64::from(width_emu) / 2)
        / u64::from(width_emu))
    .clamp(1, u64::from(u32::MAX)) as u32;
    picture.size(max_width_emu.max(1), scaled_height)
}

fn fit_docx_picture_to_content_box(picture: Pic, max_width_emu: u32, max_height_emu: u32) -> Pic {
    let (width, height) = picture.size;
    if width == 0 || height == 0 {
        return picture;
    }
    let scale_width = f64::from(max_width_emu) / f64::from(width);
    let scale_height = f64::from(max_height_emu) / f64::from(height);
    let scale = 1.0_f64.min(scale_width).min(scale_height);
    picture.size(
        (f64::from(width) * scale).round().max(1.0) as u32,
        (f64::from(height) * scale).round().max(1.0) as u32,
    )
}

enum DocxWork {
    Nodes(Vec<NodeId>, usize, usize, Option<(usize, usize)>),
    List(Vec<NodeId>, usize, usize, usize, usize),
    InlineParagraph(Vec<NodeId>, usize, Option<(usize, usize)>),
}

fn is_docx_inline_node(node: &SemanticNode) -> bool {
    match node {
        SemanticNode::Highlight { .. } => true,
        SemanticNode::Element { tag, .. } => matches!(
            tag,
            Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Superscript
                | Tag::Subscript
                | Tag::Link { .. }
                | Tag::Image { .. }
        ),
        SemanticNode::Event(Event::Rule | Event::Html(_)) => false,
        SemanticNode::Event(_) => true,
    }
}

impl<'a> DocxContext<'a> {
    fn new(
        request: &'a ExportRequest,
        document: &'a SemanticDocument,
        diagrams: &'a HashMap<String, RasterDiagram>,
        resources: ExportResourceResolver<'a>,
    ) -> Self {
        let footnotes = collect_footnotes(document);
        let layout = DocxPageLayout::from_request(request);
        let docx = add_base_numbering_definitions(
            Docx::new()
                .page_size(layout.width_twips, layout.height_twips)
                .page_orient(layout.orientation)
                .page_margin(layout.page_margin()),
        );
        Self {
            request,
            document,
            docx,
            resources,
            diagram_rasters: diagrams,
            diagram_picture_alts: Vec::new(),
            footnote_images: Vec::new(),
            footnote_links: Vec::new(),
            footnotes,
            heading_bookmarks: collect_heading_bookmarks(document),
            next_bookmark_id: 1,
            next_numbering_id: 4,
            content_width_emu: layout.content_width_emu(),
            content_height_emu: layout.content_height_emu(),
            math_replacements: Vec::new(),
            math_slot_prefix: math_slot_prefix(request, document),
            math_count: 0,
        }
    }

    fn render_document(&mut self, document: &SemanticDocument) {
        if self.request.options.include_title {
            let paragraph = Paragraph::new()
                .style("Title")
                .add_run(Run::new().add_text(&self.request.snapshot.title));
            self.push_paragraph(paragraph);
        }
        self.render_blocks(document.roots(), 0, None);
    }

    fn render_blocks(
        &mut self,
        nodes: &[NodeId],
        quote_depth: usize,
        numbering: Option<(usize, usize)>,
    ) {
        let mut pending = vec![DocxWork::Nodes(nodes.to_vec(), 0, quote_depth, numbering)];
        while let Some(work) = pending.pop() {
            let (nodes, index, quote_depth, numbering) = match work {
                DocxWork::Nodes(nodes, index, quote_depth, numbering) => {
                    (nodes, index, quote_depth, numbering)
                }
                DocxWork::List(nodes, item_index, quote_depth, level, numbering_id) => {
                    self.render_list_item(
                        nodes,
                        item_index,
                        quote_depth,
                        level,
                        numbering_id,
                        &mut pending,
                    );
                    continue;
                }
                DocxWork::InlineParagraph(nodes, quote_depth, numbering) => {
                    let mut paragraph = self.inline_paragraph(&nodes, InlineStyle::default());
                    if quote_depth > 0 {
                        paragraph =
                            paragraph.indent(Some(720 * quote_depth as i32), None, None, None);
                    }
                    if let Some((id, level)) = numbering {
                        paragraph =
                            paragraph.numbering(NumberingId::new(id), IndentLevel::new(level));
                    }
                    self.push_paragraph(paragraph);
                    continue;
                }
            };
            let Some(node) = nodes.get(index).copied() else {
                continue;
            };
            pending.push(DocxWork::Nodes(nodes, index + 1, quote_depth, numbering));
            match self.document.node(node) {
                SemanticNode::Event(Event::Toc(toc)) => {
                    for heading in toc.headings() {
                        let name = self
                            .heading_bookmarks
                            .get(&heading.slug)
                            .expect("TOC headings originate from the same document");
                        let paragraph = Paragraph::new()
                            .indent(
                                Some(i32::from(heading.level.saturating_sub(1)) * 360),
                                None,
                                None,
                                None,
                            )
                            .add_hyperlink(
                                Hyperlink::new(name.clone(), HyperlinkType::Anchor)
                                    .add_run(Run::new().add_text(&heading.title)),
                            );
                        self.push_paragraph(paragraph);
                    }
                }
                SemanticNode::Highlight { .. } => {
                    let paragraph = self.inline_paragraph(&[node], InlineStyle::default());
                    self.push_paragraph(paragraph);
                }
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
                        if let Some(name) = id
                            .as_ref()
                            .and_then(|id| self.heading_bookmarks.get(id.as_ref()))
                        {
                            let bookmark_id = self.next_bookmark_id;
                            self.next_bookmark_id += 1;
                            paragraph = paragraph
                                .add_bookmark_start(bookmark_id, name.clone())
                                .add_bookmark_end(bookmark_id);
                        }
                        self.push_paragraph(paragraph);
                    }
                    Tag::BlockQuote(kind) => {
                        if let Some(kind) = kind {
                            self.push_paragraph(
                                Paragraph::new()
                                    .add_run(Run::new().add_text(alert_label(*kind)).bold())
                                    .keep_next(true),
                            );
                        }
                        pending.push(DocxWork::Nodes(
                            children.clone(),
                            0,
                            quote_depth + 1,
                            numbering,
                        ));
                    }
                    Tag::CodeBlock(kind) => {
                        if let Some(source) = self.document.diagram_source(node) {
                            if let Some(raster) = self.diagram_rasters.get(&source.diagram_id) {
                                let paragraph = self.diagram_paragraph(
                                    Paragraph::new(),
                                    node,
                                    raster,
                                    MathPart::Document,
                                );
                                self.push_paragraph(paragraph);
                                continue;
                            }
                        }
                        let text = self.document.plain_text(children);
                        let info = match kind {
                            pulldown_cmark::CodeBlockKind::Fenced(info) => info.as_ref(),
                            _ => "",
                        };
                        let tokens = match self.document.code_tokens(&text, info) {
                            Ok(tokens) => tokens,
                            Err(error) => {
                                self.resources.warnings_mut().push(ExportWarning::new(
                                    "CODE_HIGHLIGHT_FAILED",
                                    error,
                                    None,
                                ));
                                std::sync::Arc::from([])
                            }
                        };
                        let run = code_run;
                        let mut paragraph = Paragraph::new();
                        let mut at = 0;
                        for token in tokens.iter() {
                            if token.start > at {
                                paragraph = paragraph.add_run(run(&text[at..token.start]));
                            }
                            let mut colored = run(&text[token.start..token.end]);
                            if let Some(color) = super::code_highlight::color(token.class) {
                                colored = colored.color(color);
                            }
                            paragraph = paragraph.add_run(colored);
                            at = token.end;
                        }
                        if at < text.len() {
                            paragraph = paragraph.add_run(run(&text[at..]));
                        }
                        self.push_paragraph(paragraph);
                    }
                    Tag::List(start) => {
                        let numbering_id = self.allocate_numbering(*start, 0);
                        pending.push(DocxWork::List(
                            children.clone(),
                            0,
                            quote_depth,
                            0,
                            numbering_id,
                        ))
                    }
                    Tag::Table(alignments) => self.render_table(children, alignments),
                    Tag::FootnoteDefinition(_) => {}
                    Tag::DefinitionList => {
                        pending.push(DocxWork::Nodes(children.clone(), 0, quote_depth, numbering))
                    }
                    Tag::DefinitionListTitle => {
                        let paragraph = self
                            .inline_paragraph(
                                children,
                                InlineStyle {
                                    bold: true,
                                    ..InlineStyle::default()
                                },
                            )
                            .keep_next(true);
                        self.push_paragraph(paragraph);
                    }
                    Tag::DefinitionListDefinition => pending.push(DocxWork::Nodes(
                        children.clone(),
                        0,
                        quote_depth + 1,
                        numbering,
                    )),
                    Tag::HtmlBlock => {
                        self.resources.warnings_mut().push(ExportWarning::new(
                            "RAW_HTML_DEGRADED",
                            "DOCX 不执行原始 HTML，已按纯文本导出",
                            None,
                        ));
                        self.push_paragraph(
                            Paragraph::new()
                                .add_run(Run::new().add_text(self.document.plain_text(children))),
                        );
                    }
                    Tag::Item | Tag::TableHead | Tag::TableRow | Tag::TableCell => {
                        pending.push(DocxWork::Nodes(children.clone(), 0, quote_depth, numbering))
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
                    self.resources.warnings_mut().push(ExportWarning::new(
                        "RAW_HTML_DEGRADED",
                        "DOCX 不执行原始 HTML，已按纯文本导出",
                        None,
                    ));
                    self.push_paragraph(
                        Paragraph::new().add_run(Run::new().add_text(value.as_ref())),
                    );
                }
                SemanticNode::Event(Event::InlineMath(_) | Event::DisplayMath(_)) => {
                    let paragraph = self.inline_paragraph(&[node], InlineStyle::default());
                    self.push_paragraph(paragraph);
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

    fn render_list_item(
        &mut self,
        nodes: Vec<NodeId>,
        item_index: usize,
        quote_depth: usize,
        level: usize,
        numbering_id: usize,
        pending: &mut Vec<DocxWork>,
    ) {
        let Some((actual_index, children)) = nodes
            .iter()
            .copied()
            .enumerate()
            .skip(item_index)
            .find_map(|(index, node)| match self.document.node(node) {
                SemanticNode::Element {
                    tag: Tag::Item,
                    children,
                } => Some((index, children.clone())),
                _ => None,
            })
        else {
            return;
        };
        pending.push(DocxWork::List(
            nodes,
            actual_index + 1,
            quote_depth,
            level,
            numbering_id,
        ));

        let mut rendered_primary = false;
        let mut inline_nodes = Vec::new();
        let mut item_works = Vec::new();
        for child in children.iter().copied() {
            if is_docx_inline_node(self.document.node(child)) {
                inline_nodes.push(child);
                continue;
            }

            if !inline_nodes.is_empty() {
                let numbering = if rendered_primary {
                    None
                } else {
                    rendered_primary = true;
                    Some((numbering_id, level))
                };
                item_works.push(DocxWork::InlineParagraph(
                    std::mem::take(&mut inline_nodes),
                    quote_depth,
                    numbering,
                ));
            }

            match self.document.node(child) {
                SemanticNode::Element {
                    tag: Tag::List(nested_start),
                    children,
                } => {
                    if !rendered_primary {
                        rendered_primary = true;
                        item_works.push(DocxWork::InlineParagraph(
                            Vec::new(),
                            quote_depth,
                            Some((numbering_id, level)),
                        ));
                    }
                    let nested_id = self.allocate_numbering(*nested_start, level + 1);
                    item_works.push(DocxWork::List(
                        children.clone(),
                        0,
                        quote_depth,
                        level + 1,
                        nested_id,
                    ));
                }
                SemanticNode::Element {
                    tag: Tag::Paragraph,
                    ..
                } if !rendered_primary => {
                    rendered_primary = true;
                    item_works.push(DocxWork::Nodes(
                        vec![child],
                        0,
                        quote_depth,
                        Some((numbering_id, level)),
                    ));
                }
                _ => {
                    if !rendered_primary {
                        rendered_primary = true;
                        item_works.push(DocxWork::InlineParagraph(
                            Vec::new(),
                            quote_depth,
                            Some((numbering_id, level)),
                        ));
                    }
                    item_works.push(DocxWork::Nodes(vec![child], 0, quote_depth, None));
                }
            }
        }
        if !inline_nodes.is_empty() {
            let numbering = if rendered_primary {
                None
            } else {
                rendered_primary = true;
                Some((numbering_id, level))
            };
            item_works.push(DocxWork::InlineParagraph(
                inline_nodes,
                quote_depth,
                numbering,
            ));
        }
        if !rendered_primary {
            item_works.push(DocxWork::InlineParagraph(
                Vec::new(),
                quote_depth,
                Some((numbering_id, level)),
            ));
        }
        pending.extend(item_works.into_iter().rev());
    }

    fn allocate_numbering(&mut self, start: Option<u64>, level: usize) -> usize {
        if let Some(start) = start {
            let id = self.next_numbering_id;
            self.next_numbering_id += 1;
            let start = usize::try_from(start).unwrap_or(usize::MAX);
            self.docx = std::mem::take(&mut self.docx).add_numbering(
                Numbering::new(id, 2).add_override(LevelOverride::new(level.min(8)).start(start)),
            );
            id
        } else {
            3
        }
    }

    fn render_table(&mut self, nodes: &[NodeId], alignments: &[Alignment]) {
        let rows = collect_table_rows(self.document, nodes);
        if rows.is_empty() {
            return;
        }
        let rows = rows
            .into_iter()
            .map(|cells| {
                TableRow::new(
                    cells
                        .into_iter()
                        .enumerate()
                        .map(|(column, cell)| {
                            let alignment =
                                alignments.get(column).copied().unwrap_or(Alignment::None);
                            TableCell::new().add_paragraph(
                                self.inline_paragraph(&cell, InlineStyle::default())
                                    .align(docx_alignment(alignment)),
                            )
                        })
                        .collect(),
                )
            })
            .collect();
        self.docx = std::mem::take(&mut self.docx).add_table(Table::new(rows));
    }

    fn diagram_paragraph(
        &mut self,
        paragraph: Paragraph,
        node: NodeId,
        raster: &RasterDiagram,
        part: MathPart,
    ) -> Paragraph {
        let source = self.document.diagram_source(node).expect("diagram source");
        let bookmark_id = self.next_bookmark_id;
        self.next_bookmark_id += 1;
        self.diagram_picture_alts.push(DiagramPictureAlt {
            bookmark_id,
            part,
            description: format!(
                "Mermaid 图表，源码字节 {}-{}",
                source.source_start_byte, source.source_end_byte
            ),
        });
        let picture = fit_docx_picture_to_content_box(
            Pic::new_with_dimensions(raster.png.clone(), raster.width, raster.height),
            self.content_width_emu,
            self.content_height_emu,
        );
        let picture = self.register_picture(picture, part);
        // A bookmark identifies this occurrence; docx-rs may rewrite the media relationship.
        paragraph
            .add_bookmark_start(bookmark_id, format!("_marklite_diagram_{bookmark_id}"))
            .add_run(Run::new().add_image(picture))
            .add_bookmark_end(bookmark_id)
    }

    fn register_picture(&mut self, mut picture: Pic, part: MathPart) -> Pic {
        if part == MathPart::Footnotes {
            // docx-rs does not collect media nested inside footnote references.
            // Keep geometry in the drawing and transfer its bytes to package assembly.
            let bytes = std::mem::take(&mut picture.image);
            let mut asset = picture.clone();
            asset.image = bytes;
            self.footnote_images.push(asset);
        }
        picture
    }

    fn inline_paragraph(&mut self, nodes: &[NodeId], style: InlineStyle) -> Paragraph {
        let mut paragraph = Paragraph::new();
        for node in nodes.iter().copied() {
            paragraph = self.add_inline(paragraph, node, style);
        }
        paragraph
    }

    fn add_inline(&mut self, paragraph: Paragraph, node: NodeId, style: InlineStyle) -> Paragraph {
        match self.document.node(node) {
            SemanticNode::Highlight { children } => self.add_inline_children(
                paragraph,
                children,
                InlineStyle {
                    highlight: true,
                    ..style
                },
            ),
            SemanticNode::Element { tag, children } => match tag {
                Tag::CodeBlock(_) if self.document.diagram_source(node).is_some() => {
                    let source = self.document.diagram_source(node).expect("diagram source");
                    if let Some(raster) = self.diagram_rasters.get(&source.diagram_id) {
                        self.diagram_paragraph(
                            paragraph,
                            node,
                            raster,
                            if style.in_footnote {
                                MathPart::Footnotes
                            } else {
                                MathPart::Document
                            },
                        )
                    } else {
                        self.add_inline_children(paragraph, children, style)
                    }
                }
                tag if style.for_tag(tag).is_some() => self.add_inline_children(
                    paragraph,
                    children,
                    style.for_tag(tag).expect("style tag was recognized"),
                ),
                Tag::Link { .. } => {
                    let resolved = self
                        .document
                        .link_target(node)
                        .expect("link node must have a shared target")
                        .clone();
                    match resolved {
                        Ok(link) => {
                            let (kind, value) = match link {
                                ExportLink::Anchor(fragment) => {
                                    let Some(name) = self.heading_bookmarks.get(&fragment) else {
                                        self.resources.warnings_mut().push(ExportWarning::new(
                                            "DOCX_ANCHOR_NOT_FOUND",
                                            "DOCX 中找不到对应标题书签，已保留链接文字",
                                            Some(fragment),
                                        ));
                                        return paragraph.add_run(styled_run(
                                            Run::new().add_text(self.document.plain_text(children)),
                                            style,
                                        ));
                                    };
                                    (HyperlinkType::Anchor, name.clone())
                                }
                                ExportLink::External(url) => (HyperlinkType::External, url),
                                ExportLink::Email(address) => {
                                    (HyperlinkType::External, format!("mailto:{address}"))
                                }
                            };
                            let hyperlink = self.add_hyperlink_children(
                                Hyperlink::new(value, kind),
                                children,
                                style,
                            );
                            if style.in_footnote
                                && matches!(hyperlink.link, docx_rs::HyperlinkData::External { .. })
                            {
                                self.footnote_links.push(hyperlink.link.clone());
                            }
                            paragraph.add_hyperlink(hyperlink)
                        }
                        Err(warning) => {
                            self.resources.warnings_mut().push(warning);
                            self.add_inline_children(paragraph, children, style)
                        }
                    }
                }
                Tag::Image { dest_url, .. } => {
                    let alt = self.document.plain_text(children);
                    let content_width_emu = self.content_width_emu;
                    let prepared_image =
                        self.resources
                            .docx_image(dest_url)
                            .map(|(bytes, mime, path)| match mime {
                                "image/png" | "image/jpeg" => {
                                    Ok(fit_docx_picture_to_content_width(
                                        Pic::new(bytes),
                                        content_width_emu,
                                    ))
                                }
                                _ => Err(path.to_string()),
                            });
                    match prepared_image {
                        Some(Ok(image)) => {
                            let part = if style.in_footnote {
                                MathPart::Footnotes
                            } else {
                                MathPart::Document
                            };
                            let image = self.register_picture(image, part);
                            paragraph.add_run(Run::new().add_image(image))
                        }
                        Some(Err(path)) => {
                            self.resources.warnings_mut().push(ExportWarning::new(
                                "DOCX_IMAGE_FORMAT_DEGRADED",
                                "DOCX 当前只嵌入 PNG/JPEG；GIF/WebP 已保留替代文本",
                                Some(path),
                            ));
                            paragraph.add_run(styled_run(Run::new().add_text(alt), style))
                        }
                        None => paragraph.add_run(styled_run(Run::new().add_text(alt), style)),
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
            SemanticNode::Event(Event::SoftBreak) => {
                paragraph.add_run(styled_run(Run::new().add_text(" "), style))
            }
            SemanticNode::Event(Event::HardBreak) => {
                paragraph.add_run(Run::new().add_break(BreakType::TextWrapping))
            }
            SemanticNode::Event(Event::TaskListMarker(done)) => {
                paragraph.add_run(Run::new().add_text(if *done { "☒ " } else { "☐ " }))
            }
            SemanticNode::Event(Event::FootnoteReference(label)) => {
                if let Some(nodes) = self.footnotes.get(label.as_ref()).cloned() {
                    let content = self.inline_paragraph(
                        &nodes,
                        InlineStyle {
                            in_footnote: true,
                            ..InlineStyle::default()
                        },
                    );
                    let footnote = Footnote::new().add_content(content);
                    paragraph.add_run(Run::new().add_footnote_reference(footnote))
                } else {
                    self.resources.warnings_mut().push(ExportWarning::new(
                        "MISSING_FOOTNOTE_DEFINITION",
                        "脚注引用没有对应定义，已按文本导出",
                        Some(label.to_string()),
                    ));
                    paragraph.add_run(Run::new().add_text(format!("[^{label}]")))
                }
            }
            SemanticNode::Event(Event::InlineMath(value) | Event::DisplayMath(value)) => {
                self.math_count += 1;
                let rendered = if self.math_count > math_service::MAX_FORMULAS {
                    Err(math_service::MathRenderError {
                        code: "MATH_DOCUMENT_LIMIT_EXCEEDED",
                        message: format!(
                            "document contains more than {} formulas",
                            math_service::MAX_FORMULAS
                        ),
                    })
                } else {
                    math_service::render_omml(value.as_ref())
                };
                match rendered {
                    Ok(omml) => {
                        let placeholder = format!(
                            "{}{}END",
                            self.math_slot_prefix,
                            self.math_replacements.len()
                        );
                        self.math_replacements.push(MathReplacement {
                            part: if style.in_footnote {
                                MathPart::Footnotes
                            } else {
                                MathPart::Document
                            },
                            token: placeholder.clone(),
                            omml,
                        });
                        paragraph.add_run(styled_run(Run::new().add_text(placeholder), style))
                    }
                    Err(error) => {
                        self.resources.warnings_mut().push(ExportWarning::new(
                            error.code,
                            error.message,
                            Some(value.to_string()),
                        ));
                        paragraph.add_run(styled_run(Run::new().add_text(value.as_ref()), style))
                    }
                }
            }
            SemanticNode::Event(Event::Html(value) | Event::InlineHtml(value)) => {
                self.resources.warnings_mut().push(ExportWarning::new(
                    "RAW_HTML_DEGRADED",
                    "DOCX 不执行原始 HTML，已按纯文本导出",
                    None,
                ));
                paragraph.add_run(styled_run(Run::new().add_text(value.as_ref()), style))
            }
            SemanticNode::Event(Event::Rule) => paragraph.add_run(Run::new().add_text("────────")),
            SemanticNode::Event(
                Event::Start(_)
                | Event::End(_)
                | Event::StartHighlight
                | Event::EndHighlight
                | Event::Toc(_),
            ) => paragraph,
        }
    }

    fn add_inline_children(
        &mut self,
        mut paragraph: Paragraph,
        nodes: &[NodeId],
        style: InlineStyle,
    ) -> Paragraph {
        let mut pending = nodes
            .iter()
            .rev()
            .copied()
            .map(|node| (node, style))
            .collect::<Vec<_>>();
        while let Some((node, style)) = pending.pop() {
            match self.document.node(node) {
                SemanticNode::Highlight { children } => {
                    let nested = InlineStyle {
                        highlight: true,
                        ..style
                    };
                    pending.extend(children.iter().rev().copied().map(|node| (node, nested)));
                }
                SemanticNode::Element { tag, children } if style.for_tag(tag).is_some() => {
                    let nested = style.for_tag(tag).expect("style tag was recognized");
                    pending.extend(children.iter().rev().copied().map(|node| (node, nested)));
                }
                _ => paragraph = self.add_inline(paragraph, node, style),
            }
        }
        paragraph
    }

    fn add_hyperlink_children(
        &self,
        mut hyperlink: Hyperlink,
        nodes: &[NodeId],
        style: InlineStyle,
    ) -> Hyperlink {
        let mut pending = nodes
            .iter()
            .rev()
            .copied()
            .map(|node| (node, style))
            .collect::<Vec<_>>();
        while let Some((node, style)) = pending.pop() {
            match self.document.node(node) {
                SemanticNode::Highlight { children } => {
                    let nested = InlineStyle {
                        highlight: true,
                        ..style
                    };
                    pending.extend(children.iter().rev().copied().map(|node| (node, nested)));
                }
                SemanticNode::Element { tag, children } if style.for_tag(tag).is_some() => {
                    let nested = style.for_tag(tag).expect("style tag was recognized");
                    pending.extend(children.iter().rev().copied().map(|node| (node, nested)));
                }
                SemanticNode::Element { children, .. } => {
                    pending.extend(children.iter().rev().copied().map(|node| (node, style)))
                }
                SemanticNode::Event(Event::Text(value)) => {
                    hyperlink =
                        hyperlink.add_run(link_run(Run::new().add_text(value.as_ref()), style));
                }
                SemanticNode::Event(Event::Code(value)) => {
                    hyperlink = hyperlink.add_run(link_run(
                        Run::new()
                            .add_text(value.as_ref())
                            .fonts(docx_rs::RunFonts::new().ascii("Consolas"))
                            .shading(Shading::new().fill("F6F8FA")),
                        InlineStyle {
                            code: true,
                            ..style
                        },
                    ));
                }
                SemanticNode::Event(Event::SoftBreak) => {
                    hyperlink = hyperlink.add_run(link_run(Run::new().add_text(" "), style));
                }
                SemanticNode::Event(Event::HardBreak) => {
                    hyperlink = hyperlink.add_run(link_run(
                        Run::new().add_break(BreakType::TextWrapping),
                        style,
                    ));
                }
                SemanticNode::Event(event) => {
                    let text = event_text(event);
                    if !text.is_empty() {
                        hyperlink = hyperlink.add_run(link_run(Run::new().add_text(text), style));
                    }
                }
            }
        }
        hyperlink
    }

    fn push_paragraph(&mut self, paragraph: Paragraph) {
        self.docx = std::mem::take(&mut self.docx).add_paragraph(paragraph);
    }
}

fn math_slot_prefix(request: &ExportRequest, document: &SemanticDocument) -> String {
    let digest = Sha256::digest(request.snapshot.content.as_bytes());
    let seed = format!("MARKLITEOMML{digest:x}");
    for nonce in 0.. {
        let prefix = format!("{seed}_{nonce}_");
        if !request.snapshot.content.contains(&prefix)
            && !request.snapshot.title.contains(&prefix)
            && !semantic_contains(document, &prefix)
        {
            return prefix;
        }
    }
    unreachable!("a free formula slot prefix must exist")
}

fn semantic_contains(document: &SemanticDocument, needle: &str) -> bool {
    let mut pending = document.roots().iter().rev().copied().collect::<Vec<_>>();
    while let Some(id) = pending.pop() {
        match document.node(id) {
            SemanticNode::Element { children, .. } | SemanticNode::Highlight { children } => {
                pending.extend(children.iter().rev().copied());
            }
            SemanticNode::Event(event) if event_text(event).contains(needle) => return true,
            SemanticNode::Event(_) => {}
        }
    }
    false
}

fn inject_omml(
    bytes: Vec<u8>,
    replacements: &[MathReplacement],
    diagrams: &[DiagramPictureAlt],
    footnote_rels: Option<&str>,
) -> Result<Vec<u8>, AppError> {
    if replacements.is_empty() && diagrams.is_empty() && footnote_rels.is_none() {
        return Ok(bytes);
    }
    let mut source = ZipArchive::new(Cursor::new(bytes)).map_err(docx_postprocess_error)?;
    let mut output = Cursor::new(Vec::new());
    let mut seen_parts = [false; 2];
    {
        let mut destination = ZipWriter::new(&mut output);
        if let Some(rels) = footnote_rels {
            destination
                .start_file(
                    "word/_rels/footnotes.xml.rels",
                    SimpleFileOptions::default(),
                )
                .map_err(docx_postprocess_error)?;
            destination
                .write_all(rels.as_bytes())
                .map_err(docx_postprocess_error)?;
        }
        for index in 0..source.len() {
            let mut entry = source.by_index(index).map_err(docx_postprocess_error)?;
            let part = match entry.name() {
                "word/document.xml" => MathPart::Document,
                "word/footnotes.xml" => MathPart::Footnotes,
                _ => {
                    destination
                        .raw_copy_file(entry)
                        .map_err(docx_postprocess_error)?;
                    continue;
                }
            };
            let part_replacements = replacements
                .iter()
                .filter(|replacement| replacement.part == part)
                .collect::<Vec<_>>();
            let part_diagrams = diagrams
                .iter()
                .filter(|diagram| diagram.part == part)
                .collect::<Vec<_>>();
            if part_replacements.is_empty() && part_diagrams.is_empty() {
                destination
                    .raw_copy_file(entry)
                    .map_err(docx_postprocess_error)?;
                continue;
            }
            seen_parts[usize::from(part == MathPart::Footnotes)] = true;
            let options: SimpleFileOptions = entry.options();
            let name = entry.name().to_string();
            let mut part_xml = String::new();
            entry
                .read_to_string(&mut part_xml)
                .map_err(docx_postprocess_error)?;
            drop(entry);

            let part_xml = rewrite_math_part(&part_xml, part, &part_replacements)?;
            let part_xml = rewrite_diagram_alt(&part_xml, &part_diagrams)?;
            destination
                .start_file(name, options)
                .map_err(docx_postprocess_error)?;
            destination
                .write_all(part_xml.as_bytes())
                .map_err(docx_postprocess_error)?;
        }
        destination.finish().map_err(docx_postprocess_error)?;
    }
    for part in [MathPart::Document, MathPart::Footnotes] {
        if (replacements
            .iter()
            .any(|replacement| replacement.part == part)
            || diagrams.iter().any(|diagram| diagram.part == part))
            && !seen_parts[usize::from(part == MathPart::Footnotes)]
        {
            return Err(docx_postprocess_error(format!(
                "missing formula part: {}",
                part.zip_name()
            )));
        }
    }
    let bytes = output.into_inner();
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(docx_postprocess_error)?;
    for part in [MathPart::Document, MathPart::Footnotes] {
        if diagrams.iter().any(|diagram| diagram.part == part)
            || (part == MathPart::Footnotes && footnote_rels.is_some())
        {
            let mut xml = String::new();
            archive
                .by_name(part.zip_name())
                .map_err(docx_postprocess_error)?
                .read_to_string(&mut xml)
                .map_err(docx_postprocess_error)?;
            validate_image_relations(&mut archive, &xml, part)?;
        }
    }
    Ok(archive.into_inner().into_inner())
}

fn validate_image_relations(
    archive: &mut ZipArchive<Cursor<Vec<u8>>>,
    xml: &str,
    part: MathPart,
) -> Result<(), AppError> {
    let name = part.zip_name().strip_prefix("word/").expect("word part");
    let mut relationships = String::new();
    archive
        .by_name(&format!("word/_rels/{name}.rels"))
        .map_err(docx_postprocess_error)?
        .read_to_string(&mut relationships)
        .map_err(docx_postprocess_error)?;
    let mut images = HashMap::new();
    let mut reader = XmlReader::from_str(&relationships);
    loop {
        match reader.read_event().map_err(docx_postprocess_error)? {
            XmlEvent::Empty(event) if event.name().as_ref() == b"Relationship" => {
                let mut id = None;
                let mut target = None;
                let mut image = false;
                let mut external = false;
                for attribute in event.attributes() {
                    let attribute = attribute.map_err(docx_postprocess_error)?;
                    let value = attribute
                        .decoded_and_normalized_value(
                            quick_xml::XmlVersion::Implicit1_0,
                            reader.decoder(),
                        )
                        .map_err(docx_postprocess_error)?
                        .into_owned();
                    match attribute.key.as_ref() {
                        b"Id" => id = Some(value),
                        b"Target" => target = Some(value),
                        b"Type" => image = value == "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image",
                        b"TargetMode" => external = value == "External",
                        _ => {}
                    }
                }
                if image {
                    let id =
                        id.ok_or_else(|| docx_postprocess_error("image relationship missing ID"))?;
                    let target = target.ok_or_else(|| {
                        docx_postprocess_error("image relationship missing target")
                    })?;
                    // Writer-generated image relationships are local media, never remote URLs.
                    if external
                        || !target.starts_with("media/")
                        || target.contains("..")
                        || target.contains('\\')
                    {
                        return Err(docx_postprocess_error("invalid image relationship target"));
                    }
                    archive
                        .by_name(&format!("word/{target}"))
                        .map_err(docx_postprocess_error)?;
                    if images.insert(id, target).is_some() {
                        return Err(docx_postprocess_error("duplicate image relationship ID"));
                    }
                }
            }
            XmlEvent::Eof => break,
            _ => {}
        }
    }
    let mut reader = XmlReader::from_str(xml);
    loop {
        match reader.read_event().map_err(docx_postprocess_error)? {
            XmlEvent::Empty(event) if event.name().as_ref() == b"a:blip" => {
                for attribute in event.attributes() {
                    let attribute = attribute.map_err(docx_postprocess_error)?;
                    if attribute.key.as_ref() == b"r:embed" {
                        let id = attribute
                            .decoded_and_normalized_value(
                                quick_xml::XmlVersion::Implicit1_0,
                                reader.decoder(),
                            )
                            .map_err(docx_postprocess_error)?;
                        if !images.contains_key(id.as_ref()) {
                            return Err(docx_postprocess_error(
                                "image references missing relationship",
                            ));
                        }
                    }
                }
            }
            XmlEvent::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

fn rewrite_diagram_alt(xml: &str, diagrams: &[&DiagramPictureAlt]) -> Result<String, AppError> {
    if diagrams.is_empty() {
        return Ok(xml.to_string());
    }
    let lookup = diagrams
        .iter()
        .enumerate()
        .map(|(index, diagram)| (diagram.bookmark_id.to_string(), index))
        .collect::<HashMap<_, _>>();
    let mut seen = vec![0usize; diagrams.len()];
    let mut spans = Vec::new();
    let mut reader = XmlReader::from_str(xml);
    let mut drawing = false;
    let mut doc_pr = None;
    let mut diagram_index = None;
    let mut embed = false;
    loop {
        let start = reader.buffer_position() as usize;
        match reader.read_event().map_err(docx_postprocess_error)? {
            XmlEvent::Empty(event) if event.name().as_ref() == b"w:bookmarkStart" => {
                for attribute in event.attributes() {
                    let attribute = attribute.map_err(docx_postprocess_error)?;
                    if attribute.key.as_ref() == b"w:id" {
                        let id = std::str::from_utf8(attribute.value.as_ref())
                            .map_err(docx_postprocess_error)?;
                        if let Some(&index) = lookup.get(id) {
                            if diagram_index.replace(index).is_some() {
                                return Err(docx_postprocess_error(
                                    "overlapping diagram bookmarks",
                                ));
                            }
                        }
                    }
                }
            }
            XmlEvent::Empty(event) if event.name().as_ref() == b"w:bookmarkEnd" => {
                for attribute in event.attributes() {
                    let attribute = attribute.map_err(docx_postprocess_error)?;
                    if attribute.key.as_ref() == b"w:id" {
                        let id = std::str::from_utf8(attribute.value.as_ref())
                            .map_err(docx_postprocess_error)?;
                        if let Some(&index) = lookup.get(id) {
                            if diagram_index != Some(index) || drawing || seen[index] != 1 {
                                return Err(docx_postprocess_error(
                                    "invalid diagram bookmark range",
                                ));
                            }
                            diagram_index = None;
                        }
                    }
                }
            }
            XmlEvent::Start(event) if event.name().as_ref() == b"w:drawing" => {
                if drawing {
                    return Err(docx_postprocess_error("nested DOCX image drawing"));
                }
                drawing = true;
                doc_pr = None;
                embed = false;
            }
            XmlEvent::Empty(event) if drawing && event.name().as_ref() == b"wp:docPr" => {
                if doc_pr
                    .replace((start, reader.buffer_position() as usize))
                    .is_some()
                {
                    return Err(docx_postprocess_error("duplicate image description slot"));
                }
            }
            XmlEvent::Empty(event) if drawing && event.name().as_ref() == b"a:blip" => {
                for attribute in event.attributes() {
                    let attribute = attribute.map_err(docx_postprocess_error)?;
                    if attribute.key.as_ref() == b"r:embed" {
                        if embed || attribute.value.is_empty() {
                            return Err(docx_postprocess_error(
                                "invalid diagram image relationship",
                            ));
                        }
                        embed = true;
                    }
                }
            }
            XmlEvent::End(event) if event.name().as_ref() == b"w:drawing" => {
                if let Some(index) = diagram_index {
                    if !embed {
                        return Err(docx_postprocess_error("diagram image relationship missing"));
                    }
                    let (start, end) = doc_pr.ok_or_else(|| {
                        docx_postprocess_error("diagram image has no description slot")
                    })?;
                    seen[index] += 1;
                    spans.push((start, end, index));
                }
                drawing = false;
            }
            XmlEvent::Eof => break,
            _ => {}
        }
    }
    if drawing || diagram_index.is_some() || seen.iter().any(|count| *count != 1) {
        return Err(docx_postprocess_error(
            "diagram image relationship is missing or duplicated",
        ));
    }
    let mut rewritten = String::with_capacity(xml.len() + diagrams.len() * 96);
    let mut cursor = 0;
    for (start, end, index) in spans {
        let slot = &xml[start..end];
        let prefix = slot
            .strip_suffix("/>")
            .ok_or_else(|| docx_postprocess_error("unexpected image description slot"))?;
        rewritten.push_str(&xml[cursor..start]);
        rewritten.push_str(prefix);
        rewritten.push_str(" descr=\"");
        rewritten.push_str(&html_escape::encode_double_quoted_attribute(
            &diagrams[index].description,
        ));
        rewritten.push_str("\"/>");
        cursor = end;
    }
    rewritten.push_str(&xml[cursor..]);
    Ok(rewritten)
}

fn rewrite_math_part(
    xml: &str,
    part: MathPart,
    replacements: &[&MathReplacement],
) -> Result<String, AppError> {
    let token_index = replacements
        .iter()
        .enumerate()
        .map(|(index, replacement)| (replacement.token.as_bytes(), index))
        .collect::<HashMap<_, _>>();
    let mut counts = vec![0usize; replacements.len()];
    let mut spans = Vec::with_capacity(replacements.len());
    let mut reader = XmlReader::from_str(xml);
    let mut in_text = false;
    loop {
        let start = reader.buffer_position() as usize;
        match reader.read_event().map_err(docx_postprocess_error)? {
            XmlEvent::Start(event) if event.name().as_ref() == b"w:t" => in_text = true,
            XmlEvent::End(event) if event.name().as_ref() == b"w:t" => in_text = false,
            XmlEvent::Text(event) if in_text => {
                if let Some(&index) = token_index.get::<[u8]>(event.as_ref()) {
                    counts[index] += 1;
                    spans.push((start, reader.buffer_position() as usize, index));
                }
            }
            XmlEvent::Eof => break,
            _ => {}
        }
    }
    for (index, count) in counts.iter().enumerate() {
        if *count != 1 {
            return Err(docx_postprocess_error(format!(
                "formula slot in {} occurred {count} times: {}",
                part.zip_name(),
                replacements[index].token
            )));
        }
    }

    let mut rewritten = String::with_capacity(xml.len());
    let mut cursor = 0;
    for (start, end, index) in spans {
        rewritten.push_str(&xml[cursor..start]);
        rewritten.push_str("</w:t></w:r>");
        rewritten.push_str(&replacements[index].omml);
        rewritten.push_str("<w:r><w:t>");
        cursor = end;
    }
    rewritten.push_str(&xml[cursor..]);
    if !rewritten.contains("xmlns:m=") {
        let root = part.root_tag();
        if !rewritten.contains(root) {
            return Err(docx_postprocess_error(format!(
                "missing XML root in {}",
                part.zip_name()
            )));
        }
        rewritten = rewritten.replacen(
            root,
            &format!(
                "{root}xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" "
            ),
            1,
        );
    }
    let mut reader = XmlReader::from_str(&rewritten);
    loop {
        if reader.read_event().map_err(docx_postprocess_error)? == XmlEvent::Eof {
            break;
        }
    }
    Ok(rewritten)
}

fn docx_postprocess_error(error: impl std::fmt::Display) -> AppError {
    AppError::new("DOCX_EXPORT_FAILED", format!("写入可编辑公式失败：{error}"))
}

fn styled_run(mut run: Run, style: InlineStyle) -> Run {
    if style.highlight {
        run = run.highlight("yellow");
    }
    if style.bold {
        run = run.bold();
    }
    if style.italic {
        run = run.italic();
    }
    if style.strike {
        run = run.strike();
    }
    if let Some(vertical) = style.vertical {
        run.run_property = run.run_property.vert_align(vertical);
    }
    if style.code {
        run = run.fonts(docx_rs::RunFonts::new().ascii("Consolas"));
    }
    run
}

fn link_run(run: Run, style: InlineStyle) -> Run {
    styled_run(run, style).color("0969DA").underline("single")
}

fn alert_label(kind: BlockQuoteKind) -> &'static str {
    match kind {
        BlockQuoteKind::Note => "NOTE",
        BlockQuoteKind::Tip => "TIP",
        BlockQuoteKind::Important => "IMPORTANT",
        BlockQuoteKind::Warning => "WARNING",
        BlockQuoteKind::Caution => "CAUTION",
    }
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

fn docx_alignment(alignment: Alignment) -> AlignmentType {
    match alignment {
        Alignment::None | Alignment::Left => AlignmentType::Left,
        Alignment::Center => AlignmentType::Center,
        Alignment::Right => AlignmentType::Right,
    }
}

fn collect_heading_bookmarks(document: &SemanticDocument) -> HashMap<String, String> {
    let mut bookmarks = HashMap::new();
    let mut pending = document.roots().iter().rev().copied().collect::<Vec<_>>();
    while let Some(id) = pending.pop() {
        if let SemanticNode::Element { tag, children } = document.node(id) {
            if let Tag::Heading { id: Some(id), .. } = tag {
                if !bookmarks.contains_key(id.as_ref()) {
                    bookmarks.insert(id.to_string(), format!("marklite_{}", bookmarks.len() + 1));
                }
            }
            pending.extend(children.iter().rev().copied());
        }
    }
    bookmarks
}

fn add_base_numbering_definitions(mut docx: Docx) -> Docx {
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

fn collect_footnotes(document: &SemanticDocument) -> HashMap<String, Vec<NodeId>> {
    let mut footnotes = HashMap::new();
    let mut pending = document.roots().iter().rev().copied().collect::<Vec<_>>();
    while let Some(node) = pending.pop() {
        if let SemanticNode::Element { tag, children } = document.node(node) {
            if let Tag::FootnoteDefinition(label) = tag {
                footnotes.insert(label.to_string(), children.clone());
            }
            pending.extend(children.iter().rev().copied());
        }
    }
    footnotes
}

fn collect_table_rows(document: &SemanticDocument, nodes: &[NodeId]) -> Vec<Vec<Vec<NodeId>>> {
    let mut rows = Vec::new();
    let mut pending = nodes.iter().rev().copied().collect::<Vec<_>>();
    while let Some(node) = pending.pop() {
        if let SemanticNode::Element { tag, children } = document.node(node) {
            match tag {
                Tag::TableHead | Tag::TableRow => {
                    let cells = children
                        .iter()
                        .filter_map(|child| match document.node(*child) {
                            SemanticNode::Element {
                                tag: Tag::TableCell,
                                children,
                            } => Some(children.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    if !cells.is_empty() {
                        rows.push(cells);
                    }
                }
                _ => pending.extend(children.iter().rev().copied()),
            }
        }
    }
    rows
}

#[cfg(test)]
mod diagram_tests {
    use super::*;

    #[test]
    fn docx_semantic_scripts_preserve_bold_and_hyperlink_run_styles() {
        use crate::models::export::{ExportFormat, ExportOptions, ExportSnapshot};
        let content = "**x** [linked](https://example.com)";
        let mut document = SemanticDocument::parse(content, None);
        let SemanticNode::Element { children, .. } = document.node(document.roots()[0]) else {
            panic!("fixture starts with a paragraph")
        };
        let children = children.clone();
        let sup = document.append_fragment_node(SemanticNode::Element {
            tag: Tag::Superscript,
            children: children.clone(),
        });
        let sub = document.append_fragment_node(SemanticNode::Element {
            tag: Tag::Subscript,
            children,
        });
        let paragraph = document.append_fragment_node(SemanticNode::Element {
            tag: Tag::Paragraph,
            children: vec![sup, sub],
        });
        document.replace_roots(vec![paragraph]);
        let request = ExportRequest {
            snapshot: ExportSnapshot {
                job_id: "script-style".into(),
                tab_id: "test".into(),
                content_revision: 1,
                source_path: None,
                title: "Scripts".into(),
                content: content.into(),
            },
            target_path: "unused.docx".into(),
            target_kind: Default::default(),
            format: ExportFormat::Docx,
            options: ExportOptions {
                paper_size: ExportPaperSize::A4,
                orientation: ExportOrientation::Portrait,
                margin: ExportMarginPreset::Normal,
                include_title: false,
                include_local_images: false,
            },
            mind_map_svg: None,
        };
        let (bytes, warnings) = render(
            &request,
            &document,
            &HashMap::new(),
            &ExportReporter::silent("script-style", ExportFormat::Docx),
        )
        .unwrap();
        assert!(warnings.is_empty());
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut xml = String::new();
        archive
            .by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        assert!(xml.contains("w:val=\"superscript\""), "{xml}");
        assert!(xml.contains("w:val=\"subscript\""), "{xml}");
        for link in xml.split("<w:hyperlink").skip(1) {
            let body = link.split("</w:hyperlink>").next().unwrap();
            assert!(body.contains("<w:vertAlign"), "{body}");
        }
        assert_eq!(xml.matches("<w:hyperlink").count(), 2);
        assert!(xml.contains("<w:b"));
    }

    fn diagram() -> DiagramPictureAlt {
        DiagramPictureAlt {
            bookmark_id: 7,
            part: MathPart::Document,
            description: "A & B".into(),
        }
    }

    #[test]
    fn docx_table_diagram_instances_keep_descriptions_with_shared_media() {
        let mut docx = Docx::new();
        let diagrams = (1..=2)
            .map(|id| DiagramPictureAlt {
                bookmark_id: id,
                part: MathPart::Document,
                description: format!("table diagram {id}"),
            })
            .collect::<Vec<_>>();
        let cells = diagrams
            .iter()
            .map(|diagram| {
                TableCell::new().add_paragraph(
                    Paragraph::new()
                        .add_bookmark_start(
                            diagram.bookmark_id,
                            format!("diagram{}", diagram.bookmark_id),
                        )
                        .add_run(Run::new().add_image(Pic::new_with_dimensions(
                            vec![1, 2, 3],
                            1,
                            1,
                        )))
                        .add_bookmark_end(diagram.bookmark_id),
                )
            })
            .collect();
        docx = docx.add_table(Table::new(vec![TableRow::new(cells)]));
        let mut bytes = Cursor::new(Vec::new());
        docx.build().pack(&mut bytes).unwrap();
        let bytes = inject_omml(bytes.into_inner(), &[], &diagrams, None).unwrap();
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut xml = String::new();
        archive
            .by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        assert!(xml.contains("descr=\"table diagram 1\""));
        assert!(xml.contains("descr=\"table diagram 2\""));
        assert_eq!(
            archive
                .file_names()
                .filter(|name| name.starts_with("word/media/") && !name.ends_with('/'))
                .count(),
            1
        );
    }

    #[test]
    fn docx_image_relations_reject_missing_media_and_broken_targets() {
        let relation = r#"<Relationship Id="image1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>"#;
        for (relationships, media, embed, valid) in [
            (relation.to_string(), true, "image1", true),
            (relation.to_string(), false, "image1", false),
            (relation.to_string(), true, "missing", false),
            (relation.repeat(2), true, "image1", false),
            (
                relation.replace("media/image1.png", "../image1.png"),
                true,
                "image1",
                false,
            ),
            (
                relation.replace("/>", " TargetMode=\"External\"/>"),
                true,
                "image1",
                false,
            ),
        ] {
            let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
            writer
                .start_file("word/_rels/document.xml.rels", SimpleFileOptions::default())
                .unwrap();
            write!(writer, "<Relationships>{relationships}</Relationships>").unwrap();
            if media {
                writer
                    .start_file("word/media/image1.png", SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(b"image bytes").unwrap();
            }
            let mut archive = ZipArchive::new(writer.finish().unwrap()).unwrap();
            let xml = format!(r#"<a:blip r:embed="{embed}"/>"#);
            assert_eq!(
                validate_image_relations(&mut archive, &xml, MathPart::Document).is_ok(),
                valid
            );
        }
    }

    #[test]
    fn docx_diagram_slots_reject_missing_duplicate_and_malformed_drawings() {
        let diagram = diagram();
        let drawing = r#"<w:drawing><wp:docPr id="1"/><a:blip r:embed="rIdImage1"/></w:drawing>"#;
        let range = format!(r#"<w:bookmarkStart w:id="7"/>{drawing}<w:bookmarkEnd w:id="7"/>"#);
        let rewritten = rewrite_diagram_alt(&range, &[&diagram]).unwrap();
        assert!(rewritten.contains("descr=\"A &amp; B\""));
        for invalid in [
            drawing.to_string(),
            range.repeat(2),
            range.replace(drawing, ""),
            range.replace(drawing, &drawing.repeat(2)),
            range.replace("r:embed=\"rIdImage1\"", ""),
            range.replace("<wp:docPr id=\"1\"/>", ""),
            range.replace("<w:bookmarkEnd w:id=\"7\"/>", ""),
        ] {
            assert!(
                rewrite_diagram_alt(&invalid, &[&diagram]).is_err(),
                "{invalid}"
            );
        }
    }
}
