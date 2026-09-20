use quick_xml::{events::Event as XmlEvent, Reader};

use crate::models::{
    app_error::AppError,
    diagram::{DiagramDiagnostic, DiagramSource, RenderedDiagram},
};

pub const RENDERER_ID: &str = "mermaid-offline-11.17.2";
pub const MAX_DIAGRAMS_PER_DOCUMENT: usize = 64;
pub const MAX_DIAGRAM_SOURCE_BYTES: usize = 256 * 1024;
pub const MAX_DIAGRAM_SOURCE_LINES: usize = 4_000;
pub const MAX_DOCUMENT_DIAGRAM_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_SVG_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SVG_ELEMENTS: usize = 50_000;
pub const MAX_NODE_ELEMENTS: usize = 10_000;
pub const MAX_EDGE_ELEMENTS: usize = 20_000;
pub const MAX_SVG_SIDE: f64 = 16_384.0;
pub const MAX_SVG_PIXELS: f64 = 16_000_000.0;

const SUPPORTED_PREFIXES: &[&str] = &[
    "flowchart",
    "graph",
    "swimlane-beta",
    "sequenceDiagram",
    "classDiagram",
    "stateDiagram",
    "stateDiagram-v2",
    "erDiagram",
    "journey",
    "gantt",
    "pie",
    "quadrantChart",
    "requirementDiagram",
    "gitGraph",
    "C4Context",
    "C4Container",
    "C4Component",
    "C4Dynamic",
    "C4Deployment",
    "mindmap",
    "timeline",
    "sankey-beta",
    "xychart-beta",
    "block-beta",
    "packet",
    "kanban",
    "architecture-beta",
    "radar-beta",
    "eventmodeling",
    "treemap-beta",
    "venn-beta",
];

pub fn validate_sources(sources: &[DiagramSource]) -> Vec<DiagramDiagnostic> {
    let mut diagnostics = Vec::new();
    let total_bytes = sources.iter().fold(0usize, |total, source| {
        total.saturating_add(source.source_utf8.len())
    });
    if sources.len() > MAX_DIAGRAMS_PER_DOCUMENT || total_bytes > MAX_DOCUMENT_DIAGRAM_BYTES {
        for source in sources {
            diagnostics.push(diagnostic(
                source,
                "DIAGRAM_DOCUMENT_LIMIT_EXCEEDED",
                "文档中的 Mermaid 图表数量或源码总量超过上限",
                false,
            ));
        }
        return diagnostics;
    }

    for source in sources {
        if source.source_utf8.len() > MAX_DIAGRAM_SOURCE_BYTES
            || source.source_utf8.lines().count() > MAX_DIAGRAM_SOURCE_LINES
        {
            diagnostics.push(diagnostic(
                source,
                "DIAGRAM_SOURCE_TOO_LARGE",
                "Mermaid 图表源码超过 256 KiB 或 4000 行上限",
                false,
            ));
            continue;
        }
        let trimmed = source.source_utf8.trim_start();
        let lower_source = source.source_utf8.to_ascii_lowercase();
        if trimmed.starts_with("---")
            || lower_source.contains("http://")
            || lower_source.contains("https://")
            || lower_source.contains("data:")
            || lower_source.contains("file:")
            || lower_source.contains("icon:")
            || lower_source.contains("img:")
            || source.source_utf8.lines().any(|line| {
                let line = line.trim_start();
                let lower = line.to_ascii_lowercase();
                line.starts_with("%%{")
                    || lower.starts_with("click ")
                    || lower.starts_with("href ")
                    || lower.starts_with("classdef ")
                    || lower.starts_with("style ")
                    || lower.starts_with("linkstyle ")
            })
        {
            diagnostics.push(diagnostic(
                source,
                "DIAGRAM_UNSAFE_CONFIGURATION",
                "Mermaid 文档不能覆盖配置、注册 click/href 或声明 front matter",
                false,
            ));
            continue;
        }
        let first = source
            .source_utf8
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with("%%"));
        if !first.is_some_and(is_supported_header) {
            diagnostics.push(diagnostic(
                source,
                "DIAGRAM_UNSUPPORTED_TYPE",
                "当前离线 Mermaid 运行时不支持该图种",
                false,
            ));
        }
    }
    diagnostics
}

