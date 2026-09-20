use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    models::{
        app_error::AppError,
        diagram::{DiagramSource, RenderedDiagram},
        export::ExportWarning,
    },
    services::{
        diagram_runtime_service, diagram_service, export_semantic::SemanticDocument,
        pdf_artifact::PdfWorkspace,
    },
    utils::path_utils::app_data_dir,
};
use tauri::WebviewUrl;

const NORMALIZATION_JS: &str = include_str!("../../../src/shared/mermaidSvgNormalization.mjs");
const FONT_FAMILY: &str = "Arial, system-ui, sans-serif";
const MAX_BATCH_SVG_BYTES: usize = 32 * 1024 * 1024;
const MAX_RASTER_BYTES: usize = 8 * 1024 * 1024;
const MAX_BATCH_RASTER_BYTES: usize = 32 * 1024 * 1024;
const MAX_BROWSER_RESULT_BYTES: usize = 160 * 1024 * 1024;
const RENDER_TIMEOUT: Duration = Duration::from_secs(30);
static RENDER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub(crate) enum DiagramExportMode {
    Html,
    Pdf,
    Docx,
}

impl DiagramExportMode {
    fn rasterize(self) -> bool {
        matches!(self, Self::Docx)
    }

    fn print_mode(self) -> bool {
        matches!(self, Self::Pdf)
    }
}

#[derive(Debug)]
pub(crate) struct PreparedDiagrams {
    pub artifacts: HashMap<String, RenderedDiagram>,
    pub print_artifacts: HashMap<String, RenderedDiagram>,
    pub rasters: HashMap<String, RasterDiagram>,
    pub warnings: Vec<ExportWarning>,
}

