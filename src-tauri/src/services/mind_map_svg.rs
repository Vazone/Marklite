use quick_xml::{events::Event, Reader, XmlVersion};

use crate::models::app_error::AppError;

const MAX_MIND_MAP_SVG_BYTES: usize = 16 * 1024 * 1024;
const MAX_MIND_MAP_SVG_ELEMENTS: usize = 250_000;
const MAX_PATH_BYTES: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ElementKind {
    Svg,
    Title,
    Rect,
    Group,
    Path,
    Text,
    Tspan,
}

pub(crate) fn validate(svg: &str) -> Result<(), AppError> {
    if svg.len() > MAX_MIND_MAP_SVG_BYTES {
        return Err(AppError::new(
            "MIND_MAP_SVG_TOO_LARGE",
            "脑图 SVG 超过 16 MiB",
        ));
    }

    let mut reader = Reader::from_str(svg);
    reader.config_mut().check_end_names = true;
    let mut stack = Vec::with_capacity(4);
    let mut declaration_seen = false;
    let mut root_seen = false;
    let mut root_closed = false;
    let mut marker_seen = false;
    let mut title_count = 0_usize;
    let mut element_count = 0_usize;

    loop {
        let event = reader.read_event().map_err(|_| invalid_svg())?;
        match event {
            Event::Decl(declaration) => {
                if declaration_seen
                    || root_seen
                    || declaration.version().ok().as_deref() != Some(b"1.0")
                {
                    return Err(invalid_svg());
                }
                let encoding = declaration
                    .encoding()
                    .and_then(Result::ok)
                    .ok_or_else(invalid_svg)?;
                if !encoding.eq_ignore_ascii_case(b"UTF-8") || declaration.standalone().is_some() {
                    return Err(invalid_svg());
                }
                declaration_seen = true;
            }
            Event::Start(start) => {
                if root_closed {
                    return Err(invalid_svg());
                }
                element_count = element_count.checked_add(1).ok_or_else(invalid_svg)?;
                if element_count > MAX_MIND_MAP_SVG_ELEMENTS {
                    return Err(invalid_svg());
                }
                let kind = validate_element(
                    &start,
                    stack.last().copied(),
                    reader.decoder(),
                    &mut marker_seen,
                )?;
                if kind == ElementKind::Svg {
                    if root_seen || !declaration_seen {
                        return Err(invalid_svg());
                    }
                    root_seen = true;
                } else if !root_seen {
                    return Err(invalid_svg());
                }
                if kind == ElementKind::Title {
                    title_count += 1;
                }
                stack.push(kind);
                if stack.len() > 5 {
                    return Err(invalid_svg());
                }
            }
            Event::Empty(start) => {
                if root_closed || !root_seen {
                    return Err(invalid_svg());
                }
                element_count = element_count.checked_add(1).ok_or_else(invalid_svg)?;
                if element_count > MAX_MIND_MAP_SVG_ELEMENTS {
                    return Err(invalid_svg());
                }
                let kind = validate_element(
                    &start,
                    stack.last().copied(),
                    reader.decoder(),
                    &mut marker_seen,
                )?;
                if !matches!(kind, ElementKind::Rect | ElementKind::Path) {
                    return Err(invalid_svg());
                }
            }
            Event::End(_) => {
                let closed = stack.pop().ok_or_else(invalid_svg)?;
                if closed == ElementKind::Svg {
                    root_closed = true;
                }
            }
            Event::Text(text) => {
                let decoded = text.decode().map_err(|_| invalid_svg())?;
                if !decoded.trim().is_empty()
                    && !matches!(
                        stack.last(),
                        Some(ElementKind::Title | ElementKind::Text | ElementKind::Tspan)
                    )
                {
                    return Err(invalid_svg());
                }
            }
            Event::GeneralRef(reference) => {
                if !matches!(
                    stack.last(),
                    Some(ElementKind::Title | ElementKind::Text | ElementKind::Tspan)
                ) || (!is_predefined_reference(reference.as_ref())
                    && reference
                        .resolve_char_ref()
                        .map_err(|_| invalid_svg())?
                        .is_none())
                {
                    return Err(invalid_svg());
                }
            }
            Event::Eof => break,
            Event::Comment(_) | Event::CData(_) | Event::PI(_) | Event::DocType(_) => {
                return Err(invalid_svg());
            }
        }
    }

    if !declaration_seen
        || !root_seen
        || !root_closed
        || !stack.is_empty()
        || !marker_seen
        || title_count != 1
    {
        return Err(invalid_svg());
    }
    Ok(())
}

