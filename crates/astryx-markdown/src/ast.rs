//! Markdown's canonical AST, upstream `Markdown/ast.ts`: mdast-aligned nodes, with
//! `visitMarkdownNodes` and `markdownAstText`. Serialized
//! with serde, a tree reads as upstream's JSON: the same `type` tags and field names, absent
//! optional fields left out. Offsets are UTF-8 bytes (upstream's are UTF-16 units).

use serde::Serialize;

/// Upstream `MarkdownAstDataValue`: finite JSON-like data an extension node carries.
pub type DataValue = serde_json::Value;

/// Upstream `MarkdownAstPoint`: a place in the source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Point {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
}

/// Upstream `MarkdownAstPosition`: where a node starts and ends in the source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Position {
    pub start: Point,
    pub end: Point,
}

impl Position {
    /// A position with offsets only, as the parser stamps top-level blocks.
    pub fn offsets(start: usize, end: usize) -> Self {
        Position {
            start: Point {
                offset: Some(start),
                ..Point::default()
            },
            end: Point {
                offset: Some(end),
                ..Point::default()
            },
        }
    }
}

/// Upstream `MarkdownAstExtensionNode`: a plugin's node.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ExtensionNode {
    pub plugin: String,
    pub name: String,
    pub data: DataValue,
    pub display: ExtensionDisplay,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

impl ExtensionNode {
    /// Upstream `isMarkdownExtensionNode`: whether this is `plugin`'s node named `name`.
    pub fn is(&self, plugin: &str, name: &str) -> bool {
        self.plugin == plugin && self.name == name
    }
}

/// Whether an extension node is phrasing or flow content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionDisplay {
    Inline,
    Block,
}

/// A node with a string value: `text`, `inlineCode`, `inlineMath` and block `math`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Literal {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// A phrasing parent: `strong`, `emphasis` and `delete`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Parent {
    pub children: Vec<Phrasing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// Upstream `MarkdownAstLink`. An unsafe destination is the empty string.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Link {
    pub url: String,
    pub children: Vec<Phrasing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// Upstream `MarkdownAstImage`, inline or as a block of its own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Image {
    pub url: String,
    pub alt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// Upstream `MarkdownAstCitation`: `[id]` or `【id】` naming a known source.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// A node with no content: `break` and `thematicBreak`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Void {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// Upstream `MarkdownAstPhrasingContent`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Phrasing {
    Text(Literal),
    Strong(Parent),
    Emphasis(Parent),
    Delete(Parent),
    InlineCode(Literal),
    InlineMath(Literal),
    Link(Link),
    Image(Image),
    Citation(Citation),
    Break(Void),
    /// An extension node whose `display` is `inline`.
    Extension(ExtensionNode),
}

/// Upstream `MarkdownAstHeading`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Heading {
    /// 1 to 6.
    pub depth: u8,
    pub children: Vec<Phrasing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
    /// Upstream's hidden source-heading marker: which parsed heading this is, so a
    /// plugin-authored transform cannot re-level, forge or duplicate it. A new heading has
    /// none; a clone keeps its original's, as a JavaScript spread copy does.
    #[serde(skip)]
    pub identity: HeadingIdentity,
}

/// The identity a parsed heading carries through the transform pipeline (upstream's
/// `sourceHeadingMarker`). Only the pipeline can give one; it never serializes, and it
/// takes no part in equality, since it is provenance rather than content.
#[derive(Clone, Copy, Default)]
pub struct HeadingIdentity(pub(crate) Option<SourceHeading>);

/// One run's mark on one parsed heading: the run, the heading's index in it, and the
/// depth it was parsed with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SourceHeading {
    pub(crate) run: u64,
    pub(crate) index: usize,
    pub(crate) depth: u8,
}

impl HeadingIdentity {
    /// Whether the transform pipeline marked this heading as parsed from source.
    pub fn is_source(&self) -> bool {
        self.0.is_some()
    }
}

impl PartialEq for HeadingIdentity {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for HeadingIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "HeadingIdentity(source)"
        } else {
            "HeadingIdentity(none)"
        })
    }
}

