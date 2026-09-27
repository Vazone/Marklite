//! The accepted Mermaid style subset contains values, never CSS resource syntax.

pub(super) fn allowed(statement: &str) -> bool {
    let Some((_, tail)) = statement.split_once(char::is_whitespace) else {
        return false;
    };
    let Some((targets, declarations)) = tail.trim_start().split_once(char::is_whitespace) else {
        return false;
    };
    if targets.is_empty()
        || !targets
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ','))
    {
        return false;
    }
    let declarations = declarations.trim().trim_end_matches(';');
    !declarations.is_empty()
        && declarations.split(',').all(|declaration| {
            let Some((property, value)) = declaration.trim().split_once(':') else {
                return false;
            };
            let value = value.trim();
            match property.trim() {
                "fill" | "stroke" | "color" => color(value),
                "stroke-width" => number(value.strip_suffix("px").unwrap_or(value), 64.0),
                "opacity" | "fill-opacity" | "stroke-opacity" => number(value, 1.0),
                "stroke-dasharray" => {
                    value == "none"
                        || (!value.is_empty()
                            && value.split_whitespace().count() <= 16
                            && value.split_whitespace().all(|part| number(part, 256.0)))
                }
                "stroke-linecap" => matches!(value, "butt" | "round" | "square"),
                "stroke-linejoin" => matches!(value, "miter" | "round" | "bevel"),
                "font-weight" => matches!(
                    value,
                    "normal"
                        | "bold"
                        | "100"
                        | "200"
                        | "300"
                        | "400"
                        | "500"
                        | "600"
                        | "700"
                        | "800"
                        | "900"
                ),
                "font-style" => matches!(value, "normal" | "italic" | "oblique"),
                _ => false,
            }
        })
}

pub(super) fn color(value: &str) -> bool {
    if let Some(hex) = value.strip_prefix('#') {
        matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
    } else {
        // Unknown color names are inert invalid CSS values; no punctuation means
        // functions, escapes, declarations and resource references cannot pass.
        !value.is_empty() && value.len() <= 32 && value.bytes().all(|b| b.is_ascii_alphabetic())
    }
}

fn number(value: &str, maximum: f64) -> bool {
    !value.is_empty()
        && value.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && value
            .parse::<f64>()
            .is_ok_and(|v| v.is_finite() && v >= 0.0 && v <= maximum)
}

#[cfg(test)]
mod tests {
    use super::allowed;

    #[test]
    fn permits_bounded_colors_and_lines() {
        for statement in [
            "classDef accent fill:#f9f,stroke:#333,stroke-width:2px",
            "style A fill:transparent,color:rebeccapurple,opacity:0.5",
            "linkStyle 0,1 stroke:#123456,stroke-dasharray:5 3,stroke-linecap:round;",
        ] {
            assert!(allowed(statement), "{statement}");
        }
    }

    #[test]
    fn rejects_resources_injection_and_unbounded_values() {
        for statement in [
            "style A fill:url(https://example.test/a)",
            "classDef a fill:var(--remote)",
            "style A fill:#fff;filter:url(#a)",
            "style A fill:red!important",
            "style A stroke-width:999999px",
            "style A opacity:NaN",
            "style A font-family:remote",
            "style A fill:expression(alert(1))",
            "style A fill:rgb(0,0,0)",
            "classDef a}body{ fill:red",
        ] {
            assert!(!allowed(statement), "{statement}");
        }
    }
}
