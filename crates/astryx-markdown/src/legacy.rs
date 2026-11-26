//! The released parser-node projection, upstream `Markdown/parser.ts` ("Types",
//! `projectInlineNode`, `projectBlockNode` and their helpers): the `InlineNode` and
//! `BlockNode` shapes `parseMarkdown`, `parseInline` and `parseMarkdownIncremental` have
//! always returned (`bold`, `italic`, `codeblock`, `hr`, …), projected from the
//! canonical tree. Serialized with serde, a node reads as upstream's JSON. A top-level
//! block carries its `range` only when the caller asked for `source_ranges`; offsets are
//! UTF-8 bytes (upstream's are UTF-16 units).

use serde::Serialize;

use crate::ast::{self, Block, ExtensionNode, Phrasing, Root, TableAlign};

/// Upstream `SourceRange`: where a block sits in the source it was parsed from;
/// `&source[start..end]` is the block, without its trailing blank lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct SourceRange {
    pub start: usize,
    pub end: usize,
}

/// Upstream `InlineNode` (with `InlineNodeWithMath`'s `math`): an inline node of the
/// released shape. `math` appears only in a `math` parse.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InlineNode {
    Text {
        content: String,
    },
    Bold {
        children: Vec<InlineNode>,
    },
    Italic {
        children: Vec<InlineNode>,
    },
    Strikethrough {
        children: Vec<InlineNode>,
    },
    Code {
        content: String,
    },
    Math {
        value: String,
    },
    Link {
        href: String,
        children: Vec<InlineNode>,
    },
    Image {
        src: String,
        alt: String,
    },
    Citation {
        #[serde(rename = "sourceId")]
        source_id: String,
    },
    Break,
    /// A plugin's inline extension node, passed through.
    Extension(ExtensionNode),
}

/// Upstream `ListItemNode`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ListItemNode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    pub children: Vec<BlockNode>,
}

/// Upstream `TableCellNode`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TableCellNode {
    pub children: Vec<InlineNode>,
}

/// Upstream `BlockNode` (with `BlockNodeWithMath`'s `math`): a block of the released shape.
/// `range` is set on top-level blocks of a `source_ranges` parse only.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BlockNode {
    Heading {
        /// 1 to 6.
        level: u8,
        children: Vec<InlineNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<SourceRange>,
    },
    Paragraph {
        children: Vec<InlineNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<SourceRange>,
    },
    /// A fenced code block. `language` is the info string's leading word characters, or
    /// `plaintext`.
    Codeblock {
        language: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<SourceRange>,
    },
    Math {
        value: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<SourceRange>,
    },
    Blockquote {
        children: Vec<BlockNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<SourceRange>,
    },
    List {
        ordered: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        start: Option<i64>,
        /// The ordered-list marker delimiter, `.` or `)`; `None` for bullets.
        #[serde(skip_serializing_if = "Option::is_none")]
        delimiter: Option<char>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loose: Option<bool>,
        items: Vec<ListItemNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<SourceRange>,
    },
    Table {
        headers: Vec<TableCellNode>,
        alignments: Vec<Option<TableAlign>>,
        rows: Vec<Vec<TableCellNode>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<SourceRange>,
    },
    Hr {
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<SourceRange>,
    },
    Image {
        src: String,
        alt: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        range: Option<SourceRange>,
    },
    /// A plugin's block extension node, passed through.
    Extension(ExtensionNode),
}

impl BlockNode {
    /// The block's `type`, as upstream's JSON spells it.
    pub fn type_name(&self) -> &'static str {
        match self {
            BlockNode::Heading { .. } => "heading",
            BlockNode::Paragraph { .. } => "paragraph",
            BlockNode::Codeblock { .. } => "codeblock",
            BlockNode::Math { .. } => "math",
            BlockNode::Blockquote { .. } => "blockquote",
            BlockNode::List { .. } => "list",
            BlockNode::Table { .. } => "table",
            BlockNode::Hr { .. } => "hr",
            BlockNode::Image { .. } => "image",
            BlockNode::Extension(_) => "extension",
        }
    }

    /// The block's `range`, when it carries one.
    pub fn range(&self) -> Option<SourceRange> {
        match self {
            BlockNode::Heading { range, .. }
            | BlockNode::Paragraph { range, .. }
            | BlockNode::Codeblock { range, .. }
            | BlockNode::Math { range, .. }
            | BlockNode::Blockquote { range, .. }
            | BlockNode::List { range, .. }
            | BlockNode::Table { range, .. }
            | BlockNode::Hr { range }
            | BlockNode::Image { range, .. } => *range,
            BlockNode::Extension(_) => None,
        }
    }
}

/// Upstream `projectRange`: a canonical position's offsets as a `range`.
fn project_range(position: Option<&ast::Position>) -> Option<SourceRange> {
    let position = position?;
    Some(SourceRange {
        start: position.start.offset?,
        end: position.end.offset?,
    })
}

/// Upstream `projectExtensionNode`: an extension node passes through, without the
/// canonical `position` unless the caller asked for provenance.
fn project_extension_node(node: &ExtensionNode, with_ranges: bool) -> ExtensionNode {
    if with_ranges || node.position.is_none() {
        return node.clone();
    }
    ExtensionNode {
        position: None,
        ..node.clone()
    }
}

