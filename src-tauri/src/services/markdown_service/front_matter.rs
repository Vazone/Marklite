use std::ops::Range;

use saphyr_parser::{Event, Parser};

const MAX_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 32;
const MAX_EVENTS: usize = 4096;

use crate::models::markdown::MarkdownDiagnostic as Diagnostic;

#[derive(Debug)]
pub(super) struct FrontMatter {
    pub range: Range<usize>,
    pub diagnostic: Option<Diagnostic>,
}

fn issue(code: &'static str, message: impl Into<String>, line: usize, column: usize) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        message: message.into(),
        line,
        column,
    }
}

// Only a column-zero opening fence on the first line opts into metadata.
// Scan at most the metadata budget; do not walk a large document looking for
// an absent closing fence. Returned ranges always refer to the original bytes.
pub(super) fn inspect(source: &str) -> Option<FrontMatter> {
    let start = if source.starts_with('\u{feff}') { 3 } else { 0 };
    if !source[start..].starts_with("---") {
        return None;
    }
    // Keep enough lookahead to avoid mistaking a UTF-8-truncated line for a
    // complete closing fence at the byte boundary.
    let mut limit = source.len().min(MAX_BYTES + 4);
    while !source.is_char_boundary(limit) {
        limit -= 1;
    }
    let scan = &source[..limit];
    let (first, body_start) = line(scan, start);
    if first.trim_end_matches([' ', '\t']) != "---" {
        return None;
    }
    let mut at = body_start;
    while at < scan.len() && at <= MAX_BYTES {
        let (text, next) = line(scan, at);
        if next > MAX_BYTES {
            break;
        }
        if matches!(text.trim_end_matches([' ', '\t']), "---" | "...") {
            return Some(FrontMatter {
                range: 0..next,
                diagnostic: validate(&source[body_start..at]).err(),
            });
        }
        at = next;
    }
    let (code, message) = if source.len() > MAX_BYTES {
        (
            "FRONT_MATTER_TOO_LARGE",
            "Front Matter exceeds the 64 KiB limit",
        )
    } else {
        (
            "FRONT_MATTER_UNCLOSED",
            "Front Matter requires a closing --- or ... line",
        )
    };
    Some(FrontMatter {
        range: 0..body_start,
        diagnostic: Some(issue(code, message, 1, 1)),
    })
}

fn line(source: &str, start: usize) -> (&str, usize) {
    let end = source[start..]
        .find(['\r', '\n'])
        .map_or(source.len(), |offset| start + offset);
    let next = if source.as_bytes().get(end) == Some(&b'\r')
        && source.as_bytes().get(end + 1) == Some(&b'\n')
    {
        end + 2
    } else if end < source.len() {
        end + 1
    } else {
        end
    };
    (&source[start..end], next)
}

