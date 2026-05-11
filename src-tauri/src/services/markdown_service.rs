use html_escape::encode_text;
use pulldown_cmark::{html, Options, Parser};

use crate::{
    models::{
        app_error::AppError,
        document::{DocumentStats, OutlineItem, RenderedMarkdownDto},
    },
    utils::security::sanitize_html,
};

pub fn render_markdown_to_html(markdown: &str) -> Result<String, AppError> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

    let parser = Parser::new_ext(markdown, options);
    let mut raw_html = String::new();
    html::push_html(&mut raw_html, parser);
    Ok(sanitize_html(&raw_html))
}

pub fn render_markdown(markdown: &str) -> Result<RenderedMarkdownDto, AppError> {
    Ok(RenderedMarkdownDto {
        html: render_markdown_to_html(markdown)?,
        outline: extract_outline(markdown),
        stats: calculate_stats(markdown),
    })
}

pub fn extract_outline(markdown: &str) -> Vec<OutlineItem> {
    markdown
        .lines()
        .enumerate()
        .filter_map(|(index, line)| parse_heading(line).map(|(level, title)| (index, level, title)))
        .map(|(index, level, title)| OutlineItem {
            level,
            slug: slugify(&title),
            title,
            line: index + 1,
        })
        .collect()
}

pub fn calculate_stats(markdown: &str) -> DocumentStats {
    let outline = extract_outline(markdown);
    DocumentStats {
        word_count: markdown.split_whitespace().filter(|word| !word.trim().is_empty()).count(),
        character_count: markdown.chars().count(),
        line_count: if markdown.is_empty() { 1 } else { markdown.lines().count() },
        heading_count: outline.len(),
        link_count: count_markdown_links(markdown),
        image_count: markdown.matches("![").count(),
    }
}

pub fn render_standalone_html(title: &str, markdown: &str) -> Result<String, AppError> {
    let body = render_markdown_to_html(markdown)?;
    let safe_title = encode_text(title);
    Ok(format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{safe_title}</title>
  <style>
    body {{ margin: 0; color: #1f2328; background: #ffffff; font: 16px/1.7 "Segoe UI", system-ui, sans-serif; }}
    main {{ max-width: 860px; margin: 0 auto; padding: 48px 28px; }}
    pre {{ overflow: auto; padding: 16px; border-radius: 8px; background: #f6f8fa; }}
    code {{ font-family: "Cascadia Code", Consolas, monospace; }}
    table {{ border-collapse: collapse; width: 100%; }}
    th, td {{ border: 1px solid #d0d7de; padding: 8px 10px; }}
    blockquote {{ margin-left: 0; padding-left: 16px; color: #57606a; border-left: 4px solid #d0d7de; }}
    img {{ max-width: 100%; }}
  </style>
</head>
<body>
  <main>{body}</main>
</body>
</html>"#
    ))
}

fn parse_heading(line: &str) -> Option<(u8, String)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|char| *char == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }

    if !trimmed.chars().nth(level).is_some_and(|char| char.is_whitespace()) {
        return None;
    }

    let title = trimmed[level..]
        .trim()
        .trim_end_matches('#')
        .trim()
        .to_string();

    if title.is_empty() {
        None
    } else {
        Some((level as u8, title))
    }
}

fn slugify(title: &str) -> String {
    let mut slug = title
        .chars()
        .filter_map(|char| {
            if char.is_ascii_alphanumeric() {
                Some(char.to_ascii_lowercase())
            } else if char.is_whitespace() || char == '-' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>();

    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    slug.trim_matches('-').to_string()
}

fn count_markdown_links(markdown: &str) -> usize {
    markdown
        .match_indices("](")
        .filter(|(index, _)| !markdown[..*index].ends_with('!'))
        .count()
}

#[cfg(test)]
mod tests {
    use super::{calculate_stats, extract_outline, render_markdown_to_html};

    #[test]
    fn renders_tables_and_task_lists() {
        let html = render_markdown_to_html("- [x] Done\n\n| A | B |\n| - | - |\n| 1 | 2 |").unwrap();
        assert!(html.contains("<table>"));
        assert!(html.contains("checkbox"));
    }

    #[test]
    fn removes_script_tags() {
        let html = render_markdown_to_html("<script>alert(1)</script>\n\n# Safe").unwrap();
        assert!(!html.contains("<script>"));
        assert!(html.contains("<h1"));
    }

    #[test]
    fn extracts_outline() {
        let outline = extract_outline("# A\nText\n### B");
        assert_eq!(outline.len(), 2);
        assert_eq!(outline[1].level, 3);
    }

    #[test]
    fn calculates_stats() {
        let stats = calculate_stats("# Title\n![img](x.png)\n[site](https://example.com)");
        assert_eq!(stats.heading_count, 1);
        assert_eq!(stats.image_count, 1);
        assert_eq!(stats.link_count, 1);
    }
}
