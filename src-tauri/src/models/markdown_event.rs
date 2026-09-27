use pulldown_cmark::{CowStr, Event as ParserEvent, Tag, TagEnd};

/// Product events retain typed formatting until the final writer. In particular,
/// highlight is not represented by raw user HTML or a collision-prone text token.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Event<'a> {
    Start(Tag<'a>),
    End(TagEnd),
    Text(CowStr<'a>),
    Code(CowStr<'a>),
    InlineMath(CowStr<'a>),
    DisplayMath(CowStr<'a>),
    Html(CowStr<'a>),
    InlineHtml(CowStr<'a>),
    FootnoteReference(CowStr<'a>),
    SoftBreak,
    HardBreak,
    Rule,
    TaskListMarker(bool),
    StartHighlight,
    EndHighlight,
    Toc(super::markdown::DocumentToc),
}

impl<'a> From<ParserEvent<'a>> for Event<'a> {
    fn from(event: ParserEvent<'a>) -> Self {
        match event {
            ParserEvent::Start(tag) => Self::Start(tag),
            ParserEvent::End(tag) => Self::End(tag),
            ParserEvent::Text(text) => Self::Text(text),
            ParserEvent::Code(text) => Self::Code(text),
            ParserEvent::InlineMath(text) => Self::InlineMath(text),
            ParserEvent::DisplayMath(text) => Self::DisplayMath(text),
            ParserEvent::Html(text) => Self::Html(text),
            ParserEvent::InlineHtml(text) => Self::InlineHtml(text),
            ParserEvent::FootnoteReference(text) => Self::FootnoteReference(text),
            ParserEvent::SoftBreak => Self::SoftBreak,
            ParserEvent::HardBreak => Self::HardBreak,
            ParserEvent::Rule => Self::Rule,
            ParserEvent::TaskListMarker(done) => Self::TaskListMarker(done),
        }
    }
}

impl<'a> Event<'a> {
    /// Call only at HTML serialization. The result still passes through the
    /// existing sanitizer; raw HTML input keeps its separate policy and warnings.
    fn into_html(self) -> ParserEvent<'a> {
        match self {
            Self::Toc(toc) => {
                let mut html = String::new();
                push_html(&mut html, toc.events());
                ParserEvent::Html(html.into())
            }
            Self::Start(tag) => ParserEvent::Start(tag),
            Self::End(tag) => ParserEvent::End(tag),
            Self::Text(text) => ParserEvent::Text(text),
            Self::Code(text) => ParserEvent::Code(text),
            Self::InlineMath(text) => ParserEvent::InlineMath(text),
            Self::DisplayMath(text) => ParserEvent::DisplayMath(text),
            Self::Html(text) => ParserEvent::Html(text),
            Self::InlineHtml(text) => ParserEvent::InlineHtml(text),
            Self::FootnoteReference(text) => ParserEvent::FootnoteReference(text),
            Self::SoftBreak => ParserEvent::SoftBreak,
            Self::HardBreak => ParserEvent::HardBreak,
            Self::Rule => ParserEvent::Rule,
            Self::TaskListMarker(done) => ParserEvent::TaskListMarker(done),
            Self::StartHighlight => ParserEvent::InlineHtml(CowStr::Borrowed("<mark>")),
            Self::EndHighlight => ParserEvent::InlineHtml(CowStr::Borrowed("</mark>")),
        }
    }

    pub(crate) fn into_static(self) -> Event<'static> {
        match self {
            Self::Toc(toc) => Event::Toc(toc),
            Self::StartHighlight => Event::StartHighlight,
            Self::EndHighlight => Event::EndHighlight,
            native => Event::from(native.into_html().into_static()),
        }
    }
}

pub(crate) fn push_html<'a>(output: &mut String, events: impl Iterator<Item = Event<'a>>) {
    let mut images = 0usize;
    pulldown_cmark::html::push_html(
        output,
        events.filter_map(move |event| {
            match &event {
                Event::Start(Tag::Image { .. }) => images += 1,
                Event::End(TagEnd::Image) => images = images.saturating_sub(1),
                Event::StartHighlight | Event::EndHighlight if images > 0 => return None,
                _ => {}
            }
            Some(event.into_html())
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_stays_typed_until_html_serialization_without_larger_events() {
        assert_eq!(Event::StartHighlight.into_static(), Event::StartHighlight);
        assert_eq!(Event::EndHighlight.into_static(), Event::EndHighlight);
        assert!(
            std::mem::size_of::<Event<'static>>() <= std::mem::size_of::<ParserEvent<'static>>()
        );
        let mut html = String::new();
        pulldown_cmark::html::push_html(
            &mut html,
            [
                Event::StartHighlight,
                Event::Text("<literal>".into()),
                Event::EndHighlight,
            ]
            .into_iter()
            .map(Event::into_html),
        );
        assert_eq!(html, "<mark>&lt;literal&gt;</mark>");
    }
}