fn is_supported_header(header: &str) -> bool {
    SUPPORTED_PREFIXES.iter().any(|prefix| {
        header == *prefix
            || header
                .strip_prefix(prefix)
                .is_some_and(|tail| tail.starts_with(char::is_whitespace))
    })
}

fn diagnostic(
    source: &DiagramSource,
    code: &str,
    message: &str,
    retryable: bool,
) -> DiagramDiagnostic {
    DiagramDiagnostic {
        code: code.to_string(),
        diagram_id: source.diagram_id.clone(),
        source_start_byte: source.source_start_byte,
        source_end_byte: source.source_end_byte,
        message: message.to_string(),
        retryable,
    }
}

pub fn validate_rendered_diagram(diagram: RenderedDiagram) -> Result<RenderedDiagram, AppError> {
    if diagram.renderer_id != RENDERER_ID
        || diagram.diagram_id.is_empty()
        || diagram.source_sha256.len() != 64
        || diagram.cache_key.len() != 64
        || !diagram.source_sha256.bytes().all(is_lower_hex)
        || !diagram.cache_key.bytes().all(is_lower_hex)
    {
        return Err(svg_rejected("图表身份或 renderer 版本无效"));
    }
    let metrics = validate_svg(&diagram.svg_utf8)?;
    if !same_number(diagram.width, metrics.view_box[2])
        || !same_number(diagram.height, metrics.view_box[3])
        || diagram
            .view_box
            .iter()
            .zip(metrics.view_box)
            .any(|(actual, expected)| !same_number(*actual, expected))
    {
        return Err(svg_rejected(format!(
            "图表尺寸与 SVG viewBox 不一致：DTO {:?}/{},{}，SVG {:?}",
            diagram.view_box, diagram.width, diagram.height, metrics.view_box
        )));
    }
    Ok(diagram)
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn same_number(left: f64, right: f64) -> bool {
    left.is_finite() && right.is_finite() && (left - right).abs() <= 0.01
}

#[derive(Debug, Clone, PartialEq)]
struct SvgMetrics {
    view_box: [f64; 4],
}

fn validate_svg(svg: &str) -> Result<SvgMetrics, AppError> {
    if svg.len() > MAX_SVG_BYTES {
        return Err(AppError::new(
            "DIAGRAM_OUTPUT_TOO_LARGE",
            "Mermaid SVG 超过 8 MiB 上限",
        ));
    }
    let mut reader = Reader::from_str(svg);
    reader.config_mut().trim_text(false);
    let mut depth = 0usize;
    let mut roots = 0usize;
    let mut elements = 0usize;
    let mut nodes = 0usize;
    let mut edges = 0usize;
    let mut view_box = None;
    let mut inside_style = 0usize;
    let mut math_foreign_object: Option<bool> = None;

    loop {
        match reader.read_event() {
            Ok(event @ (XmlEvent::Start(_) | XmlEvent::Empty(_))) => {
                let (element, empty) = match event {
                    XmlEvent::Start(element) => (element, false),
                    XmlEvent::Empty(element) => (element, true),
                    _ => unreachable!("match arm restricts XML event variants"),
                };
                let name = std::str::from_utf8(element.name().as_ref())
                    .map_err(|_| svg_rejected("SVG 元素名不是 UTF-8"))?
                    .to_string();
                if !is_allowed_element(&name) {
                    return Err(svg_rejected(format!("SVG 含不允许的元素：{name}")));
                }
                if name == "foreignObject" {
                    if empty || math_foreign_object.is_some() {
                        return Err(svg_rejected("SVG 只允许包含 MathML 的非嵌套 foreignObject"));
                    }
                    math_foreign_object = Some(false);
                } else if let Some(math_seen) = math_foreign_object.as_mut() {
                    if !matches!(
                        name.as_str(),
                        "div"
                            | "span"
                            | "b"
                            | "math"
                            | "mfrac"
                            | "mi"
                            | "mn"
                            | "mo"
                            | "mrow"
                            | "msup"
                    ) {
                        return Err(svg_rejected("MathML foreignObject 含非固定元素"));
                    }
                    if name == "math" {
                        *math_seen = true;
                    }
                }
                elements += 1;
                if elements > MAX_SVG_ELEMENTS {
                    return Err(output_too_large("SVG 元素数量超过上限"));
                }
                if depth == 0 {
                    roots += 1;
                    if roots != 1 || name != "svg" {
                        return Err(svg_rejected("SVG 必须只有一个 svg 根元素"));
                    }
                }
                let mut class_value = String::new();
                for attribute in element.attributes().with_checks(true) {
                    let attribute = attribute.map_err(|_| svg_rejected("SVG 属性格式无效"))?;
                    let key = std::str::from_utf8(attribute.key.as_ref())
                        .map_err(|_| svg_rejected("SVG 属性名不是 UTF-8"))?;
                    let value = attribute
                        .decoded_and_normalized_value(
                            quick_xml::XmlVersion::Implicit1_0,
                            reader.decoder(),
                        )
                        .map_err(|_| svg_rejected("SVG 属性值无法解码"))?;
                    validate_attribute(&name, key, value.as_ref())?;
                    if key == "class" {
                        class_value.push_str(value.as_ref());
                    }
                    if depth == 0 && name == "svg" && key == "viewBox" {
                        view_box = Some(parse_view_box(value.as_ref())?);
                    }
                }
                if class_value.split_ascii_whitespace().any(|class| {
                    class.contains("node") || class.contains("cluster") || class.contains("actor")
                }) {
                    nodes += 1;
                    if nodes > MAX_NODE_ELEMENTS {
                        return Err(output_too_large("SVG 节点数量超过上限"));
                    }
                }
                if name == "path"
                    || class_value
                        .split_ascii_whitespace()
                        .any(|class| class.contains("edge") || class.contains("relation"))
                {
                    edges += 1;
                    if edges > MAX_EDGE_ELEMENTS {
                        return Err(output_too_large("SVG 边数量超过上限"));
                    }
                }
                if name == "style" {
                    inside_style += 1;
                }
                if !empty {
                    depth += 1;
                }
            }
            Ok(XmlEvent::End(element)) => {
                let name = std::str::from_utf8(element.name().as_ref())
                    .map_err(|_| svg_rejected("SVG 结束元素名不是 UTF-8"))?
                    .to_string();
                if name == "style" {
                    inside_style = inside_style.saturating_sub(1);
                }
                if name == "foreignObject" {
                    let math_seen = math_foreign_object
                        .take()
                        .ok_or_else(|| svg_rejected("SVG foreignObject 层级无效"))?;
                    if !math_seen {
                        return Err(svg_rejected("SVG foreignObject 必须包含 MathML"));
                    }
                }
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| svg_rejected("SVG 元素层级无效"))?;
            }
            Ok(XmlEvent::Text(text)) if inside_style > 0 => {
                validate_css(&String::from_utf8_lossy(text.as_ref()))?;
            }
            Ok(XmlEvent::GeneralRef(reference)) => {
                let reference = std::str::from_utf8(reference.as_ref())
                    .map_err(|_| svg_rejected("SVG 字符引用不是 UTF-8"))?;
                if !matches!(reference, "amp" | "lt" | "gt" | "quot" | "apos")
                    && !reference.starts_with('#')
                {
                    return Err(svg_rejected("SVG 禁止自定义实体引用"));
                }
            }
            Ok(XmlEvent::Text(_) | XmlEvent::Comment(_) | XmlEvent::Decl(_)) => {}
            Ok(XmlEvent::Eof) => break,
            Ok(XmlEvent::DocType(_) | XmlEvent::PI(_) | XmlEvent::CData(_)) => {
                return Err(svg_rejected("SVG 禁止 DOCTYPE、PI 和 CDATA"));
            }
            Err(error) => return Err(svg_rejected(format!("SVG XML 无效：{error}"))),
        }
    }
    if depth != 0 || roots != 1 {
        return Err(svg_rejected("SVG 根元素或层级不完整"));
    }
    let view_box = view_box.ok_or_else(|| svg_rejected("SVG 缺少 viewBox"))?;
    if view_box[2] <= 0.0
        || view_box[3] <= 0.0
        || view_box[2] > MAX_SVG_SIDE
        || view_box[3] > MAX_SVG_SIDE
        || view_box[2] * view_box[3] > MAX_SVG_PIXELS
    {
        return Err(output_too_large("SVG viewBox 超过尺寸或像素预算"));
    }
    Ok(SvgMetrics { view_box })
}