fn is_predefined_reference(name: &[u8]) -> bool {
    [b"amp".as_slice(), b"lt", b"gt", b"quot", b"apos"].contains(&name)
}

fn validate_element(
    element: &quick_xml::events::BytesStart<'_>,
    parent: Option<ElementKind>,
    decoder: quick_xml::encoding::Decoder,
    marker_seen: &mut bool,
) -> Result<ElementKind, AppError> {
    let kind = match element.name().as_ref() {
        b"svg" => ElementKind::Svg,
        b"title" => ElementKind::Title,
        b"rect" => ElementKind::Rect,
        b"g" => ElementKind::Group,
        b"path" => ElementKind::Path,
        b"text" => ElementKind::Text,
        b"tspan" => ElementKind::Tspan,
        _ => return Err(invalid_svg()),
    };
    if !valid_parent(kind, parent) {
        return Err(invalid_svg());
    }

    let mut root_xmlns = false;
    let mut root_marker = false;
    let mut root_width = false;
    let mut root_height = false;
    let mut root_view_box = false;
    for attribute in element.attributes().with_checks(true) {
        let attribute = attribute.map_err(|_| invalid_svg())?;
        let name = attribute.key.as_ref();
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Explicit1_0, decoder)
            .map_err(|_| invalid_svg())?;
        if !valid_attribute(kind, name, &value) {
            return Err(invalid_svg());
        }
        if kind == ElementKind::Svg {
            match name {
                b"xmlns" => root_xmlns = true,
                b"data-marklite-mind-map" => root_marker = true,
                b"width" => root_width = true,
                b"height" => root_height = true,
                b"viewBox" => root_view_box = true,
                _ => {}
            }
        }
    }
    if kind == ElementKind::Svg {
        if !(root_xmlns && root_marker && root_width && root_height && root_view_box) {
            return Err(invalid_svg());
        }
        *marker_seen = true;
    }
    Ok(kind)
}

fn valid_parent(kind: ElementKind, parent: Option<ElementKind>) -> bool {
    match kind {
        ElementKind::Svg => parent.is_none(),
        ElementKind::Title => parent == Some(ElementKind::Svg),
        ElementKind::Rect => matches!(parent, Some(ElementKind::Svg | ElementKind::Group)),
        ElementKind::Group => matches!(parent, Some(ElementKind::Svg | ElementKind::Group)),
        ElementKind::Path | ElementKind::Text => parent == Some(ElementKind::Group),
        ElementKind::Tspan => parent == Some(ElementKind::Text),
    }
}