/// Upstream `MarkdownAstParagraph` (and the content of a table cell).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Paragraph {
    pub children: Vec<Phrasing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// Upstream `MarkdownAstCode`: a fenced code block.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Code {
    /// The info string's first word; `null` in JSON when the fence names none.
    pub lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
    /// Upstream's hidden `markdownFenceProposal`: the block extension node a semantic-fence
    /// transform (`create_markdown_fence_transform`) proposes to render in this fence's
    /// place. It never serializes.
    #[serde(skip)]
    pub proposal: FenceProposal,
}

/// A semantic-fence transform's proposal for a code node (upstream
/// `MarkdownFenceProposal`). Only that helper can make one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FenceProposal(pub(crate) Option<Box<ExtensionNode>>);

impl FenceProposal {
    /// The proposed block extension node, if a fence transform claimed this fence.
    pub fn node(&self) -> Option<&ExtensionNode> {
        self.0.as_deref()
    }
}

impl Code {
    /// Upstream `getMarkdownFenceProposal`: the extension node a semantic-fence transform
    /// proposed for this fence.
    pub fn fence_proposal(&self) -> Option<&ExtensionNode> {
        self.proposal.node()
    }

    /// Upstream `getMarkdownAstLegacyCodeLanguage`: the info string's leading ASCII
    /// word characters (`info.match(/^(\w*)/)?.[1] || null`), the language the
    /// released `code` renderer receives.
    pub fn legacy_language(&self) -> Option<&str> {
        let lang = self.lang.as_deref()?;
        let end = lang
            .bytes()
            .position(|b| !(b.is_ascii_alphanumeric() || b == b'_'))
            .unwrap_or(lang.len());
        (end > 0).then(|| &lang[..end])
    }
}

/// Upstream `MarkdownAstBlockquote`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Blockquote {
    pub children: Vec<Block>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// Upstream `MarkdownAstListItem`. `checked` is set on a task item.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(tag = "type", rename = "listItem")]
pub struct ListItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    pub children: Vec<Block>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// Upstream `MarkdownAstList`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct List {
    pub ordered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spread: Option<bool>,
    /// `.` or `)`, on ordered lists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<char>,
    pub children: Vec<ListItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
    /// Upstream's hidden `listMarkerShapes` entry: the markers a list the block parser
    /// built opened its items with. The incremental parser joins a streamed list with the
    /// one before it only when their shapes agree, as the full parse does. It never
    /// serializes and takes no part in equality.
    #[serde(skip)]
    pub marker_shape: ListMarkerShape,
}

/// The indentation and the bullet (or ordered delimiter) of a parsed list's markers
/// (upstream `${baseIndent}:${ordered ? delim : bullet}`). A list a transform or the
/// Remark adapter made has none.
#[derive(Clone, Copy, Default)]
pub struct ListMarkerShape(pub(crate) Option<(usize, char)>);

impl ListMarkerShape {
    /// Whether the block parser recorded this list's markers.
    pub fn is_known(&self) -> bool {
        self.0.is_some()
    }
}

impl PartialEq for ListMarkerShape {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl std::fmt::Debug for ListMarkerShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some((indent, marker)) => write!(f, "ListMarkerShape({indent}:{marker})"),
            None => f.write_str("ListMarkerShape(none)"),
        }
    }
}

/// Upstream `MarkdownAstTableAlignment`; `None` (JSON `null`) when a column sets none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TableAlign {
    Left,
    Center,
    Right,
}