fn is_allowed_element(name: &str) -> bool {
    matches!(
        name,
        "svg"
            | "g"
            | "path"
            | "rect"
            | "circle"
            | "ellipse"
            | "polygon"
            | "polyline"
            | "line"
            | "text"
            | "tspan"
            | "defs"
            | "marker"
            | "style"
            | "title"
            | "desc"
            | "linearGradient"
            | "radialGradient"
            | "stop"
            | "clipPath"
            | "mask"
            | "pattern"
            | "symbol"
            | "use"
            | "filter"
            | "feDropShadow"
            | "foreignObject"
            | "switch"
            | "div"
            | "span"
            | "b"
            | "math"
            | "mfrac"
            | "mi"
            | "mn"
            | "mo"
            | "mrow"
            | "msup"
    )
}

fn validate_attribute(element: &str, key: &str, value: &str) -> Result<(), AppError> {
    if key == "xmlns" && value == "http://www.w3.org/2000/svg" {
        return Ok(());
    }
    if key == "xmlns"
        && matches!(element, "div" | "span" | "b")
        && value == "http://www.w3.org/1999/xhtml"
    {
        return Ok(());
    }
    if key == "xmlns"
        && matches!(
            element,
            "math" | "mfrac" | "mi" | "mn" | "mo" | "mrow" | "msup"
        )
        && value == "http://www.w3.org/1998/Math/MathML"
    {
        return Ok(());
    }
    if key == "xmlns:xlink" && value == "http://www.w3.org/1999/xlink" {
        return Ok(());
    }
    if key.to_ascii_lowercase().starts_with("on") {
        return Err(svg_rejected("SVG 禁止事件属性"));
    }
    if key.contains(':') && !matches!(key, "xmlns:xlink" | "xlink:href" | "xml:space") {
        return Err(svg_rejected("SVG 禁止未知命名空间属性"));
    }
    if matches!(key, "href" | "xlink:href") && !value.starts_with('#') {
        return Err(svg_rejected("SVG 引用只能指向当前文档 fragment"));
    }
    let lower = value.to_ascii_lowercase();
    if lower.contains("javascript:")
        || lower.contains("data:")
        || lower.contains("file:")
        || lower.contains("http:")
        || lower.contains("https:")
        || lower.contains("@import")
    {
        return Err(svg_rejected("SVG 属性禁止外部或可执行资源"));
    }
    if key == "style" {
        validate_css(value)?;
    }
    Ok(())
}

