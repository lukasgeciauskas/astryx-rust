//! Heading projection, upstream `Markdown/headingProjection.ts` (`projectMarkdownHeadings`):
//! heading ids, labels and permalink URLs, computed once after every transform ran, and
//! the Outline items, so Markdown and Outline agree on every heading's identity (FR16).

use std::collections::HashMap;

use serde::Serialize;

use crate::ast::{Block, Heading, Root, markdown_ast_text_with};
use crate::plugins::PreparedMarkdownPlugins;
use crate::plugins::diagnostics::Diagnostics;
use crate::plugins::extension_text;
use crate::plugins::heading_links::project_markdown_heading_links;
use crate::slug::{slugify, unique_slug};

/// Upstream `OutlineItem`: one heading of a document's outline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OutlineItem {
    pub id: String,
    pub label: String,
    /// The heading's depth, 1 to 6.
    pub level: u8,
}

/// An identified heading of a [`MarkdownHeadingProjection`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedHeading {
    pub id: String,
    /// Its plain text, trimmed.
    pub label: String,
    /// The canonical permalink the heading-links plugin copies; `None` without it.
    pub permalink_url: Option<String>,
}

/// Upstream `MarkdownHeadingProjection`. Upstream keys its maps by heading object; the
/// port keys them by each heading's index in a depth-first walk of the document (the
/// order [`crate::ast::visit_markdown_nodes`] visits headings in), which a renderer walking
/// the tree in document order counts as it goes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkdownHeadingProjection {
    /// The headings with generated ids: by default the root's; with the heading-links
    /// plugin, every one.
    pub headings: HashMap<usize, ProjectedHeading>,
    /// Root headings only, preserving the released Outline scope.
    pub outline: Vec<OutlineItem>,
}

impl MarkdownHeadingProjection {
    /// The heading at depth-first index `index`, if it has an id.
    pub fn heading(&self, index: usize) -> Option<&ProjectedHeading> {
        self.headings.get(&index)
    }
}

/// Every heading of `blocks` depth first, with whether it sits at the root.
fn collect_headings<'a>(blocks: &'a [Block], at_root: bool, out: &mut Vec<(&'a Heading, bool)>) {
    for block in blocks {
        match block {
            Block::Heading(heading) => out.push((heading, at_root)),
            Block::Blockquote(quote) => collect_headings(&quote.children, false, out),
            Block::List(list) => {
                for item in &list.children {
                    collect_headings(&item.children, false, out);
                }
            }
            _ => {}
        }
    }
}

/// Upstream `projectMarkdownHeadings`: ids once, after every transform ran.
pub fn project_markdown_headings(
    root: &Root,
    plugins: Option<&PreparedMarkdownPlugins>,
) -> MarkdownHeadingProjection {
    project(root, plugins, &Diagnostics::default())
}

pub(crate) fn project(
    root: &Root,
    plugins: Option<&PreparedMarkdownPlugins>,
    diagnostics: &Diagnostics,
) -> MarkdownHeadingProjection {
    let mut all = Vec::new();
    collect_headings(&root.children, true, &mut all);
    if let Some(links) = project_markdown_heading_links(root, plugins, diagnostics) {
        let mut projection = MarkdownHeadingProjection::default();
        for (index, ((id, label, permalink), (heading, at_root))) in
            links.headings.into_iter().zip(&all).enumerate()
        {
            if *at_root {
                projection.outline.push(OutlineItem {
                    id: id.clone(),
                    label: label.clone(),
                    level: heading.depth,
                });
            }
            projection.headings.insert(
                index,
                ProjectedHeading {
                    id,
                    label,
                    permalink_url: Some(permalink),
                },
            );
        }
        return projection;
    }
    let mut projection = MarkdownHeadingProjection::default();
    let mut counts = HashMap::new();
    for (index, (heading, at_root)) in all.into_iter().enumerate() {
        if !at_root {
            continue;
        }
        let label = crate::js::trim(&markdown_ast_text_with(&heading.children, &|node| {
            extension_text(plugins, node, diagnostics)
        }))
        .to_string();
        let id = unique_slug(&slugify(&label), &mut counts);
        projection.outline.push(OutlineItem {
            id: id.clone(),
            label: label.clone(),
            level: heading.depth,
        });
        projection.headings.insert(
            index,
            ProjectedHeading {
                id,
                label,
                permalink_url: None,
            },
        );
    }
    projection
}
