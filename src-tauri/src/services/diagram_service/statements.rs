//! Statement boundaries outside quoted labels and node/shape delimiters.
pub(super) fn split(source: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut comment = false;
    let mut previous = ' ';
    for (offset, character) in source.char_indices() {
        if comment {
            if character == '\n' {
                result.push(&source[start..offset]);
                start = offset + 1;
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
        } else {
            match character {
                '%' if source[offset..].starts_with("%%") => comment = true,
                '"' => quote = Some(character),
                '\'' if previous.is_whitespace() || "([{,:=".contains(previous) => {
                    quote = Some(character)
                }
                '[' | '(' | '{' => depth += 1,
                ']' | ')' | '}' => depth = depth.saturating_sub(1),
                '\n' | '\r' => {
                    result.push(&source[start..offset]);
                    start = offset + character.len_utf8();
                }
                ';' if depth == 0 => {
                    result.push(&source[start..offset]);
                    start = offset + 1;
                }
                _ => (),
            }
        }
        previous = character;
    }
    result.push(&source[start..]);
    result
}