// Validate the event stream without materializing or expanding a YAML graph.
// Metadata is retained verbatim, including unknown keys; no tags, aliases or
// anchors are interpreted and no filesystem/network resolver is installed.
fn validate(body: &str) -> Result<(), Diagnostic> {
    let mut depth = 0;
    let mut documents = 0;
    let mut root_seen = false;
    for (count, event) in Parser::new_from_str(body).enumerate() {
        let (event, span) = event.map_err(|error| {
            issue(
                "FRONT_MATTER_INVALID",
                error.info(),
                error.marker().line() + 1,
                error.marker().col() + 1,
            )
        })?;
        let fail =
            |code, message| issue(code, message, span.start.line() + 1, span.start.col() + 1);
        if count >= MAX_EVENTS {
            return Err(fail(
                "FRONT_MATTER_EVENT_LIMIT",
                "Front Matter exceeds the 4096 event limit",
            ));
        }
        match &event {
            Event::DocumentStart(_) => {
                documents += 1;
                if documents > 1 {
                    return Err(fail(
                        "FRONT_MATTER_MULTIPLE_DOCUMENTS",
                        "Only one metadata document is allowed",
                    ));
                }
            }
            Event::Alias(_) => {
                return Err(fail(
                    "FRONT_MATTER_ALIAS",
                    "Metadata aliases are not supported",
                ))
            }
            Event::Scalar(_, _, anchor, tag)
            | Event::SequenceStart(anchor, tag)
            | Event::MappingStart(anchor, tag) => {
                if *anchor != 0 {
                    return Err(fail(
                        "FRONT_MATTER_ANCHOR",
                        "Metadata anchors are not supported",
                    ));
                }
                if tag.is_some() {
                    return Err(fail(
                        "FRONT_MATTER_TAG",
                        "Explicit metadata tags are not supported",
                    ));
                }
            }
            _ => {}
        }
        match event {
            Event::MappingStart(..) | Event::SequenceStart(..) => {
                if !root_seen {
                    root_seen = true;
                    if !matches!(event, Event::MappingStart(..)) {
                        return Err(fail("FRONT_MATTER_ROOT", "Metadata must be a mapping"));
                    }
                }
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(fail(
                        "FRONT_MATTER_DEPTH",
                        "Metadata nesting exceeds 32 levels",
                    ));
                }
            }
            Event::MappingEnd | Event::SequenceEnd => depth -= 1,
            Event::Scalar(value, _, _, _) if !root_seen => {
                root_seen = true;
                if !value.is_empty() {
                    return Err(fail("FRONT_MATTER_ROOT", "Metadata must be a mapping"));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_mapping_empty_and_preserves_unknown_source() {
        for source in [
            "---\ntitle: Example\nunknown: [a, b]\n---\n# Body",
            "---\n---",
            "\u{feff}---\r\ntitle: 中文\r\n...\r\n# Body",
            "---\rtitle: Example\r---\rBody",
        ] {
            let metadata = inspect(source).unwrap();
            assert!(metadata.diagnostic.is_none(), "{metadata:?}");
            assert!(source[metadata.range.clone()].starts_with(['-', '\u{feff}']));
        }
        let source = "---\nunknown: [a, b]\n---\n# Body";
        let metadata = inspect(source).unwrap();
        assert_eq!(
            &source[metadata.range.clone()],
            "---\nunknown: [a, b]\n---\n"
        );
        assert_eq!(&source[metadata.range.end..], "# Body");
    }

    #[test]
    fn ignores_noninitial_indented_and_code_fences() {
        for source in [
            "Text\n---\na: b\n---",
            " ---\na: b\n---",
            "```yaml\na: b\n```",
            "\n---\na: b\n---",
            "----\na: b\n---",
        ] {
            assert!(inspect(source).is_none(), "{source}");
        }
    }

    #[test]
    fn rejects_unsafe_or_malformed_metadata_with_source_location() {
        for (body, code) in [
            ("a: &a value", "FRONT_MATTER_ANCHOR"),
            ("a: !include file", "FRONT_MATTER_TAG"),
            ("a: [", "FRONT_MATTER_INVALID"),
            ("[a, b]", "FRONT_MATTER_ROOT"),
            ("plain", "FRONT_MATTER_ROOT"),
        ] {
            let diagnostic = inspect(&format!("---\n{body}\n---\n# Body"))
                .unwrap()
                .diagnostic
                .unwrap();
            assert_eq!(diagnostic.code, code);
            assert!(
                diagnostic.line >= 2 && diagnostic.column >= 1,
                "{diagnostic:?}"
            );
        }
    }

    #[test]
    fn enforces_byte_depth_event_and_closing_limits() {
        assert_eq!(
            inspect("---\na: b").unwrap().diagnostic.unwrap().code,
            "FRONT_MATTER_UNCLOSED"
        );
        let huge = format!("---\nx: {}\n---", "x".repeat(MAX_BYTES));
        assert_eq!(
            inspect(&huge).unwrap().diagnostic.unwrap().code,
            "FRONT_MATTER_TOO_LARGE"
        );
        let nested = format!(
            "---\nx: {}0{}\n---",
            "[".repeat(MAX_DEPTH),
            "]".repeat(MAX_DEPTH)
        );
        assert_eq!(
            inspect(&nested).unwrap().diagnostic.unwrap().code,
            "FRONT_MATTER_DEPTH"
        );
        let many = format!("---\nx: [{}]\n---", "0,".repeat(MAX_EVENTS));
        assert_eq!(
            inspect(&many).unwrap().diagnostic.unwrap().code,
            "FRONT_MATTER_EVENT_LIMIT"
        );
        let exact = format!("---\na: {}\n---\n中文", "x".repeat(MAX_BYTES - 12));
        let metadata = inspect(&exact).unwrap();
        assert!(metadata.diagnostic.is_none(), "{metadata:?}");
        assert_eq!(metadata.range.end, MAX_BYTES);
        let not_a_fence = format!("---\na: {}\n---中文", "x".repeat(MAX_BYTES - 12));
        assert!(inspect(&not_a_fence).unwrap().diagnostic.is_some());
    }

    #[test]
    fn valid_header_is_not_a_heading_and_source_positions_stay_original() {
        use crate::services::markdown_service::{normalized_document, render_markdown};
        let source = "\u{feff}---\r\ntitle: 中文\r\nunknown: [a, b]\r\n---\r\n# Body\r\n\r\nText";
        let result = render_markdown(source).unwrap();
        assert_eq!(result.outline.len(), 1);
        assert_eq!(result.outline[0].line, 5);
        assert_eq!(result.outline[0].title, "Body");
        assert!(!result.html.contains("unknown"));
        let document = normalized_document(source);
        assert_eq!(document.source_blocks[0].start_line, 5);
        assert_eq!(
            document.source_blocks[0].start_utf16,
            source[..source.find("# Body").unwrap()]
                .encode_utf16()
                .count()
        );
    }

    #[test]
    fn diagnostics_reach_analysis_and_both_preview_modes() {
        use crate::services::markdown_service::{
            analyze_markdown, preview::prepare_virtual_preview, render_markdown,
        };
        let source = "---\nvalue: [\n---\n# Body";
        let expected = inspect(source).unwrap().diagnostic.unwrap();
        let analysis = analyze_markdown(source);
        assert_eq!(analysis.markdown_diagnostics, vec![expected.clone()]);
        let rendered = render_markdown(source).unwrap();
        assert_eq!(rendered.markdown_diagnostics, vec![expected.clone()]);
        let (virtual_rendered, _) = prepare_virtual_preview(source, "diagnostics".into()).unwrap();
        assert_eq!(virtual_rendered.markdown_diagnostics, vec![expected]);
        assert!(analyze_markdown("---\ntitle: OK\n---\n# Body")
            .markdown_diagnostics
            .is_empty());
    }

    #[test]
    fn invalid_header_keeps_visible_source_and_standard_profiles_are_unchanged() {
        use crate::services::markdown_service::{profile_events, render_markdown, SyntaxProfile};
        let result = render_markdown("---\nvalue: [\n---\n# Body").unwrap();
        assert!(result.html.contains("value: ["));
        for profile in [SyntaxProfile::CommonMark, SyntaxProfile::Gfm] {
            let events = profile_events("---\ntitle: Example\n---\n# Body", profile);
            assert!(events
                .iter()
                .any(|event| matches!(event, crate::models::markdown_event::Event::Rule)));
        }
    }
}
