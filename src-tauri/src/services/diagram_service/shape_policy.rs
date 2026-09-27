//! Inspect actual flowchart shape metadata; text labels are not resource declarations.
use saphyr_parser::{Event, Parser};

pub(super) fn has_resource(source: &str) -> bool {
    let mut chars = source.char_indices().peekable();
    let mut quote = None;
    let mut escaped = false;
    let mut comment = false;
    let mut previous = ' ';
    while let Some((offset, character)) = chars.next() {
        if comment {
            if matches!(character, '\n' | '\r') {
                comment = false;
            }
        } else if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == delimiter {
                quote = None;
            }
        } else if character == '"'
            || (character == '\'' && (previous.is_whitespace() || "([{,:=".contains(previous)))
        {
            quote = Some(character);
        } else if source[offset..].starts_with("%%") {
            comment = true;
        } else if source[offset..].starts_with("@{") {
            let start = offset + 1;
            let Some(end) = shape_end(&source[start..]) else {
                return true;
            };
            if unsafe_mapping(&source[start..start + end]) {
                return true;
            }
            while chars.peek().is_some_and(|(at, _)| *at < start + end) {
                chars.next();
            }
        }
        previous = character;
    }
    false
}

fn shape_end(source: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (offset, character) in source.char_indices() {
        if let Some(delimiter) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == delimiter {
                quote = None;
            }
        } else {
            match character {
                '"' | '\'' => quote = Some(character),
                '{' => depth += 1,
                '}' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(offset + 1);
                    }
                }
                _ => (),
            }
        }
    }
    None
}

fn unsafe_mapping(source: &str) -> bool {
    let mut depth = 0usize;
    let mut key = true;
    for (count, event) in Parser::new_from_str(source).enumerate() {
        if count >= 1024 {
            return true;
        }
        let Ok((event, _)) = event else { return true };
        match event {
            Event::Alias(_) => return true,
            Event::Scalar(_, _, anchor, ref tag)
            | Event::MappingStart(anchor, ref tag)
            | Event::SequenceStart(anchor, ref tag)
                if anchor != 0 || tag.is_some() =>
            {
                return true
            }
            Event::MappingStart(..) | Event::SequenceStart(..) => depth += 1,
            Event::MappingEnd | Event::SequenceEnd => {
                depth = depth.saturating_sub(1);
                if depth == 1 {
                    key = !key;
                }
            }
            Event::Scalar(value, ..) if depth == 1 => {
                if key && matches!(value.as_ref(), "img" | "icon") {
                    return true;
                }
                key = !key;
            }
            _ => (),
        }
    }
    false
}
