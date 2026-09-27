use lopdf::{dictionary, Document, Object};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, VecDeque},
    ops::Range,
    path::Path,
};
use tauri_plugin_marklite_mobile::{DrawPdfRequest, MarkliteMobileExt};

use crate::{
    models::{
        app_error::AppError,
        export::{
            ExportMarginPreset, ExportOrientation, ExportPaperSize, ExportRequest, ExportWarning,
        },
        resource::ResourceRef,
    },
    platform::android::{diagram, export::StagedArtifact, export_resources},
};

use super::{
    diagram_export_service::DiagramExportMode,
    export_control::CaptureControl,
    export_html_writer,
    export_progress::{ExportReporter, ExportStage, ExportWork, WorkKind},
    export_semantic::SemanticDocument,
    pdf_assembler::PdfAssembler,
    pdf_chunks,
};

pub(crate) async fn stage(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    source_resource: Option<&ResourceRef>,
    staged: &StagedArtifact,
    reporter: &ExportReporter,
    control: &CaptureControl,
) -> Result<Vec<ExportWarning>, AppError> {
    // Canvas PDF pages are rasterized. The 512 KiB pressure sample already
    // approaches the 512 MiB output budget; reject larger sources before
    // parsing or allocating a native surface.
    if request.snapshot.content.len() > 512 * 1024 {
        return Err(AppError::new(
            "PDF_SOURCE_BUDGET_EXCEEDED",
            "Android PDF 源文档超过 512 KiB 渲染预算，请缩短内容后导出",
        ));
    }
    reporter.phase(ExportStage::Parsing);
    control.check()?;
    let content = request.snapshot.content.clone();
    let mut document =
        tauri::async_runtime::spawn_blocking(move || SemanticDocument::parse(&content, None))
            .await
            .map_err(|_| AppError::new("EXPORT_TASK_FAILED", "Android PDF 解析任务异常结束"))?;
    pdf_chunks::split_large_text_blocks_with_limits(&mut document, pdf_chunks::Limits::ANDROID);
    let mut ranges: VecDeque<Range<usize>> =
        pdf_chunks::plan_with_limits(&document, pdf_chunks::Limits::ANDROID)?.into();
    let diagrams = diagram::prepare(app, &document, DiagramExportMode::Pdf, reporter).await?;
    let artifacts = if diagrams.print_artifacts.is_empty() {
        &diagrams.artifacts
    } else {
        &diagrams.print_artifacts
    };
    let mut resources = export_resources::resolver(
        app,
        &document,
        source_resource,
        request.options.include_local_images,
    )
    .await?;
    let mut assembler = PdfAssembler::create(staged.output())?;
    let paper_size = match request.options.paper_size {
        ExportPaperSize::A4 => "a4",
        ExportPaperSize::Letter => "letter",
    };
    let orientation = match request.options.orientation {
        ExportOrientation::Portrait => "portrait",
        ExportOrientation::Landscape => "landscape",
    };
    let margin = match request.options.margin {
        ExportMarginPreset::Narrow => "narrow",
        ExportMarginPreset::Normal => "normal",
        ExportMarginPreset::Wide => "wide",
    };
    let mut completed = 0u32;
    while let Some(range) = ranges.pop_front() {
        control.check()?;
        let total = completed + ranges.len() as u32 + 1;
        let work = ExportWork {
            kind: WorkKind::Part,
            index: completed + 1,
            total,
            completed,
        };
        reporter.processing(ExportStage::Rendering, work);
        let body = export_html_writer::render_roots_with_resources(
            &document,
            &document.roots()[range.clone()],
            request,
            artifacts,
            &mut resources,
            true,
        );
        let html = export_html_writer::standalone_with_title(
            request,
            &body,
            None,
            completed == 0 && request.options.include_title,
        );
        let path = staged.part_path();
        let path_string = path
            .to_str()
            .ok_or_else(|| AppError::new("EXPORT_TEMP_FAILED", "PDF 暂存路径不是 UTF-8"))?
            .to_owned();
        let app_for_draw = app.clone();
        reporter.processing(ExportStage::Printing, work);
        let result = tauri::async_runtime::spawn_blocking(move || {
            app_for_draw
                .marklite_mobile()
                .draw_export_pdf(DrawPdfRequest {
                    html: &html,
                    staged_path: &path_string,
                    paper_size,
                    orientation,
                    margin,
                })
                .map_err(|error| AppError::new("PDF_RENDER_FAILED", error.to_string()))
        })
        .await
        .map_err(|_| AppError::new("PDF_RENDER_FAILED", "Android PDF 渲染任务异常结束"))?;
        let result = match result {
            Ok(result) => result,
            Err(error) if error.message.contains("PDF_BATCH_HEIGHT_LIMIT") && range.len() > 1 => {
                let middle = range.start + range.len() / 2;
                ranges.push_front(middle..range.end);
                ranges.push_front(range.start..middle);
                continue;
            }
            Err(error) => {
                control.check()?;
                return Err(error);
            }
        };
        control.check()?;
        if result.pages == 0
            || result.pages > 16
            || result.bytes == 0
            || result.bytes > 64 * 1024 * 1024
            || std::fs::metadata(&path)
                .map_err(|error| AppError::new("PDF_INVALID_ARTIFACT", error.to_string()))?
                .len()
                != result.bytes
        {
            return Err(AppError::new(
                "PDF_INVALID_ARTIFACT",
                "Android PDF 批次元数据不匹配",
            ));
        }
        annotate(&path, &result.layout, request, result.pages)?;
        reporter.processing(
            ExportStage::Merging,
            ExportWork {
                completed: completed + 1,
                ..work
            },
        );
        assembler.append(&path)?;
        std::fs::remove_file(&path)
            .map_err(|error| AppError::new("EXPORT_TEMP_FAILED", error.to_string()))?;
        completed += 1;
    }
    reporter.phase(ExportStage::Merging);
    assembler.finish()?;
    control.check()?;
    let mut warnings = resources.into_warnings();
    warnings.extend(diagrams.warnings);
    Ok(warnings)
}

