use super::export_progress::{ExportReporter, ExportStage};
#[cfg(test)]
use crate::models::export::ExportWarning;
use crate::{
    models::{
        app_error::AppError,
        export::{ExportRequest, ExportResult},
    },
    services::{
        diagram_export_service::{self, DiagramExportMode, PreparedDiagrams},
        export_core, export_docx_writer, export_html_writer,
        export_semantic::SemanticDocument,
        mind_map_svg,
    },
};

pub use export_core::ExportCommitPolicy;

#[cfg(test)]
const MAX_PDF_READY_TOKEN_BYTES: usize = 128;

#[cfg(test)]
#[derive(Debug, Clone)]
pub struct PreparedPdf {
    pub html: String,
    pub warnings: Vec<ExportWarning>,
}

#[cfg(test)]
pub fn export_html(request: &ExportRequest) -> Result<ExportResult, AppError> {
    export_html_with_policy(request, ExportCommitPolicy::Replace)
}

pub async fn export_html_with_runtime(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    isolate_profile: bool,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Parsing);
    let document = SemanticDocument::parse(
        &request.snapshot.content,
        request.snapshot.source_path.as_deref(),
    );
    export_html_with_runtime_document(
        app,
        request,
        policy,
        isolate_profile,
        &document,
        || None,
        reporter,
    )
    .await
}

pub(crate) async fn export_html_with_runtime_document(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    isolate_profile: bool,
    document: &SemanticDocument,
    cancelled: impl Fn() -> Option<AppError> + Send + Sync,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Validating);
    let target = export_core::validate_request(request)?;
    export_core::preflight_commit(request, policy)?;
    reporter.phase(ExportStage::Resources);
    if document.diagram_sources().is_empty() {
        return export_html_from_document_controlled(
            request,
            policy,
            document,
            PreparedDiagrams::empty(),
            || None,
            reporter,
        );
    }
    let prepared = diagram_export_service::prepare(
        app,
        document,
        &target,
        isolate_profile,
        None,
        &cancelled,
        DiagramExportMode::Html,
    )
    .await?;
    export_html_from_document_controlled(request, policy, document, prepared, cancelled, reporter)
}

pub(crate) fn export_html_with_document(
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    document: &SemanticDocument,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Validating);
    export_core::validate_request(request)?;
    reporter.phase(ExportStage::Resources);
    export_html_from_document_controlled(
        request,
        policy,
        document,
        PreparedDiagrams::empty(),
        || None,
        reporter,
    )
}

#[cfg(test)]
pub fn export_html_with_policy(
    request: &ExportRequest,
    policy: ExportCommitPolicy,
) -> Result<ExportResult, AppError> {
    export_html_with_prepared(request, policy, PreparedDiagrams::empty())
}

#[cfg(test)]
pub(crate) fn export_html_with_prepared(
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    prepared: PreparedDiagrams,
) -> Result<ExportResult, AppError> {
    export_core::validate_request(request)?;
    let document = SemanticDocument::parse(
        &request.snapshot.content,
        request.snapshot.source_path.as_deref(),
    );
    export_html_from_document(request, policy, &document, prepared)
}

#[cfg(test)]
fn export_html_from_document(
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    document: &SemanticDocument,
    prepared: PreparedDiagrams,
) -> Result<ExportResult, AppError> {
    export_html_from_document_controlled(
        request,
        policy,
        document,
        prepared,
        || None,
        &ExportReporter::silent(&request.snapshot.job_id, request.format),
    )
}

fn export_html_from_document_controlled(
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    document: &SemanticDocument,
    prepared: PreparedDiagrams,
    cancelled: impl Fn() -> Option<AppError>,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Rendering);
    let (body, mut warnings) =
        export_html_writer::render_body_with_diagrams(document, request, &prepared.artifacts);
    warnings.extend(prepared.warnings);
    let html = export_html_writer::standalone(request, &body, None);
    if let Some(error) = cancelled() {
        return Err(error);
    }
    reporter.phase(ExportStage::Committing);
    export_core::commit(request, html.as_bytes(), policy)?;
    reporter.phase(ExportStage::CleaningUp);
    Ok(export_core::result(request, warnings))
}

#[cfg(test)]
pub fn export_docx(request: &ExportRequest) -> Result<ExportResult, AppError> {
    export_docx_with_policy(request, ExportCommitPolicy::Replace)
}

pub async fn export_docx_with_runtime(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    isolate_profile: bool,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Parsing);
    let document = SemanticDocument::parse(
        &request.snapshot.content,
        request.snapshot.source_path.as_deref(),
    );
    export_docx_with_runtime_document(
        app,
        request,
        policy,
        isolate_profile,
        &document,
        || None,
        reporter,
    )
    .await
}

pub(crate) async fn export_docx_with_runtime_document(
    app: &tauri::AppHandle,
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    isolate_profile: bool,
    document: &SemanticDocument,
    cancelled: impl Fn() -> Option<AppError> + Send + Sync,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Validating);
    let target = export_core::validate_request(request)?;
    export_core::preflight_commit(request, policy)?;
    reporter.phase(ExportStage::Resources);
    if document.diagram_sources().is_empty() {
        return export_docx_from_document_controlled(
            request,
            policy,
            document,
            PreparedDiagrams::empty(),
            || None,
            reporter,
        );
    }
    let prepared = diagram_export_service::prepare(
        app,
        document,
        &target,
        isolate_profile,
        None,
        &cancelled,
        DiagramExportMode::Docx,
    )
    .await?;
    export_docx_from_document_controlled(request, policy, document, prepared, cancelled, reporter)
}

pub(crate) fn export_docx_with_document(
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    document: &SemanticDocument,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Validating);
    export_core::validate_request(request)?;
    reporter.phase(ExportStage::Resources);
    export_docx_from_document_controlled(
        request,
        policy,
        document,
        PreparedDiagrams::empty(),
        || None,
        reporter,
    )
}

#[cfg(test)]
pub fn export_docx_with_policy(
    request: &ExportRequest,
    policy: ExportCommitPolicy,
) -> Result<ExportResult, AppError> {
    export_core::validate_request(request)?;
    let document = SemanticDocument::parse(
        &request.snapshot.content,
        request.snapshot.source_path.as_deref(),
    );
    export_docx_from_document(request, policy, &document, PreparedDiagrams::empty())
}

#[cfg(test)]
fn export_docx_from_document(
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    document: &SemanticDocument,
    prepared: PreparedDiagrams,
) -> Result<ExportResult, AppError> {
    export_docx_from_document_controlled(
        request,
        policy,
        document,
        prepared,
        || None,
        &ExportReporter::silent(&request.snapshot.job_id, request.format),
    )
}

