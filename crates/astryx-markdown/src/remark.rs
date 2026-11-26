//! The Remark adapter's meaning, upstream `Markdown/remark.ts` (`createMarkdownRemarkTransform`).
//! There is no Remark in Rust, so the port keeps what the adapter guarantees: the
//! canonical tree converts to the documented supported mdast subset (as
//! `serde_json::Value`), a caller's synchronous transformer runs over a fresh copy with an
//! isolated file, and its result converts back only if it round-trips: anything outside
//! the subset, a forged or changed position, a re-levelled source heading, an edited
//! citation or extension node, raw-markup data, an unsafe URL or a ragged table is
//! rejected, and the document stays as it was.
//!
//! The subset, by node: `root`, `paragraph`, `blockquote`, `strong`, `emphasis`, `delete`,
//! `tableRow`, `tableCell` (`children`); `heading` (`depth`, `children`); `text`, `inlineCode`,
//! `inlineMath`, `math` (`value`); `code` (`lang`, `meta` — `null` when absent — and `value`);
//! `link` (`url`, `title`: `null` only, `children`); `image` (`url`, `title`: `null` only, `alt`);
//! `list` (`ordered`, `start`, `spread`, `delimiter`, `children`); `listItem` (`checked`,
//! `spread`: never true, `children`); `table` (`align`, `children`); `thematicBreak`, `break`;
//! `citation` (`sourceId`) and `extension` (`plugin`, `name`, `display`, `source`), which are
//! read-only. Every node may carry `data` (JSON, without hast's `hName`, `hProperties`,
//! `hChildren`) and its source `position`.
//!
//! Provenance: upstream links each copy to its canonical node with an enumerable symbol,
//! which a spread copy keeps. JSON has no symbols, so each copy carries
//! [`REMARK_ORIGIN_KEY`] instead, a token naming this run and the node; a transformer that
//! copies a node keeps it, a node without one is new, and a token from another run, or
//! naming another kind of node, or used twice, is rejected as upstream rejects a forged,
//! retyped or duplicated origin.
//!
//! A failed run is the transform's `Err` (one `transform-failed` diagnostic), its text the
//! adapter's own reason; anything the transformer authored (its error, its file messages)
//! never leaves the file.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Map, Number, Value, json};

use crate::ast::{
    Block, Blockquote, Citation, Code, ExtensionDisplay, Heading, Image, Link, List, ListItem,
    Literal, NodeRef, Paragraph, Parent, Phrasing, Point, Position, Root, Table, TableAlign,
    TableCell, TableRow, Void,
};
use crate::plugins::{Transform, TransformContext};
use crate::url::{is_safe_markdown_parser_url, normalized_markdown_destination};

/// The key each converted node carries its provenance token under.
pub const REMARK_ORIGIN_KEY: &str = "astryx:origin";

/// hast's escape channels: raw-markup sinks, not Markdown data.
const RAW_MARKUP_DATA_KEYS: [&str; 3] = ["hName", "hProperties", "hChildren"];

const PHRASING_TYPES: [&str; 10] = [
    "text",
    "inlineCode",
    "inlineMath",
    "break",
    "strong",
    "emphasis",
    "delete",
    "link",
    "image",
    "citation",
];
const BLOCK_TYPES: [&str; 9] = [
    "heading",
    "paragraph",
    "code",
    "math",
    "blockquote",
    "list",
    "table",
    "thematicBreak",
    "image",
];
const PHRASING_PARENTS: [&str; 7] = [
    "heading",
    "paragraph",
    "strong",
    "emphasis",
    "delete",
    "link",
    "tableCell",
];
const BLOCK_PARENTS: [&str; 3] = ["root", "blockquote", "listItem"];

/// Upstream `FIELDS_BY_TYPE`: the supported subset.
fn fields_of(node_type: &str) -> Option<&'static [&'static str]> {
    Some(match node_type {
        "root" | "paragraph" | "blockquote" | "strong" | "emphasis" | "delete" | "tableRow"
        | "tableCell" => &["children"],
        "heading" => &["depth", "children"],
        "text" | "inlineCode" | "inlineMath" | "math" => &["value"],
        "code" => &["lang", "meta", "value"],
        "link" => &["url", "title", "children"],
        "image" => &["url", "title", "alt"],
        "list" => &["ordered", "start", "spread", "delimiter", "children"],
        "listItem" => &["checked", "spread", "children"],
        "table" => &["align", "children"],
        "thematicBreak" | "break" => &[],
        "citation" => &["sourceId"],
        "extension" => &["plugin", "name", "display", "source"],
        _ => return None,
    })
}

