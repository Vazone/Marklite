//! Adapt validated syntax to the fixed MathML engine without changing user source.
use std::borrow::Cow;

pub(super) fn mathml_source(source: &str) -> Cow<'_, str> {
    let mut chars = source.char_indices().peekable();
    let mut result = String::new();
    let mut copied = 0;
    while let Some((start, ch)) = chars.next() {
        if ch != '\\' {
            continue;
        }
        let name_start = chars.peek().map_or(source.len(), |(i, _)| *i);
        while chars.peek().is_some_and(|(_, c)| c.is_ascii_alphabetic()) {
            chars.next();
        }
        let name_end = chars.peek().map_or(source.len(), |(i, _)| *i);
        if name_start == name_end {
            // A control symbol (including \\) cannot start an environment.
            chars.next();
            continue;
        }
        let command = &source[name_start..name_end];
        let alias = match command {
            "le" => Some(r"\leq "),
            "ge" => Some(r"\geq "),
            "neg" => Some(r"\lnot "),
            "mathcal" => Some(r"\mathscr "),
            _ => None,
        };
        if let Some(alias) = alias {
            result.push_str(&source[copied..start]);
            result.push_str(alias);
            copied = name_end;
            continue;
        }
        if command != "begin" && command != "end" {
            continue;
        }
        while chars.peek().is_some_and(|(_, c)| c.is_ascii_whitespace()) {
            chars.next();
        }
        if !matches!(chars.next(), Some((_, '{'))) {
            continue;
        }
        let env_start = chars.peek().map_or(source.len(), |(i, _)| *i);
        for (end, ch) in chars.by_ref() {
            if ch != '}' {
                continue;
            }
            if &source[env_start..end] == "cases" {
                result.push_str(&source[copied..start]);
                // align supplies left-aligned rows; the invisible closing fence
                // preserves the conventional single brace of a piecewise function.
                result.push_str(if command == "begin" {
                    r"\left\{\begin{align}"
                } else {
                    r"\end{align}\right."
                });
                copied = end + 1;
            }
            break;
        }
    }
    if copied == 0 {
        Cow::Borrowed(source)
    } else {
        result.push_str(&source[copied..]);
        Cow::Owned(result)
    }
}

#[cfg(test)]
mod tests {
    use super::mathml_source;

    #[test]
    fn leaves_escaped_commands_and_other_environments_unchanged() {
        for source in [
            r"\\begin{cases}",
            r"\begin{matrix}a\end{matrix}",
            r"\text{cases}",
        ] {
            assert_eq!(mathml_source(source), source);
        }
    }
}
