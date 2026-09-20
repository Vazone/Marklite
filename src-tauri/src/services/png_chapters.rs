use std::collections::{HashMap, HashSet};

use pulldown_cmark::{Event, HeadingLevel, Tag};

use crate::{
    models::app_error::AppError,
    services::export_semantic::{NodeId, SemanticDocument, SemanticNode},
};

pub(crate) const MAX_CHAPTERS: usize = 10_000;
const MAX_CHAPTER_NODES: usize = 64_000;
const MAX_CHAPTER_TEXT_BYTES: usize = 1024 * 1024;

pub(crate) struct Chapter {
    pub roots: Vec<NodeId>,
    pub title: String,
    pub filename: String,
}

pub(crate) fn plan(document: &SemanticDocument) -> Result<Vec<Chapter>, AppError> {
    let roots = document.roots();
    let headings = roots
        .iter()
        .enumerate()
        .filter_map(|(index, id)| match document.node(*id) {
            SemanticNode::Element {
                tag: Tag::Heading { level, .. },
                ..
            } => Some((index, *level)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut level = headings.iter().map(|(_, level)| *level as u8).min();
    let opening_title = headings.first() == Some(&(0, HeadingLevel::H1))
        && headings
            .iter()
            .filter(|(_, level)| *level == HeadingLevel::H1)
            .count()
            == 1
        && headings
            .iter()
            .filter(|(_, level)| *level == HeadingLevel::H2)
            .count()
            >= 2;
    if opening_title {
        level = Some(HeadingLevel::H2 as u8);
    }
    let boundaries = headings
        .iter()
        .filter_map(|(index, heading_level)| {
            (Some(*heading_level as u8) == level).then_some(*index)
        })
        .collect::<Vec<_>>();
    let mut spans = Vec::new();
    if let Some(&first) = boundaries.first() {
        // A lone document H1 accompanies chapter one. Real introductory prose
        // remains a separate preface image, preserving its original ordering.
        if first > 0 && !(opening_title && first == 1) {
            spans.push((0..first, "序言".to_owned()));
        }
        for (position, &start) in boundaries.iter().enumerate() {
            let end = boundaries.get(position + 1).copied().unwrap_or(roots.len());
            let title = document.plain_text(&[roots[start]]);
            let start = if position == 0 && opening_title && first == 1 {
                0
            } else {
                start
            };
            spans.push((start..end, title));
        }
    } else {
        spans.push((0..roots.len(), "正文".to_owned()));
    }
    if spans.len() > MAX_CHAPTERS {
        return Err(AppError::new(
            "PNG_TOO_MANY_CHAPTERS",
            "章节图片最多导出 10000 章",
        ));
    }
    let definitions = roots
        .iter()
        .filter_map(|id| match document.node(*id) {
            SemanticNode::Element {
                tag: Tag::FootnoteDefinition(label),
                ..
            } => Some((label.to_string(), *id)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let digits = spans.len().to_string().len().max(4);
    spans
        .into_iter()
        .enumerate()
        .map(|(index, (range, title))| {
            let mut chapter_roots = roots[range].to_vec();
            let mut included = chapter_roots.iter().copied().collect::<HashSet<_>>();
            let mut pending = chapter_roots.iter().rev().copied().collect::<Vec<_>>();
            let (mut nodes, mut bytes) = (0usize, 0usize);
            while let Some(id) = pending.pop() {
                nodes += 1;
                match document.node(id) {
                    SemanticNode::Element { tag, children } => {
                        if let Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } = tag {
                            bytes += dest_url.len();
                        }
                        pending.extend(children.iter().rev().copied());
                    }
                    SemanticNode::Event(Event::FootnoteReference(label)) => {
                        if let Some(&definition) = definitions.get(label.as_ref()) {
                            if included.insert(definition) {
                                chapter_roots.push(definition);
                                pending.push(definition);
                            }
                        }
                    }
                    SemanticNode::Event(event) => {
                        bytes += match event {
                            Event::Text(text)
                            | Event::Code(text)
                            | Event::Html(text)
                            | Event::InlineHtml(text)
                            | Event::InlineMath(text)
                            | Event::DisplayMath(text) => text.len(),
                            _ => 0,
                        };
                    }
                }
                if nodes > MAX_CHAPTER_NODES || bytes > MAX_CHAPTER_TEXT_BYTES {
                    return Err(AppError::new(
                        "PNG_CHAPTER_TOO_LARGE",
                        format!("第 {} 章超过单章内容预算，未裁切或拆分", index + 1),
                    ));
                }
            }
            let filename = format!("{:0digits$}-{}.png", index + 1, safe_name(&title));
            Ok(Chapter {
                roots: chapter_roots,
                title,
                filename,
            })
        })
        .collect()
}

pub(crate) fn safe_name(value: &str) -> String {
    let mut name = String::new();
    for ch in value.trim().chars() {
        let ch = if ch.is_control() || "<>:\"/\\|?*".contains(ch) {
            '-'
        } else {
            ch
        };
        if name.len() + ch.len_utf8() > 160 {
            break;
        }
        name.push(ch);
    }
    let name = name.trim_end_matches(['.', ' ']);
    if name.is_empty() {
        "Untitled".to_owned()
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_title_and_subchapters_preserve_all_content() {
        let document = SemanticDocument::parse(
            "# Book\n\n## One\n\nfirst\n\n### Detail\n\ntext\n\n## Two\n\nlast",
            None,
        );
        let chapters = plan(&document).unwrap();
        assert_eq!(
            chapters
                .iter()
                .map(|c| c.title.as_str())
                .collect::<Vec<_>>(),
            ["One", "Two"]
        );
        let all = chapters
            .iter()
            .flat_map(|c| c.roots.iter().copied())
            .collect::<Vec<_>>();
        assert_eq!(all, document.roots());
        assert!(document
            .plain_text(&chapters[0].roots)
            .starts_with("BookOne"));
    }

    #[test]
    fn preface_and_nested_heading_do_not_create_false_chapters() {
        let document = SemanticDocument::parse(
            "# Book\n\nPreface\n\n## One\n\n> # Quoted\n\n## Two\n\nend",
            None,
        );
        let chapters = plan(&document).unwrap();
        assert_eq!(
            chapters
                .iter()
                .map(|c| c.title.as_str())
                .collect::<Vec<_>>(),
            ["序言", "One", "Two"]
        );
        assert!(document.plain_text(&chapters[1].roots).contains("Quoted"));
        assert_eq!(
            plan(&SemanticDocument::parse("no headings", None))
                .unwrap()
                .len(),
            1
        );
        assert_eq!(plan(&SemanticDocument::parse("", None)).unwrap().len(), 1);
    }

    #[test]
    fn global_footnotes_are_available_in_each_referencing_image() {
        let document = SemanticDocument::parse(
            "# One\n\nA[^n]\n\n# Two\n\nB[^n]\n\n[^n]: shared note",
            None,
        );
        let chapters = plan(&document).unwrap();
        assert_eq!(chapters.len(), 2);
        for chapter in chapters {
            assert_eq!(
                document
                    .plain_text(&chapter.roots)
                    .matches("shared note")
                    .count(),
                1
            );
        }
    }

    #[test]
    fn duplicate_and_unsafe_names_are_ordered_and_bounded() {
        let document = SemanticDocument::parse("# Same\n\n# Same", None);
        let chapters = plan(&document).unwrap();
        assert_eq!(chapters[0].filename, "0001-Same.png");
        assert_eq!(chapters[1].filename, "0002-Same.png");
        assert!(!safe_name("../a\\b:*?<>|").contains(['/', '\\', ':', '*', '?', '<', '>', '|']));
        assert!(safe_name(&"🪶".repeat(100)).len() <= 160);
    }

    #[test]
    fn rejects_a_huge_chapter_without_partial_output() {
        let document = SemanticDocument::parse(&"x".repeat(MAX_CHAPTER_TEXT_BYTES + 1), None);
        assert_eq!(plan(&document).err().unwrap().code, "PNG_CHAPTER_TOO_LARGE");
    }
}