/// Upstream `NAMED_UNSUPPORTED_TYPES`.
const NAMED_UNSUPPORTED_TYPES: [&str; 17] = [
    "html",
    "yaml",
    "toml",
    "definition",
    "footnote",
    "footnoteDefinition",
    "footnoteReference",
    "imageReference",
    "linkReference",
    "containerDirective",
    "leafDirective",
    "textDirective",
    "mdxjsEsm",
    "mdxFlowExpression",
    "mdxTextExpression",
    "mdxJsxFlowElement",
    "mdxJsxTextElement",
];

/// The reason reported when the transformer declares the document unsupported.
const FAILED_BY_PLUGIN: &str = "the plugin reported the document as unsupported";

/// Upstream `knownType`: a type echoed only when it is the adapter's own vocabulary.
fn known_type(node_type: &str) -> &str {
    if fields_of(node_type).is_some() || NAMED_UNSUPPORTED_TYPES.contains(&node_type) {
        node_type
    } else {
        "unknown"
    }
}

/// Upstream `knownField`.
fn known_field<'a>(node_type: &str, field: &'a str) -> &'a str {
    if matches!(field, "position" | "data" | "type")
        || RAW_MARKUP_DATA_KEYS.contains(&field)
        || fields_of(node_type).is_some_and(|fields| fields.contains(&field))
    {
        field
    } else {
        "unknown"
    }
}

/// Upstream `MarkdownRemarkMessage`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemarkMessage {
    pub reason: String,
    pub fatal: bool,
    pub rule_id: Option<String>,
}

/// Upstream `MarkdownRemarkFile`: the isolated per-run file. It has the source, JSON data
/// and messages, and nothing else: no path, processor, history or I/O.
#[derive(Debug)]
pub struct RemarkFile {
    value: String,
    /// The run's own data; it never outlives the run.
    pub data: Map<String, Value>,
    messages: Vec<RemarkMessage>,
    failed: bool,
}

impl RemarkFile {
    fn new(source: &str) -> Self {
        RemarkFile {
            value: source.to_string(),
            data: Map::new(),
            messages: Vec::new(),
            failed: false,
        }
    }

    /// The source the document was parsed from.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// The messages recorded so far.
    pub fn messages(&self) -> &[RemarkMessage] {
        &self.messages
    }

    /// Records a message. A message the transformer marks fatal by hand fails nothing.
    pub fn message(
        &mut self,
        reason: impl Into<String>,
        rule_id: Option<&str>,
    ) -> &mut RemarkMessage {
        self.messages.push(RemarkMessage {
            reason: reason.into(),
            fatal: false,
            rule_id: rule_id.map(str::to_string),
        });
        self.messages.last_mut().expect("a message was pushed")
    }

    /// Upstream `file.fail`: records a fatal message and declares the document unsupported.
    /// The verdict is the run's: returning a tree afterwards does not undo it. The returned
    /// error is for the transformer to return.
    pub fn fail(&mut self, reason: impl Into<String>, rule_id: Option<&str>) -> String {
        self.messages.push(RemarkMessage {
            reason: reason.into(),
            fatal: true,
            rule_id: rule_id.map(str::to_string),
        });
        self.failed = true;
        FAILED_BY_PLUGIN.to_string()
    }
}

/// A transformer: the mdast tree in, the mdast tree out (the same one, changed, or a
/// new one). An `Err` fails the run; its text is never reported.
pub type RemarkTransformer = dyn Fn(Value, &mut RemarkFile) -> Result<Value, String>;

static RUNS: AtomicU64 = AtomicU64::new(1);

/// A canonical tree converted to mdast, with what converting it back needs.
pub struct RemarkTree<'a> {
    /// The mdast tree, each node carrying its [`REMARK_ORIGIN_KEY`] token.
    pub tree: Value,
    run: u64,
    origins: Vec<NodeRef<'a>>,
}

