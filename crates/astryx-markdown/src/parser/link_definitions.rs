//! Link reference definitions, upstream `Markdown/parser.ts` ("Link reference
//! definitions", `topLevelFences`, `extractLinkDefinitions`): collected from a document
//! before its blocks are read, their lines removed.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use super::inline::decode_link_destination;
use super::lines::{
    can_continue_paragraph_lazily, closes_fence, fence_line, is_atx_heading_line,
    is_horizontal_rule, is_ordered_one_marker, match_display_math_block,
};
use super::{LinkDefinitions, MAX_LINK_LABEL_LENGTH};
use crate::js::{self, get_indent};
use crate::url::normalized_markdown_destination;

/// Upstream `LINK_DEFINITION_RE`: up to 3 spaces, a bracketed label, `:`, a destination
/// (bare or `<…>`), and an optional same-line title (group 4, so a title-less definition
/// can take a title from the next line). A `^` label (`[^1]:`) is a footnote, which is
/// not supported, and stays text.
static LINK_DEFINITION: LazyLock<Regex> = LazyLock::new(|| {
    let pattern = r#"^ {0,3}\[([^\]^](?:\\[^\n\r\x{2028}\x{2029}]|[^\]\\])*)\]:[ \t]*(?:<([^<>\n]*)>|([^{ws}]+))([ \t]+(?:"[^"\n]*"|'[^'\n]*'|\([^()\n]*\)))?[ \t]*$"#;
    Regex::new(&pattern.replace("{ws}", js::WHITESPACE_CLASS)).expect("LINK_DEFINITION is valid")
});

/// Upstream `LINK_TITLE_ONLY_RE`: a line that is only a title, the continuation a
/// definition's destination may take its title from.
static LINK_TITLE_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^ {0,3}(?:"[^"\n]*"|'[^'\n]*'|\([^()\n]*\))[ \t]*$"#)
        .expect("LINK_TITLE_ONLY is valid")
});

/// Upstream `normalizeLinkLabel`: labels match case-insensitively, trimmed, with inner
/// whitespace runs collapsed to one space.
pub(super) fn normalize_link_label(label: &str) -> String {
    let mut normalized = String::with_capacity(label.len());
    let mut space = false;
    for c in js::trim(label).chars() {
        if js::is_whitespace(c) {
            space = true;
        } else {
            if space {
                normalized.push(' ');
                space = false;
            }
            normalized.push(c);
        }
    }
    normalized.to_lowercase()
}

/// A definition line: its normalized label and decoded destination.
struct LinkDefinition {
    label: String,
    destination: String,
    has_title: bool,
}

/// Upstream `matchLinkDefinition`. A label longer than the CommonMark limit defines
/// nothing, so its line stays text, as a reference with that label does.
fn match_link_definition(line: &str) -> Option<LinkDefinition> {
    let captures = LINK_DEFINITION.captures(line)?;
    let label = captures.get(1)?.as_str();
    if js::longer_than_utf16(label, MAX_LINK_LABEL_LENGTH) {
        return None;
    }
    let label = normalize_link_label(label);
    // An `<…>` destination may be empty (`<>`); a bare one never is.
    let destination = captures.get(2).or_else(|| captures.get(3))?.as_str();
    if label.is_empty() {
        return None;
    }
    Some(LinkDefinition {
        label,
        destination: normalized_markdown_destination(&decode_link_destination(destination)),
        has_title: captures.get(4).is_some(),
    })
}

/// Upstream `ListItemScope`: where a list item's lines sit.
#[derive(Clone, Copy)]
struct ListItemScope {
    /// The marker's indentation.
    base: usize,
    /// Where the item's content starts: past the marker and its spaces.
    content: usize,
    ordered: bool,
}

/// Upstream `listItemScopeOf`: the list item `line` opens, as the block parser reads one
/// (`/^( {0,9})([-*+]|\d+[.)]) /`). A thematic break opens no item.
fn list_item_scope_of(line: &str) -> Option<ListItemScope> {
    if is_horizontal_rule(line) {
        return None;
    }
    let base = get_indent(line);
    if base > 9 {
        return None;
    }
    let bytes = line.as_bytes();
    let (marker_end, ordered) = match bytes.get(base) {
        Some(b'-' | b'*' | b'+') => (base + 1, false),
        _ => {
            let digits = bytes[base..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            if digits == 0 || !matches!(bytes.get(base + digits), Some(b'.' | b')')) {
                return None;
            }
            (base + digits + 1, true)
        }
    };
    if bytes.get(marker_end) != Some(&b' ') {
        return None;
    }
    let marker_end = marker_end + 1;
    let spaces_after = get_indent(&line[marker_end..]);
    Some(ListItemScope {
        base,
        content: if spaces_after >= 4 {
            marker_end
        } else {
            marker_end + spaces_after
        },
        ordered,
    })
}

/// What a line does to the top-level code fence.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FenceEvent {
    Open,
    Close,
    Inside,
}

/// Upstream `topLevelFences`: follows the code fences that open at the top level of a
/// document, a line at a time, as the block parser reads them. A fence indented into an
/// open list item belongs to that item: it neither opens nor closes a top-level fence.
#[derive(Default)]
struct TopLevelFences<'a> {
    /// The open top-level fence.
    fence: Option<&'a str>,
    /// The outermost open list item, and what it last read. After a blank line inside the
    /// item, only lines indented to its content stay in it.
    item: Option<ListItemScope>,
    item_had_blank: bool,
    item_fence: Option<&'a str>,
    item_ends_in_paragraph: bool,
    /// A number other than 1 cannot interrupt an open top-level paragraph.
    paragraph_open: bool,
}