#[derive(Deserialize)]
struct Layout {
    links: Vec<Link>,
    anchors: Vec<Anchor>,
    #[serde(rename = "pageOffsets")]
    page_offsets: Vec<f32>,
}

#[derive(Deserialize)]
struct Link {
    href: String,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

#[derive(Deserialize)]
struct Anchor {
    id: String,
    top: f32,
}

fn annotate(
    path: &Path,
    layout_json: &str,
    request: &ExportRequest,
    page_count: u32,
) -> Result<(), AppError> {
    if layout_json.len() > 1024 * 1024 {
        return Err(AppError::new(
            "PDF_INVALID_ARTIFACT",
            "PDF 链接布局超过预算",
        ));
    }
    let layout: Layout = serde_json::from_str(layout_json)
        .map_err(|_| AppError::new("PDF_INVALID_ARTIFACT", "PDF 链接布局无效"))?;
    if layout.links.len() > 2048 || layout.anchors.len() > 2048 {
        return Err(AppError::new(
            "PDF_INVALID_ARTIFACT",
            "PDF 链接数量超过预算",
        ));
    }
    let mut doc = Document::load(path)
        .map_err(|error| AppError::new("PDF_INVALID_ARTIFACT", error.to_string()))?;
    let pages = doc.get_pages().values().copied().collect::<Vec<_>>();
    if pages.len() != page_count as usize {
        return Err(AppError::new("PDF_INVALID_ARTIFACT", "PDF 页数不匹配"));
    }
    if layout.page_offsets.len() != pages.len()
        || layout.page_offsets.first().copied() != Some(0.0)
        || layout
            .page_offsets
            .iter()
            .any(|offset| !offset.is_finite() || *offset < 0.0)
        || layout
            .page_offsets
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(AppError::new("PDF_INVALID_ARTIFACT", "PDF 分页坐标无效"));
    }
    let (mut width, mut height): (f32, f32) = match request.options.paper_size {
        ExportPaperSize::A4 => (595.0, 842.0),
        ExportPaperSize::Letter => (612.0, 792.0),
    };
    if request.options.orientation == ExportOrientation::Landscape {
        std::mem::swap(&mut width, &mut height);
    }
    let margin = match request.options.margin {
        ExportMarginPreset::Narrow => 36.0,
        ExportMarginPreset::Normal => 72.0,
        ExportMarginPreset::Wide => 108.0,
    };
    let scale = (width - 2.0 * margin) / 1200.0;
    let source_page_height = (height - 2.0 * margin) / scale;
    let mut names = BTreeMap::new();
    for anchor in layout.anchors {
        if anchor.id.is_empty()
            || anchor.id.len() > 256
            || !anchor.top.is_finite()
            || anchor.top < 0.0
        {
            continue;
        }
        let page = page_for_y(&layout.page_offsets, anchor.top);
        let Some(page_id) = pages.get(page) else {
            continue;
        };
        let y = height - margin - (anchor.top - layout.page_offsets[page]) * scale;
        names.entry(anchor.id).or_insert_with(|| {
            Object::Array(vec![
                Object::Reference(*page_id),
                Object::Name(b"XYZ".to_vec()),
                Object::Null,
                Object::Real(y),
                Object::Null,
            ])
        });
    }
    for link in layout.links {
        if link.href.len() > 4096
            || ![link.left, link.top, link.right, link.bottom]
                .iter()
                .all(|n| n.is_finite())
            || link.left < 0.0
            || link.top < 0.0
            || link.right <= link.left
            || link.bottom <= link.top
        {
            continue;
        }
        let valid_url = url::Url::parse(&link.href)
            .is_ok_and(|url| matches!(url.scheme(), "https" | "http" | "mailto"));
        if !valid_url {
            continue;
        }
        let page = page_for_y(&layout.page_offsets, link.top);
        let Some(page_id) = pages.get(page) else {
            continue;
        };
        let bottom = (link.bottom - layout.page_offsets[page]).min(source_page_height);
        let top = link.top - layout.page_offsets[page];
        let rect = vec![
            Object::Real(margin + link.left * scale),
            Object::Real(height - margin - bottom * scale),
            Object::Real(margin + link.right * scale),
            Object::Real(height - margin - top * scale),
        ];
        let annotation = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Link", "Rect" => rect,
            "Border" => vec![Object::Integer(0),Object::Integer(0),Object::Integer(0)],
            "A" => dictionary! { "S" => "URI", "URI" => Object::string_literal(link.href) },
        });
        let page_dict = doc
            .get_dictionary_mut(*page_id)
            .map_err(|error| AppError::new("PDF_INVALID_ARTIFACT", error.to_string()))?;
        match page_dict.get_mut(b"Annots") {
            Ok(Object::Array(items)) => items.push(Object::Reference(annotation)),
            Ok(_) => return Err(AppError::new("PDF_INVALID_ARTIFACT", "PDF 注释结构无效")),
            Err(_) => page_dict.set("Annots", vec![Object::Reference(annotation)]),
        }
    }
    if !names.is_empty() {
        let mut items = Vec::with_capacity(names.len() * 2);
        for (name, dest) in names {
            items.push(Object::string_literal(name));
            items.push(dest);
        }
        let dests = doc.add_object(dictionary! { "Names" => items });
        let names = doc.add_object(dictionary! { "Dests" => Object::Reference(dests) });
        doc.catalog_mut()
            .map_err(|error| AppError::new("PDF_INVALID_ARTIFACT", error.to_string()))?
            .set("Names", Object::Reference(names));
    }
    let updated = path.with_extension("annotated.pdf");
    doc.save(&updated)
        .map_err(|error| AppError::new("PDF_INVALID_ARTIFACT", error.to_string()))?;
    std::fs::rename(&updated, path)
        .map_err(|error| AppError::new("PDF_INVALID_ARTIFACT", error.to_string()))?;
    Ok(())
}

fn page_for_y(offsets: &[f32], y: f32) -> usize {
    offsets
        .partition_point(|offset| *offset <= y)
        .saturating_sub(1)
}