fn point_value(point: &Point) -> Value {
    let mut map = Map::new();
    for (key, value) in [
        ("line", point.line),
        ("column", point.column),
        ("offset", point.offset),
    ] {
        if let Some(value) = value {
            map.insert(key.to_string(), Value::from(value));
        }
    }
    Value::Object(map)
}

fn position_value(position: &Position) -> Value {
    json!({ "start": point_value(&position.start), "end": point_value(&position.end) })
}

fn align_value(align: &Option<TableAlign>) -> Value {
    match align {
        None => Value::Null,
        Some(TableAlign::Left) => "left".into(),
        Some(TableAlign::Center) => "center".into(),
        Some(TableAlign::Right) => "right".into(),
    }
}

/// Upstream `toRemarkNode` (canonical tree → fresh mutable mdast tree): converts `root`.
pub fn markdown_ast_to_remark(root: &Root) -> RemarkTree<'_> {
    let mut tree = RemarkTree {
        tree: Value::Null,
        run: RUNS.fetch_add(1, Ordering::Relaxed),
        origins: Vec::new(),
    };
    tree.tree = tree.convert(NodeRef::Root(root));
    tree
}

impl<'a> RemarkTree<'a> {
    fn convert(&mut self, node: NodeRef<'a>) -> Value {
        let index = self.origins.len();
        self.origins.push(node);
        let mut copy = Map::new();
        copy.insert("type".into(), node.node_type().as_str().into());
        let children = |tree: &mut Self, node: NodeRef<'a>| {
            Value::Array(
                node.children()
                    .into_iter()
                    .map(|child| tree.convert(child))
                    .collect(),
            )
        };
        match node {
            NodeRef::ListItem(item) => {
                if let Some(checked) = item.checked {
                    copy.insert("checked".into(), checked.into());
                }
                copy.insert("children".into(), children(self, node));
            }
            NodeRef::Heading(heading) => {
                copy.insert("depth".into(), heading.depth.into());
                copy.insert("children".into(), children(self, node));
            }
            NodeRef::Text(n)
            | NodeRef::InlineCode(n)
            | NodeRef::InlineMath(n)
            | NodeRef::Math(n) => {
                copy.insert("value".into(), n.value.clone().into());
            }
            NodeRef::Code(code) => {
                copy.insert("lang".into(), code.lang.clone().into());
                copy.insert("meta".into(), code.meta.clone().into());
                copy.insert("value".into(), code.value.clone().into());
            }
            NodeRef::Link(link) => {
                copy.insert("url".into(), link.url.clone().into());
                copy.insert("title".into(), Value::Null);
                copy.insert("children".into(), children(self, node));
            }
            NodeRef::Image(image) => {
                copy.insert("url".into(), image.url.clone().into());
                copy.insert("title".into(), Value::Null);
                copy.insert("alt".into(), image.alt.clone().into());
            }
            NodeRef::List(list) => {
                copy.insert("ordered".into(), list.ordered.into());
                if let Some(start) = list.start {
                    copy.insert("start".into(), start.into());
                }
                if let Some(spread) = list.spread {
                    copy.insert("spread".into(), spread.into());
                }
                if let Some(delimiter) = list.delimiter {
                    copy.insert("delimiter".into(), delimiter.to_string().into());
                }
                copy.insert("children".into(), children(self, node));
            }
            NodeRef::Table(table) => {
                copy.insert(
                    "align".into(),
                    table.align.iter().map(align_value).collect(),
                );
                copy.insert("children".into(), children(self, node));
            }
            NodeRef::Citation(citation) => {
                copy.insert("sourceId".into(), citation.source_id.clone().into());
            }
            NodeRef::Extension(extension) => {
                copy.insert("plugin".into(), extension.plugin.clone().into());
                copy.insert("name".into(), extension.name.clone().into());
                copy.insert(
                    "display".into(),
                    match extension.display {
                        ExtensionDisplay::Inline => "inline",
                        ExtensionDisplay::Block => "block",
                    }
                    .into(),
                );
                if let Some(source) = &extension.source {
                    copy.insert("source".into(), source.clone().into());
                }
            }
            NodeRef::Break(_) | NodeRef::ThematicBreak(_) => {}
            _ => {
                copy.insert("children".into(), children(self, node));
            }
        }
        if let Some(data) = node.data() {
            copy.insert("data".into(), data.clone());
        }
        if let Some(position) = crate::plugins::node_position(node) {
            copy.insert("position".into(), position_value(position));
        }
        copy.insert(
            REMARK_ORIGIN_KEY.into(),
            format!("{}:{index}", self.run).into(),
        );
        Value::Object(copy)
    }

    /// Upstream `fromRemarkNode` (mutated mdast tree → validated canonical tree): converts
    /// `tree` back, or says why it cannot. `display` inline requires one paragraph.
    pub fn into_markdown_ast(
        &self,
        tree: &Value,
        display: ExtensionDisplay,
    ) -> Result<Root, String> {
        let mut back = Back {
            tree: self,
            used: HashSet::new(),
        };
        let Converted::Root(root) = back.node(tree, None, false)? else {
            return Err("a transformer must return the document root".into());
        };
        if display == ExtensionDisplay::Inline
            && !matches!(root.children.as_slice(), [Block::Paragraph(_)])
        {
            return Err("an inline document must stay one paragraph of phrasing content".into());
        }
        Ok(root)
    }
}