fn project_children(nodes: &[Phrasing], with_ranges: bool) -> Vec<InlineNode> {
    nodes
        .iter()
        .map(|node| project_inline_node(node, with_ranges))
        .collect()
}

/// Upstream `projectInlineNode`: a canonical phrasing node in the released shape.
pub fn project_inline_node(node: &Phrasing, with_ranges: bool) -> InlineNode {
    match node {
        Phrasing::Text(text) => InlineNode::Text {
            content: text.value.clone(),
        },
        Phrasing::Strong(parent) => InlineNode::Bold {
            children: project_children(&parent.children, with_ranges),
        },
        Phrasing::Emphasis(parent) => InlineNode::Italic {
            children: project_children(&parent.children, with_ranges),
        },
        Phrasing::Delete(parent) => InlineNode::Strikethrough {
            children: project_children(&parent.children, with_ranges),
        },
        Phrasing::InlineCode(code) => InlineNode::Code {
            content: code.value.clone(),
        },
        Phrasing::InlineMath(math) => InlineNode::Math {
            value: math.value.clone(),
        },
        Phrasing::Link(link) => InlineNode::Link {
            href: link.url.clone(),
            children: project_children(&link.children, with_ranges),
        },
        Phrasing::Image(image) => InlineNode::Image {
            src: image.url.clone(),
            alt: image.alt.clone(),
        },
        Phrasing::Citation(citation) => InlineNode::Citation {
            source_id: citation.source_id.clone(),
        },
        Phrasing::Break(_) => InlineNode::Break,
        Phrasing::Extension(node) => {
            InlineNode::Extension(project_extension_node(node, with_ranges))
        }
    }
}

/// Upstream `projectTableCell`.
fn project_table_cell(cell: &ast::TableCell, with_ranges: bool) -> TableCellNode {
    TableCellNode {
        children: project_children(&cell.children, with_ranges),
    }
}

/// Upstream `projectListItem`.
fn project_list_item(item: &ast::ListItem, with_ranges: bool) -> ListItemNode {
    ListItemNode {
        checked: item.checked,
        children: item
            .children
            .iter()
            .map(|child| project_block_node(child, with_ranges))
            .collect(),
    }
}

/// Upstream `projectBlockNode`: a canonical block in the released shape, with its
/// `range` when `with_ranges` (the caller's `source_ranges`) and it has offsets.
pub fn project_block_node(node: &Block, with_ranges: bool) -> BlockNode {
    let range = |position: &Option<ast::Position>| {
        if with_ranges {
            project_range(position.as_ref())
        } else {
            None
        }
    };
    let blocks = |children: &[Block]| {
        children
            .iter()
            .map(|child| project_block_node(child, with_ranges))
            .collect()
    };
    match node {
        Block::Heading(heading) => BlockNode::Heading {
            level: heading.depth,
            children: project_children(&heading.children, with_ranges),
            range: range(&heading.position),
        },
        Block::Paragraph(paragraph) => BlockNode::Paragraph {
            children: project_children(&paragraph.children, with_ranges),
            range: range(&paragraph.position),
        },
        Block::Code(code) => BlockNode::Codeblock {
            language: code.legacy_language().unwrap_or("plaintext").to_string(),
            content: code.value.clone(),
            range: range(&code.position),
        },
        Block::Math(math) => BlockNode::Math {
            value: math.value.clone(),
            range: range(&math.position),
        },
        Block::Blockquote(quote) => BlockNode::Blockquote {
            children: blocks(&quote.children),
            range: range(&quote.position),
        },
        Block::List(list) => BlockNode::List {
            ordered: list.ordered,
            start: list.start,
            delimiter: list.delimiter,
            loose: list.spread,
            items: list
                .children
                .iter()
                .map(|item| project_list_item(item, with_ranges))
                .collect(),
            range: range(&list.position),
        },
        Block::Table(table) => {
            let mut rows = table.children.iter();
            let headers = rows.next().map_or_else(Vec::new, |header| {
                header
                    .children
                    .iter()
                    .map(|cell| project_table_cell(cell, with_ranges))
                    .collect()
            });
            BlockNode::Table {
                headers,
                alignments: table.align.clone(),
                rows: rows
                    .map(|row| {
                        row.children
                            .iter()
                            .map(|cell| project_table_cell(cell, with_ranges))
                            .collect()
                    })
                    .collect(),
                range: range(&table.position),
            }
        }
        Block::ThematicBreak(rule) => BlockNode::Hr {
            range: range(&rule.position),
        },
        Block::Image(image) => BlockNode::Image {
            src: image.url.clone(),
            alt: image.alt.clone(),
            range: range(&image.position),
        },
        Block::Extension(node) => BlockNode::Extension(project_extension_node(node, with_ranges)),
    }
}

/// Upstream `projectInlineNodes`.
pub fn project_inline_nodes(nodes: &[Phrasing], with_ranges: bool) -> Vec<InlineNode> {
    project_children(nodes, with_ranges)
}

/// Upstream `projectMarkdownRoot`: a canonical root's blocks in the released shape.
pub fn project_markdown_root(root: &Root, with_ranges: bool) -> Vec<BlockNode> {
    root.children
        .iter()
        .map(|block| project_block_node(block, with_ranges))
        .collect()
}
