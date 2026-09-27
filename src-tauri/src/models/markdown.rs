use serde::Serialize;

use super::diagram::{DiagramDiagnostic, DiagramSource};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineItem {
    pub level: u8,
    pub title: String,
    pub line: usize,
    pub slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownDiagnostic {
    pub code: String,
    pub message: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentStats {
    pub word_count: usize,
    pub character_count: usize,
    pub line_count: usize,
    pub heading_count: usize,
    pub link_count: usize,
    pub image_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderedMarkdownDto {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub markdown_diagnostics: Vec<MarkdownDiagnostic>,
    pub html: String,
    pub outline: Vec<OutlineItem>,
    pub stats: DocumentStats,
    pub source_blocks: Vec<SourceBlock>,
    pub diagrams: Vec<DiagramSource>,
    pub diagram_diagnostics: Vec<DiagramDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub virtual_preview: Option<VirtualPreviewIndex>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualPreviewIndex {
    pub session_id: String,
    pub segments: Vec<VirtualPreviewSegment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualPreviewSegment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_continuation: Option<bool>,
    pub start_utf16: usize,
    pub end_utf16: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub estimated_height: usize,
    pub estimated_nodes: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualPreviewWindow {
    pub session_id: String,
    pub start: usize,
    pub end: usize,
    pub segments: Vec<VirtualPreviewRenderedSegment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualPreviewRenderedSegment {
    pub index: usize,
    pub html: String,
    pub source_block_start: usize,
    pub source_blocks: Vec<SourceBlock>,
    pub diagrams: Vec<DiagramSource>,
    pub diagram_diagnostics: Vec<DiagramDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBlock {
    pub start_utf16: usize,
    pub end_utf16: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownAnalysisDto {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub markdown_diagnostics: Vec<MarkdownDiagnostic>,
    pub outline: Vec<OutlineItem>,
    pub stats: DocumentStats,
}

/// A document TOC owns only a shared index and a range, even when repeated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DocumentToc {
    index: std::sync::Arc<TocIndex>,
    range: std::ops::Range<usize>,
}

#[derive(Debug, PartialEq, Eq)]
struct TocIndex {
    headings: Vec<OutlineItem>,
    byte_prefix: Vec<usize>,
}

impl DocumentToc {
    pub(crate) fn new(headings: Vec<OutlineItem>) -> Self {
        let mut byte_prefix = Vec::with_capacity(headings.len() + 1);
        byte_prefix.push(0);
        for heading in &headings {
            byte_prefix
                .push(byte_prefix.last().unwrap() + heading.title.len() + heading.slug.len());
        }
        Self {
            range: 0..headings.len(),
            index: std::sync::Arc::new(TocIndex {
                headings,
                byte_prefix,
            }),
        }
    }

    pub(crate) fn headings(&self) -> &[OutlineItem] {
        &self.index.headings[self.range.clone()]
    }

    pub(crate) fn text_bytes(&self) -> usize {
        self.index.byte_prefix[self.range.end] - self.index.byte_prefix[self.range.start]
    }

    pub(crate) fn events(
        &self,
    ) -> impl Iterator<Item = super::markdown_event::Event<'static>> + '_ {
        use super::markdown_event::Event;
        use pulldown_cmark::{LinkType, Tag, TagEnd};
        std::iter::once(Event::Html("<ul class=\"document-toc\">\n".into()))
            .chain(self.headings().iter().flat_map(|heading| {
                [
                    Event::Html(format!("<li class=\"toc-level-{}\">", heading.level).into()),
                    Event::Start(Tag::Link {
                        link_type: LinkType::Inline,
                        dest_url: format!("#{}", heading.slug).into(),
                        title: "".into(),
                        id: "".into(),
                    }),
                    Event::Text(heading.title.clone().into()),
                    Event::End(TagEnd::Link),
                    Event::Html("</li>\n".into()),
                ]
            }))
            .chain(std::iter::once(Event::Html("</ul>\n".into())))
    }

    /// Every heading is retained. An indivisible over-budget title occupies its
    /// own range so callers can report their existing block-limit error.
    pub(crate) fn chunks(
        &self,
        max_entries: usize,
        max_bytes: usize,
    ) -> impl Iterator<Item = Self> + '_ {
        assert!(max_entries > 0 && max_bytes > 0);
        let mut start = self.range.start;
        std::iter::from_fn(move || {
            if start == self.range.end {
                return None;
            }
            let mut end = start + 1;
            while end < self.range.end
                && end - start < max_entries
                && self.index.byte_prefix[end + 1] - self.index.byte_prefix[start] <= max_bytes
            {
                end += 1;
            }
            let part = Self {
                index: self.index.clone(),
                range: start..end,
            };
            start = end;
            Some(part)
        })
    }
}

#[cfg(test)]
mod toc_tests {
    use super::*;

    #[test]
    fn repeated_tocs_and_chunks_share_global_targets_without_copying() {
        let headings = (0..10_000)
            .map(|index| OutlineItem {
                title: "重复中文".into(),
                slug: format!("重复中文-{index}"),
                line: index + 1,
                level: 2,
            })
            .collect();
        let toc = DocumentToc::new(headings);
        let repeated = toc.clone();
        assert!(std::sync::Arc::ptr_eq(&toc.index, &repeated.index));
        let chunks: Vec<_> = toc.chunks(128, 4096).collect();
        assert!(chunks.len() > 1);
        let mut count = 0;
        for chunk in &chunks {
            assert!(std::sync::Arc::ptr_eq(&toc.index, &chunk.index));
            assert!(chunk.headings().len() <= 128);
            assert!(chunk.text_bytes() <= 4096);
            for heading in chunk.headings() {
                assert_eq!(heading.slug, format!("重复中文-{count}"));
                count += 1;
            }
        }
        assert_eq!(count, 10_000);
        assert_eq!(
            chunks.iter().map(DocumentToc::text_bytes).sum::<usize>(),
            toc.text_bytes()
        );
    }

    #[test]
    fn chunking_keeps_empty_and_indivisible_entries_explicit() {
        assert_eq!(DocumentToc::new(vec![]).chunks(1, 1).count(), 0);
        let toc = DocumentToc::new(vec![OutlineItem {
            title: "大".repeat(100),
            slug: "big".into(),
            line: 1,
            level: 1,
        }]);
        let parts: Vec<_> = toc.chunks(2, 10).collect();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].headings()[0].title, "大".repeat(100));
        assert!(parts[0].text_bytes() > 10);
    }
}
