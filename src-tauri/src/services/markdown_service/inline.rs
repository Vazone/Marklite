use crate::models::markdown_event::Event;
use std::{collections::HashMap, ops::Range, sync::LazyLock};

use pulldown_cmark::{LinkType, Tag, TagEnd};
use serde::Deserialize;

#[derive(Deserialize)]
struct EmojiRegistry {
    aliases: HashMap<String, String>,
    #[serde(rename = "maxShortcodeLength")]
    max_shortcode_length: usize,
}

static EMOJI: LazyLock<EmojiRegistry> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../../src/shared/emoji-registry.json"))
        .expect("checked-in emoji registry must match its generated schema")
});

// Work on parser events before text coalescing loses escape/entity provenance.
// The original ranges and number of events remain unchanged for source indexing.
pub(super) fn emoji_events<'a>(
    source: &'a str,
    events: impl Iterator<Item = (Event<'a>, Range<usize>)>,
    enabled: bool,
) -> impl Iterator<Item = (Event<'a>, Range<usize>)> {
    let mut protected = 0usize;
    let mut html_tags = Vec::new();
    let mut urls = SourceUrls::default();
    let mut script_depth = 0usize;
    events.map(move |(mut event, range)| {
        if !enabled {
            return (event, range);
        }
        urls.observe(&event, &range);
        match &event {
            Event::Start(Tag::Subscript | Tag::Superscript) => script_depth += 1,
            Event::End(TagEnd::Subscript | TagEnd::Superscript) => script_depth -= 1,
            Event::Start(Tag::CodeBlock(_))
            | Event::Start(Tag::Link {
                link_type: LinkType::Autolink | LinkType::Email,
                ..
            }) => protected += 1,
            Event::End(TagEnd::CodeBlock | TagEnd::Link) => {
                protected = protected.saturating_sub(1);
            }
            Event::InlineHtml(html) => track_html(html, &mut html_tags),
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::TableCell
                | TagEnd::Item
                | TagEnd::DefinitionListTitle
                | TagEnd::DefinitionListDefinition,
            ) => html_tags.clear(),
            Event::Text(text)
                if protected == 0
                    && html_tags.is_empty()
                    && text.contains(':')
                    && source.get(range.clone()) == Some(text.as_ref()) =>
            {
                if let Some(replaced) = replace_emoji(source, range.clone(), &mut urls) {
                    event = Event::Text(replaced.into());
                }
            }
            _ => {}
        }
        // Only accepted script spans permit escaped spaces. Decode at the
        // semantic boundary after Emoji matching; editor/source ranges stay raw.
        if script_depth > 0 && protected == 0 && html_tags.is_empty() {
            if let Event::Text(text) = &mut event {
                if text.contains("\\ ") {
                    *text = text.replace("\\ ", "\u{a0}").into();
                }
            }
        }
        (event, range)
    })
}

pub(super) fn escaped(source: &str, at: usize) -> bool {
    source.as_bytes()[..at]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count()
        % 2
        != 0
}

fn replace_emoji(source: &str, range: Range<usize>, urls: &mut SourceUrls) -> Option<String> {
    let text = &source[range.clone()];
    let mut output = String::new();
    let mut copied = 0;
    let mut skip_until = 0;
    let mut previous = None;
    for (at, character) in text.char_indices() {
        if at >= skip_until {
            // Bare URLs are still Text here. Respect the same recognizer used by
            // GFM normalization instead of interpreting shortcodes in URL paths.
            if let Some(end) = url_end(text, at, previous) {
                skip_until = end;
            }
            if at >= skip_until
                && character == ':'
                && !escaped(source, range.start + at)
                && !urls.contains(source, range.start + at)
            {
                let suffix = &text[at + 1..];
                let length = suffix
                    .bytes()
                    .take(EMOJI.max_shortcode_length + 1)
                    .take_while(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || matches!(byte, b'_' | b'+' | b'-')
                    })
                    .count();
                if length > 0
                    && length <= EMOJI.max_shortcode_length
                    && suffix.as_bytes().get(length) == Some(&b':')
                {
                    if let Some(emoji) = EMOJI.aliases.get(&suffix[..length]) {
                        output.push_str(&text[copied..at]);
                        output.push_str(emoji);
                        copied = at + length + 2;
                        skip_until = copied;
                    }
                }
            }
        }
        previous = Some(character);
    }
    if copied == 0 {
        None
    } else {
        output.push_str(&text[copied..]);
        Some(output)
    }
}

pub(super) fn url_end(text: &str, at: usize, previous: Option<char>) -> Option<usize> {
    if at != 0 && !previous.is_some_and(gfm_autolinks::check_prev) {
        return None;
    }
    let (_, count) = gfm_autolinks::match_start(&text[at..])?;
    Some(
        at + text[at..]
            .char_indices()
            .nth(count)
            .map_or(text.len() - at, |(byte, _)| byte),
    )
}

// Parser escapes/entities may split a URL into several Text events. Look up
// candidates in the original source word and cache it, rather than repeatedly
// walking the prefix of a long URL. Opaque syntax closes the lookup scope.
#[derive(Default)]
pub(super) struct SourceUrls {
    floor: usize,
    word: Range<usize>,
    urls: Vec<Range<usize>>,
}