enum Converted {
    Root(Root),
    Block(Block),
    Phrasing(Phrasing),
    ListItem(ListItem),
    TableRow(TableRow),
    TableCell(TableCell),
}

struct Back<'t, 'a> {
    tree: &'t RemarkTree<'a>,
    used: HashSet<usize>,
}

type Object = Map<String, Value>;

fn reject<T>(reason: impl Into<String>) -> Result<T, String> {
    Err(reason.into())
}

impl<'a> Back<'_, 'a> {
    /// Upstream `resolveOrigin`.
    fn origin(&mut self, node: &Object, node_type: &str) -> Result<Option<NodeRef<'a>>, String> {
        let Some(token) = node.get(REMARK_ORIGIN_KEY) else {
            return Ok(None);
        };
        let forged = || {
            format!(
                "a \"{}\" node carries forged provenance",
                known_type(node_type)
            )
        };
        let index = token
            .as_str()
            .and_then(|token| token.split_once(':'))
            .filter(|(run, _)| run.parse::<u64>().ok() == Some(self.tree.run))
            .and_then(|(_, index)| index.parse::<usize>().ok())
            .filter(|&index| index < self.tree.origins.len())
            .ok_or_else(forged)?;
        let origin = self.tree.origins[index];
        let origin_type = origin.node_type().as_str();
        if origin_type != node_type {
            return reject(format!(
                "a source \"{}\" node cannot be retyped",
                known_type(origin_type)
            ));
        }
        if !self.used.insert(index) {
            return reject(format!(
                "a source \"{}\" node cannot be duplicated",
                known_type(node_type)
            ));
        }
        Ok(Some(origin))
    }

    fn children(
        &mut self,
        node: &Object,
        node_type: &str,
        inside_link: bool,
    ) -> Result<Vec<Converted>, String> {
        let Some(Value::Array(children)) = node.get("children") else {
            return reject(format!(
                "a \"{}\" node requires a children array",
                known_type(node_type)
            ));
        };
        let inside_link = inside_link || node_type == "link";
        children
            .iter()
            .map(|child| self.node(child, Some(node_type), inside_link))
            .collect()
    }

    fn blocks(
        &mut self,
        node: &Object,
        node_type: &str,
        inside_link: bool,
    ) -> Result<Vec<Block>, String> {
        Ok(self
            .children(node, node_type, inside_link)?
            .into_iter()
            .filter_map(|child| match child {
                Converted::Block(block) => Some(block),
                _ => None,
            })
            .collect())
    }

    fn phrasing(
        &mut self,
        node: &Object,
        node_type: &str,
        inside_link: bool,
    ) -> Result<Vec<Phrasing>, String> {
        Ok(self
            .children(node, node_type, inside_link)?
            .into_iter()
            .filter_map(|child| match child {
                Converted::Phrasing(phrasing) => Some(phrasing),
                _ => None,
            })
            .collect())
    }

    fn node(
        &mut self,
        value: &Value,
        parent: Option<&str>,
        inside_link: bool,
    ) -> Result<Converted, String> {
        let Value::Object(node) = value else {
            return reject("every transformed node must be a plain object");
        };
        let Some(node_type) = node.get("type").and_then(Value::as_str) else {
            return reject("every transformed node requires a string type");
        };
        if node_type == "html" {
            return reject("raw HTML has no Astryx representation");
        }
        let Some(allowed) = fields_of(node_type) else {
            return reject(format!(
                "the node type \"{}\" is outside the supported MDAST subset",
                known_type(node_type)
            ));
        };
        // Upstream `assertKnownFields`.
        for key in node.keys() {
            if !matches!(
                key.as_str(),
                "type" | "position" | "data" | REMARK_ORIGIN_KEY
            ) && !allowed.contains(&key.as_str())
            {
                return reject(format!(
                    "a \"{}\" node cannot carry the unsupported field \"{}\"",
                    known_type(node_type),
                    known_field(node_type, key)
                ));
            }
        }
        // Upstream `assertPlacement`.
        let display = node.get("display").and_then(Value::as_str);
        if let Some(parent) = parent {
            if node_type == "root" {
                return reject("a document can only contain one root");
            }
            let phrasing = PHRASING_TYPES.contains(&node_type)
                || (node_type == "extension" && display == Some("inline"));
            let block = BLOCK_TYPES.contains(&node_type)
                || (node_type == "extension" && display == Some("block"));
            if (PHRASING_PARENTS.contains(&parent) && !phrasing)
                || (BLOCK_PARENTS.contains(&parent) && !block)
                || (parent == "list" && node_type != "listItem")
                || (parent == "table" && node_type != "tableRow")
                || (parent == "tableRow" && node_type != "tableCell")
            {
                return reject(format!(
                    "a \"{}\" node cannot be a child of a \"{}\" node",
                    known_type(node_type),
                    known_type(parent)
                ));
            }
            if inside_link && node_type == "link" {
                return reject("a link cannot be nested inside another link");
            }
        }
        let origin = self.origin(node, node_type)?;
        let origin_position = origin.and_then(crate::plugins::node_position).copied();
        match origin {
            None if node.contains_key("position") => {
                return reject(format!(
                    "a new \"{}\" node cannot carry a source position",
                    known_type(node_type)
                ));
            }
            Some(_)
                if node.get("position")
                    != origin_position.as_ref().map(position_value).as_ref() =>
            {
                return reject(format!(
                    "a source \"{}\" node's position must stay exactly as Core authored it",
                    known_type(node_type)
                ));
            }
            _ => {}
        }
        let in_phrasing = parent.is_some_and(|parent| PHRASING_PARENTS.contains(&parent));
        let string = |field: &str| -> Result<String, String> {
            match node.get(field) {
                Some(Value::String(value)) => Ok(value.clone()),
                _ => reject(format!(
                    "a \"{}\" node requires a string \"{}\"",
                    known_type(node_type),
                    known_field(node_type, field)
                )),
            }
        };
        let absent_text = |field: &str| -> Result<(), String> {
            match node.get(field) {
                None | Some(Value::Null) => Ok(()),
                Some(Value::String(text)) if text.is_empty() => Ok(()),
                _ => reject(format!(
                    "a \"{}\" node's \"{}\" has no Astryx representation",
                    known_type(node_type),
                    known_field(node_type, field)
                )),
            }
        };
        // Citations and extension nodes are Astryx's own: they come back exactly as they
        // went out, or not at all.
        if node_type == "citation" || node_type == "extension" {
            let unchanged = origin.is_some_and(|origin| {
                node.get("data") == origin.data()
                    && match origin {
                        NodeRef::Citation(citation) => {
                            node.get("sourceId").and_then(Value::as_str)
                                == Some(&citation.source_id)
                        }
                        NodeRef::Extension(extension) => {
                            node.get("plugin").and_then(Value::as_str) == Some(&extension.plugin)
                                && node.get("name").and_then(Value::as_str) == Some(&extension.name)
                                && display
                                    == Some(match extension.display {
                                        ExtensionDisplay::Inline => "inline",
                                        ExtensionDisplay::Block => "block",
                                    })
                                && node.get("source").and_then(Value::as_str)
                                    == extension.source.as_deref()
                                && node.get("source").is_none_or(Value::is_string)
                        }
                        _ => false,
                    }
            });
            if !unchanged {
                return reject(format!(
                    "an Astryx \"{}\" node cannot be authored or changed here",
                    known_type(node_type)
                ));
            }
            return Ok(match origin {
                Some(NodeRef::Citation(citation)) => {
                    Converted::Phrasing(Phrasing::Citation(citation.clone()))
                }
                Some(NodeRef::Extension(extension))
                    if extension.display == ExtensionDisplay::Inline =>
                {
                    Converted::Phrasing(Phrasing::Extension(extension.clone()))
                }
                Some(NodeRef::Extension(extension)) => {
                    Converted::Block(Block::Extension(extension.clone()))
                }
                _ => unreachable!("an unchanged node has an origin"),
            });
        }

        let mut converted = match node_type {
            "root" => Converted::Root(Root {
                children: self.blocks(node, node_type, inside_link)?,
                ..Root::default()
            }),
            "paragraph" => Converted::Block(Block::Paragraph(Paragraph {
                children: self.phrasing(node, node_type, inside_link)?,
                ..Paragraph::default()
            })),
            "blockquote" => Converted::Block(Block::Blockquote(Blockquote {
                children: self.blocks(node, node_type, inside_link)?,
                ..Blockquote::default()
            })),
            "strong" | "emphasis" | "delete" => {
                let parent = Parent {
                    children: self.phrasing(node, node_type, inside_link)?,
                    ..Parent::default()
                };
                Converted::Phrasing(match node_type {
                    "strong" => Phrasing::Strong(parent),
                    "emphasis" => Phrasing::Emphasis(parent),
                    _ => Phrasing::Delete(parent),
                })
            }
            "tableRow" => Converted::TableRow(TableRow {
                children: self
                    .children(node, node_type, inside_link)?
                    .into_iter()
                    .filter_map(|child| match child {
                        Converted::TableCell(cell) => Some(cell),
                        _ => None,
                    })
                    .collect(),
                ..TableRow::default()
            }),
            "tableCell" => Converted::TableCell(TableCell {
                children: self.phrasing(node, node_type, inside_link)?,
                ..TableCell::default()
            }),
            "heading" => {
                let depth = node
                    .get("depth")
                    .and_then(Value::as_f64)
                    .filter(|depth| depth.fract() == 0.0 && (1.0..=6.0).contains(depth))
                    .map(|depth| depth as u8);
                let Some(depth) = depth else {
                    return reject("a heading requires a depth between 1 and 6");
                };
                if let Some(NodeRef::Heading(origin)) = origin
                    && origin.depth != depth
                {
                    return reject("a source heading's depth cannot change");
                }
                Converted::Block(Block::Heading(Heading {
                    depth,
                    children: self.phrasing(node, node_type, inside_link)?,
                    ..Heading::default()
                }))
            }
            "listItem" => {
                let checked = match node.get("checked") {
                    None | Some(Value::Null) => None,
                    Some(Value::Bool(checked)) => Some(*checked),
                    Some(_) => return reject("a list item requires a boolean \"checked\" value"),
                };
                if node.get("spread") == Some(&Value::Bool(true)) {
                    return reject("a loose list item has no Astryx representation");
                }
                Converted::ListItem(ListItem {
                    checked,
                    children: self.blocks(node, node_type, inside_link)?,
                    ..ListItem::default()
                })
            }
            "text" | "inlineCode" | "inlineMath" | "math" => {
                let literal = Literal {
                    value: string("value")?,
                    ..Literal::default()
                };
                match node_type {
                    "text" => Converted::Phrasing(Phrasing::Text(literal)),
                    "inlineCode" => Converted::Phrasing(Phrasing::InlineCode(literal)),
                    "inlineMath" => Converted::Phrasing(Phrasing::InlineMath(literal)),
                    _ => Converted::Block(Block::Math(literal)),
                }
            }
            "code" => {
                let lang = match node.get("lang") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(lang)) => Some(lang.clone()),
                    Some(_) => return reject("a code node requires a string or null \"lang\""),
                };
                let meta = match node.get("meta") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(meta)) => (!meta.is_empty()).then(|| meta.clone()),
                    Some(_) => return reject("a code node requires a string or null \"meta\""),
                };
                Converted::Block(Block::Code(Code {
                    lang,
                    meta,
                    value: string("value")?,
                    ..Code::default()
                }))
            }
            "link" => {
                let url = string("url")?;
                if !is_safe_markdown_parser_url(&url) {
                    return reject("a link destination was rejected by the navigation owner");
                }
                absent_text("title")?;
                Converted::Phrasing(Phrasing::Link(Link {
                    url: normalized_markdown_destination(&url),
                    children: self.phrasing(node, node_type, inside_link)?,
                    ..Link::default()
                }))
            }
            "image" => {
                let url = string("url")?;
                if !is_safe_markdown_parser_url(&url) {
                    return reject("an image source was rejected by the resource owner");
                }
                absent_text("title")?;
                let Some(Value::String(alt)) = node.get("alt") else {
                    return reject("an image requires its text alternative");
                };
                let image = Image {
                    url: normalized_markdown_destination(&url),
                    alt: alt.clone(),
                    ..Image::default()
                };
                if in_phrasing {
                    Converted::Phrasing(Phrasing::Image(image))
                } else {
                    Converted::Block(Block::Image(image))
                }
            }
            "list" => {
                let Some(Value::Bool(ordered)) = node.get("ordered") else {
                    return reject("a list requires a boolean \"ordered\" value");
                };
                let start = match node.get("start") {
                    None | Some(Value::Null) => None,
                    Some(Value::Number(start)) => match integer(start) {
                        Some(start) => Some(start),
                        None => return reject("a list requires a finite \"start\" value"),
                    },
                    Some(_) => return reject("a list requires a finite \"start\" value"),
                };
                let spread = node.get("spread").and_then(Value::as_bool);
                let delimiter = match node.get("delimiter") {
                    None => None,
                    Some(Value::String(d)) if d == "." || d == ")" => d.chars().next(),
                    Some(_) => return reject("a list requires a \".\" or \")\" delimiter"),
                };
                Converted::Block(Block::List(List {
                    ordered: *ordered,
                    start,
                    spread,
                    delimiter,
                    children: self
                        .children(node, node_type, inside_link)?
                        .into_iter()
                        .filter_map(|child| match child {
                            Converted::ListItem(item) => Some(item),
                            _ => None,
                        })
                        .collect(),
                    ..List::default()
                }))
            }
            "table" => {
                let align = match node.get("align") {
                    Some(Value::Array(align)) => align
                        .iter()
                        .map(|entry| match entry {
                            Value::Null => Some(None),
                            Value::String(a) if a == "left" => Some(Some(TableAlign::Left)),
                            Value::String(a) if a == "center" => Some(Some(TableAlign::Center)),
                            Value::String(a) if a == "right" => Some(Some(TableAlign::Right)),
                            _ => None,
                        })
                        .collect::<Option<Vec<_>>>(),
                    _ => None,
                };
                let Some(align) = align else {
                    return reject("a table requires normalized alignment values");
                };
                Converted::Block(Block::Table(Table {
                    align,
                    children: self
                        .children(node, node_type, inside_link)?
                        .into_iter()
                        .filter_map(|child| match child {
                            Converted::TableRow(row) => Some(row),
                            _ => None,
                        })
                        .collect(),
                    ..Table::default()
                }))
            }
            "thematicBreak" => Converted::Block(Block::ThematicBreak(Void::default())),
            "break" => Converted::Phrasing(Phrasing::Break(Void::default())),
            _ => {
                return reject(format!(
                    "the node type \"{}\" is outside the supported MDAST subset",
                    known_type(node_type)
                ));
            }
        };

        // Upstream `convertData`.
        let data = node.get("data").cloned();
        if let Some(Value::Object(object)) = &data
            && let Some(key) = RAW_MARKUP_DATA_KEYS
                .iter()
                .find(|key| object.contains_key(**key))
        {
            return reject(format!(
                "a \"{}\" node cannot carry the raw-markup data channel \"{key}\"",
                known_type(node_type)
            ));
        }
        apply_fields(&mut converted, data, origin_position, origin);
        // A table that changed may not be ragged (an untouched one keeps its shape).
        if let Converted::Block(Block::Table(table)) = &converted {
            let changed = !matches!(origin, Some(NodeRef::Table(origin)) if table == origin);
            let width = table.children.first().map_or(0, |row| row.children.len());
            if changed && table.children.iter().any(|row| row.children.len() != width) {
                return reject("a transformed table cannot have ragged rows");
            }
        }
        Ok(converted)
    }
}