fn validate_css(css: &str) -> Result<(), AppError> {
    let lower = css.to_ascii_lowercase();
    if lower.contains("@import")
        || lower.contains("javascript:")
        || lower.contains("data:")
        || lower.contains("http:")
        || lower.contains("https:")
    {
        return Err(svg_rejected("SVG CSS 禁止外部或可执行资源"));
    }
    for suffix in lower.split("url(").skip(1) {
        let target = suffix.trim_start_matches(|character: char| {
            character.is_ascii_whitespace() || matches!(character, '\'' | '"')
        });
        if !target.starts_with('#') {
            return Err(svg_rejected("SVG CSS url 只能指向当前文档 fragment"));
        }
    }
    Ok(())
}

fn parse_view_box(value: &str) -> Result<[f64; 4], AppError> {
    let values = value
        .split(|character: char| character.is_ascii_whitespace() || character == ',')
        .filter(|value| !value.is_empty())
        .map(|value| value.parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| svg_rejected("SVG viewBox 不是有限数值"))?;
    if values.len() != 4 || values.iter().any(|value| !value.is_finite()) {
        return Err(svg_rejected("SVG viewBox 必须包含四个有限数值"));
    }
    Ok([values[0], values[1], values[2], values[3]])
}

fn svg_rejected(message: impl Into<String>) -> AppError {
    AppError::new("DIAGRAM_SVG_REJECTED", message)
}

fn output_too_large(message: impl Into<String>) -> AppError {
    AppError::new("DIAGRAM_OUTPUT_TOO_LARGE", message)
}

#[cfg(test)]
mod tests {
    use super::{validate_rendered_diagram, validate_sources, RENDERER_ID};
    use crate::models::diagram::{DiagramSource, RenderedDiagram};

