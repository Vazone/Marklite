use std::sync::atomic::{AtomicU64, Ordering};
use std::{iter::Peekable, str::CharIndices};

use html_escape::{encode_double_quoted_attribute, encode_text};
use latex2mathml::{latex_to_mathml, DisplayStyle};
use pulldown_cmark::{CowStr, Event};

use crate::models::export::ExportWarning;

pub(crate) const MAX_FORMULAS: usize = 256;
const MAX_FORMULA_BYTES: usize = 4_096;
const MAX_NESTING: usize = 64;
const MAX_COMMANDS: usize = 256;
const SUPPORTED_ENVIRONMENTS: &[&str] = &["matrix", "pmatrix", "bmatrix", "vmatrix", "align"];
static RENDER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const ALLOWED_COMMANDS: &[&str] = &[
    "alpha",
    "beta",
    "gamma",
    "delta",
    "epsilon",
    "varepsilon",
    "zeta",
    "eta",
    "theta",
    "vartheta",
    "iota",
    "kappa",
    "lambda",
    "mu",
    "nu",
    "xi",
    "pi",
    "varpi",
    "rho",
    "varrho",
    "sigma",
    "varsigma",
    "tau",
    "upsilon",
    "phi",
    "varphi",
    "chi",
    "psi",
    "omega",
    "Gamma",
    "Delta",
    "Theta",
    "Lambda",
    "Xi",
    "Pi",
    "Sigma",
    "Upsilon",
    "Phi",
    "Psi",
    "Omega",
    "frac",
    "sqrt",
    "sum",
    "prod",
    "int",
    "iint",
    "iiint",
    "lim",
    "sin",
    "cos",
    "tan",
    "log",
    "ln",
    "exp",
    "min",
    "max",
    "left",
    "right",
    "cdot",
    "times",
    "div",
    "pm",
    "mp",
    "le",
    "leq",
    "ge",
    "geq",
    "neq",
    "approx",
    "sim",
    "equiv",
    "in",
    "notin",
    "subset",
    "subseteq",
    "supset",
    "supseteq",
    "cup",
    "cap",
    "to",
    "rightarrow",
    "leftarrow",
    "Rightarrow",
    "Leftarrow",
    "leftrightarrow",
    "infty",
    "partial",
    "nabla",
    "forall",
    "exists",
    "neg",
    "land",
    "lor",
    "overline",
    "underline",
    "hat",
    "bar",
    "vec",
    "text",
    "mathrm",
    "mathbf",
    "mathit",
    "mathbb",
    "mathcal",
    "mathfrak",
    "operatorname",
    "begin",
    "end",
    "quad",
    "qquad",
    ",",
    ";",
    "!",
    " ",
    "{",
    "}",
    "%",
    "&",
    "_",
    "$",
    "\\",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MathRenderError {
    pub code: &'static str,
    pub message: String,
}

pub(crate) struct MathHtml {
    pub events: Vec<Event<'static>>,
    replacements: Vec<(String, String)>,
    pub warnings: Vec<ExportWarning>,
    #[cfg(test)]
    pub formula_count: usize,
}

pub(crate) fn render_math_events(events: Vec<Event<'static>>) -> MathHtml {
    render_math_events_from(events, 0)
}

pub(crate) fn render_math_events_from(
    events: Vec<Event<'static>>,
    prior_formulas: usize,
) -> MathHtml {
    if !events
        .iter()
        .any(|event| matches!(event, Event::InlineMath(_) | Event::DisplayMath(_)))
    {
        return MathHtml {
            events,
            replacements: Vec::new(),
            warnings: Vec::new(),
            #[cfg(test)]
            formula_count: 0,
        };
    }
    let token = format!(
        "MARKLITE_MATH_{}_{}",
        std::process::id(),
        RENDER_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let mut output = Vec::with_capacity(events.len());
    let mut replacements = Vec::new();
    let mut warnings = Vec::new();
    let mut formula_count = 0usize;

    for event in events {
        let (source, display) = match event {
            Event::InlineMath(source) => (source, false),
            Event::DisplayMath(source) => (source, true),
            other => {
                output.push(other);
                continue;
            }
        };
        formula_count += 1;
        let document_ordinal = prior_formulas + formula_count;
        if document_ordinal > MAX_FORMULAS + 1 {
            let delimiter = if display { "$$" } else { "$" };
            output.push(Event::Text(CowStr::Boxed(
                format!("{delimiter}{source}{delimiter}").into_boxed_str(),
            )));
            continue;
        }
        let rendered = if document_ordinal > MAX_FORMULAS {
            Err(MathRenderError {
                code: "MATH_DOCUMENT_LIMIT_EXCEEDED",
                message: format!("document contains more than {MAX_FORMULAS} formulas"),
            })
        } else {
            render_mathml(source.as_ref(), display)
        };
        let html = match rendered {
            Ok(mathml) => mathml,
            Err(error) => {
                warnings.push(ExportWarning::new(
                    error.code,
                    error.message.clone(),
                    Some(source.to_string()),
                ));
                math_error_html(source.as_ref(), display, &error)
            }
        };
        let placeholder = format!("{token}_{}_END", replacements.len());
        output.push(Event::Text(CowStr::Boxed(
            placeholder.clone().into_boxed_str(),
        )));
        replacements.push((placeholder, html));
    }

    MathHtml {
        events: output,
        replacements,
        warnings,
        #[cfg(test)]
        formula_count,
    }
}

impl MathHtml {
    pub(crate) fn substitute(self, sanitized_html: &str) -> (String, Vec<ExportWarning>) {
        let mut html = String::with_capacity(sanitized_html.len());
        let mut remaining = sanitized_html;
        for (placeholder, replacement) in self.replacements {
            if let Some(offset) = remaining.find(&placeholder) {
                html.push_str(&remaining[..offset]);
                html.push_str(&replacement);
                remaining = &remaining[offset + placeholder.len()..];
            }
        }
        html.push_str(remaining);
        (html, self.warnings)
    }
}

pub(crate) fn render_mathml(source: &str, display: bool) -> Result<String, MathRenderError> {
    validate_formula(source)?;
    latex_to_mathml(
        source,
        if display {
            DisplayStyle::Block
        } else {
            DisplayStyle::Inline
        },
    )
    .map_err(|error| MathRenderError {
        code: "MATH_RENDER_FAILED",
        message: format!("unsupported or invalid math expression: {error}"),
    })
}

pub(crate) fn render_omml(source: &str) -> Result<String, MathRenderError> {
    validate_formula(source)?;
    let omml = tex2word_math::to_omath(source);
    if !omml.starts_with("<m:oMath>") || !omml.ends_with("</m:oMath>") {
        return Err(MathRenderError {
            code: "MATH_OMML_FAILED",
            message: "math renderer did not produce a complete OfficeMath element".to_string(),
        });
    }
    Ok(omml)
}

fn validate_formula(source: &str) -> Result<(), MathRenderError> {
    if source.is_empty() || source.len() > MAX_FORMULA_BYTES {
        return Err(limit_error(format!(
            "formula must contain 1..={MAX_FORMULA_BYTES} UTF-8 bytes"
        )));
    }
    let mut open_braces = Vec::new();
    let mut environments: Vec<(String, usize)> = Vec::new();
    let mut commands = 0usize;
    let mut chars = source.char_indices().peekable();
    while let Some((byte_index, character)) = chars.next() {
        match character {
            '{' => {
                open_braces.push(byte_index);
                if open_braces.len() > MAX_NESTING {
                    return Err(limit_error(format!(
                        "formula nesting exceeds {MAX_NESTING}"
                    )));
                }
            }
            '}' => {
                if open_braces.pop().is_none() {
                    return Err(syntax_error_at(
                        "formula contains an unmatched closing brace",
                        byte_index,
                    ));
                }
            }
            '\\' => {
                commands += 1;
                if commands > MAX_COMMANDS {
                    return Err(limit_error(format!(
                        "formula contains more than {MAX_COMMANDS} commands"
                    )));
                }
                let command = if chars
                    .peek()
                    .is_some_and(|(_, next)| next.is_ascii_alphabetic())
                {
                    let mut command = String::new();
                    while let Some((_, next)) = chars.peek().copied() {
                        if !next.is_ascii_alphabetic() {
                            break;
                        }
                        command.push(next);
                        chars.next();
                    }
                    command
                } else {
                    chars
                        .next()
                        .map(|(_, value)| value.to_string())
                        .unwrap_or_default()
                };
                if !ALLOWED_COMMANDS.contains(&command.as_str()) {
                    return Err(MathRenderError {
                        code: "MATH_COMMAND_UNSUPPORTED",
                        message: format!(
                            "math command \\{command} at byte {byte_index} is outside the supported subset"
                        ),
                    });
                }
                if command == "begin" || command == "end" {
                    let environment = read_environment_name(&mut chars, byte_index)?;
                    if !SUPPORTED_ENVIRONMENTS.contains(&environment.as_str()) {
                        return Err(MathRenderError {
                            code: "MATH_ENVIRONMENT_UNSUPPORTED",
                            message: format!(
                                "math environment {environment} at byte {byte_index} is outside the supported subset"
                            ),
                        });
                    }
                    if command == "begin" {
                        environments.push((environment, byte_index));
                        if environments.len() + open_braces.len() > MAX_NESTING {
                            return Err(limit_error(format!(
                                "formula nesting exceeds {MAX_NESTING} at byte {byte_index}"
                            )));
                        }
                    } else if let Some((open, _)) = environments.pop() {
                        if open != environment {
                            return Err(syntax_error_at(
                                &format!("math environment {open} closes as {environment}"),
                                byte_index,
                            ));
                        }
                    } else {
                        return Err(syntax_error_at(
                            &format!("math environment {environment} closes without an opening"),
                            byte_index,
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    if let Some((environment, byte_index)) = environments.pop() {
        return Err(syntax_error_at(
            &format!("math environment {environment} is not closed"),
            byte_index,
        ));
    }
    if let Some(byte_index) = open_braces.pop() {
        return Err(syntax_error_at(
            "formula contains an unclosed brace",
            byte_index,
        ));
    }
    Ok(())
}

fn read_environment_name(
    chars: &mut Peekable<CharIndices<'_>>,
    command_index: usize,
) -> Result<String, MathRenderError> {
    while chars
        .peek()
        .is_some_and(|(_, value)| value.is_ascii_whitespace())
    {
        chars.next();
    }
    if !matches!(chars.next(), Some((_, '{'))) {
        return Err(syntax_error_at(
            "math environment command requires a braced name",
            command_index,
        ));
    }
    let mut name = String::new();
    for (_, value) in chars.by_ref() {
        if value == '}' {
            if name.is_empty() {
                return Err(syntax_error_at(
                    "math environment name cannot be empty",
                    command_index,
                ));
            }
            return Ok(name);
        }
        if !value.is_ascii_alphabetic() {
            return Err(syntax_error_at(
                "math environment name must contain only ASCII letters",
                command_index,
            ));
        }
        name.push(value);
    }
    Err(syntax_error_at(
        "math environment name is not closed",
        command_index,
    ))
}

fn math_error_html(source: &str, display: bool, error: &MathRenderError) -> String {
    let tag = if display { "div" } else { "code" };
    format!(
        "<{tag} class=\"math-error\" title=\"{}\">{}</{tag}>",
        encode_double_quoted_attribute(&error.message),
        encode_text(source)
    )
}

fn limit_error(message: String) -> MathRenderError {
    MathRenderError {
        code: "MATH_LIMIT_EXCEEDED",
        message,
    }
}

fn syntax_error(message: &str) -> MathRenderError {
    MathRenderError {
        code: "MATH_SYNTAX_INVALID",
        message: message.to_string(),
    }
}

fn syntax_error_at(message: &str, byte_index: usize) -> MathRenderError {
    syntax_error(&format!("{message} at byte {byte_index}"))
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{render_math_events, render_mathml, render_omml};
    use pulldown_cmark::{html, CowStr, Event};

    #[test]
    fn renders_the_shared_basic_subset_to_mathml_and_editable_omml() {
        let source = r"\frac{-b \pm \sqrt{b^2-4ac}}{2a}";
        let mathml = render_mathml(source, false).unwrap();
        let omml = render_omml(source).unwrap();
        assert!(mathml.contains("<math"));
        assert!(mathml.contains("<mfrac>"));
        assert!(omml.contains("<m:oMath>"));
        assert!(omml.contains("<m:f>"));
    }

    #[test]
    fn renders_each_named_shared_math_family_to_mathml_and_editable_omml() {
        let cases = [
            ("scripts", r"x_i^2", "<msub>", "<m:sSubSup>"),
            ("greek", r"\alpha + \Omega", "<mi>α</mi>", "α"),
            ("fraction", r"\frac{a}{b}", "<mfrac>", "<m:f>"),
            ("root", r"\sqrt{x}", "<msqrt>", "<m:rad>"),
            ("sum", r"\sum_{i=1}^{n} i", "<munderover>", "<m:nary>"),
            ("integral", r"\int_0^1 x\,dx", "<msubsup>", "<m:nary>"),
            (
                "matrix",
                r"\begin{pmatrix}a & b \\ c & d\end{pmatrix}",
                "<mtable",
                "<m:m>",
            ),
            (
                "alignment",
                r"\begin{align}a &= b + c \\ d &= e\end{align}",
                "<mtable",
                "<m:m>",
            ),
        ];
        for (name, source, mathml_marker, omml_marker) in cases {
            let mathml = render_mathml(source, true)
                .unwrap_or_else(|error| panic!("{name} MathML: {error:?}"));
            let omml = render_omml(source).unwrap_or_else(|error| panic!("{name} OMML: {error:?}"));
            assert!(mathml.contains(mathml_marker), "{name}: {mathml}");
            assert!(omml.contains(omml_marker), "{name}: {omml}");
        }
    }

    #[test]
    fn rejects_unknown_or_mismatched_environments_before_either_renderer() {
        for (source, expected) in [
            (
                r"\begin{cases}x & x>0\end{cases}",
                "MATH_ENVIRONMENT_UNSUPPORTED",
            ),
            (r"\begin{matrix}x\end{pmatrix}", "MATH_SYNTAX_INVALID"),
            (r"\begin{matrix}x", "MATH_SYNTAX_INVALID"),
        ] {
            let mathml = render_mathml(source, true).unwrap_err();
            let omml = render_omml(source).unwrap_err();
            assert_eq!(mathml.code, expected, "{source}");
            assert_eq!(omml.code, expected, "{source}");
            assert!(mathml.message.contains("byte"), "{}", mathml.message);
            assert!(omml.message.contains("byte"), "{}", omml.message);
        }
    }

    #[test]
    fn rejects_urls_user_macros_and_unbounded_inputs() {
        assert_eq!(
            render_mathml(r"\href{https://example.com}{x}", false)
                .unwrap_err()
                .code,
            "MATH_COMMAND_UNSUPPORTED"
        );
        assert!(render_mathml(r"x + \href{https://example.com}{x}", false)
            .unwrap_err()
            .message
            .contains("byte 4"));
        assert_eq!(
            render_mathml(r"\gdef\x{1}\x", false).unwrap_err().code,
            "MATH_COMMAND_UNSUPPORTED"
        );
        assert_eq!(
            render_mathml(&"{".repeat(65), false).unwrap_err().code,
            "MATH_LIMIT_EXCEEDED"
        );
        assert_eq!(
            render_mathml(&"x".repeat(4097), false).unwrap_err().code,
            "MATH_LIMIT_EXCEEDED"
        );
    }

    #[test]
    fn mathml_renderer_escapes_document_controlled_text() {
        let mathml =
            render_mathml(r"\text{</annotation><script>alert(1)</script>}", false).unwrap();
        assert!(!mathml.contains("<script>"), "{mathml}");
        assert!(!mathml.contains("</annotation><script>"), "{mathml}");
    }

    #[test]
    fn invalid_math_is_visible_and_structurally_warned() {
        let rendered =
            render_math_events(vec![Event::InlineMath(CowStr::Borrowed(r"\unknown{x}"))]);
        let mut raw = String::new();
        html::push_html(&mut raw, rendered.events.iter().cloned());
        let (rendered_html, warnings) = rendered.substitute(&raw);
        assert!(rendered_html.contains("math-error"));
        assert_eq!(warnings[0].code, "MATH_COMMAND_UNSUPPORTED");
    }

    #[test]
    fn bounds_dense_documents_without_dropping_formula_source() {
        let events = (0..=super::MAX_FORMULAS)
            .map(|_| Event::InlineMath(CowStr::Borrowed("x^2")))
            .collect();
        let rendered = render_math_events(events);
        assert_eq!(rendered.formula_count, super::MAX_FORMULAS + 1);
        assert_eq!(rendered.warnings.len(), 1);
        assert_eq!(rendered.warnings[0].code, "MATH_DOCUMENT_LIMIT_EXCEEDED");
        let mut raw = String::new();
        html::push_html(&mut raw, rendered.events.iter().cloned());
        let (html, _) = rendered.substitute(&raw);
        assert_eq!(html.matches("<math").count(), super::MAX_FORMULAS);
        assert!(html.contains("math-error"));
        assert!(html.contains("x^2"));
    }

    #[test]
    fn excess_formulas_have_one_warning_and_bounded_error_slots() {
        let events = (0..super::MAX_FORMULAS + 20)
            .map(|index| Event::InlineMath(CowStr::Boxed(format!("x_{index}").into_boxed_str())))
            .collect();
        let rendered = render_math_events(events);
        assert_eq!(rendered.formula_count, super::MAX_FORMULAS + 20);
        assert_eq!(rendered.warnings.len(), 1);
        assert_eq!(rendered.replacements.len(), super::MAX_FORMULAS + 1);
        let mut raw = String::new();
        html::push_html(&mut raw, rendered.events.iter().cloned());
        let (html, _) = rendered.substitute(&raw);
        assert_eq!(html.matches("<math").count(), super::MAX_FORMULAS);
        assert_eq!(html.matches("math-error").count(), 1);
        assert!(html.contains("$x_275$"));
    }

    #[test]
    #[ignore = "reference-machine release performance probe; run explicitly"]
    fn measure_plain_and_dense_math_dispatch_cost() {
        fn measure(events: &[Event<'static>]) -> Vec<f64> {
            let mut samples = Vec::with_capacity(30);
            for _ in 0..30 {
                let started = Instant::now();
                let rendered = render_math_events(events.to_vec());
                assert!(rendered.warnings.is_empty());
                samples.push(started.elapsed().as_secs_f64() * 1_000.0);
            }
            samples.sort_by(f64::total_cmp);
            samples
        }

        let plain = vec![Event::Text(CowStr::Borrowed("ordinary text")); 256];
        let dense = (0..super::MAX_FORMULAS)
            .map(|_| Event::InlineMath(CowStr::Borrowed(r"\frac{x^2+\alpha}{\sqrt{y}}")))
            .collect::<Vec<_>>();
        let plain_samples = measure(&plain);
        let dense_samples = measure(&dense);
        println!(
            "plain_p50_ms={:.3} plain_p95_ms={:.3} dense256_p50_ms={:.3} dense256_p95_ms={:.3}",
            plain_samples[14], plain_samples[28], dense_samples[14], dense_samples[28]
        );
    }

    #[test]
    #[ignore = "reference-machine release performance probe; run explicitly"]
    fn measure_dense_math_substitution_cost() {
        let mut events = vec![Event::Text(CowStr::Boxed(
            "a".repeat(512_000).into_boxed_str(),
        ))];
        events.extend((0..super::MAX_FORMULAS).map(|_| Event::InlineMath(CowStr::Borrowed("x^2"))));
        let mut samples = Vec::with_capacity(30);
        for _ in 0..30 {
            let rendered = render_math_events(events.clone());
            let mut raw = String::new();
            html::push_html(&mut raw, rendered.events.iter().cloned());
            let started = Instant::now();
            let (output, warnings) = rendered.substitute(&raw);
            samples.push(started.elapsed().as_secs_f64() * 1_000.0);
            assert!(warnings.is_empty());
            assert_eq!(output.matches("<math").count(), super::MAX_FORMULAS);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "dense_math_substitution_500k_256_p50_ms={:.3} p95_ms={:.3}",
            samples[14], samples[28]
        );
    }
}
