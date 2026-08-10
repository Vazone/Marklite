use html_escape::encode_text;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag};

use crate::{
    models::{
        app_error::AppError,
        document::{DocumentStats, OutlineItem, RenderedMarkdownDto},
    },
    utils::security::sanitize_html,
};

pub fn render_markdown_to_html(markdown: &str) -> Result<String, AppError> {
    render_markdown_html(markdown, true)
}

fn render_markdown_html(markdown: &str, encode_targets: bool) -> Result<String, AppError> {
    let parser = Parser::new_ext(markdown, markdown_options()).map(|event| {
        if encode_targets {
            encode_navigation_target(event)
        } else {
            event
        }
    });
    let mut raw_html = String::new();
    html::push_html(&mut raw_html, parser);
    Ok(sanitize_html(&raw_html))
}

fn encode_navigation_target(event: Event<'_>) -> Event<'_> {
    match event {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: encoded_target(dest_url),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: encoded_target(dest_url),
            title,
            id,
        }),
        event => event,
    }
}

fn encoded_target(target: CowStr<'_>) -> CowStr<'_> {
    format!(
        "marklite:{}",
        utf8_percent_encode(target.as_ref(), NON_ALPHANUMERIC)
    )
    .into()
}

fn markdown_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options
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
    let (link_count, image_count) = count_markdown_resources(markdown);
    DocumentStats {
        word_count: markdown
            .split_whitespace()
            .filter(|word| !word.trim().is_empty())
            .count(),
        character_count: markdown.chars().count(),
        line_count: if markdown.is_empty() {
            1
        } else {
            markdown.lines().count()
        },
        heading_count: outline.len(),
        link_count,
        image_count,
    }
}

pub fn render_standalone_html(title: &str, markdown: &str) -> Result<String, AppError> {
    let body = render_markdown_html(markdown, false)?;
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

    if !trimmed
        .chars()
        .nth(level)
        .is_some_and(|char| char.is_whitespace())
    {
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

fn count_markdown_resources(markdown: &str) -> (usize, usize) {
    Parser::new_ext(markdown, markdown_options()).fold(
        (0, 0),
        |(link_count, image_count), event| match event {
            Event::Start(Tag::Link { .. }) => (link_count + 1, image_count),
            Event::Start(Tag::Image { .. }) => (link_count, image_count + 1),
            _ => (link_count, image_count),
        },
    )
}