    fn source(text: &str) -> DiagramSource {
        DiagramSource {
            diagram_id: "diagram-0-aaaaaaaaaaaa".to_string(),
            ordinal: 0,
            source_utf8: text.to_string(),
            source_sha256: "a".repeat(64),
            source_start_byte: 10,
            source_end_byte: 10 + text.len(),
        }
    }

    fn rendered(svg: &str) -> RenderedDiagram {
        RenderedDiagram {
            diagram_id: "diagram-0-aaaaaaaaaaaa".to_string(),
            source_sha256: "a".repeat(64),
            cache_key: "b".repeat(64),
            renderer_id: RENDERER_ID.to_string(),
            svg_utf8: svg.to_string(),
            width: 100.0,
            height: 50.0,
            view_box: [0.0, 0.0, 100.0, 50.0],
            accessible_title: Some("Flow".to_string()),
            accessible_description: None,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn accepts_frozen_types_and_rejects_configuration_or_unknown_types() {
        assert!(validate_sources(&[
            source("flowchart TD\nA-->B"),
            source("sequenceDiagram\nA->>B: $$x^2$$"),
            source("venn-beta\nset A")
        ])
        .is_empty());
        let diagnostics = validate_sources(&[
            source("---\nconfig:\n  securityLevel: loose\n---\nflowchart TD\nA-->B"),
            source("zenuml\nA->B: call"),
            source("flowchart TD\nclick A callback"),
        ]);
        assert_eq!(diagnostics.len(), 3);
        assert_eq!(diagnostics[0].code, "DIAGRAM_UNSAFE_CONFIGURATION");
        assert_eq!(diagnostics[1].code, "DIAGRAM_UNSUPPORTED_TYPE");
        assert_eq!(diagnostics[2].source_start_byte, 10);
    }

    #[test]
    fn accepts_local_svg_structure_and_rejects_active_or_external_content() {
        let safe = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50"><defs><marker id="m"><path d="M0 0L1 1"/></marker><style>.e{marker-end:url(#m)}</style></defs><g class="node"><rect width="10" height="10"/><text><tspan>Safe</tspan></text></g><path class="edgePath" marker-end="url(#m)" d="M0 0L10 10"/></svg>"##;
        let result = validate_rendered_diagram(rendered(safe));
        assert!(result.is_ok(), "{result:?}");

        let math = r#"<svg viewBox="0 0 100 50"><foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><span><math xmlns="http://www.w3.org/1998/Math/MathML"><msup><mi>x</mi><mn>2</mn></msup></math></span></div></foreignObject></svg>"#;
        assert!(validate_rendered_diagram(rendered(math)).is_ok());

        for unsafe_svg in [
            r#"<svg viewBox="0 0 100 50"><script>alert(1)</script></svg>"#,
            r#"<svg viewBox="0 0 100 50"><image href="https://example.test/a.png"/></svg>"#,
            r#"<svg viewBox="0 0 100 50"><foreignObject><script>alert(1)</script></foreignObject></svg>"#,
            r#"<svg viewBox="0 0 100 50"><foreignObject><div>plain HTML</div></foreignObject></svg>"#,
            r#"<svg viewBox="0 0 100 50" onclick="bad()"><rect/></svg>"#,
            r#"<!DOCTYPE svg><svg viewBox="0 0 100 50"><rect/></svg>"#,
            r#"<svg viewBox="0 0 20000 50"><rect/></svg>"#,
        ] {
            assert!(validate_rendered_diagram(rendered(unsafe_svg)).is_err());
        }
    }

    #[test]
    fn rejects_forged_dimensions_and_renderer_identity() {
        let svg = r#"<svg viewBox="0 0 100 50"><rect width="10" height="10"/></svg>"#;
        let mut diagram = rendered(svg);
        diagram.width = 99.0;
        assert_eq!(
            validate_rendered_diagram(diagram).unwrap_err().code,
            "DIAGRAM_SVG_REJECTED"
        );
        let mut diagram = rendered(svg);
        diagram.renderer_id = "mermaid-offline-12.0.0".to_string();
        assert!(validate_rendered_diagram(diagram).is_err());
    }
}
