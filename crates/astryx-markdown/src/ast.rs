//! Markdown's canonical AST, upstream `Markdown/ast.ts`: mdast-aligned nodes. Serialized
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

/// Whether an extension node is phrasing or flow content.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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
}

/// A phrasing parent: `strong`, `emphasis` and `delete`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Parent {
    pub children: Vec<Phrasing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

/// Upstream `MarkdownAstLink`. An unsafe destination is the empty string.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Link {
    pub url: String,
    pub children: Vec<Phrasing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

/// Upstream `MarkdownAstImage`, inline or as a block of its own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Image {
    pub url: String,
    pub alt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

/// Upstream `MarkdownAstCitation`: `[id]` or `【id】` naming a known source.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

/// A node with no content: `break` and `thematicBreak`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Void {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
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
}

/// Upstream `MarkdownAstParagraph` (and the content of a table cell).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Paragraph {
    pub children: Vec<Phrasing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

/// Upstream `MarkdownAstCode`: a fenced code block.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Code {
    /// The info string's first word; `null` in JSON when the fence names none.
    pub lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

impl Code {
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
}

/// Upstream `MarkdownAstTableRow`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(tag = "type", rename = "tableRow")]
pub struct TableRow {
    pub children: Vec<TableCell>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

/// Upstream `MarkdownAstTable`: the first row is the header.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Table {
    pub align: Vec<Option<TableAlign>>,
    pub children: Vec<TableRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
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
}

/// Upstream `markdownAstText`: the plain text of phrasing content. Images give their alt
/// text, citations and breaks nothing, extension nodes their `source`.
pub fn markdown_ast_text(nodes: &[Phrasing]) -> String {
    let mut text = String::new();
    push_text(nodes, &mut text);
    text
}

fn push_text(nodes: &[Phrasing], text: &mut String) {
    for node in nodes {
        match node {
            Phrasing::Text(n) | Phrasing::InlineCode(n) | Phrasing::InlineMath(n) => {
                text.push_str(&n.value)
            }
            Phrasing::Strong(n) | Phrasing::Emphasis(n) | Phrasing::Delete(n) => {
                push_text(&n.children, text)
            }
            Phrasing::Link(n) => push_text(&n.children, text),
            Phrasing::Image(n) => text.push_str(&n.alt),
            Phrasing::Citation(_) | Phrasing::Break(_) => {}
            Phrasing::Extension(n) => text.push_str(n.source.as_deref().unwrap_or("")),
        }
    }
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
                                position: None,
                            })],
                            position: None,
                        }],
                        position: None,
                    }],
                    position: Some(Position::offsets(0, 3)),
                }),
                Block::ThematicBreak(Void::default()),
            ],
            position: None,
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
