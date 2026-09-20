use std::collections::HashMap;

use pulldown_cmark::{CodeBlockKind, Event, Tag};

use crate::{
    models::{diagram::DiagramSource, export::ExportWarning},
    services::{
        export_resources::{resolve_export_link, ExportLink},
        markdown_service,
    },
};

pub(crate) type NodeId = usize;

#[derive(Debug, Clone)]
pub(crate) enum SemanticNode {
    Element {
        tag: Tag<'static>,
        children: Vec<NodeId>,
    },
    Event(Event<'static>),
}

/// Flat, index-addressed representation of the Markdown event hierarchy.
///
/// Keeping child links as indices is intentional: deeply nested Markdown must
/// not turn either traversal *or destruction* of the parsed document into a
/// recursive call chain on the process stack.
#[derive(Debug, Clone)]
pub(crate) struct SemanticDocument {
    nodes: Vec<SemanticNode>,
    roots: Vec<NodeId>,
    link_targets: HashMap<NodeId, Result<ExportLink, ExportWarning>>,
    diagram_sources: HashMap<NodeId, DiagramSource>,
    footnote_numbers: HashMap<String, usize>,
}

impl SemanticDocument {
    pub(crate) fn parse(markdown: &str, source_path: Option<&str>) -> Self {
        let mut document = Self {
            nodes: Vec::new(),
            roots: Vec::new(),
            link_targets: HashMap::new(),
            diagram_sources: HashMap::new(),
            footnote_numbers: HashMap::new(),
        };
        let mut parents = Vec::new();
        let (events, diagrams) = markdown_service::normalized_markdown_with_diagrams(markdown);
        let mut diagrams = diagrams.into_iter();

        for event in events {
            if let Event::FootnoteReference(label) | Event::Start(Tag::FootnoteDefinition(label)) =
                &event
            {
                let next = document.footnote_numbers.len() + 1;
                document
                    .footnote_numbers
                    .entry(label.to_string())
                    .or_insert(next);
            }
            match event {
                Event::Start(tag) => {
                    let link = match &tag {
                        Tag::Link { dest_url, .. } => {
                            Some(resolve_export_link(source_path, dest_url))
                        }
                        _ => None,
                    };
                    let id = document.push_node(
                        parents.last().copied(),
                        SemanticNode::Element {
                            tag,
                            children: Vec::new(),
                        },
                    );
                    if let Some(link) = link {
                        document.link_targets.insert(id, link);
                    }
                    if matches!(
                        &document.nodes[id],
                        SemanticNode::Element {
                            tag: Tag::CodeBlock(CodeBlockKind::Fenced(info)),
                            ..
                        } if info.trim().eq_ignore_ascii_case("mermaid")
                    ) {
                        if let Some(source) = diagrams.next() {
                            document.diagram_sources.insert(id, source);
                        }
                    }
                    parents.push(id);
                }
                Event::End(_) => {
                    parents.pop();
                }
                event => {
                    document.push_node(parents.last().copied(), SemanticNode::Event(event));
                }
            }
        }

        document
    }

    fn push_node(&mut self, parent: Option<NodeId>, node: SemanticNode) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(node);
        if let Some(parent) = parent {
            match &mut self.nodes[parent] {
                SemanticNode::Element { children, .. } => children.push(id),
                SemanticNode::Event(_) => unreachable!("events cannot own semantic children"),
            }
        } else {
            self.roots.push(id);
        }
        id
    }

    pub(crate) fn footnote_number(&self, label: &str) -> usize {
        self.footnote_numbers
            .get(label)
            .copied()
            .expect("parsed footnote has a number")
    }

    pub(crate) fn append_fragment_node(&mut self, node: SemanticNode) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(node);
        id
    }

    pub(crate) fn replace_roots(&mut self, roots: Vec<NodeId>) {
        self.roots = roots;
    }

    pub(crate) fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    pub(crate) fn node(&self, id: NodeId) -> &SemanticNode {
        &self.nodes[id]
    }

    pub(crate) fn link_target(&self, id: NodeId) -> Option<&Result<ExportLink, ExportWarning>> {
        self.link_targets.get(&id)
    }

    pub(crate) fn diagram_source(&self, id: NodeId) -> Option<&DiagramSource> {
        self.diagram_sources.get(&id)
    }

    pub(crate) fn diagram_sources(&self) -> Vec<DiagramSource> {
        let mut sources = self.diagram_sources.values().cloned().collect::<Vec<_>>();
        sources.sort_unstable_by_key(|source| source.ordinal);
        sources
    }

    pub(crate) fn plain_text(&self, roots: &[NodeId]) -> String {
        let mut text = String::new();
        let mut pending = roots.iter().rev().copied().collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            match self.node(id) {
                SemanticNode::Element { children, .. } => {
                    pending.extend(children.iter().rev().copied());
                }
                SemanticNode::Event(event) => text.push_str(&event_text(event)),
            }
        }
        text
    }

    #[cfg(test)]
    fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