/// A JSON number as a list start: an integer in range.
fn integer(number: &Number) -> Option<i64> {
    number.as_i64().or_else(|| {
        number
            .as_f64()
            .filter(|n| n.is_finite() && n.fract() == 0.0 && n.abs() < 9.0e15)
            .map(|n| n as i64)
    })
}

/// Sets the converted node's data and its source position, and carries Core's own
/// markers (heading identity, a fence proposal) over from its origin, as upstream's
/// `buildCanonicalNode` carries its enumerable symbols.
fn apply_fields(
    converted: &mut Converted,
    data: Option<Value>,
    position: Option<Position>,
    origin: Option<NodeRef<'_>>,
) {
    macro_rules! set {
        ($node:expr) => {{
            $node.data = data;
            $node.position = position;
        }};
    }
    match converted {
        Converted::Root(n) => set!(n),
        Converted::ListItem(n) => set!(n),
        Converted::TableRow(n) => set!(n),
        Converted::TableCell(n) => set!(n),
        Converted::Block(block) => match block {
            Block::Heading(n) => {
                set!(n);
                if let Some(NodeRef::Heading(origin)) = origin {
                    n.identity = origin.identity;
                }
            }
            Block::Paragraph(n) => set!(n),
            Block::Code(n) => {
                set!(n);
                if let Some(NodeRef::Code(origin)) = origin {
                    n.proposal = origin.proposal.clone();
                }
            }
            Block::Math(n) => set!(n),
            Block::Blockquote(n) => set!(n),
            Block::List(n) => set!(n),
            Block::Table(n) => set!(n),
            Block::ThematicBreak(n) => set!(n),
            Block::Image(n) => set!(n),
            Block::Extension(_) => {}
        },
        Converted::Phrasing(phrasing) => match phrasing {
            Phrasing::Text(n) | Phrasing::InlineCode(n) | Phrasing::InlineMath(n) => set!(n),
            Phrasing::Strong(n) | Phrasing::Emphasis(n) | Phrasing::Delete(n) => set!(n),
            Phrasing::Link(n) => set!(n),
            Phrasing::Image(n) => set!(n),
            Phrasing::Break(n) => set!(n),
            Phrasing::Citation(Citation { .. }) | Phrasing::Extension(_) => {}
        },
    }
}