#[derive(Debug)]
pub(crate) struct RasterDiagram {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl PreparedDiagrams {
    pub fn empty() -> Self {
        Self {
            artifacts: HashMap::new(),
            print_artifacts: HashMap::new(),
            rasters: HashMap::new(),
            warnings: Vec::new(),
        }
    }
}

pub(crate) fn inspect(document: &SemanticDocument) -> (Vec<DiagramSource>, Vec<ExportWarning>) {
    let sources = document.diagram_sources();
    let warnings = diagram_service::validate_sources(&sources)
        .into_iter()
        .map(|diagnostic| {
            ExportWarning::new(
                diagnostic.code,
                diagnostic.message,
                Some(format!(
                    "{}:{}-{}",
                    diagnostic.diagram_id, diagnostic.source_start_byte, diagnostic.source_end_byte
                )),
            )
        })
        .collect();
    (sources, warnings)
}

pub(crate) fn render_page(
    runtime_script: &str,
    sources: &[DiagramSource],
    mode: DiagramExportMode,
) -> Result<String, AppError> {
    let source_json = serde_json::to_string(sources)
        .map_err(|error| AppError::new("DIAGRAM_RUNTIME_CRASHED", error.to_string()))?
        .replace('<', "\\u003c")
        .replace('&', "\\u0026");
    let runtime_script = runtime_script.replace("</script", "<\\/script");
    let rasterize = mode.rasterize();
    let print_mode = mode.print_mode();
    Ok(format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src blob:; font-src 'none'; connect-src 'none'; object-src 'none'; frame-src 'none'; base-uri 'none'; form-action 'none'"></head><body><script>{runtime_script}</script><script type="module">
{NORMALIZATION_JS}
const sources = {source_json};
const rasterize = {rasterize};
const printMode = {print_mode};
function readableMath(node) {{
  const children = [...node.children];
  if (node.localName === 'msup' && children.length === 2) return readableMath(children[0]) + '^' + readableMath(children[1]);
  if (node.localName === 'msub' && children.length === 2) return readableMath(children[0]) + '_' + readableMath(children[1]);
  if (node.localName === 'mfrac' && children.length === 2) return '(' + readableMath(children[0]) + ')/(' + readableMath(children[1]) + ')';
  if (node.localName === 'msqrt') return 'sqrt(' + children.map(readableMath).join('') + ')';
  return children.length ? children.map(readableMath).join('') : (node.textContent || '');
}}
function printSafeSvg(svgUtf8) {{
  const rasterRoot = new DOMParser().parseFromString(svgUtf8, 'image/svg+xml').documentElement;
  for (const foreignObject of [...rasterRoot.querySelectorAll('foreignObject')]) {{
    const math = foreignObject.querySelector('math');
    const text = rasterRoot.ownerDocument.createElementNS('http://www.w3.org/2000/svg', 'text');
    const x = Number(foreignObject.getAttribute('x') || 0);
    const y = Number(foreignObject.getAttribute('y') || 0);
    const labelWidth = Number(foreignObject.getAttribute('width') || 0);
    const labelHeight = Number(foreignObject.getAttribute('height') || 0);
    text.setAttribute('x', String(x + labelWidth / 2));
    text.setAttribute('y', String(y + labelHeight / 2));
    text.setAttribute('text-anchor', 'middle');
    text.setAttribute('dominant-baseline', 'middle');
    text.setAttribute('font-size', '16');
    text.textContent = math ? readableMath(math) : (foreignObject.textContent || '').replace(/\s+/g, ' ').trim();
    foreignObject.replaceWith(text);
  }}
  for (const switchElement of [...rasterRoot.querySelectorAll('switch')]) switchElement.replaceWith(...switchElement.children);
  return new XMLSerializer().serializeToString(rasterRoot);
}}
async function rasterSvg(svgUtf8, width, height) {{
  if (!(width > 0 && height > 0) || svgUtf8.length > 8 * 1024 * 1024) throw new Error('SVG dimensions or bytes exceed raster budget');
  const scale = Math.min(2, 4096 / width, 4096 / height, Math.sqrt(4_000_000 / (width * height)));
  const pixelWidth = Math.max(1, Math.round(width * scale));
  const pixelHeight = Math.max(1, Math.round(height * scale));
  const canvas = document.createElement('canvas');
  canvas.width = pixelWidth; canvas.height = pixelHeight;
  const rasterSvgUtf8 = printSafeSvg(svgUtf8);
  const blobUrl = URL.createObjectURL(new Blob([rasterSvgUtf8], {{ type: 'image/svg+xml' }}));
  try {{
    const image = new Image();
    image.src = blobUrl;
    await image.decode();
    const context = canvas.getContext('2d');
    if (!context) throw new Error('Canvas 2D is unavailable');
    context.drawImage(image, 0, 0, pixelWidth, pixelHeight);
    const dataUrl = canvas.toDataURL('image/png');
    if (!dataUrl.startsWith('data:image/png;base64,')) throw new Error('PNG encoding failed');
    return {{ pngBase64: dataUrl.slice(22), width: pixelWidth, height: pixelHeight }};
  }} finally {{ URL.revokeObjectURL(blobUrl); canvas.width = 0; canvas.height = 0; }}
}}
window.__markliteDiagramExport = {{ status: 'pending' }};
(async () => {{
  const artifacts = [];
  const printSvgs = [];
  const rasters = [];
  const rasterErrors = [];
  globalThis.mermaid.initialize({{
    startOnLoad: false, securityLevel: 'strict', theme: 'default', look: 'classic',
    layout: 'dagre', htmlLabels: true, fontFamily: 'Arial, system-ui, sans-serif',
    themeVariables: {{ fontFamily: 'Arial, system-ui, sans-serif', fontSize: '16px' }},
    logLevel: 'fatal', flowchart: {{ htmlLabels: true, defaultRenderer: 'dagre-wrapper' }}
  }});
  document.body.style.fontFamily = 'Arial, system-ui, sans-serif';
  document.body.style.fontSize = '16px';
  document.body.style.lineHeight = '1.6';
  const errors = [];
  for (const source of sources) {{
   try {{
    let timer;
    const rendered = await Promise.race([
      globalThis.mermaid.render('marklite-export-' + source.ordinal, source.sourceUtf8),
      new Promise((_, reject) => {{ timer = setTimeout(() => reject(new Error('DIAGRAM_TIMEOUT')), 5000); }})
    ]).finally(() => clearTimeout(timer));
    const parsed = new DOMParser().parseFromString(rendered.svg, 'image/svg+xml');
    const root = normalizeMermaidSvgDocument(parsed.documentElement);
    const svgUtf8 = new XMLSerializer().serializeToString(root);
    const viewBox = (root.getAttribute('viewBox') || '').trim().split(/[\s,]+/).map(Number);
    if (viewBox.length !== 4 || viewBox.some((value) => !Number.isFinite(value))) throw new Error('Mermaid SVG has no finite viewBox');
    artifacts.push({{
      diagramId: source.diagramId, sourceSha256: source.sourceSha256,
      rendererId: 'mermaid-offline-11.17.2', cacheKey: '0'.repeat(64),
      svgUtf8, width: viewBox[2], height: viewBox[3], viewBox,
      accessibleTitle: root.querySelector('title')?.textContent || null,
      accessibleDescription: root.querySelector('desc')?.textContent || null, warnings: []
    }});
    if (printMode) printSvgs.push({{ diagramId: source.diagramId, svgUtf8: root.querySelector('foreignObject') ? printSafeSvg(svgUtf8) : svgUtf8 }});
    if (rasterize) {{
      try {{ rasters.push({{ diagramId: source.diagramId, ...await rasterSvg(svgUtf8, viewBox[2], viewBox[3]) }}); }}
      catch (error) {{ rasterErrors.push({{ diagramId: source.diagramId, message: (error instanceof Error ? error.message : String(error)).slice(0, 1024) }}); }}
    }}
   }} catch (error) {{
    if (error instanceof Error && error.message === 'DIAGRAM_TIMEOUT') throw error;
    errors.push({{ diagramId: source.diagramId, message: (error instanceof Error ? error.message : String(error)).slice(0, 1024) }});
   }}
  }}
  window.__markliteDiagramExport = {{ status: 'ready', artifacts, errors, printSvgs, rasters, rasterErrors }};
}})().catch((error) => {{
  window.__markliteDiagramExport = {{ status: 'failed', message: error instanceof Error ? error.message : String(error) }};
}});
</script></body></html>"#
    ))
}

