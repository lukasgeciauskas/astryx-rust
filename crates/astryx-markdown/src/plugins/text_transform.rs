//! Text-matching transforms, upstream `Markdown/plugins/textTransform.ts`
//! (`createMarkdownTextTransform`): a regex over eligible built-in prose (text in headings,
//! paragraphs, emphasis, strong, strikethrough and table cells), each match replaced by
//! phrasing a callback authors. Links, images, code, math, citations, breaks and extension
//! nodes are protected contexts.
//!
//! Upstream takes a global JavaScript `RegExp`; the port takes a [`regex::Regex`], which
//! always scans globally. A pattern with an `i` flag group (`(?i)`, `(?i:…)`) counts as
//! case-insensitive, which, as upstream's `ignoreCase`, turns off the required-substring
//! fast paths. An empty match advances by one character (upstream: one code point under
//! `u`, one unit otherwise).

use std::rc::Rc;
use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::{
    HelperOwnership, PluginError, Transform, TransformContext, adopt_markdown_helper_nodes,
    normalize_phrasing,
};
use crate::ast::{Block, Literal, Phrasing, Root};

/// Upstream `MarkdownTextTransformContext['parentType']`: the node a matched text sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextParentType {
    Heading,
    Paragraph,
    Strong,
    Emphasis,
    Delete,
    TableCell,
}

/// Upstream `MarkdownTextTransformContext`.
#[derive(Clone, Copy, Debug)]
pub struct TextTransformContext {
    pub parent_type: TextParentType,
}

/// A replacement callback: the nodes a match becomes.
pub type TextReplace = dyn Fn(&Captures<'_>, &TextTransformContext) -> Vec<Phrasing>;

/// Upstream `getEndIndex`: where a claim ends (at least the match's end), or `None` to
/// decline the match.
pub type TextEndIndex = dyn Fn(&str, &Captures<'_>) -> Option<usize>;

/// Upstream `MarkdownTextTransformOptions`.
#[derive(Clone)]
pub struct TextTransformOptions {
    /// Matched against eligible prose only.
    pub pattern: Regex,
    /// Literal substrings such that source holding an eligible match holds at least one:
    /// without any, Core skips the transform (and each block and text without one).
    pub required_substrings: Option<Vec<String>>,
    /// Optionally decline or extend a claim.
    pub get_end_index: Option<Rc<TextEndIndex>>,
    pub replace: Rc<TextReplace>,
}

impl TextTransformOptions {
    pub fn new(
        pattern: Regex,
        replace: impl Fn(&Captures<'_>, &TextTransformContext) -> Vec<Phrasing> + 'static,
    ) -> Self {
        TextTransformOptions {
            pattern,
            required_substrings: None,
            get_end_index: None,
            replace: Rc::new(replace),
        }
    }
}

/// Whether the pattern sets the `i` flag anywhere (upstream `pattern.ignoreCase`).
fn is_case_insensitive(pattern: &Regex) -> bool {
    static FLAG: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\(\?[a-zA-Z]*i[a-zA-Z]*[-:)]").expect("FLAG is valid"));
    FLAG.is_match(pattern.as_str())
}

/// The index past the character at `index` (upstream `advanceStringIndex`).
fn advance(value: &str, index: usize) -> usize {
    crate::js::char_at(value, index).map_or(index + 1, |c| index + c.len_utf8())
}

fn text(value: &str) -> Phrasing {
    Phrasing::Text(Literal {
        value: value.to_string(),
        ..Literal::default()
    })
}

struct Run<'a> {
    options: &'a TextTransformOptions,
    /// The required substrings, unless the pattern is case-insensitive.
    required: Option<&'a [String]>,
    ownership: HelperOwnership<'a>,
}

impl Run<'_> {
    /// Upstream `replaceTextNode`: `value` with its claims replaced, or `None` without any.
    fn replace_text(
        &self,
        value: &str,
        parent_type: TextParentType,
    ) -> Result<Option<Vec<Phrasing>>, String> {
        if self
            .required
            .is_some_and(|required| !required.iter().any(|hint| value.contains(hint.as_str())))
        {
            return Ok(None);
        }
        let mut output = Vec::new();
        let mut cursor = 0;
        let mut changed = false;
        let mut last_index = 0;
        while last_index <= value.len() {
            let Some(captures) = self.options.pattern.captures_at(value, last_index) else {
                break;
            };
            let matched = captures.get(0).expect("a match has a group 0");
            let (start, default_end) = (matched.start(), matched.end());
            last_index = default_end;
            let resolved = match &self.options.get_end_index {
                Some(get_end_index) => get_end_index(value, &captures),
                None => Some(default_end),
            };
            let Some(resolved) = resolved else {
                if matched.is_empty() {
                    last_index = advance(value, last_index);
                }
                continue;
            };
            if resolved < default_end || resolved > value.len() || !value.is_char_boundary(resolved)
            {
                return Err("Markdown text transform returned an invalid end index".to_string());
            }
            if start < cursor {
                continue;
            }
            if start > cursor {
                output.push(text(&value[cursor..start]));
            }
            let mut replacement =
                (self.options.replace)(&captures, &TextTransformContext { parent_type });
            // The callback's nodes are the only untrusted structure this helper adds: they
            // are checked under the rules Core applies to new nodes, and their URLs stored
            // normalized.
            adopt_markdown_helper_nodes(&replacement, &self.ownership, false)?;
            normalize_phrasing(&mut replacement);
            output.extend(replacement);
            cursor = resolved;
            last_index = if resolved == start {
                advance(value, last_index)
            } else {
                last_index.max(resolved)
            };
            changed = true;
        }
        if !changed {
            return Ok(None);
        }
        if cursor < value.len() {
            output.push(text(&value[cursor..]));
        }
        Ok(Some(output))
    }

    /// Upstream `transformPhrasing`. Returns whether anything changed.
    fn phrasing(
        &self,
        children: &mut Vec<Phrasing>,
        parent_type: TextParentType,
    ) -> Result<bool, String> {
        let mut changed = false;
        for node in std::mem::take(children) {
            match node {
                Phrasing::Text(literal) => match self.replace_text(&literal.value, parent_type)? {
                    Some(replacement) => {
                        children.extend(replacement);
                        changed = true;
                    }
                    None => children.push(Phrasing::Text(literal)),
                },
                Phrasing::Strong(mut parent) => {
                    changed |= self.phrasing(&mut parent.children, TextParentType::Strong)?;
                    children.push(Phrasing::Strong(parent));
                }
                Phrasing::Emphasis(mut parent) => {
                    changed |= self.phrasing(&mut parent.children, TextParentType::Emphasis)?;
                    children.push(Phrasing::Emphasis(parent));
                }
                Phrasing::Delete(mut parent) => {
                    changed |= self.phrasing(&mut parent.children, TextParentType::Delete)?;
                    children.push(Phrasing::Delete(parent));
                }
                // Protected contexts keep their own semantics.
                other => children.push(other),
            }
        }
        Ok(changed)
    }

    /// Upstream `transformBlocks`.
    fn blocks(
        &self,
        blocks: &mut [Block],
        required: &mut Option<RequiredSource<'_>>,
    ) -> Result<bool, String> {
        let mut changed = false;
        for block in blocks {
            if let Some(required) = required
                && !required.block_may_contain(block)
            {
                continue;
            }
            changed |= match block {
                Block::Heading(node) => {
                    self.phrasing(&mut node.children, TextParentType::Heading)?
                }
                Block::Paragraph(node) => {
                    self.phrasing(&mut node.children, TextParentType::Paragraph)?
                }
                Block::Blockquote(node) => self.blocks(&mut node.children, required)?,
                Block::List(node) => {
                    let mut changed = false;
                    for item in &mut node.children {
                        changed |= self.blocks(&mut item.children, required)?;
                    }
                    changed
                }
                Block::Table(node) => {
                    let mut changed = false;
                    for row in &mut node.children {
                        for cell in &mut row.children {
                            changed |=
                                self.phrasing(&mut cell.children, TextParentType::TableCell)?;
                        }
                    }
                    changed
                }
                Block::Code(_)
                | Block::Math(_)
                | Block::Image(_)
                | Block::ThematicBreak(_)
                | Block::Extension(_) => false,
            };
        }
        Ok(changed)
    }
}

/// Upstream `RequiredSource`: the source, and for each hint the first place it occurs at
/// or after the last block asked about, so blocks in source order read it once a hint.
struct RequiredSource<'a> {
    text: &'a str,
    /// `(hint, from, found)`.
    searches: Vec<(&'a str, usize, Option<usize>)>,
}

impl RequiredSource<'_> {
    /// Upstream `blockMayContainRequiredSource`: whether a hint starts inside the block's
    /// span. A block without offsets may hold anything.
    fn block_may_contain(&mut self, block: &Block) -> bool {
        let Some(position) = crate::plugins::node_position(crate::ast::NodeRef::from(block)) else {
            return true;
        };
        let (Some(start), Some(end)) = (position.start.offset, position.end.offset) else {
            return true;
        };
        let text = self.text;
        self.searches.iter_mut().any(|(hint, from, found)| {
            if start < *from || found.is_some_and(|found| found < start) {
                *from = start;
                *found = text
                    .get(start..)
                    .and_then(|rest| rest.find(*hint))
                    .map(|at| start + at);
            }
            found.is_some_and(|found| found < end)
        })
    }
}