/// A heading or a thematic break: a block of one line.
fn is_heading_or_break(text: &str) -> bool {
    is_atx_heading_line(text) || is_horizontal_rule(text)
}

impl<'a> TopLevelFences<'a> {
    fn is_open(&self) -> bool {
        self.fence.is_some()
    }

    /// Reads a line of the open item, without the item's indentation.
    fn read_item_line(&mut self, text: &'a str) {
        if let Some(fence) = self.item_fence {
            if closes_fence(text, fence) {
                self.item_fence = None;
            }
            self.item_ends_in_paragraph = false;
            return;
        }
        if let Some((_, fence)) = fence_line(text) {
            self.item_fence = Some(fence);
            self.item_ends_in_paragraph = false;
            return;
        }
        self.item_ends_in_paragraph = !js::is_blank(text) && !is_heading_or_break(text);
    }

    fn read(&mut self, lines: &[&'a str], index: usize) -> Option<FenceEvent> {
        let line = lines[index];
        if let Some(fence) = self.fence {
            if closes_fence(line, fence) {
                self.fence = None;
                return Some(FenceEvent::Close);
            }
            return Some(FenceEvent::Inside);
        }
        if js::is_blank(line) {
            self.item_had_blank = self.item.is_some();
            self.paragraph_open = false;
            return None;
        }
        let indent = get_indent(line);
        if let Some(item) = self.item
            && (if self.item_had_blank {
                indent >= item.content
            } else {
                indent > item.base
            })
        {
            self.read_item_line(&line[indent.min(item.content)..]);
            return None;
        }
        if let Some(scope) = list_item_scope_of(line)
            && !(self.paragraph_open
                && self.item.is_none()
                && scope.ordered
                && !is_ordered_one_marker(line))
        {
            self.item = Some(scope);
            self.item_had_blank = false;
            self.item_fence = None;
            self.paragraph_open = false;
            self.read_item_line(&line[scope.content..]);
            return None;
        }
        if let Some((_, fence)) = fence_line(line) {
            self.item = None;
            self.paragraph_open = false;
            self.fence = Some(fence);
            return Some(FenceEvent::Open);
        }
        if self.item.is_some()
            && !self.item_had_blank
            && self.item_ends_in_paragraph
            && can_continue_paragraph_lazily(lines, index, false)
        {
            // A lazy continuation line of the item's paragraph.
            return None;
        }
        self.item = None;
        self.paragraph_open = !is_heading_or_break(line);
        None
    }
}

/// What [`extract_link_definitions`] found.
pub(super) struct ExtractedDefinitions<'a> {
    pub(super) defs: LinkDefinitions,
    /// The input without its definition lines.
    pub(super) cleaned: Cow<'a, str>,
    /// For each line of `cleaned`, the input line it came from; `None` when nothing was
    /// removed.
    pub(super) line_map: Option<Vec<usize>>,
}

/// Upstream `extractLinkDefinitions`: the link reference definitions of `input`, and
/// `input` without their lines. A definition is recognized at a block boundary (the
/// start, after a blank line, after another definition, or after a heading, thematic
/// break or closed fence), never inside a fence or as a paragraph's continuation. The
/// first definition of a label wins. Only top-level definitions are collected; one nested
/// in a blockquote or list item resolves within it, through its own parse.
pub(super) fn extract_link_definitions(input: &str, math: bool) -> ExtractedDefinitions<'_> {
    let lines: Vec<&str> = input.split('\n').collect();
    let mut defs = HashMap::new();
    let mut keep = vec![true; lines.len()];
    let mut at_boundary = true;
    let mut fences = TopLevelFences::default();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if fences.is_open() {
            // The line after a closed fence begins a new block.
            at_boundary = fences.read(&lines, index) == Some(FenceEvent::Close);
            index += 1;
            continue;
        }
        if math && let Some(display_math) = match_display_math_block(&lines, index) {
            // Math is opaque: definition-shaped TeX defines nothing.
            index = display_math.end_line + 1;
            at_boundary = true;
            continue;
        }
        if fences.read(&lines, index) == Some(FenceEvent::Open) {
            at_boundary = false;
            index += 1;
            continue;
        }
        if js::is_blank(line) {
            at_boundary = true;
            index += 1;
            continue;
        }
        if at_boundary && let Some(def) = match_link_definition(line) {
            defs.entry(def.label).or_insert(def.destination);
            keep[index] = false;
            // A title-less definition takes a title on the next line, which then also
            // produces no output.
            if !def.has_title
                && lines
                    .get(index + 1)
                    .is_some_and(|next| LINK_TITLE_ONLY.is_match(next))
            {
                keep[index + 1] = false;
                index += 1;
            }
            // Consecutive definitions stay at a block boundary.
            index += 1;
            continue;
        }
        // A heading or thematic break is a block of one line, so a definition may follow.
        at_boundary = is_heading_or_break(line);
        index += 1;
    }

    if defs.is_empty() {
        return ExtractedDefinitions {
            defs,
            cleaned: Cow::Borrowed(input),
            line_map: None,
        };
    }
    let line_map: Vec<usize> = (0..lines.len()).filter(|&index| keep[index]).collect();
    let cleaned = line_map
        .iter()
        .map(|&index| lines[index])
        .collect::<Vec<_>>()
        .join("\n");
    ExtractedDefinitions {
        defs,
        cleaned: Cow::Owned(cleaned),
        line_map: Some(line_map),
    }
}