#[derive(Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum BrowserResult {
    Pending,
    Ready {
        artifacts: Vec<RenderedDiagram>,
        errors: Vec<BrowserError>,
        print_svgs: Vec<BrowserPrintSvg>,
        rasters: Vec<BrowserRaster>,
        raster_errors: Vec<BrowserError>,
    },
    Failed {
        message: String,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserError {
    diagram_id: String,
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserRaster {
    diagram_id: String,
    png_base64: String,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserPrintSvg {
    diagram_id: String,
    svg_utf8: String,
}

pub(crate) fn parse_browser_result(value: &str) -> Result<BrowserResult, AppError> {
    if value.len() > MAX_BROWSER_RESULT_BYTES {
        return Err(AppError::new(
            "DIAGRAM_OUTPUT_TOO_LARGE",
            "图表渲染结果超过回调预算",
        ));
    }
    // WebView callbacks serialize the evaluation result as JSON; the evaluated
    // expression itself is a JSON string so one layer of quoting is expected.
    let json: String = serde_json::from_str(value).map_err(|_| browser_result_error())?;
    serde_json::from_str(&json).map_err(|_| browser_result_error())
}

pub(crate) fn validate_batch(
    sources: &[DiagramSource],
    artifacts: Vec<RenderedDiagram>,
    errors: Vec<BrowserError>,
    print_svgs: Vec<BrowserPrintSvg>,
    rasters: Vec<BrowserRaster>,
    raster_errors: Vec<BrowserError>,
    mode: DiagramExportMode,
) -> Result<PreparedDiagrams, AppError> {
    let rasterize = mode.rasterize();
    let print_mode = mode.print_mode();
    if sources.len() != artifacts.len() + errors.len() {
        return Err(browser_result_error());
    }
    let mut bytes = 0usize;
    let mut result = HashMap::with_capacity(sources.len());
    let mut warnings = Vec::new();
    let mut seen = HashSet::new();
    for mut artifact in artifacts {
        let source = sources
            .iter()
            .find(|source| source.diagram_id == artifact.diagram_id)
            .ok_or_else(browser_result_error)?;
        if !seen.insert(source.diagram_id.as_str())
            || artifact.source_sha256 != source.source_sha256
        {
            return Err(browser_result_error());
        }
        bytes = bytes.saturating_add(artifact.svg_utf8.len());
        if bytes > MAX_BATCH_SVG_BYTES {
            return Err(AppError::new(
                "DIAGRAM_OUTPUT_TOO_LARGE",
                "导出图表 SVG 合计超过 32 MiB",
            ));
        }
        artifact.cache_key = cache_key(&source.source_sha256);
        let artifact = diagram_service::validate_rendered_diagram(artifact)?;
        result.insert(source.diagram_id.clone(), artifact);
    }
    for error in errors {
        let source = sources
            .iter()
            .find(|source| source.diagram_id == error.diagram_id)
            .ok_or_else(browser_result_error)?;
        if !seen.insert(source.diagram_id.as_str()) || error.message.len() > 4096 {
            return Err(browser_result_error());
        }
        warnings.push(ExportWarning::new(
            "DIAGRAM_PARSE_FAILED",
            format!("Mermaid 图表解析失败：{}", error.message),
            Some(format!(
                "{}:{}-{}",
                source.diagram_id, source.source_start_byte, source.source_end_byte
            )),
        ));
    }
    if !print_mode && !print_svgs.is_empty() || print_mode && print_svgs.len() != result.len() {
        return Err(browser_result_error());
    }
    let mut print_artifacts = HashMap::new();
    let mut print_bytes = 0usize;
    for print in print_svgs {
        let source = sources
            .iter()
            .find(|source| source.diagram_id == print.diagram_id)
            .ok_or_else(browser_result_error)?;
        let original = result
            .get(&print.diagram_id)
            .ok_or_else(browser_result_error)?;
        if print_artifacts.contains_key(&print.diagram_id) {
            return Err(browser_result_error());
        }
        print_bytes = print_bytes.saturating_add(print.svg_utf8.len());
        if print_bytes > MAX_BATCH_SVG_BYTES {
            return Err(AppError::new(
                "DIAGRAM_OUTPUT_TOO_LARGE",
                "PDF 图表 SVG 合计超过 32 MiB",
            ));
        }
        let mut artifact = original.clone();
        artifact.svg_utf8 = print.svg_utf8;
        artifact.cache_key = format!(
            "{:x}",
            Sha256::digest(
                format!("{}\0pdf-print-text-v1", cache_key(&source.source_sha256)).as_bytes()
            )
        );
        print_artifacts.insert(
            print.diagram_id,
            diagram_service::validate_rendered_diagram(artifact)?,
        );
    }
    if !rasterize && (!rasters.is_empty() || !raster_errors.is_empty()) {
        return Err(browser_result_error());
    }
    if rasterize && rasters.len() + raster_errors.len() != result.len() {
        return Err(browser_result_error());
    }
    let mut raster_map = HashMap::new();
    let mut raster_bytes = 0usize;
    let mut raster_seen = HashSet::new();
    for raster in rasters {
        if !result.contains_key(&raster.diagram_id)
            || !raster_seen.insert(raster.diagram_id.clone())
            || raster.png_base64.len() > (MAX_RASTER_BYTES * 4 / 3 + 8)
        {
            return Err(browser_result_error());
        }
        let png = STANDARD
            .decode(&raster.png_base64)
            .map_err(|_| browser_result_error())?;
        raster_bytes = raster_bytes.saturating_add(png.len());
        if png.len() > MAX_RASTER_BYTES
            || raster_bytes > MAX_BATCH_RASTER_BYTES
            || !(1..=4096).contains(&raster.width)
            || !(1..=4096).contains(&raster.height)
            || u64::from(raster.width) * u64::from(raster.height) > 4_000_000
            || !png.starts_with(b"\x89PNG\r\n\x1a\n")
            || !imagesize::blob_size(&png).is_ok_and(|size| {
                size.width == raster.width as usize && size.height == raster.height as usize
            })
        {
            return Err(AppError::new(
                "DIAGRAM_OUTPUT_TOO_LARGE",
                "DOCX 图表 PNG 尺寸或内容超出预算",
            ));
        }
        raster_map.insert(
            raster.diagram_id,
            RasterDiagram {
                png,
                width: raster.width,
                height: raster.height,
            },
        );
    }
    for error in raster_errors {
        let source = sources
            .iter()
            .find(|source| source.diagram_id == error.diagram_id)
            .ok_or_else(browser_result_error)?;
        if !result.contains_key(&error.diagram_id)
            || !raster_seen.insert(error.diagram_id.clone())
            || error.message.len() > 4096
        {
            return Err(browser_result_error());
        }
        warnings.push(ExportWarning::new(
            "DIAGRAM_RASTER_FAILED",
            format!("DOCX 图表图像生成失败：{}", error.message),
            Some(format!(
                "{}:{}-{}",
                source.diagram_id, source.source_start_byte, source.source_end_byte
            )),
        ));
    }
    Ok(PreparedDiagrams {
        artifacts: result,
        print_artifacts,
        rasters: raster_map,
        warnings,
    })
}

pub(crate) async fn prepare(
    app: &tauri::AppHandle,
    document: &SemanticDocument,
    target: &Path,
    isolate_profile: bool,
    deadline: Option<Instant>,
    cancelled: impl Fn() -> Option<AppError>,
    mode: DiagramExportMode,
) -> Result<PreparedDiagrams, AppError> {
    let (sources, mut warnings) = inspect(document);
    if sources.is_empty() {
        return Ok(PreparedDiagrams::empty());
    }
    let invalid: HashSet<_> = warnings
        .iter()
        .filter_map(|warning| warning.target.as_deref())
        .filter_map(|target| target.split(':').next())
        .collect();
    let valid: Vec<_> = sources
        .into_iter()
        .filter(|source| !invalid.contains(source.diagram_id.as_str()))
        .collect();
    if valid.is_empty() {
        return Ok(PreparedDiagrams {
            artifacts: HashMap::new(),
            print_artifacts: HashMap::new(),
            rasters: HashMap::new(),
            warnings,
        });
    }
    let runtime = match diagram_runtime_service::load_from_root(&app_data_dir()?) {
        Ok(runtime) => runtime,
        Err(error)
            if matches!(
                error.code.as_str(),
                "DIAGRAM_RUNTIME_UNAVAILABLE" | "DIAGRAM_RUNTIME_INVALID"
            ) =>
        {
            warnings.extend(valid.iter().map(|source| {
                ExportWarning::new(
                    error.code.clone(),
                    error.message.clone(),
                    Some(format!(
                        "{}:{}-{}",
                        source.diagram_id, source.source_start_byte, source.source_end_byte
                    )),
                )
            }));
            return Ok(PreparedDiagrams {
                artifacts: HashMap::new(),
                print_artifacts: HashMap::new(),
                rasters: HashMap::new(),
                warnings,
            });
        }
        Err(error) => return Err(error),
    };
    let page = render_page(&runtime.script_utf8, &valid, mode)?;
    let workspace = PdfWorkspace::create(target)?;
    workspace.write_html(&page)?;
    if isolate_profile {
        workspace.register_post_runtime_cleanup();
    }
    let url = url::Url::from_file_path(workspace.html_path())
        .map_err(|_| AppError::new("DIAGRAM_RUNTIME_CRASHED", "无法建立图表渲染页面 URL"))?;
    let sequence = RENDER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut builder = tauri::WebviewWindowBuilder::new(
        app,
        format!("diagram-export-{}-{sequence}", std::process::id()),
        WebviewUrl::External(url),
    )
    .title("MarkLite Diagram Export")
    .visible(false)
    .focused(false)
    .decorations(false)
    .resizable(false)
    .skip_taskbar(true)
    .inner_size(1024.0, 768.0)
    .on_navigation(|url| url.scheme() == "file");
    if isolate_profile {
        builder = builder.data_directory(workspace.webview_data_path().to_path_buf());
    }
    #[cfg(target_os = "macos")]
    let builder =
        builder.background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    let window = builder.build().map_err(|error| {
        AppError::new(
            "DIAGRAM_RUNTIME_CRASHED",
            format!("无法创建图表渲染面：{error}"),
        )
    })?;
    let started = Instant::now();
    let operation = async {
        loop {
            if let Some(error) = cancelled() {
                return Err(error);
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err(AppError::new(
                    "PDF_EXPORT_TIMEOUT",
                    "PDF 导出超过 CLI 全程时限",
                ));
            }
            if started.elapsed() >= RENDER_TIMEOUT {
                return Err(AppError::new(
                    "DIAGRAM_TIMEOUT",
                    "Mermaid 导出渲染超过 30 秒",
                ));
            }
            let (sender, receiver) = mpsc::channel();
            window
                .eval_with_callback(
                    "JSON.stringify(window.__markliteDiagramExport || {status:'pending'})",
                    move |value| {
                        let _ = sender.send(value);
                    },
                )
                .map_err(|error| {
                    AppError::new(
                        "DIAGRAM_RUNTIME_CRASHED",
                        format!("图表渲染查询失败：{error}"),
                    )
                })?;
            let value = tauri::async_runtime::spawn_blocking(move || {
                receiver.recv_timeout(Duration::from_millis(150))
            })
            .await
            .map_err(|_| browser_result_error())?;
            match value {
                Ok(value) => match parse_browser_result(&value)? {
                    BrowserResult::Pending => {}
                    BrowserResult::Ready {
                        artifacts,
                        errors,
                        print_svgs,
                        rasters,
                        raster_errors,
                    } => {
                        return validate_batch(
                            &valid,
                            artifacts,
                            errors,
                            print_svgs,
                            rasters,
                            raster_errors,
                            mode,
                        )
                    }
                    BrowserResult::Failed { message } if message == "DIAGRAM_TIMEOUT" => {
                        return Err(AppError::new(
                            "DIAGRAM_TIMEOUT",
                            "Mermaid 单图渲染超过 5 秒",
                        ))
                    }
                    BrowserResult::Failed { message } => {
                        return Err(AppError::new(
                            "DIAGRAM_RUNTIME_CRASHED",
                            format!("Mermaid 导出渲染失败：{message}"),
                        ))
                    }
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(browser_result_error()),
            }
            tauri::async_runtime::spawn_blocking(|| std::thread::sleep(Duration::from_millis(50)))
                .await
                .map_err(|_| browser_result_error())?;
        }
    }
    .await;
    let closed = window.destroy().map_err(|error| {
        AppError::new(
            "DIAGRAM_RUNTIME_CRASHED",
            format!("销毁图表渲染面失败：{error}"),
        )
    });
    closed?;
    let mut prepared = operation?;
    warnings.append(&mut prepared.warnings);
    prepared.warnings = warnings;
    Ok(prepared)
}

fn cache_key(source_sha256: &str) -> String {
    let font_key = format!(
        "{FONT_FAMILY}\0{font_size}\0{line_height}",
        font_size = 16,
        line_height = 1.6
    );
    let parts = [
        diagram_service::RENDERER_ID,
        "2",
        "light",
        &font_key,
        source_sha256,
    ];
    let digest = Sha256::digest(parts.join("\0").as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn browser_result_error() -> AppError {
    AppError::new("DIAGRAM_RUNTIME_CRASHED", "图表渲染面返回无效或错位制品")
}

#[cfg(test)]
mod tests {
    use super::{
        inspect, parse_browser_result, render_page, validate_batch, BrowserError, BrowserPrintSvg,
        BrowserResult, DiagramExportMode,
    };
    use crate::models::diagram::RenderedDiagram;
    use crate::services::export_semantic::SemanticDocument;

    #[test]
    fn browser_page_uses_shared_normalizer_and_escapes_source() {
        let document =
            SemanticDocument::parse("```mermaid\nflowchart TD\nA[</script>]-->B\n```", None);
        let (sources, warnings) = inspect(&document);
        assert_eq!(sources.len(), 1);
        assert!(warnings.is_empty());
        let page =
            render_page("globalThis.mermaid={};", &sources, DiagramExportMode::Html).unwrap();
        assert!(page.contains("function normalizeMermaidSvgDocument"));
        assert!(page.contains("\\u003c/script"));
        assert!(page.contains("connect-src 'none'"));
        assert!(!page.contains("A[</script>]-->B"));
    }

    #[test]
    fn browser_callback_requires_a_complete_typed_state() {
        assert!(matches!(
            parse_browser_result("\"{\\\"status\\\":\\\"pending\\\"}\"").unwrap(),
            BrowserResult::Pending
        ));
        assert!(parse_browser_result("{\"status\":\"ready\"}").is_err());
    }

    #[test]
    fn browser_batch_rejects_wrong_identity_and_active_svg_before_writing() {
        let document = SemanticDocument::parse("```mermaid\nflowchart TD\nA-->B\n```", None);
        let (sources, _) = inspect(&document);
        let source = &sources[0];
        let svg = "<svg viewBox=\"0 0 100 50\"><text>Safe</text></svg>";
        let artifact = RenderedDiagram {
            diagram_id: source.diagram_id.clone(),
            source_sha256: source.source_sha256.clone(),
            cache_key: "0".repeat(64),
            renderer_id: "mermaid-offline-11.17.2".into(),
            svg_utf8: svg.into(),
            width: 100.0,
            height: 50.0,
            view_box: [0.0, 0.0, 100.0, 50.0],
            accessible_title: None,
            accessible_description: None,
            warnings: Vec::new(),
        };
        let accepted = validate_batch(
            &sources,
            vec![artifact.clone()],
            vec![],
            vec![],
            vec![],
            vec![],
            DiagramExportMode::Html,
        )
        .unwrap();
        assert_eq!(accepted.artifacts.len(), 1);
        let print = validate_batch(
            &sources,
            vec![artifact.clone()],
            vec![],
            vec![BrowserPrintSvg {
                diagram_id: source.diagram_id.clone(),
                svg_utf8: svg.into(),
            }],
            vec![],
            vec![],
            DiagramExportMode::Pdf,
        )
        .unwrap();
        assert_eq!(print.print_artifacts.len(), 1);
        assert_eq!(
            validate_batch(
                &sources,
                vec![artifact.clone()],
                vec![],
                vec![BrowserPrintSvg {
                    diagram_id: source.diagram_id.clone(),
                    svg_utf8: "<svg viewBox=\"0 0 100 50\"><script>bad()</script></svg>".into()
                }],
                vec![],
                vec![],
                DiagramExportMode::Pdf
            )
            .unwrap_err()
            .code,
            "DIAGRAM_SVG_REJECTED"
        );
        let mut wrong = artifact.clone();
        wrong.source_sha256 = "b".repeat(64);
        assert_eq!(
            validate_batch(
                &sources,
                vec![wrong],
                vec![],
                vec![],
                vec![],
                vec![],
                DiagramExportMode::Html
            )
            .unwrap_err()
            .code,
            "DIAGRAM_RUNTIME_CRASHED"
        );
        let mut active = artifact;
        active.svg_utf8 = "<svg viewBox=\"0 0 100 50\"><script>bad()</script></svg>".into();
        assert_eq!(
            validate_batch(
                &sources,
                vec![active],
                vec![],
                vec![],
                vec![],
                vec![],
                DiagramExportMode::Html
            )
            .unwrap_err()
            .code,
            "DIAGRAM_SVG_REJECTED"
        );
        let fallback = validate_batch(
            &sources,
            vec![],
            vec![BrowserError {
                diagram_id: source.diagram_id.clone(),
                message: "syntax".into(),
            }],
            vec![],
            vec![],
            vec![],
            DiagramExportMode::Html,
        )
        .unwrap();
        assert!(fallback.artifacts.is_empty());
        assert_eq!(fallback.warnings[0].code, "DIAGRAM_PARSE_FAILED");
    }
}