impl SourceUrls {
    pub(super) fn observe(&mut self, event: &Event<'_>, range: &Range<usize>) {
        let floor = match event {
            Event::Start(
                Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::TableCell
                | Tag::Item
                | Tag::Link { .. }
                | Tag::Image { .. },
            ) => Some(range.start),
            Event::Code(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::InlineHtml(_)
            | Event::Html(_)
            | Event::End(TagEnd::Link | TagEnd::Image) => Some(range.end),
            _ => None,
        };
        if let Some(floor) = floor {
            self.floor = floor;
            self.word = 0..0;
            self.urls.clear();
        }
    }

    pub(super) fn contains(&mut self, source: &str, at: usize) -> bool {
        if !self.word.contains(&at) {
            let start = source[self.floor.min(at)..at]
                .char_indices()
                .rev()
                .find(|(_, ch)| ch.is_whitespace())
                .map_or(self.floor.min(at), |(offset, ch)| {
                    self.floor.min(at) + offset + ch.len_utf8()
                });
            let end = source[at..]
                .char_indices()
                .find(|(_, ch)| ch.is_whitespace())
                .map_or(source.len(), |(offset, _)| at + offset);
            self.word = start..end;
            self.urls.clear();
            let text = &source[start..end];
            let mut previous = None;
            let mut skip = 0;
            for (offset, ch) in text.char_indices() {
                if offset >= skip {
                    if let Some(end) = url_end(text, offset, previous) {
                        self.urls.push(start + offset..start + end);
                        skip = end;
                    }
                }
                previous = Some(ch);
            }
        }
        self.urls.iter().any(|url| url.contains(&at))
    }
}

pub(super) fn track_html(html: &str, stack: &mut Vec<String>) {
    let Some(tag) = html.strip_prefix('<') else {
        return;
    };
    let closing = tag.starts_with('/');
    let tag = tag.strip_prefix('/').unwrap_or(tag);
    let length = tag
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        .count();
    if length == 0 {
        return;
    }
    let name = tag[..length].to_ascii_lowercase();
    if closing {
        if let Some(index) = stack.iter().rposition(|open| open == &name) {
            stack.truncate(index);
        }
    } else if !html.trim_end().ends_with("/>")
        && !matches!(
            name.as_str(),
            "area"
                | "base"
                | "br"
                | "col"
                | "embed"
                | "hr"
                | "img"
                | "input"
                | "link"
                | "meta"
                | "param"
                | "source"
                | "track"
                | "wbr"
        )
    {
        stack.push(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::markdown_event as html;
    use crate::services::markdown_service::{profile_events, SyntaxProfile};

    fn html(source: &str) -> String {
        let mut html = String::new();
        html::push_html(
            &mut html,
            profile_events(source, SyntaxProfile::GfmWithMarkLiteExtensions).into_iter(),
        );
        html
    }

    #[test]
    fn emoji_known_unknown_adjacent_and_nested() {
        assert_eq!(
            html(":smile::+1::-1: :missing_shortcode: :SMILE:"),
            "<p>😄👍👎 :missing_shortcode: :SMILE:</p>\n"
        );
        assert_eq!(
            html("**:smile:** [:+1:](https://example.com/:smile:)"),
            "<p><strong>😄</strong> <a href=\"https://example.com/:smile:\">👍</a></p>\n"
        );
    }

    #[test]
    fn emoji_protects_escapes_entities_code_math_html_and_urls() {
        for source in [
            r"\:smile:",
            r":smile\:",
            "&#58;smile:",
            ":smile&#58;",
            "`:smile:`",
            "```text\n:smile:\n```",
            "    :smile:",
            "$:smile:$",
            "$$:smile:$$",
            "<span>:smile:</span>",
            "<span title=\">\"><b>:smile:</b></span>",
            "https://example.com/:smile:/x",
            "<https://example.com/:smile:>",
        ] {
            assert!(!html(source).contains('😄'), "{source}: {}", html(source));
        }
        assert!(html("<span>:smile:</span> :smile:").contains("</span> 😄"));
        assert!(html("<br> :smile:").contains("😄"));
        assert!(html(r"\\:smile:").contains("\\😄"));
        assert!(html("- <span>:smile:\n- :smile:").contains("<li>😄</li>"));
        assert!(html("| A | B |\n|---|---|\n| <span>:smile: | :smile: |").contains("<td>😄</td>"));
        assert!(!html("<x-a><x-b>:smile:</x-b>:smile:</x-a>").contains('😄'));
    }

    #[test]
    fn emoji_is_only_enabled_in_the_product_profile() {
        for profile in [SyntaxProfile::CommonMark, SyntaxProfile::Gfm] {
            let mut rendered = String::new();
            html::push_html(
                &mut rendered,
                profile_events(":smile:", profile).into_iter(),
            );
            assert_eq!(rendered, "<p>:smile:</p>\n");
        }
    }

    #[test]
    fn shared_inline_fixtures_match_editor_protection() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../src/shared/markdown-inline-fixtures.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let events = profile_events(
                case["source"].as_str().unwrap(),
                SyntaxProfile::GfmWithMarkLiteExtensions,
            );
            let mut actual = Vec::new();
            for event in events {
                if let Event::Text(text) = event {
                    actual.extend(
                        text.chars()
                            .filter(|c| matches!(c, '😄' | '👍' | '👎'))
                            .map(|c| c.to_string()),
                    );
                }
            }
            let expected = case["emojis"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{}", case["id"]);
        }
    }

    #[test]
    fn every_registry_alias_is_recognized_without_changing_source_ranges() {
        for (alias, emoji) in &EMOJI.aliases {
            let source = format!(":{alias}:");
            let events = emoji_events(
                &source,
                pulldown_cmark::Parser::new(&source)
                    .into_offset_iter()
                    .map(|(event, range)| (event.into(), range)),
                true,
            )
            .collect::<Vec<_>>();
            assert!(
                matches!(&events[1], (Event::Text(text), range)
                if text.as_ref() == emoji && range == &(0..source.len())),
                "{alias}: {events:?}"
            );
        }
    }
}
