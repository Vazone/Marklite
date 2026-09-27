use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, OnceLock,
    },
    time::Duration,
};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

const NAMES: [&str; 12] = [
    "keyword",
    "operator",
    "string",
    "comment",
    "number",
    "boolean",
    "constant",
    "type",
    "property",
    "tag",
    "attribute",
    "function",
];
const CLASSES: [&str; 12] = [
    "tok-keyword",
    "tok-operator",
    "tok-string",
    "tok-comment",
    "tok-number",
    "tok-bool",
    "tok-atom",
    "tok-typeName",
    "tok-propertyName",
    "tok-tagName",
    "tok-propertyName",
    "tok-variableName",
];
static CONFIGS: [OnceLock<Result<HighlightConfiguration, String>>; 10] =
    [const { OnceLock::new() }; 10];

#[derive(Debug, Clone)]
pub(crate) struct CodeToken {
    pub start: usize,
    pub end: usize,
    pub class: &'static str,
}

#[derive(serde::Deserialize)]
struct PaletteEntry {
    classes: Vec<String>,
    light: String,
}

fn palette() -> &'static [PaletteEntry] {
    static PALETTE: OnceLock<Vec<PaletteEntry>> = OnceLock::new();
    PALETTE.get_or_init(|| {
        serde_json::from_str(include_str!("../../../src/shared/code-palette.json"))
            .expect("bundled code palette must be valid")
    })
}

pub(crate) fn color(class: &str) -> Option<&'static str> {
    palette()
        .iter()
        .find(|entry| entry.classes.iter().any(|name| name == class))
        .map(|entry| entry.light.as_str())
}