fn valid_attribute(kind: ElementKind, name: &[u8], value: &str) -> bool {
    match (kind, name) {
        (ElementKind::Svg, b"xmlns") => value == "http://www.w3.org/2000/svg",
        (ElementKind::Svg, b"data-marklite-mind-map") => value == "1",
        (ElementKind::Svg, b"viewBox") => valid_view_box(value),
        (ElementKind::Svg, b"width" | b"height") => valid_positive_number(value),
        (ElementKind::Rect, b"x" | b"y") => valid_number(value),
        (ElementKind::Rect, b"width" | b"height" | b"rx") => valid_positive_number(value),
        (ElementKind::Rect, b"fill" | b"stroke") => valid_paint(value),
        (ElementKind::Group, b"aria-label") => !value.is_empty() && value.len() <= 4 * 1024,
        (ElementKind::Group, b"data-node-level") => {
            value.parse::<u8>().is_ok_and(|level| level <= 6)
        }
        (ElementKind::Path, b"d") => valid_path(value),
        (ElementKind::Path, b"fill") => value == "none",
        (ElementKind::Path, b"stroke") => valid_paint(value),
        (ElementKind::Path, b"stroke-width") => value == "2",
        (ElementKind::Path, b"stroke-linecap") => value == "round",
        (ElementKind::Path, b"opacity") => value == "0.76",
        (ElementKind::Text | ElementKind::Tspan, b"x" | b"y") => valid_number(value),
        (ElementKind::Text, b"fill") => valid_paint(value),
        (ElementKind::Text, b"font-size") => valid_positive_number(value),
        (ElementKind::Text, b"font-weight") => matches!(value, "500" | "700"),
        (ElementKind::Text, b"xml:space") => value == "preserve",
        _ => false,
    }
}

fn valid_number(value: &str) -> bool {
    value
        .parse::<f64>()
        .is_ok_and(|number| number.is_finite() && number.abs() <= 1_000_000_000.0)
}

fn valid_positive_number(value: &str) -> bool {
    value
        .parse::<f64>()
        .is_ok_and(|number| number.is_finite() && number > 0.0 && number <= 1_000_000_000.0)
}

fn valid_view_box(value: &str) -> bool {
    let values = value.split_ascii_whitespace().collect::<Vec<_>>();
    values.len() == 4
        && values[0] == "0"
        && values[1] == "0"
        && valid_positive_number(values[2])
        && valid_positive_number(values[3])
}

fn valid_paint(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..]
            .iter()
            .all(|character| character.is_ascii_hexdigit())
}

fn valid_path(value: &str) -> bool {
    value.len() <= MAX_PATH_BYTES
        && value.starts_with("M ")
        && value.contains(" C ")
        && value.bytes().all(|character| {
            character.is_ascii_digit()
                || matches!(character, b'M' | b'C' | b' ' | b'.' | b',' | b'-')
        })
}

fn invalid_svg() -> AppError {
    AppError::new(
        "INVALID_MIND_MAP_SVG",
        "脑图 SVG 不是 MarkLite 生成的独立安全矢量画布",
    )
}

#[cfg(test)]
mod tests {
    use super::validate;

    const VALID: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 80" width="100" height="80" data-marklite-mind-map="1">
  <title>A &amp; B</title>
  <rect width="100" height="80" fill="#ffffff"/>
  <g aria-label="connectors"><path d="M 1 2 C 3 2, 3 4, 5 4" fill="none" stroke="#159b9b" stroke-width="2" stroke-linecap="round" opacity="0.76"/></g>
  <g aria-label="nodes"><g data-node-level="1"><rect x="1" y="2" width="30" height="20" rx="9" fill="#eefafa" stroke="#159b9b"/><text xml:space="preserve" fill="#172033" font-size="15" font-weight="500"><tspan x="4" y="8">Node</tspan></text></g></g>
</svg>"##;

    #[test]
    fn accepts_only_the_generator_vocabulary() {
        validate(VALID).unwrap();
    }

    #[test]
    fn rejects_events_links_styles_entities_and_unbounded_geometry() {
        for attack in [
            VALID.replace("<svg ", "<svg onload=\"alert(1)\" "),
            VALID.replace("<rect width", "<rect href=\"https://example.com/a\" width"),
            VALID.replace(
                "<text xml:space",
                "<text style=\"fill:url(https://example.com)\" xml:space",
            ),
            VALID.replace(
                "<title>A &amp; B</title>",
                "<!DOCTYPE svg [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><title>&x;</title>",
            ),
            VALID.replace("width=\"100\"", "width=\"1e999\""),
            VALID.replace("<rect width", "<foreignObject><rect width"),
        ] {
            assert_eq!(validate(&attack).unwrap_err().code, "INVALID_MIND_MAP_SVG");
        }
    }
}