/// Upstream `createMarkdownTextTransform`: a trusted transform (every node it returns is
/// one Core validated, or one it just checked), claiming source that holds a required
/// substring when the pattern is case-sensitive.
pub fn create_markdown_text_transform(
    options: TextTransformOptions,
) -> Result<Transform, PluginError> {
    if options
        .required_substrings
        .as_ref()
        .is_some_and(|required| required.is_empty() || required.iter().any(String::is_empty))
    {
        return Err(PluginError::new(
            "Markdown text transform requiredSubstrings must be non-empty strings",
        ));
    }
    let case_insensitive = is_case_insensitive(&options.pattern);
    let options = Rc::new(options);
    let run_options = options.clone();
    let transform = Transform::new(move |root: &Root, context: &TransformContext<'_>| {
        let ownership = context.ownership().unwrap_or(HelperOwnership::UNOWNED);
        let required = run_options
            .required_substrings
            .as_deref()
            .filter(|_| !case_insensitive);
        let run = Run {
            options: &run_options,
            required,
            ownership,
        };
        let mut source = required
            .filter(|_| ownership.source_unchanged)
            .map(|required| RequiredSource {
                text: context.source(),
                searches: required
                    .iter()
                    .map(|hint| (hint.as_str(), usize::MAX, None))
                    .collect(),
            });
        let mut next = root.clone();
        Ok(run.blocks(&mut next.children, &mut source)?.then_some(next))
    })
    .trusted();
    match options.required_substrings.clone() {
        Some(required) if !case_insensitive => Ok(transform
            .with_claim(move |source| required.iter().any(|hint| source.contains(hint.as_str())))),
        _ => Ok(transform),
    }
}