fn export_docx_from_document_controlled(
    request: &ExportRequest,
    policy: ExportCommitPolicy,
    document: &SemanticDocument,
    prepared: PreparedDiagrams,
    cancelled: impl Fn() -> Option<AppError>,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Rendering);
    let (bytes, mut warnings) =
        export_docx_writer::render(request, document, &prepared.rasters, reporter)?;
    warnings.extend(prepared.warnings);
    if let Some(error) = cancelled() {
        return Err(error);
    }
    reporter.phase(ExportStage::Committing);
    export_core::commit(request, &bytes, policy)?;
    reporter.phase(ExportStage::CleaningUp);
    Ok(export_core::result(request, warnings))
}

#[cfg(test)]
fn export_docx_with_prepared(
    request: &ExportRequest,
    prepared: PreparedDiagrams,
) -> Result<ExportResult, AppError> {
    export_core::validate_request(request)?;
    let document = SemanticDocument::parse(
        &request.snapshot.content,
        request.snapshot.source_path.as_deref(),
    );
    export_docx_from_document(request, ExportCommitPolicy::Replace, &document, prepared)
}

#[cfg(test)]
pub fn export_svg(request: &ExportRequest) -> Result<ExportResult, AppError> {
    export_svg_reported(
        request,
        &ExportReporter::silent(&request.snapshot.job_id, request.format),
    )
}

pub fn export_svg_reported(
    request: &ExportRequest,
    reporter: &ExportReporter,
) -> Result<ExportResult, AppError> {
    reporter.phase(ExportStage::Validating);
    export_core::validate_request(request)?;
    let svg = request.mind_map_svg.as_deref().ok_or_else(|| {
        AppError::new("INVALID_MIND_MAP_SVG", "脑图 SVG 导出请求缺少矢量画布内容")
    })?;
    mind_map_svg::validate(svg)?;
    reporter.phase(ExportStage::Committing);
    export_core::commit(request, svg.as_bytes(), ExportCommitPolicy::Replace)?;
    reporter.phase(ExportStage::CleaningUp);
    Ok(export_core::result(request, Vec::new()))
}

#[cfg(test)]
pub fn prepare_pdf(request: &ExportRequest, ready_token: &str) -> Result<PreparedPdf, AppError> {
    prepare_pdf_with_diagrams(request, ready_token, PreparedDiagrams::empty())
}

#[cfg(test)]
pub(crate) fn prepare_pdf_with_diagrams(
    request: &ExportRequest,
    ready_token: &str,
    prepared: PreparedDiagrams,
) -> Result<PreparedPdf, AppError> {
    export_core::validate_request(request)?;
    let document = SemanticDocument::parse(
        &request.snapshot.content,
        request.snapshot.source_path.as_deref(),
    );
    prepare_pdf_from_document(request, ready_token, &document, prepared)
}

#[cfg(test)]
pub(crate) fn prepare_pdf_from_document(
    request: &ExportRequest,
    ready_token: &str,
    document: &SemanticDocument,
    prepared: PreparedDiagrams,
) -> Result<PreparedPdf, AppError> {
    export_core::validate_request(request)?;
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
    let diagrams = if prepared.print_artifacts.is_empty() {
        &prepared.artifacts
    } else {
        &prepared.print_artifacts
    };
    let (body, mut warnings) =
        export_html_writer::render_body_with_diagrams(document, request, diagrams);
    warnings.extend(prepared.warnings);
    Ok(PreparedPdf {
        html: export_html_writer::standalone(request, &body, Some(ready_token)),
        warnings,
    })
}

pub fn validate_request(request: &ExportRequest) -> Result<std::path::PathBuf, AppError> {
    export_core::validate_request(request)
}