/// Upstream `createMarkdownRemarkTransform`: adapts one synchronous transformer over the
/// supported mdast subset to a Markdown transform. It is plugin-authored structure, so
/// Core validates what it returns as it validates any plugin's.
pub fn create_markdown_remark_transform(
    transformer: impl Fn(Value, &mut RemarkFile) -> Result<Value, String> + 'static,
) -> Transform {
    let transformer: Rc<RemarkTransformer> = Rc::new(transformer);
    Transform::new(move |document: &Root, context: &TransformContext<'_>| {
        run_remark_transformer(document, context, &*transformer)
            .map_err(|reason| format!("Remark adapter: {reason}"))
    })
}

/// One run of the adapter: the document converted, the transformer run over it with a
/// fresh file, and the result converted back (`None` when it round-trips to the same
/// document), or the adapter's reason for refusing it.
pub fn run_remark_transformer(
    document: &Root,
    context: &TransformContext<'_>,
    transformer: &RemarkTransformer,
) -> Result<Option<Root>, String> {
    let converted = markdown_ast_to_remark(document);
    let mut file = RemarkFile::new(context.source());
    let returned = transformer(converted.tree.clone(), &mut file);
    // The verdict is sticky: a transformer that called `fail` has declared the document
    // unsupported whatever it returns.
    if file.failed {
        return reject(FAILED_BY_PLUGIN);
    }
    let output = returned.map_err(|_| "the plugin threw an error".to_string())?;
    let root = converted.into_markdown_ast(&output, context.display())?;
    Ok((root != *document).then_some(root))
}
