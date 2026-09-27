use crate::models::markdown_event::Event;
use pulldown_cmark::{CowStr, Tag, TagEnd};
use std::{
    collections::{HashMap, VecDeque},
    ops::Range,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Highlight,
    Subscript,
    Superscript,
}

impl Kind {
    fn width(self) -> usize {
        if self == Self::Highlight {
            2
        } else {
            1
        }
    }
    fn event(self, opening: bool) -> Event<'static> {
        match (self, opening) {
            (Self::Highlight, true) => Event::StartHighlight,
            (Self::Highlight, false) => Event::EndHighlight,
            (Self::Subscript, true) => Event::Start(Tag::Subscript),
            (Self::Subscript, false) => Event::End(TagEnd::Subscript),
            (Self::Superscript, true) => Event::Start(Tag::Superscript),
            (Self::Superscript, false) => Event::End(TagEnd::Superscript),
        }
    }
}

type SourceEvent<'a> = (Event<'a>, Range<usize>);

/// Hold only the current inline run. Block boundaries and native inline parent
/// identities prevent crossing paragraphs, table cells, links or emphasis tags.
pub(super) fn events<'a>(
    source: &'a str,
    mut input: impl Iterator<Item = SourceEvent<'a>>,
    enabled: bool,
) -> impl Iterator<Item = SourceEvent<'a>> {
    let mut buffer = Vec::new();
    let mut ready = VecDeque::new();
    let mut code = false;
    std::iter::from_fn(move || {
        if !enabled {
            return input.next();
        }
        loop {
            if let Some(event) = ready.pop_front() {
                return Some(event);
            }
            let Some((mut event, mut range)) = input.next() else {
                flush(source, &mut buffer, &mut ready);
                return ready.pop_front();
            };
            // The base parser accepts single-tilde strikeout. In the product
            // dialect single tildes belong to subscript; keep ~~ native.
            if matches!(
                event,
                Event::Start(Tag::Strikethrough) | Event::End(TagEnd::Strikethrough)
            ) && source[range.clone()].starts_with('~')
                && !source[range.clone()].starts_with("~~")
            {
                range = if matches!(event, Event::Start(_)) {
                    range.start..range.start + 1
                } else {
                    range.end - 1..range.end
                };
                event = Event::Text(CowStr::Borrowed(&source[range.clone()]));
            }
            if boundary(&event) {
                flush(source, &mut buffer, &mut ready);
                match &event {
                    Event::Start(Tag::CodeBlock(_)) => code = true,
                    Event::End(TagEnd::CodeBlock) => code = false,
                    _ => {}
                }
                ready.push_back((event, range));
            } else if code {
                return Some((event, range));
            } else {
                buffer.push((event, range));
            }
        }
    })
}

fn boundary(event: &Event<'_>) -> bool {
    match event {
        Event::Start(tag) => !matches!(
            tag,
            Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Subscript
                | Tag::Superscript
                | Tag::Link { .. }
                | Tag::Image { .. }
        ),
        Event::End(tag) => !matches!(
            tag,
            TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Subscript
                | TagEnd::Superscript
                | TagEnd::Link
                | TagEnd::Image
        ),
        Event::Rule | Event::Html(_) => true,
        _ => false,
    }
}