pub(crate) fn event_text(event: &Event<'_>) -> String {
    match event {
        Event::Text(value)
        | Event::Code(value)
        | Event::InlineMath(value)
        | Event::DisplayMath(value)
        | Event::Html(value)
        | Event::InlineHtml(value) => value.to_string(),
        Event::FootnoteReference(value) => format!("[^{value}]"),
        Event::SoftBreak | Event::HardBreak => "\n".to_string(),
        Event::Rule => "────────────────".to_string(),
        Event::TaskListMarker(done) => {
            if *done {
                "☒ ".to_string()
            } else {
                "☐ ".to_string()
            }
        }
        Event::Start(_) | Event::End(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use pulldown_cmark::Tag;

    use super::{SemanticDocument, SemanticNode};
    use crate::services::export_resources::ExportLink;

    #[test]
    fn binds_typed_link_targets_and_rejections_to_shared_nodes() {
        let document = SemanticDocument::parse(
            "[heading](#named) [mail](mailto:writer@example.org) [bad](javascript:alert(1))",
            None,
        );
        let links = document
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(id, node)| match node {
                SemanticNode::Element {
                    tag: Tag::Link { .. },
                    ..
                } => document.link_target(id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(links.len(), 3);
        assert_eq!(links[0], &Ok(ExportLink::Anchor("named".to_string())));
        assert_eq!(
            links[1],
            &Ok(ExportLink::Email("writer@example.org".to_string()))
        );
        let warning = links[2].as_ref().unwrap_err();
        assert_eq!(warning.code, "UNSUPPORTED_LINK_SCHEME");
        assert_eq!(warning.target.as_deref(), Some("javascript:alert(1)"));
    }

    #[test]
    fn parses_and_drops_deep_nesting_without_recursive_ownership() {
        let depth = 8_000;
        let markdown = format!("{}leaf", "> ".repeat(depth));
        let document = SemanticDocument::parse(&markdown, None);
        assert!(document.node_count() >= depth);
        assert!(document.plain_text(document.roots()).contains("leaf"));
        drop(document);
    }

    #[test]
    fn reuses_preview_heading_ids_in_the_export_semantic_tree() {
        let document =
            SemanticDocument::parse("#\n\n# 中文\n\n# 中文\n\n## Explicit {#fixed}", None);
        let ids = document
            .nodes
            .iter()
            .filter_map(|node| match node {
                SemanticNode::Element {
                    tag: Tag::Heading { id, .. },
                    ..
                } => id.as_ref().map(ToString::to_string),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(ids, ["section", "中文", "中文-2", "fixed"]);
    }

    #[test]
    fn code_block_urls_remain_text_in_the_shared_export_tree() {
        let document = SemanticDocument::parse(
            "```\nhttps://code.example writer@code.example\n```\n\nhttps://body.example",
            None,
        );
        let links = document
            .nodes
            .iter()
            .filter(|node| {
                matches!(
                    node,
                    SemanticNode::Element {
                        tag: Tag::Link { .. },
                        ..
                    }
                )
            })
            .count();
        assert_eq!(links, 1);
        assert!(document
            .plain_text(document.roots())
            .contains("https://code.example writer@code.example"));
    }

    #[test]
    fn binds_mermaid_identity_to_the_shared_code_block_node() {
        let document = SemanticDocument::parse(
            "```mermaid\nflowchart TD\nA-->B\n```\n\n```rust\nfn main() {}\n```",
            None,
        );
        assert_eq!(document.diagram_sources.len(), 1);
        let (&node_id, source) = document.diagram_sources.iter().next().unwrap();
        assert!(matches!(
            document.node(node_id),
            SemanticNode::Element {
                tag: Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(info)),
                ..
            } if info.as_ref() == "mermaid"
        ));
        assert_eq!(source.ordinal, 0);
        assert_eq!(source.source_utf8, "flowchart TD\nA-->B\n");
    }
}