/// Upstream `MarkdownAstTableCell`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(tag = "type", rename = "tableCell")]
pub struct TableCell {
    pub children: Vec<Phrasing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// Upstream `MarkdownAstTableRow`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(tag = "type", rename = "tableRow")]
pub struct TableRow {
    pub children: Vec<TableCell>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// Upstream `MarkdownAstTable`: the first row is the header.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Table {
    pub align: Vec<Option<TableAlign>>,
    pub children: Vec<TableRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// Upstream `MarkdownAstBlockContent`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Block {
    Heading(Heading),
    Paragraph(Paragraph),
    Code(Code),
    Math(Literal),
    Blockquote(Blockquote),
    List(List),
    Table(Table),
    ThematicBreak(Void),
    Image(Image),
    /// An extension node whose `display` is `block`.
    Extension(ExtensionNode),
}

/// Upstream `MarkdownAstRoot`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(tag = "type", rename = "root")]
pub struct Root {
    pub children: Vec<Block>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Upstream `MarkdownAstNodeBase.data`: finite JSON-like data a transform attached.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataValue>,
}

/// Upstream `markdownAstText`: the plain text of phrasing content. Images give their alt
/// text, citations and breaks nothing, extension nodes their `source`.
pub fn markdown_ast_text(nodes: &[Phrasing]) -> String {
    markdown_ast_text_with(nodes, &|node| node.source.clone().unwrap_or_default())
}

/// Upstream `markdownAstText` with its `extensionText` argument: the plain text of phrasing
/// content, each extension node given by `extension_text` (Markdown passes
/// `PreparedMarkdownPlugins::extension_text`, the owning renderer's `toText`).
pub fn markdown_ast_text_with(
    nodes: &[Phrasing],
    extension_text: &dyn Fn(&ExtensionNode) -> String,
) -> String {
    let mut text = String::new();
    push_text(nodes, &mut text, extension_text);
    text
}

fn push_text(
    nodes: &[Phrasing],
    text: &mut String,
    extension_text: &dyn Fn(&ExtensionNode) -> String,
) {
    for node in nodes {
        match node {
            Phrasing::Text(n) | Phrasing::InlineCode(n) | Phrasing::InlineMath(n) => {
                text.push_str(&n.value)
            }
            Phrasing::Strong(n) | Phrasing::Emphasis(n) | Phrasing::Delete(n) => {
                push_text(&n.children, text, extension_text)
            }
            Phrasing::Link(n) => push_text(&n.children, text, extension_text),
            Phrasing::Image(n) => text.push_str(&n.alt),
            Phrasing::Citation(_) | Phrasing::Break(_) => {}
            Phrasing::Extension(n) => text.push_str(&extension_text(n)),
        }
    }
}

/// Upstream `MarkdownAstNodeMap`'s keys: every node `type` of the canonical tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeType {
    Root,
    Text,
    Strong,
    Emphasis,
    Delete,
    InlineCode,
    InlineMath,
    Break,
    Link,
    Image,
    Citation,
    Heading,
    Paragraph,
    Code,
    Math,
    Blockquote,
    List,
    ListItem,
    Table,
    TableRow,
    TableCell,
    ThematicBreak,
    Extension,
}

impl NodeType {
    /// The node's `type` string, as upstream's JSON spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            NodeType::Root => "root",
            NodeType::Text => "text",
            NodeType::Strong => "strong",
            NodeType::Emphasis => "emphasis",
            NodeType::Delete => "delete",
            NodeType::InlineCode => "inlineCode",
            NodeType::InlineMath => "inlineMath",
            NodeType::Break => "break",
            NodeType::Link => "link",
            NodeType::Image => "image",
            NodeType::Citation => "citation",
            NodeType::Heading => "heading",
            NodeType::Paragraph => "paragraph",
            NodeType::Code => "code",
            NodeType::Math => "math",
            NodeType::Blockquote => "blockquote",
            NodeType::List => "list",
            NodeType::ListItem => "listItem",
            NodeType::Table => "table",
            NodeType::TableRow => "tableRow",
            NodeType::TableCell => "tableCell",
            NodeType::ThematicBreak => "thematicBreak",
            NodeType::Extension => "extension",
        }
    }
}