fn flush<'a>(
    source: &'a str,
    buffer: &mut Vec<SourceEvent<'a>>,
    output: &mut VecDeque<SourceEvent<'a>>,
) {
    if !buffer
        .iter()
        .any(|(event, _)| matches!(event, Event::Text(text) if text.contains(['=', '~', '^'])))
    {
        output.extend(buffer.drain(..));
        return;
    }
    let start = buffer
        .iter()
        .map(|(_, range)| range.start)
        .min()
        .unwrap_or(0);
    let end = buffer
        .iter()
        .map(|(_, range)| range.end)
        .max()
        .unwrap_or(start);
    let whitespace = source[start..end]
        .char_indices()
        .filter(|(at, ch)| {
            ch.is_whitespace() && !(*ch == ' ' && super::inline::escaped(source, start + at))
        })
        .map(|(at, _)| start + at)
        .collect::<Vec<_>>();
    let mut marks = vec![Vec::new(); buffer.len()];
    let mut parents = vec![0usize];
    let mut sequence = 0;
    let mut opens: HashMap<usize, Vec<(usize, usize, Kind)>> = HashMap::new();
    let mut html = Vec::new();
    let mut autolink = false;
    let mut urls = super::inline::SourceUrls::default();
    for (index, (event, range)) in buffer.iter().enumerate() {
        urls.observe(event, range);
        match event {
            Event::Start(tag) => {
                sequence += 1;
                parents.push(sequence);
                if matches!(
                    tag,
                    Tag::Link {
                        link_type: pulldown_cmark::LinkType::Autolink
                            | pulldown_cmark::LinkType::Email,
                        ..
                    }
                ) {
                    autolink = true;
                }
            }
            Event::End(tag) => {
                parents.pop();
                if *tag == TagEnd::Link {
                    autolink = false;
                }
            }
            Event::InlineHtml(tag) => super::inline::track_html(tag, &mut html),
            Event::Text(text)
                if html.is_empty()
                    && !autolink
                    && source.get(range.clone()) == Some(text.as_ref()) =>
            {
                let mut skip = 0;
                let mut previous = None;
                for (at, character) in text.char_indices() {
                    if at >= skip {
                        if let Some(end) = super::inline::url_end(text, at, previous) {
                            skip = end;
                        }
                    }
                    let absolute = range.start + at;
                    let kind = match character {
                        '=' if text[at..].starts_with("==") => Some(Kind::Highlight),
                        '~' => Some(Kind::Subscript),
                        '^' => Some(Kind::Superscript),
                        _ => None,
                    };
                    if let Some(kind) = kind.filter(|kind| {
                        at >= skip
                            && source.as_bytes().get(absolute.wrapping_sub(1))
                                != Some(&(character as u8))
                            && source.as_bytes().get(absolute + kind.width())
                                != Some(&(character as u8))
                            && !super::inline::escaped(source, absolute)
                            && !urls.contains(source, absolute)
                    }) {
                        let before = source[..absolute].chars().next_back();
                        let after = source[absolute + kind.width()..].chars().next();
                        let stack = opens
                            .entry(*parents.last().expect("inline root remains"))
                            .or_default();
                        let opening = stack
                            .iter()
                            .rposition(|(_, _, candidate)| *candidate == kind);
                        let mut paired = false;
                        if let Some(position) =
                            opening.filter(|_| before.is_some_and(|c| !c.is_whitespace()))
                        {
                            let (opening, offset, _) = stack[position];
                            let from = buffer[opening].1.start + offset + kind.width();
                            let white = whitespace.partition_point(|at| *at < from);
                            if kind == Kind::Highlight
                                || whitespace.get(white).is_none_or(|at| *at >= absolute)
                            {
                                marks[opening].push((offset, true, kind));
                                marks[index].push((at, false, kind));
                                // A close cancels intervening unmatched openers, so mixed
                                // extensions cannot produce crossing HTML/semantic nodes.
                                stack.truncate(position);
                                paired = true;
                            } else {
                                stack.retain(|(_, _, candidate)| *candidate != kind);
                            }
                        }
                        if !paired && after.is_some_and(|c| !c.is_whitespace()) {
                            stack.push((index, at, kind));
                        }
                        skip = at + kind.width();
                    }
                    previous = Some(character);
                }
            }
            _ => {}
        }
    }
    for ((event, range), mut markers) in buffer.drain(..).zip(marks) {
        if markers.is_empty() {
            output.push_back((event, range));
            continue;
        }
        markers.sort_unstable_by_key(|(at, _, _)| *at);
        let mut copied = range.start;
        for (offset, opening, kind) in markers {
            let at = range.start + offset;
            if copied < at {
                output.push_back((
                    Event::Text(CowStr::Borrowed(&source[copied..at])),
                    copied..at,
                ));
            }
            output.push_back((kind.event(opening), at..at + kind.width()));
            copied = at + kind.width();
        }
        if copied < range.end {
            output.push_back((
                Event::Text(CowStr::Borrowed(&source[copied..range.end])),
                copied..range.end,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::markdown_event as html;
    use crate::services::markdown_service::{profile_events, SyntaxProfile};

    #[test]
    fn scripts_keep_math_opaque_and_standard_profiles_unchanged() {
        let events = profile_events(
            "$x^2^+H~2~O+==a==$",
            SyntaxProfile::GfmWithMarkLiteExtensions,
        );
        assert!(events
            .iter()
            .any(|event| matches!(event, Event::InlineMath(_))));
        assert!(!events.iter().any(|event| matches!(
            event,
            Event::Start(Tag::Subscript | Tag::Superscript) | Event::StartHighlight
        )));
        for profile in [SyntaxProfile::CommonMark, SyntaxProfile::Gfm] {
            assert!(!profile_events("H~2~O x^2^ ==a==", profile)
                .iter()
                .any(|event| matches!(
                    event,
                    Event::Start(Tag::Subscript | Tag::Superscript) | Event::StartHighlight
                )));
        }
    }

    #[test]
    fn shared_script_fixtures_match_semantic_tags_and_html() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../src/shared/markdown-script-fixtures.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let events = profile_events(
                case["source"].as_str().unwrap(),
                SyntaxProfile::GfmWithMarkLiteExtensions,
            );
            let sub = events
                .iter()
                .filter(|event| matches!(event, Event::Start(Tag::Subscript)))
                .count();
            let sup = events
                .iter()
                .filter(|event| matches!(event, Event::Start(Tag::Superscript)))
                .count();
            assert_eq!(
                (sub, sup),
                (
                    case["sub"].as_u64().unwrap() as usize,
                    case["sup"].as_u64().unwrap() as usize
                ),
                "{}",
                case["id"]
            );
            let mut actual = String::new();
            html::push_html(&mut actual, events.into_iter());
            assert_eq!(actual, case["html"].as_str().unwrap(), "{}", case["id"]);
        }
    }

    #[test]
    fn highlight_fixture_html_and_typed_events_match() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../src/shared/markdown-highlight-fixtures.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let events = profile_events(
                case["source"].as_str().unwrap(),
                SyntaxProfile::GfmWithMarkLiteExtensions,
            );
            let marks = events
                .iter()
                .filter(|event| matches!(event, Event::StartHighlight))
                .count();
            assert_eq!(
                marks,
                case["marks"].as_u64().unwrap() as usize,
                "{}",
                case["id"]
            );
            let mut html = String::new();
            html::push_html(&mut html, events.into_iter());
            assert_eq!(html, case["html"].as_str().unwrap(), "{}", case["id"]);
        }
    }
}