pub(crate) fn styles() -> String {
    palette()
        .iter()
        .map(|entry| {
            format!(
                "{} {{ color: #{}; }}",
                entry
                    .classes
                    .iter()
                    .map(|name| format!("pre .{name}"))
                    .collect::<Vec<_>>()
                    .join(","),
                entry.light
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn html(source: &str, tokens: &[CodeToken]) -> String {
    let mut html = String::new();
    let mut at = 0;
    for token in tokens {
        html.push_str(&html_escape::encode_text(&source[at..token.start]));
        html.push_str(&format!("<span class=\"{}\">", token.class));
        html.push_str(&html_escape::encode_text(&source[token.start..token.end]));
        html.push_str("</span>");
        at = token.end;
    }
    html.push_str(&html_escape::encode_text(&source[at..]));
    html
}

pub(crate) fn node_budget(source: &str, info: &str) -> usize {
    if language(info).is_none()
        || source.encode_utf16().count() > 65536
        || source
            .split(['\n', '\r'])
            .any(|line| line.encode_utf16().count() > 16384)
    {
        return 0;
    }
    // Each token adds a span, its text, and at most one intervening text node.
    source.len().min(8192) * 3 + 1
}

#[derive(Debug, Default)]
pub(crate) struct CodeCache {
    entries: std::collections::VecDeque<(String, String, std::sync::Arc<[CodeToken]>, usize)>,
    bytes: usize,
}

impl CodeCache {
    pub(crate) fn get(
        &mut self,
        source: &str,
        info: &str,
    ) -> Result<std::sync::Arc<[CodeToken]>, String> {
        let label = info
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if let Some(index) = self
            .entries
            .iter()
            .position(|(text, lang, _, _)| text == source && lang == &label)
        {
            let entry = self.entries.remove(index).unwrap();
            let tokens = entry.2.clone();
            self.entries.push_back(entry);
            return Ok(tokens);
        }
        let tokens: std::sync::Arc<[CodeToken]> = tokens(source, info)?.into();
        if node_budget(source, info) == 0 {
            return Ok(tokens);
        }
        let size = source.len() + label.len() + tokens.len() * std::mem::size_of::<CodeToken>();
        while !self.entries.is_empty()
            && (self.entries.len() >= 128 || self.bytes + size > 1024 * 1024)
        {
            self.bytes -= self.entries.pop_front().unwrap().3;
        }
        if size <= 1024 * 1024 {
            self.bytes += size;
            self.entries
                .push_back((source.to_owned(), label, tokens.clone(), size));
        }
        Ok(tokens)
    }
}

fn language(info: &str) -> Option<usize> {
    Some(
        match info
            .split_whitespace()
            .next()?
            .to_ascii_lowercase()
            .as_str()
        {
            "js" | "jsx" | "javascript" => 0,
            "ts" | "tsx" | "typescript" => 1,
            "json" => 2,
            "html" | "htm" => 3,
            "css" => 4,
            "rust" | "rs" => 5,
            "python" | "py" => 6,
            "shell" | "sh" | "bash" | "zsh" => 7,
            "sql" => 8,
            "yaml" | "yml" => 9,
            _ => return None,
        },
    )
}

fn configuration(index: usize) -> Result<&'static HighlightConfiguration, String> {
    CONFIGS[index]
        .get_or_init(|| {
            let (grammar, query) = match index {
                0 => (
                    tree_sitter_javascript::LANGUAGE,
                    format!(
                        "{}\n{}",
                        tree_sitter_javascript::HIGHLIGHT_QUERY,
                        tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
                    ),
                ),
                1 => (
                    tree_sitter_typescript::LANGUAGE_TSX,
                    format!(
                        "{}\n{}\n{}",
                        tree_sitter_typescript::HIGHLIGHTS_QUERY,
                        tree_sitter_javascript::HIGHLIGHT_QUERY,
                        tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
                    ),
                ),
                2 => (
                    tree_sitter_json::LANGUAGE,
                    tree_sitter_json::HIGHLIGHTS_QUERY.into(),
                ),
                3 => (
                    tree_sitter_html::LANGUAGE,
                    tree_sitter_html::HIGHLIGHTS_QUERY.into(),
                ),
                4 => (
                    tree_sitter_css::LANGUAGE,
                    tree_sitter_css::HIGHLIGHTS_QUERY.into(),
                ),
                5 => (
                    tree_sitter_rust::LANGUAGE,
                    tree_sitter_rust::HIGHLIGHTS_QUERY.into(),
                ),
                6 => (
                    tree_sitter_python::LANGUAGE,
                    tree_sitter_python::HIGHLIGHTS_QUERY.into(),
                ),
                7 => (
                    tree_sitter_bash::LANGUAGE,
                    tree_sitter_bash::HIGHLIGHT_QUERY.into(),
                ),
                8 => (
                    tree_sitter_sequel::LANGUAGE,
                    tree_sitter_sequel::HIGHLIGHTS_QUERY.into(),
                ),
                _ => (
                    tree_sitter_yaml::LANGUAGE,
                    tree_sitter_yaml::HIGHLIGHTS_QUERY.into(),
                ),
            };
            let mut config =
                HighlightConfiguration::new(grammar.into(), index.to_string(), &query, "", "")
                    .map_err(|error| error.to_string())?;
            config.configure(&NAMES);
            Ok(config)
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub(crate) fn tokens(source: &str, info: &str) -> Result<Vec<CodeToken>, String> {
    let Some(index) = language(info) else {
        return Ok(Vec::new());
    };
    if source.encode_utf16().count() > 65536
        || source
            .split(['\n', '\r'])
            .any(|line| line.encode_utf16().count() > 16384)
    {
        return Err(
            "Code highlighting exceeds the block or line limit; source is preserved".into(),
        );
    }
    let config = configuration(index)?;
    let cancelled = AtomicUsize::new(0);
    // One bounded parser call, one watchdog. Completion wakes the watchdog
    // immediately; scoped joining ensures no timer thread survives this call.
    std::thread::scope(|scope| {
        let (done, wait) = mpsc::channel();
        let flag = &cancelled;
        scope.spawn(move || {
            if matches!(
                wait.recv_timeout(Duration::from_millis(100)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ) {
                flag.store(1, Ordering::SeqCst);
            }
        });
        let result = (|| {
            let mut highlighter = Highlighter::new();
            let events = highlighter
                .highlight(config, source.as_bytes(), Some(&cancelled), |_| None)
                .map_err(|error| error.to_string())?;
            let mut stack = Vec::new();
            let mut tokens = Vec::new();
            for event in events {
                match event.map_err(|error| error.to_string())? {
                    HighlightEvent::HighlightStart(kind) => stack.push(kind.0),
                    HighlightEvent::HighlightEnd => {
                        stack.pop();
                    }
                    HighlightEvent::Source { start, end } => {
                        if let Some(&kind) = stack.last() {
                            if tokens.len() == 8192 {
                                return Err("Code highlighting exceeds the token limit; source is preserved".into());
                            }
                            tokens.push(CodeToken {
                                start,
                                end,
                                class: CLASSES[kind],
                            });
                        }
                    }
                }
            }
            Ok(tokens)
        })();
        let _ = done.send(());
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_first_languages_produce_safe_ranges() {
        for (language, source) in [
            ("js", "const name = '你好';"),
            ("ts", "const name: string = '你好';"),
            ("json", "{\"name\": true}"),
            ("html", "<div class=\"x\">你好</div>"),
            ("css", "div { color: red; }"),
            ("rust", "fn main() { let x = 1; }"),
            ("python", "def f():\n    return '你好'"),
            ("sh", "echo \"$HOME\" # comment"),
            ("sql", "SELECT name FROM users WHERE id = 1"),
            ("yaml", "name: '你好'\nready: true"),
        ] {
            let result = tokens(source, language).unwrap_or_else(|e| panic!("{language}: {e}"));
            assert!(!result.is_empty(), "{language}");
            let mut at = 0;
            for token in result {
                assert!(token.start >= at && token.end > token.start);
                assert!(source.is_char_boundary(token.start) && source.is_char_boundary(token.end));
                assert!(CLASSES.contains(&token.class));
                at = token.end;
            }
        }
    }
    #[test]
    fn cache_reuses_results_and_bounds_retained_memory() {
        let mut cache = CodeCache::default();
        let first = cache.get("const x = 1", "js").unwrap();
        let same = cache.get("const x = 1", "js").unwrap();
        assert!(std::sync::Arc::ptr_eq(&first, &same));
        for index in 0..140 {
            cache
                .get(
                    &format!("// {index}\n{}", "// filler text\n".repeat(1500)),
                    "js",
                )
                .unwrap();
            assert!(cache.entries.len() <= 128);
            assert!(cache.bytes <= 1024 * 1024);
        }
    }
    #[test]
    fn node_budget_covers_added_spans_without_parsing() {
        let source = "const x = 1; // comment\n".repeat(100);
        let tokens = tokens(&source, "js").unwrap();
        assert!(tokens.len() * 3 < node_budget(&source, "js"));
        assert_eq!(node_budget(&source, "unknown"), 0);
        assert_eq!(node_budget(&"x".repeat(65537), "js"), 0);
    }
    #[test]
    fn unknown_and_oversized_code_stays_plain() {
        assert!(tokens("hello", "not-javascript").unwrap().is_empty());
        assert!(tokens(&"x".repeat(16385), "js").is_err());
        assert!(tokens(&"x\n".repeat(40000), "js").is_err());
    }
}
