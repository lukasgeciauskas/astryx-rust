//! The soft-breaks plugin, upstream `Markdown/plugins/softBreaks.ts`: every soft line
//! ending (LF, CRLF or a lone CR) in eligible phrasing becomes a hard `break`. Code, math,
//! images and extension nodes are opaque; link text is eligible.

use std::sync::LazyLock;

use regex::Regex;

use super::{MarkdownPlugin, PluginDefinition, Transform, create_markdown_plugin};
use crate::ast::{Block, Literal, Phrasing, Root, Void};

static LINE_ENDING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\r?\n|\r").expect("LINE_ENDING is valid"));

/// Upstream `replaceLineEndings`: `value` as text and breaks, or `None` with no line ending.
fn replace_line_endings(value: &str) -> Option<Vec<Phrasing>> {
    let mut output = Vec::new();
    let mut cursor = 0;
    for ending in LINE_ENDING.find_iter(value) {
        if ending.start() > cursor {
            output.push(text(&value[cursor..ending.start()]));
        }
        output.push(Phrasing::Break(Void::default()));
        cursor = ending.end();
    }
    if output.is_empty() {
        return None;
    }
    if cursor < value.len() {
        output.push(text(&value[cursor..]));
    }
    Some(output)
}

fn text(value: &str) -> Phrasing {
    Phrasing::Text(Literal {
        value: value.to_string(),
        ..Literal::default()
    })
}

/// Upstream `transformPhrasing`. Returns whether anything changed.
fn transform_phrasing(children: &mut Vec<Phrasing>) -> bool {
    let mut changed = false;
    for node in std::mem::take(children) {
        match node {
            Phrasing::Text(literal) => match replace_line_endings(&literal.value) {
                Some(replacement) => {
                    children.extend(replacement);
                    changed = true;
                }
                None => children.push(Phrasing::Text(literal)),
            },
            Phrasing::Strong(mut parent) => {
                changed |= transform_phrasing(&mut parent.children);
                children.push(Phrasing::Strong(parent));
            }
            Phrasing::Emphasis(mut parent) => {
                changed |= transform_phrasing(&mut parent.children);
                children.push(Phrasing::Emphasis(parent));
            }
            Phrasing::Delete(mut parent) => {
                changed |= transform_phrasing(&mut parent.children);
                children.push(Phrasing::Delete(parent));
            }
            Phrasing::Link(mut link) => {
                changed |= transform_phrasing(&mut link.children);
                children.push(Phrasing::Link(link));
            }
            other => children.push(other),
        }
    }
    changed
}

/// Upstream `transformBlocks`.
fn transform_blocks(blocks: &mut [Block]) -> bool {
    let mut changed = false;
    for block in blocks {
        changed |= match block {
            Block::Heading(node) => transform_phrasing(&mut node.children),
            Block::Paragraph(node) => transform_phrasing(&mut node.children),
            Block::Blockquote(node) => transform_blocks(&mut node.children),
            Block::List(node) => node.children.iter_mut().fold(false, |changed, item| {
                transform_blocks(&mut item.children) | changed
            }),
            Block::Table(node) => node.children.iter_mut().fold(false, |changed, row| {
                row.children.iter_mut().fold(changed, |changed, cell| {
                    transform_phrasing(&mut cell.children) | changed
                })
            }),
            Block::Code(_)
            | Block::Math(_)
            | Block::ThematicBreak(_)
            | Block::Image(_)
            | Block::Extension(_) => false,
        };
    }
    changed
}

/// Upstream `markdownSoftBreaksPlugin` (`soft-breaks`): soft line endings in phrasing
/// content become hard breaks. A plugin-authored transform, so Core validates its output.
pub fn markdown_soft_breaks_plugin() -> MarkdownPlugin {
    create_markdown_plugin(PluginDefinition {
        transform: Some(Transform::new(|document: &Root, _| {
            let mut next = document.clone();
            Ok(transform_blocks(&mut next.children).then_some(next))
        })),
        ..PluginDefinition::new("soft-breaks")
    })
    .expect("the soft-breaks plugin is valid")
}