/// A borrowed node of any type: what [`visit_markdown_nodes`] hands its visitor, the
/// variant narrowing it by kind as upstream's `MarkdownAstNodeMap` does.
#[derive(Clone, Copy, Debug)]
pub enum NodeRef<'a> {
    Root(&'a Root),
    Text(&'a Literal),
    Strong(&'a Parent),
    Emphasis(&'a Parent),
    Delete(&'a Parent),
    InlineCode(&'a Literal),
    InlineMath(&'a Literal),
    Break(&'a Void),
    Link(&'a Link),
    Image(&'a Image),
    Citation(&'a Citation),
    Heading(&'a Heading),
    Paragraph(&'a Paragraph),
    Code(&'a Code),
    Math(&'a Literal),
    Blockquote(&'a Blockquote),
    List(&'a List),
    ListItem(&'a ListItem),
    Table(&'a Table),
    TableRow(&'a TableRow),
    TableCell(&'a TableCell),
    ThematicBreak(&'a Void),
    Extension(&'a ExtensionNode),
}

impl<'a> NodeRef<'a> {
    /// The node's type.
    pub fn node_type(self) -> NodeType {
        match self {
            NodeRef::Root(_) => NodeType::Root,
            NodeRef::Text(_) => NodeType::Text,
            NodeRef::Strong(_) => NodeType::Strong,
            NodeRef::Emphasis(_) => NodeType::Emphasis,
            NodeRef::Delete(_) => NodeType::Delete,
            NodeRef::InlineCode(_) => NodeType::InlineCode,
            NodeRef::InlineMath(_) => NodeType::InlineMath,
            NodeRef::Break(_) => NodeType::Break,
            NodeRef::Link(_) => NodeType::Link,
            NodeRef::Image(_) => NodeType::Image,
            NodeRef::Citation(_) => NodeType::Citation,
            NodeRef::Heading(_) => NodeType::Heading,
            NodeRef::Paragraph(_) => NodeType::Paragraph,
            NodeRef::Code(_) => NodeType::Code,
            NodeRef::Math(_) => NodeType::Math,
            NodeRef::Blockquote(_) => NodeType::Blockquote,
            NodeRef::List(_) => NodeType::List,
            NodeRef::ListItem(_) => NodeType::ListItem,
            NodeRef::Table(_) => NodeType::Table,
            NodeRef::TableRow(_) => NodeType::TableRow,
            NodeRef::TableCell(_) => NodeType::TableCell,
            NodeRef::ThematicBreak(_) => NodeType::ThematicBreak,
            NodeRef::Extension(_) => NodeType::Extension,
        }
    }

    /// The node's `data`: an extension node's own, or what a transform attached to any
    /// other node.
    pub fn data(self) -> Option<&'a DataValue> {
        match self {
            NodeRef::Root(n) => n.data.as_ref(),
            NodeRef::Text(n)
            | NodeRef::InlineCode(n)
            | NodeRef::InlineMath(n)
            | NodeRef::Math(n) => n.data.as_ref(),
            NodeRef::Strong(n) | NodeRef::Emphasis(n) | NodeRef::Delete(n) => n.data.as_ref(),
            NodeRef::Break(n) | NodeRef::ThematicBreak(n) => n.data.as_ref(),
            NodeRef::Link(n) => n.data.as_ref(),
            NodeRef::Image(n) => n.data.as_ref(),
            NodeRef::Citation(n) => n.data.as_ref(),
            NodeRef::Heading(n) => n.data.as_ref(),
            NodeRef::Paragraph(n) => n.data.as_ref(),
            NodeRef::Code(n) => n.data.as_ref(),
            NodeRef::Blockquote(n) => n.data.as_ref(),
            NodeRef::List(n) => n.data.as_ref(),
            NodeRef::ListItem(n) => n.data.as_ref(),
            NodeRef::Table(n) => n.data.as_ref(),
            NodeRef::TableRow(n) => n.data.as_ref(),
            NodeRef::TableCell(n) => n.data.as_ref(),
            NodeRef::Extension(n) => Some(&n.data),
        }
    }

    /// The node's children, in order (none for a leaf).
    pub fn children(self) -> Vec<NodeRef<'a>> {
        fn phrasing(nodes: &[Phrasing]) -> Vec<NodeRef<'_>> {
            nodes.iter().map(NodeRef::from).collect()
        }
        fn blocks(nodes: &[Block]) -> Vec<NodeRef<'_>> {
            nodes.iter().map(NodeRef::from).collect()
        }
        match self {
            NodeRef::Root(n) => blocks(&n.children),
            NodeRef::Strong(n) | NodeRef::Emphasis(n) | NodeRef::Delete(n) => phrasing(&n.children),
            NodeRef::Link(n) => phrasing(&n.children),
            NodeRef::Heading(n) => phrasing(&n.children),
            NodeRef::Paragraph(n) => phrasing(&n.children),
            NodeRef::Blockquote(n) => blocks(&n.children),
            NodeRef::List(n) => n.children.iter().map(NodeRef::ListItem).collect(),
            NodeRef::ListItem(n) => blocks(&n.children),
            NodeRef::Table(n) => n.children.iter().map(NodeRef::TableRow).collect(),
            NodeRef::TableRow(n) => n.children.iter().map(NodeRef::TableCell).collect(),
            NodeRef::TableCell(n) => phrasing(&n.children),
            _ => Vec::new(),
        }
    }
}

impl<'a> From<&'a Phrasing> for NodeRef<'a> {
    fn from(node: &'a Phrasing) -> Self {
        match node {
            Phrasing::Text(n) => NodeRef::Text(n),
            Phrasing::Strong(n) => NodeRef::Strong(n),
            Phrasing::Emphasis(n) => NodeRef::Emphasis(n),
            Phrasing::Delete(n) => NodeRef::Delete(n),
            Phrasing::InlineCode(n) => NodeRef::InlineCode(n),
            Phrasing::InlineMath(n) => NodeRef::InlineMath(n),
            Phrasing::Link(n) => NodeRef::Link(n),
            Phrasing::Image(n) => NodeRef::Image(n),
            Phrasing::Citation(n) => NodeRef::Citation(n),
            Phrasing::Break(n) => NodeRef::Break(n),
            Phrasing::Extension(n) => NodeRef::Extension(n),
        }
    }
}

impl<'a> From<&'a Block> for NodeRef<'a> {
    fn from(node: &'a Block) -> Self {
        match node {
            Block::Heading(n) => NodeRef::Heading(n),
            Block::Paragraph(n) => NodeRef::Paragraph(n),
            Block::Code(n) => NodeRef::Code(n),
            Block::Math(n) => NodeRef::Math(n),
            Block::Blockquote(n) => NodeRef::Blockquote(n),
            Block::List(n) => NodeRef::List(n),
            Block::Table(n) => NodeRef::Table(n),
            Block::ThematicBreak(n) => NodeRef::ThematicBreak(n),
            Block::Image(n) => NodeRef::Image(n),
            Block::Extension(n) => NodeRef::Extension(n),
        }
    }
}

/// Upstream `visitMarkdownNodes`: calls `visitor` with every node of `node_type` in
/// `root`, depth first in document order.
pub fn visit_markdown_nodes<'a>(
    root: &'a Root,
    node_type: NodeType,
    visitor: &mut dyn FnMut(NodeRef<'a>),
) {
    fn visit<'a>(node: NodeRef<'a>, node_type: NodeType, visitor: &mut dyn FnMut(NodeRef<'a>)) {
        if node.node_type() == node_type {
            visitor(node);
        }
        for child in node.children() {
            visit(child, node_type, visitor);
        }
    }
    visit(NodeRef::Root(root), node_type, visitor);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_upstream_json() {
        let root = Root {
            children: vec![
                Block::Code(Code {
                    lang: None,
                    value: "x".into(),
                    ..Code::default()
                }),
                Block::Table(Table {
                    align: vec![None, Some(TableAlign::Center)],
                    children: vec![TableRow {
                        children: vec![TableCell {
                            children: vec![Phrasing::InlineCode(Literal {
                                value: "a".into(),
                                ..Literal::default()
                            })],
                            ..TableCell::default()
                        }],
                        ..TableRow::default()
                    }],
                    position: Some(Position::offsets(0, 3)),
                    ..Table::default()
                }),
                Block::ThematicBreak(Void::default()),
            ],
            ..Root::default()
        };
        assert_eq!(
            serde_json::to_string(&root).unwrap(),
            r#"{"type":"root","children":[{"type":"code","lang":null,"value":"x"},{"type":"table","align":[null,"center"],"children":[{"type":"tableRow","children":[{"type":"tableCell","children":[{"type":"inlineCode","value":"a"}]}]}],"position":{"start":{"offset":0},"end":{"offset":3}}},{"type":"thematicBreak"}]}"#
        );
    }

    #[test]
    fn legacy_language_is_the_leading_word() {
        let code = |lang: Option<&str>| Code {
            lang: lang.map(String::from),
            ..Code::default()
        };
        assert_eq!(code(Some("diff-tsx")).legacy_language(), Some("diff"));
        assert_eq!(code(Some("c++")).legacy_language(), Some("c"));
        assert_eq!(code(Some("+x")).legacy_language(), None);
        assert_eq!(code(None).legacy_language(), None);
    }
}