pub fn preflight_commit(
    request: &ExportRequest,
    policy: ExportCommitPolicy,
) -> Result<(), AppError> {
    export_core::preflight_commit(request, policy)
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Read, path::Path};

    use base64::{engine::general_purpose::STANDARD, Engine};
    use docx_rs::{read_docx, Pic};

    use super::{
        export_docx, export_docx_with_prepared, export_html, export_html_with_prepared, export_svg,
        prepare_pdf, prepare_pdf_with_diagrams, validate_request,
    };
    use crate::services::export_docx_writer::{fit_docx_picture_to_content_width, DocxPageLayout};
    use crate::utils::test_support::TestPath;
    use crate::{
        models::diagram::RenderedDiagram,
        services::{
            diagram_export_service::{PreparedDiagrams, RasterDiagram},
            export_semantic::SemanticDocument,
        },
    };
    use std::collections::HashMap;

    fn unique_path(extension: &str) -> TestPath {
        TestPath::new("export-service", format!("output.{extension}"))
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
            target_kind: Default::default(),
            format,
            options: ExportOptions {
                paper_size: ExportPaperSize::A4,
                orientation: ExportOrientation::Portrait,
                margin: ExportMarginPreset::Normal,
                include_title: true,
                include_local_images: false,
            },
            mind_map_svg: None,
        }
    }

    #[test]
    fn validated_diagram_is_shared_by_html_and_pdf_and_missing_artifact_keeps_source() {
        let markdown = "Before\n\n```mermaid\nflowchart TD\nA-->B\n```\n\nAfter";
        let html_path = unique_path("html");
        let html_request = request(ExportFormat::Html, &html_path, markdown);
        let source = SemanticDocument::parse(markdown, None)
            .diagram_sources()
            .remove(0);
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 50\"><text>GraphReady</text></svg>";
        let artifact = RenderedDiagram {
            diagram_id: source.diagram_id.clone(),
            source_sha256: source.source_sha256,
            cache_key: "a".repeat(64),
            renderer_id: "mermaid-offline-11.17.2".into(),
            svg_utf8: svg.into(),
            width: 100.0,
            height: 50.0,
            view_box: [0.0, 0.0, 100.0, 50.0],
            accessible_title: Some("GraphReady".into()),
            accessible_description: None,
            warnings: Vec::new(),
        };
        let diagrams = || PreparedDiagrams {
            artifacts: HashMap::from([(artifact.diagram_id.clone(), artifact.clone())]),
            print_artifacts: HashMap::new(),
            rasters: HashMap::new(),
            warnings: Vec::new(),
        };
        export_html_with_prepared(
            &html_request,
            super::ExportCommitPolicy::Replace,
            diagrams(),
        )
        .unwrap();
        let html = fs::read_to_string(&html_path).unwrap();
        assert!(html.contains("<svg ") && html.contains("GraphReady"));
        assert!(!html.contains("A--&gt;B"));
        assert!(html.contains("script-src 'none'"));

        let pdf_request = request(ExportFormat::Pdf, &unique_path("pdf"), markdown);
        let pdf = prepare_pdf_with_diagrams(&pdf_request, "diagram-ready", diagrams()).unwrap();
        assert!(pdf.html.contains(svg));
        assert!(pdf.html.contains("marklite-export"));
        let fallback = prepare_pdf(&pdf_request, "diagram-fallback").unwrap();
        assert!(fallback.html.contains("A--&gt;B"));
        assert!(!fallback.html.contains("GraphReady"));
    }

    #[test]
    fn docx_embeds_diagram_png_with_relationship_and_source_description() {
        let markdown = "```mermaid\nflowchart TD\nA-->B\n```";
        let source = SemanticDocument::parse(markdown, None)
            .diagram_sources()
            .remove(0);
        let png = STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=").unwrap();
        let prepared = PreparedDiagrams {
            artifacts: HashMap::new(),
            print_artifacts: HashMap::new(),
            rasters: HashMap::from([(
                source.diagram_id,
                RasterDiagram {
                    png: png.clone(),
                    width: 1,
                    height: 1,
                },
            )]),
            warnings: Vec::new(),
        };
        let path = unique_path("docx");
        let result =
            export_docx_with_prepared(&request(ExportFormat::Docx, &path, markdown), prepared)
                .unwrap();
        assert!(result.warnings.is_empty());
        let bytes = fs::read(&path).unwrap();
        let xml = docx_zip_entry(&bytes, "word/document.xml");
        let rels = docx_zip_entry(&bytes, "word/_rels/document.xml.rels");
        assert!(xml.contains("descr=\"Mermaid 图表，源码字节"));
        assert!(xml.contains("r:embed=\"rIdImage"));
        assert!(rels.contains(
            "Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\""
        ));
        assert!(!xml.contains("A--&gt;B"));
        let image_id = xml
            .split("r:embed=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        assert!(rels.contains(&format!("Id=\"{image_id}\"")));
        assert!(rels.contains(&format!("Target=\"media/{image_id}.png\"")));
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut picture = Vec::new();
        archive
            .by_name(&format!("word/media/{image_id}.png"))
            .unwrap()
            .read_to_end(&mut picture)
            .unwrap();
        assert_eq!(picture, png);
    }

    fn docx_zip_entry(bytes: &[u8], name: &str) -> String {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut content = String::new();
        archive
            .by_name(name)
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        content
    }

    fn first_picture_size(value: &serde_json::Value) -> Option<(u64, u64)> {
        if value.get("type").and_then(serde_json::Value::as_str) == Some("pic") {
            let size = value.pointer("/data/size")?.as_array()?;
            return Some((size.first()?.as_u64()?, size.get(1)?.as_u64()?));
        }
        match value {
            serde_json::Value::Array(values) => values.iter().find_map(first_picture_size),
            serde_json::Value::Object(values) => values.values().find_map(first_picture_size),
            _ => None,
        }
    }

    fn append_docx_text(value: &serde_json::Value, output: &mut String) {
        if value.get("type").and_then(serde_json::Value::as_str) == Some("text") {
            if let Some(text) = value
                .pointer("/data/text")
                .and_then(serde_json::Value::as_str)
            {
                output.push_str(text);
            }
        }
        if let Some(children) = value
            .pointer("/data/children")
            .and_then(serde_json::Value::as_array)
        {
            for child in children {
                append_docx_text(child, output);
            }
        }
    }

    fn docx_paragraphs(value: &serde_json::Value) -> Vec<(&serde_json::Value, String)> {
        value
            .pointer("/document/children")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|child| {
                child.get("type").and_then(serde_json::Value::as_str) == Some("paragraph")
            })
            .map(|paragraph| {
                let mut text = String::new();
                append_docx_text(paragraph, &mut text);
                (paragraph, text)
            })
            .collect()
    }

    fn docx_run_for_text<'a>(
        value: &'a serde_json::Value,
        expected: &str,
    ) -> Option<&'a serde_json::Value> {
        if value.get("type").and_then(serde_json::Value::as_str) == Some("run") {
            let mut text = String::new();
            append_docx_text(value, &mut text);
            if text == expected {
                return Some(value);
            }
        }
        value
            .pointer("/data/children")
            .and_then(serde_json::Value::as_array)?
            .iter()
            .find_map(|child| docx_run_for_text(child, expected))
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
    }

    #[test]
    fn exports_sanitized_standalone_html_without_remote_image_fetches() {
        let path = unique_path("html");
        let mut request = request(
            ExportFormat::Html,
            &path,
            "# Heading\n\n<script>alert(1)</script>\n\n![remote](https://example.com/x.png)\n\n<img src=\"https://raw.example/a.png\" srcset=\"local.png 1x, https://raw.example/b.png 2x\" alt=\"raw\">",
        );
        let result = export_html(&request).unwrap();
        let html = fs::read_to_string(&path).unwrap();
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("default-src 'none'; img-src data:"));
        assert!(html.contains("script-src 'none'"));
        assert!(html.contains("<h1 id=\"heading\">Heading</h1>"));
        assert!(!html.contains("<script>alert"));
        assert!(!html.contains("https://example.com/x.png"));
        assert!(!html.contains("raw.example"));
        assert!(!html.contains("local.png"));
        assert!(!html.contains("<img alt=\"raw\""));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.code == "REMOTE_IMAGE_SKIPPED"));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.code == "RAW_HTML_IMAGE_SKIPPED"));

        request.format = ExportFormat::Pdf;
        request.target_path = unique_path("pdf").to_string_lossy().into_owned();
        let prepared = prepare_pdf(&request, "raw-image-policy").unwrap();
        assert!(prepared.html.contains("script-src 'unsafe-inline'"));
        assert!(!prepared.html.contains("raw.example"));
        assert!(!prepared.html.contains("local.png"));
        assert!(prepared
            .warnings
            .iter()
            .any(|warning| warning.code == "RAW_HTML_IMAGE_SKIPPED"));
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
        let mut html_request = request(
            ExportFormat::Html,
            &output,
            "# 中文 😀\n\n![像素](像素.png)\n\n[site](https://example.com)",
        );
        html_request.snapshot.source_path = Some(source_path.to_string_lossy().into());
        html_request.options.include_local_images = true;

        let result = export_html(&html_request).unwrap();
        let html = fs::read_to_string(&output).unwrap();
        assert!(
            html.contains("data:image/png;base64,"),
            "warnings: {:?}",
            result.warnings
        );
        assert!(!html.contains(&image_path.to_string_lossy().to_string()));
        assert!(html.contains("https://example.com"));
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn inline_html_keeps_wrapped_text_after_whole_document_sanitization() {
        let markdown = "Before <em>emphasis</em> and <a href=\"https://example.org/help\">help</a>.\n\n<details><summary>More</summary>Safe text</details>";
        let path = unique_path("html");
        export_html(&request(ExportFormat::Html, &path, markdown)).unwrap();
        let html = fs::read_to_string(path).unwrap();
        assert!(html.contains("<em>emphasis</em>"), "{html}");
        assert!(
            html.contains("<a href=\"https://example.org/help\""),
            "{html}"
        );
        assert!(html.contains(">help</a>"), "{html}");
        assert!(html.contains("<details><summary>More</summary>Safe text</details>"));

        let pdf_path = unique_path("pdf");
        let prepared = prepare_pdf(
            &request(ExportFormat::Pdf, &pdf_path, markdown),
            "inline-ready",
        )
        .unwrap();
        assert!(prepared.html.contains("<em>emphasis</em>"));
        assert!(prepared.html.contains(">help</a>"));
        assert!(prepared
            .html
            .contains("<details><summary>More</summary>Safe text</details>"));
    }

    #[test]
    fn inline_html_security_attributes_and_raw_images_stay_filtered() {
        let markdown = "<em onclick=\"alert(1)\">safe</em> <a href=\"javascript:alert(1)\">blocked</a> <img src=\"https://remote.example/p.png\" onerror=\"bad()\"> <script>alert(2)</script>";
        let path = unique_path("html");
        let result = export_html(&request(ExportFormat::Html, &path, markdown)).unwrap();
        let html = fs::read_to_string(path).unwrap();
        assert!(html.contains("<em>safe</em>"), "{html}");
        assert!(!html.contains("onclick="));
        assert!(html.contains("blocked</a>"));
        assert!(!html.contains("javascript:alert"));
        assert!(!html.contains("onerror="));
        assert!(!html.contains("remote.example"));
        assert!(!html.contains("<script>alert(2)</script>"));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.code == "RAW_HTML_IMAGE_SKIPPED"));
    }

    #[test]
    fn repeats_one_cached_image_without_duplicate_warnings_or_full_html_replace_loops() {
        let directory = unique_path("repeated-resource");
        fs::create_dir(&directory).unwrap();
        let image_bytes = STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        fs::write(directory.join("pixel.png"), image_bytes).unwrap();
        let source = directory.join("fixture.md");
        fs::write(&source, b"fixture").unwrap();
        let output = directory.join("repeated.html");
        let markdown = "![pixel](pixel.png)\n\n".repeat(1_000);
        let mut request = request(ExportFormat::Html, &output, &markdown);
        request.snapshot.source_path = Some(source.to_string_lossy().into_owned());
        request.options.include_local_images = true;

        let result = export_html(&request).unwrap();
        let exported = fs::read_to_string(&output).unwrap();
        assert_eq!(exported.matches("data:image/png;base64,").count(), 1_000);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
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
    }

    #[test]
    fn exports_shared_math_subset_to_html_pdf_and_editable_docx() {
        let markdown = concat!(
            "Inline $x^2 + \\alpha$\n\n",
            "$$\\frac{1}{2}$$\n\n",
            "```math\n\\sqrt{x}\n```"
        );
        let html_path = unique_path("html");
        let html_request = request(ExportFormat::Html, &html_path, markdown);
        let html_result = export_html(&html_request).unwrap();
        let html = fs::read_to_string(&html_path).unwrap();
        assert_eq!(html.matches("<math").count(), 3, "{html}");
        assert!(html.contains("<mfrac>"));
        assert!(
            html_result.warnings.is_empty(),
            "{:?}",
            html_result.warnings
        );

        let pdf_path = unique_path("pdf");
        let pdf_request = request(ExportFormat::Pdf, &pdf_path, markdown);
        let prepared = prepare_pdf(&pdf_request, "math-ready").unwrap();
        assert_eq!(prepared.html.matches("<math").count(), 3);
        assert!(prepared.html.contains("<msqrt>"));
        assert!(prepared.warnings.is_empty(), "{:?}", prepared.warnings);

        let docx_path = unique_path("docx");
        let docx_request = request(ExportFormat::Docx, &docx_path, markdown);
        let docx_result = export_docx(&docx_request).unwrap();
        let document_xml = docx_zip_entry(&fs::read(&docx_path).unwrap(), "word/document.xml");
        assert_eq!(
            document_xml.matches("<m:oMath>").count(),
            3,
            "{document_xml}"
        );
        assert!(document_xml
            .contains("xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\""));
        assert!(document_xml.contains("<m:f>"), "{document_xml}");
        assert!(!document_xml.contains("MARKLITEOMML"));
        assert!(read_docx(&fs::read(&docx_path).unwrap()).is_ok());
        assert!(
            docx_result.warnings.is_empty(),
            "{:?}",
            docx_result.warnings
        );
    }

    #[test]
    fn exports_named_math_families_in_body_table_and_footnote_across_formats() {
        let markdown = concat!(
            "Inline $x_i^2 + \\alpha + \\Omega$\n\n",
            "$$\\frac{a}{b} + \\sqrt{x}$$\n\n",
            "$$\\sum_{i=1}^{n} i + \\int_0^1 x\\,dx$$\n\n",
            "```math\n\\begin{pmatrix}a & b \\\\ c & d\\end{pmatrix}\n```\n\n",
            "```math\n\\begin{align}a &= b + c \\\\ d &= e\\end{align}\n```\n\n",
            "| Formula |\n| --- |\n| $\\sqrt{z}$ |\n\n",
            "Footnote[^math]\n\n[^math]: $\\sum_{k=1}^{m} k$",
        );
        let html_path = unique_path("html");
        let html_result = export_html(&request(ExportFormat::Html, &html_path, markdown)).unwrap();
        let html = fs::read_to_string(&html_path).unwrap();
        assert_eq!(html.matches("<math").count(), 7, "{html}");
        for marker in ["<mfrac>", "<msqrt>", "<munderover>", "<mtable"] {
            assert!(html.contains(marker), "missing {marker}: {html}");
        }
        assert!(
            html_result.warnings.is_empty(),
            "{:?}",
            html_result.warnings
        );

        let pdf_path = unique_path("pdf");
        let prepared = prepare_pdf(
            &request(ExportFormat::Pdf, &pdf_path, markdown),
            "named-math-ready",
        )
        .unwrap();
        assert_eq!(prepared.html.matches("<math").count(), 7);
        assert!(prepared.html.contains("<mtable"));
        assert!(prepared.warnings.is_empty(), "{:?}", prepared.warnings);

        let docx_path = unique_path("docx");
        let docx_result = export_docx(&request(ExportFormat::Docx, &docx_path, markdown)).unwrap();
        let bytes = fs::read(docx_path).unwrap();
        let document_xml = docx_zip_entry(&bytes, "word/document.xml");
        let footnotes_xml = docx_zip_entry(&bytes, "word/footnotes.xml");
        assert_eq!(
            document_xml.matches("<m:oMath>").count() + footnotes_xml.matches("<m:oMath>").count(),
            7
        );
        for marker in ["<m:f>", "<m:rad>", "<m:nary>", "<m:m>"] {
            assert!(
                document_xml.contains(marker) || footnotes_xml.contains(marker),
                "missing {marker}"
            );
        }
        assert!(
            docx_result.warnings.is_empty(),
            "{:?}",
            docx_result.warnings
        );
        assert!(read_docx(&bytes).is_ok());
    }

    #[test]
    fn docx_math_slots_cover_document_table_and_footnote_parts() {
        let markdown = "Body $x$ and again $x$ note[^n]\n\n| Formula |\n| --- |\n| $z$ |\n\n[^n]: Foot $y$ and $y+1$";
        let path = unique_path("docx");
        let result = export_docx(&request(ExportFormat::Docx, &path, markdown)).unwrap();
        let bytes = fs::read(path).unwrap();
        let document_xml = docx_zip_entry(&bytes, "word/document.xml");
        let footnotes_xml = docx_zip_entry(&bytes, "word/footnotes.xml");
        assert_eq!(document_xml.matches("<m:oMath>").count(), 3);
        assert_eq!(footnotes_xml.matches("<m:oMath>").count(), 2);
        assert!(document_xml
            .contains("xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\""));
        assert!(footnotes_xml
            .contains("xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\""));
        assert!(!document_xml.contains("MARKLITEOMML"));
        assert!(!footnotes_xml.contains("MARKLITEOMML"));
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert!(read_docx(&bytes).is_ok());
    }

    #[test]
    fn docx_math_slots_cannot_collide_with_user_text() {
        let markdown = "MARKLITEOMML0END $x$ note[^n]\n\n[^n]: MARKLITEOMML1END $y$";
        let path = unique_path("docx");
        export_docx(&request(ExportFormat::Docx, &path, markdown)).unwrap();
        let bytes = fs::read(path).unwrap();
        let document_xml = docx_zip_entry(&bytes, "word/document.xml");
        let footnotes_xml = docx_zip_entry(&bytes, "word/footnotes.xml");
        assert!(document_xml.contains("MARKLITEOMML0END"));
        assert!(footnotes_xml.contains("MARKLITEOMML1END"));
        assert_eq!(document_xml.matches("<m:oMath>").count(), 1);
        assert_eq!(footnotes_xml.matches("<m:oMath>").count(), 1);
        assert!(read_docx(&bytes).is_ok());
    }

    #[test]
    fn invalid_footnote_math_remains_visible_with_its_warning() {
        let markdown = "Valid $x$ note[^n]\n\n[^n]: Bad $\\href{https://example.com}{x}$";
        let path = unique_path("docx");
        let result = export_docx(&request(ExportFormat::Docx, &path, markdown)).unwrap();
        let bytes = fs::read(path).unwrap();
        let document_xml = docx_zip_entry(&bytes, "word/document.xml");
        let footnotes_xml = docx_zip_entry(&bytes, "word/footnotes.xml");
        assert_eq!(document_xml.matches("<m:oMath>").count(), 1);
        assert!(!footnotes_xml.contains("<m:oMath>"));
        assert!(footnotes_xml.contains("\\href{https://example.com}{x}"));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.code == "MATH_COMMAND_UNSUPPORTED"));
        assert!(read_docx(&bytes).is_ok());
    }

    #[test]
    fn email_links_and_heading_anchors_survive_every_export_surface() {
        let markdown = "# 中文标题\n\n# 中文标题\n\n## Dash-Name\n\n## Explicit {#custom-id}\n\n[中文](#中文标题) [second](#中文标题-2) [dash](#dash-name) [explicit](#custom-id)\n\n<one@example.org> two@example.org [three](mailto:three@example.org)";
        let html_path = unique_path("html");
        let html_request = request(ExportFormat::Html, &html_path, markdown);
        export_html(&html_request).unwrap();
        let html = fs::read_to_string(&html_path).unwrap();
        for address in ["one", "two", "three"] {
            assert!(
                html.contains(&format!("href=\"mailto:{address}@example.org\"")),
                "{html}"
            );
        }

        let pdf_path = unique_path("pdf");
        let pdf_request = request(ExportFormat::Pdf, &pdf_path, markdown);
        let prepared = prepare_pdf(&pdf_request, "links-ready").unwrap();
        for address in ["one", "two", "three"] {
            assert!(prepared
                .html
                .contains(&format!("href=\"mailto:{address}@example.org\"")));
        }

        let docx_path = unique_path("docx");
        let docx_request = request(ExportFormat::Docx, &docx_path, markdown);
        export_docx(&docx_request).unwrap();
        let bytes = fs::read(docx_path).unwrap();
        let document_xml = docx_zip_entry(&bytes, "word/document.xml");
        let relations = docx_zip_entry(&bytes, "word/_rels/document.xml.rels");
        for index in 1..=4 {
            assert!(
                document_xml.contains(&format!("w:name=\"marklite_{index}\"")),
                "{document_xml}"
            );
            assert!(
                document_xml.contains(&format!("w:anchor=\"marklite_{index}\"")),
                "{document_xml}"
            );
        }
        for address in ["one", "two", "three"] {
            assert!(
                relations.contains(&format!("mailto:{address}@example.org")),
                "{relations}"
            );
        }
    }

    #[test]
    fn rejected_links_keep_visible_labels_and_matching_warnings_across_formats() {
        let markdown = "[safe](https://example.org) [blocked](javascript:alert(1))";
        let html_path = unique_path("html");
        let html_result = export_html(&request(ExportFormat::Html, &html_path, markdown)).unwrap();
        let html = fs::read_to_string(html_path).unwrap();
        assert!(html.contains("href=\"https://example.org/\""));
        assert!(html.contains("blocked"));
        assert!(!html.contains("javascript:"));

        let pdf_path = unique_path("pdf");
        let pdf = prepare_pdf(
            &request(ExportFormat::Pdf, &pdf_path, markdown),
            "link-ready",
        )
        .unwrap();
        assert!(pdf.html.contains("blocked"));
        assert!(!pdf.html.contains("javascript:"));

        let docx_path = unique_path("docx");
        let docx_result = export_docx(&request(ExportFormat::Docx, &docx_path, markdown)).unwrap();
        let bytes = fs::read(docx_path).unwrap();
        let document_xml = docx_zip_entry(&bytes, "word/document.xml");
        assert!(document_xml.contains("blocked"));
        assert!(read_docx(&bytes).is_ok());

        for warnings in [&html_result.warnings, &pdf.warnings, &docx_result.warnings] {
            assert_eq!(warnings.len(), 1, "{warnings:?}");
            assert_eq!(warnings[0].code, "UNSUPPORTED_LINK_SCHEME");
            assert_eq!(warnings[0].target.as_deref(), Some("javascript:alert(1)"));
        }
    }

    #[test]
    fn docx_bookmarks_do_not_collide_and_unknown_anchors_degrade_to_text() {
        let shared = "forty-character-heading-prefix-that-collides-when-truncated";
        let markdown = format!(
            "# First {{#{shared}-a}}\n\n# Second {{#{shared}-b}}\n\n[one](#{shared}-a) [two](#{shared}-b) [missing](#absent)"
        );
        let path = unique_path("docx");
        let result = export_docx(&request(ExportFormat::Docx, &path, &markdown)).unwrap();
        let document_xml = docx_zip_entry(&fs::read(path).unwrap(), "word/document.xml");
        for index in 1..=2 {
            assert!(document_xml.contains(&format!("w:name=\"marklite_{index}\"")));
            assert!(document_xml.contains(&format!("w:anchor=\"marklite_{index}\"")));
        }
        assert!(!document_xml.contains("w:anchor=\"absent\""));
        assert!(document_xml.contains("missing"));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.code == "DOCX_ANCHOR_NOT_FOUND"));
    }

    #[test]
    fn invalid_math_stays_visible_and_returns_structured_export_warnings() {
        let markdown = r"bad $\href{https://example.com}{x}$ tail";
        let html_path = unique_path("html");
        let html_request = request(ExportFormat::Html, &html_path, markdown);
        let html_result = export_html(&html_request).unwrap();
        let html = fs::read_to_string(&html_path).unwrap();
        assert!(html.contains("math-error"));
        assert!(html.contains(r"\href{https://example.com}{x}"));
        assert_eq!(html_result.warnings[0].code, "MATH_COMMAND_UNSUPPORTED");

        let docx_path = unique_path("docx");
        let docx_request = request(ExportFormat::Docx, &docx_path, markdown);
        let docx_result = export_docx(&docx_request).unwrap();
        let document_xml = docx_zip_entry(&fs::read(&docx_path).unwrap(), "word/document.xml");
        assert!(document_xml.contains("\\href{https://example.com}{x}"));
        assert_eq!(docx_result.warnings[0].code, "MATH_COMMAND_UNSUPPORTED");
    }

    #[test]
    fn docx_preserves_distinct_break_footnote_and_table_alignment_semantics() {
        let path = unique_path("docx");
        let markdown = "# Semantic {#semantic}\n\n- item\n\nsoft\nline  \nhard[^1]\n\n| Left | Center | Right |\n| :--- | :----: | ----: |\n| a | b | c |\n\n[^1]: **bold** *italic* [linked](https://example.com/footnote)";
        let docx_request = request(ExportFormat::Docx, &path, markdown);
        let result = export_docx(&docx_request).unwrap();
        let bytes = fs::read(&path).unwrap();
        let document_xml = docx_zip_entry(&bytes, "word/document.xml");
        let footnotes_xml = docx_zip_entry(&bytes, "word/footnotes.xml");

        assert_eq!(
            document_xml
                .matches("<w:br w:type=\"textWrapping\" />")
                .count(),
            1
        );
        assert!(document_xml.contains(">soft</w:t></w:r><w:r><w:rPr /><w:t xml:space=\"preserve\"> </w:t></w:r><w:r><w:rPr /><w:t xml:space=\"preserve\">line</w:t>"));
        // Two left-aligned cells plus the table's own default left justification.
        assert_eq!(document_xml.matches("<w:jc w:val=\"left\" />").count(), 3);
        assert_eq!(document_xml.matches("<w:jc w:val=\"center\" />").count(), 2);
        assert_eq!(document_xml.matches("<w:jc w:val=\"right\" />").count(), 2);
        assert!(footnotes_xml.contains("<w:b />"));
        assert!(footnotes_xml.contains("<w:i />"));
        assert!(footnotes_xml.contains(">linked</w:t>"));
        assert!(footnotes_xml.contains("> (https://example.com/footnote)</w:t>"));
        assert!(result.warnings.iter().any(|warning| {
            warning.code == "DOCX_FOOTNOTE_LINK_DEGRADED"
                && warning.target.as_deref() == Some("https://example.com/footnote")
        }));

        let html_path = unique_path("html");
        let html_request = request(ExportFormat::Html, &html_path, markdown);
        export_html(&html_request).unwrap();
        let html = fs::read_to_string(&html_path).unwrap();
        assert!(html.contains("<h1 id=\"semantic\">Semantic</h1>"));
        assert!(html.contains("soft\nline<br>\nhard"));
        assert!(html.contains("text-align: center"));
        assert!(html.contains("text-align: right"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("https://example.com/footnote"));

        let pdf_path = unique_path("pdf");
        let pdf_request = request(ExportFormat::Pdf, &pdf_path, markdown);
        let prepared = prepare_pdf(&pdf_request, "shared-semantic-fixture").unwrap();
        assert!(prepared.html.contains("<h1 id=\"semantic\">Semantic</h1>"));
        assert!(prepared.html.contains("soft\nline<br>\nhard"));
        assert!(prepared.html.contains("text-align: center"));
        assert!(prepared.html.contains("<strong>bold</strong>"));
        assert!(prepared.html.contains("https://example.com/footnote"));
    }

    #[test]
    fn docx_preserves_complete_tight_list_paragraphs_and_inline_styles() {
        let path = unique_path("docx");
        let request = request(
            ExportFormat::Docx,
            &path,
            concat!(
                "- alpha **bold** *italic* [linked](https://example.com) `coded` omega\n",
                "- [ ] finish task\n",
                "- [x] complete task\n\n",
                "5. fifth **strong tail**\n",
                "   1. nested *detail*\n",
                "6. sixth\n\n",
                "- loose first\n\n",
                "  loose continuation\n",
            ),
        );

        let result = export_docx(&request).unwrap();
        let reopened: serde_json::Value =
            serde_json::from_str(&read_docx(&fs::read(&path).unwrap()).unwrap().json()).unwrap();
        let paragraphs = docx_paragraphs(&reopened);
        let texts = paragraphs
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            [
                "中文 Title",
                "alpha bold italic linked coded omega",
                "☐ finish task",
                "☒ complete task",
                "fifth strong tail",
                "nested detail",
                "sixth",
                "loose first",
                "loose continuation",
            ]
        );

        let tight = paragraphs[1].0;
        assert_eq!(
            tight.pointer("/data/property/numberingProperty/level"),
            Some(&serde_json::json!(0))
        );
        assert_eq!(
            docx_run_for_text(tight, "bold").and_then(|run| run.pointer("/data/runProperty/bold")),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            docx_run_for_text(tight, "italic")
                .and_then(|run| run.pointer("/data/runProperty/italic")),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            docx_run_for_text(tight, "coded")
                .and_then(|run| run.pointer("/data/runProperty/fonts/ascii"))
                .and_then(serde_json::Value::as_str),
            Some("Consolas")
        );
        assert_eq!(
            paragraphs[5]
                .0
                .pointer("/data/property/numberingProperty/level"),
            Some(&serde_json::json!(1))
        );
        for index in [2, 3, 4, 6, 7] {
            assert_eq!(
                paragraphs[index]
                    .0
                    .pointer("/data/property/numberingProperty/level"),
                Some(&serde_json::json!(0)),
                "unexpected numbering for {:?}",
                paragraphs[index].1
            );
        }
        assert_eq!(
            paragraphs[8].0.pointer("/data/property/numberingProperty"),
            None
        );
        assert!(tight.to_string().contains("\"type\":\"hyperlink\""));
        assert!(reopened.to_string().contains("https://example.com"));
        assert!(
            reopened.to_string().contains("\"overrideStart\":5"),
            "{reopened}"
        );
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }

    #[test]
    fn common_extensions_keep_link_styles_and_definition_paragraphs_across_formats() {
        let markdown = concat!(
            "Term **one**\n: First *paragraph*.\n\n  Second paragraph with [**bold** `code`](https://example.com).\n\n",
            "> [!WARNING]\n> Keep **alert** text.\n\n",
            "[**strong** *em* `coded` ~~strike~~](https://example.org)\n",
        );
        let docx_path = unique_path("docx");
        let docx_result = export_docx(&request(ExportFormat::Docx, &docx_path, markdown)).unwrap();
        let reopened: serde_json::Value =
            serde_json::from_str(&read_docx(&fs::read(&docx_path).unwrap()).unwrap().json())
                .unwrap();
        let paragraphs = docx_paragraphs(&reopened);
        let texts = paragraphs
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>();
        assert!(texts.contains(&"First paragraph."), "{texts:?}");
        assert!(
            texts.contains(&"Second paragraph with bold code."),
            "{texts:?}"
        );
        assert!(
            texts.iter().any(|text| text.contains("WARNING")),
            "{texts:?}"
        );
        let linked = paragraphs
            .iter()
            .find(|(_, text)| text.contains("strong em coded strike"))
            .unwrap()
            .0;
        assert_eq!(
            docx_run_for_text(linked, "strong")
                .and_then(|run| run.pointer("/data/runProperty/bold")),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            docx_run_for_text(linked, "em").and_then(|run| run.pointer("/data/runProperty/italic")),
            Some(&serde_json::json!(true))
        );
        assert_eq!(
            docx_run_for_text(linked, "coded")
                .and_then(|run| run.pointer("/data/runProperty/fonts/ascii")),
            Some(&serde_json::json!("Consolas"))
        );
        assert_eq!(
            docx_run_for_text(linked, "strike")
                .and_then(|run| run.pointer("/data/runProperty/strike")),
            Some(&serde_json::json!(true))
        );
        assert!(
            docx_result.warnings.is_empty(),
            "{:?}",
            docx_result.warnings
        );

        let html_path = unique_path("html");
        export_html(&request(ExportFormat::Html, &html_path, markdown)).unwrap();
        let html = fs::read_to_string(&html_path).unwrap();
        assert!(
            html.contains("<dt>Term <strong>one</strong></dt>"),
            "{html}"
        );
        assert!(html.contains("Second paragraph with"));
        assert!(html.contains("markdown-alert-warning"));
        assert!(html.contains("<strong>strong</strong>"));
        let pdf_path = unique_path("pdf");
        let prepared = prepare_pdf(
            &request(ExportFormat::Pdf, &pdf_path, markdown),
            "common-extensions",
        )
        .unwrap();
        assert!(prepared.html.contains("markdown-alert-warning"));
        assert!(prepared.html.contains("Second paragraph with"));
    }

    #[test]
    fn maps_all_docx_page_options_to_exact_content_widths() {
        let path = unique_path("docx");
        let mut request = request(ExportFormat::Docx, &path, "body");
        let cases = [
            (
                ExportPaperSize::A4,
                ExportOrientation::Portrait,
                ExportMarginPreset::Narrow,
                6_645_910,
            ),
            (
                ExportPaperSize::A4,
                ExportOrientation::Portrait,
                ExportMarginPreset::Normal,
                5_731_510,
            ),
            (
                ExportPaperSize::A4,
                ExportOrientation::Portrait,
                ExportMarginPreset::Wide,
                4_817_110,
            ),
            (
                ExportPaperSize::A4,
                ExportOrientation::Landscape,
                ExportMarginPreset::Narrow,
                9_777_730,
            ),
            (
                ExportPaperSize::A4,
                ExportOrientation::Landscape,
                ExportMarginPreset::Normal,
                8_863_330,
            ),
            (
                ExportPaperSize::A4,
                ExportOrientation::Landscape,
                ExportMarginPreset::Wide,
                7_948_930,
            ),
            (
                ExportPaperSize::Letter,
                ExportOrientation::Portrait,
                ExportMarginPreset::Narrow,
                6_858_000,
            ),
            (
                ExportPaperSize::Letter,
                ExportOrientation::Portrait,
                ExportMarginPreset::Normal,
                5_943_600,
            ),
            (
                ExportPaperSize::Letter,
                ExportOrientation::Portrait,
                ExportMarginPreset::Wide,
                5_029_200,
            ),
            (
                ExportPaperSize::Letter,
                ExportOrientation::Landscape,
                ExportMarginPreset::Narrow,
                9_144_000,
            ),
            (
                ExportPaperSize::Letter,
                ExportOrientation::Landscape,
                ExportMarginPreset::Normal,
                8_229_600,
            ),
            (
                ExportPaperSize::Letter,
                ExportOrientation::Landscape,
                ExportMarginPreset::Wide,
                7_315_200,
            ),
        ];

        for (paper_size, orientation, margin, expected_width) in cases {
            request.options.paper_size = paper_size;
            request.options.orientation = orientation;
            request.options.margin = margin;
            assert_eq!(
                DocxPageLayout::from_request(&request).content_width_emu(),
                expected_width
            );
        }
    }

    #[test]
    fn fits_docx_pictures_to_content_width_without_upscaling() {
        let max_width = 5_731_510;
        let small = Pic::new_with_dimensions(vec![1], 100, 50);
        let small_size = small.size;
        assert_eq!(
            fit_docx_picture_to_content_width(small, max_width).size,
            small_size
        );

        let wide = fit_docx_picture_to_content_width(
            Pic::new_with_dimensions(vec![1], 2_000, 1_000),
            max_width,
        );
        assert_eq!(wide.size, (max_width, 2_865_755));

        let tall = fit_docx_picture_to_content_width(
            Pic::new_with_dimensions(vec![1], 1_000, 4_000),
            max_width,
        );
        assert_eq!(tall.size, (max_width, 22_926_040));

        let extreme = fit_docx_picture_to_content_width(
            Pic::new_with_dimensions(vec![1], 1, 1).size(u32::MAX, u32::MAX),
            max_width,
        );
        assert_eq!(extreme.size, (max_width, max_width));
    }

    #[test]
    fn writes_the_fitted_local_picture_extent_into_exported_docx() {
        let directory = unique_path("docx-image");
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("wide.png"),
            STANDARD
                .decode("iVBORw0KGgoAAAANSUhEUgAAB9AAAAABCAYAAACG/TV9AAAAAXNSR0IArs4c6QAAAARnQU1BAACxjwv8YQUAAAAJcEhZcwAADsMAAA7DAcdvqGQAAAAdSURBVGhD7cEBAQAAAIKg/p+2I8ACAAAAAAAAADrpxiKStV4fBQAAAABJRU5ErkJggg==")
                .unwrap(),
        )
        .unwrap();
        let source = directory.join("fixture.md");
        fs::write(&source, b"fixture").unwrap();
        let output = directory.join("wide.docx");
        let mut request = request(ExportFormat::Docx, &output, "![wide](wide.png)");
        request.snapshot.source_path = Some(source.to_string_lossy().into_owned());
        request.options.include_local_images = true;

        let result = export_docx(&request).unwrap();
        let reopened: serde_json::Value =
            serde_json::from_str(&read_docx(&fs::read(output).unwrap()).unwrap().json()).unwrap();
        assert_eq!(first_picture_size(&reopened), Some((5_731_510, 2_866)));
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }

    #[test]
    fn exports_relative_local_links_from_source_instead_of_output_directory() {
        let directory = unique_path("link-fixture");
        let source_directory = directory.join("源 文档");
        let output_directory = directory.join("other-output");
        fs::create_dir_all(&source_directory).unwrap();
        fs::create_dir_all(&output_directory).unwrap();
        let source = source_directory.join("index.md");
        let linked = directory.join("中文 目标.md");
        fs::write(&source, b"fixture").unwrap();
        fs::write(&linked, b"linked").unwrap();
        let output = output_directory.join("portable.html");
        let mut html_request = request(
            ExportFormat::Html,
            &output,
            "[local](../%E4%B8%AD%E6%96%87%20%E7%9B%AE%E6%A0%87.md#section)",
        );
        html_request.snapshot.source_path = Some(source.to_string_lossy().into_owned());

        let result = export_html(&html_request).unwrap();
        let exported = fs::read_to_string(output).unwrap();
        let mut expected = url::Url::from_file_path(linked.canonicalize().unwrap()).unwrap();
        expected.set_fragment(Some("section"));
        assert!(exported.contains(expected.as_str()), "{exported}");
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);

        let docx_output = output_directory.join("portable.docx");
        let mut docx_request = request(
            ExportFormat::Docx,
            &docx_output,
            "[local](../%E4%B8%AD%E6%96%87%20%E7%9B%AE%E6%A0%87.md#section)",
        );
        docx_request.snapshot.source_path = Some(source.to_string_lossy().into_owned());
        let docx_result = export_docx(&docx_request).unwrap();
        let reopened = read_docx(&fs::read(docx_output).unwrap()).unwrap().json();
        assert!(reopened.contains(expected.as_str()), "{reopened}");
        assert!(
            docx_result.warnings.is_empty(),
            "{:?}",
            docx_result.warnings
        );
    }

    #[test]
    fn docx_preserves_ordered_list_start_and_nested_instances() {
        let path = unique_path("docx");
        let request = request(
            ExportFormat::Docx,
            &path,
            "5. fifth\n6. sixth\n\n   7. nested seven\n   8. nested eight\n\nseparator\n\n99. ninety-nine",
        );
        export_docx(&request).unwrap();
        let reopened = read_docx(&fs::read(&path).unwrap()).unwrap().json();
        assert!(reopened.contains("\"overrideStart\": 5"), "{reopened}");
        assert!(reopened.contains("\"overrideStart\": 7"), "{reopened}");
        assert!(reopened.contains("\"overrideStart\": 99"), "{reopened}");
    }

    #[test]
    fn prepares_all_formats_from_thousands_of_nested_blocks_without_recursion() {
        let depth = 3_000;
        let markdown = format!("{}leaf", "> ".repeat(depth));

        let html_path = unique_path("html");
        let html_request = request(ExportFormat::Html, &html_path, &markdown);
        export_html(&html_request).unwrap();
        assert!(fs::read_to_string(&html_path).unwrap().contains("leaf"));

        let pdf_path = unique_path("pdf");
        let pdf_request = request(ExportFormat::Pdf, &pdf_path, &markdown);
        assert!(prepare_pdf(&pdf_request, "deep-nesting")
            .unwrap()
            .html
            .contains("leaf"));

        let docx_path = unique_path("docx");
        let docx_request = request(ExportFormat::Docx, &docx_path, &markdown);
        export_docx(&docx_request).unwrap();
        assert!(read_docx(&fs::read(&docx_path).unwrap())
            .unwrap()
            .json()
            .contains("leaf"));
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
    fn exports_only_marklite_owned_standalone_mind_map_svg() {
        let path = unique_path("svg");
        let svg = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 80\" width=\"100\" height=\"80\" data-marklite-mind-map=\"1\"><title>脑图</title><rect width=\"100\" height=\"80\" fill=\"#ffffff\"/></svg>";
        let mut request = request(ExportFormat::Svg, &path, "# 脑图");
        request.mind_map_svg = Some(svg.to_string());

        let result = export_svg(&request).unwrap();
        assert_eq!(result.format, ExportFormat::Svg);
        assert_eq!(fs::read_to_string(&path).unwrap(), svg);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn rejects_missing_active_or_executable_svg_payloads() {
        let path = unique_path("svg");
        let missing = request(ExportFormat::Svg, &path, "# 脑图");
        assert_eq!(
            validate_request(&missing).unwrap_err().code,
            "INVALID_MIND_MAP_SVG"
        );

        let mut executable = request(ExportFormat::Svg, &path, "# 脑图");
        executable.mind_map_svg = Some(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><svg data-marklite-mind-map=\"1\"><script>alert(1)</script></svg>"
                .to_string(),
        );
        assert_eq!(
            export_svg(&executable).unwrap_err().code,
            "INVALID_MIND_MAP_SVG"
        );

        let html_path = unique_path("html");
        let mut unexpected = request(ExportFormat::Html, &html_path, "body");
        unexpected.mind_map_svg = Some("<svg></svg>".to_string());
        assert_eq!(
            validate_request(&unexpected).unwrap_err().code,
            "INVALID_EXPORT_REQUEST"
        );
    }
}
