//! Lists, upstream `Markdown/parser.ts` (`parseList`, `listItemSource`): items of one
//! marker and indentation, their content collected (lazy continuation lines included)
//! and parsed as nested blocks.

use std::collections::BTreeSet;

use super::ResolvedOptions;
use super::block::{LineResolver, parse_markdown_impl, source_ends_in_paragraph};
use super::lines::{can_continue_paragraph_lazily, is_horizontal_rule};
use crate::ast::{Block, List, ListItem, ListMarkerShape, Point, Position};
use crate::js::{self, get_indent};

/// Upstream `listItemSource`: an item's content as one text: its first line, then its
/// other lines without the indentation they share. A lazy line keeps none of its own.
fn list_item_source(item_text: &str, sub_lines: &[&str], lazy_lines: &BTreeSet<usize>) -> String {
    if sub_lines.is_empty() {
        return item_text.to_string();
    }
    let min_indent = sub_lines
        .iter()
        .enumerate()
        .filter(|&(index, line)| !js::is_blank(line) && !lazy_lines.contains(&index))
        .map(|(_, line)| get_indent(line))
        .min()
        .unwrap_or(0);
    let mut source = String::from(item_text);
    for (index, line) in sub_lines.iter().enumerate() {
        source.push('\n');
        if !js::is_blank(line) {
            source.push_str(
                &line[if lazy_lines.contains(&index) {
                    get_indent(line)
                } else {
                    min_indent
                }..],
            );
        }
    }
    source
}

/// What opens each item of a list: its marker at the list's indentation.
struct ItemMarker {
    indent: usize,
    /// `Some(delimiter)` for an ordered list, whose items share it (CommonMark 5.2).
    delimiter: Option<u8>,
    /// A bullet list's bullet, which its items share (CommonMark 0.31 §5.3).
    bullet: u8,
}

impl ItemMarker {
    /// Where the content of the item `line` opens starts, past its marker and the space
    /// after it (upstream's `itemPattern`: `^ {N}\d+[.)] ` or `^ {N}[-*+] `); `None` when
    /// `line` opens no item of this list.
    fn content_start(&self, line: &str) -> Option<usize> {
        let bytes = line.as_bytes();
        if get_indent(line) != self.indent {
            return None;
        }
        let marker_end = match self.delimiter {
            Some(delimiter) => {
                let digits = bytes[self.indent..]
                    .iter()
                    .take_while(|b| b.is_ascii_digit())
                    .count();
                let end = self.indent + digits;
                (digits > 0 && bytes.get(end) == Some(&delimiter)).then_some(end + 1)?
            }
            None => (bytes.get(self.indent) == Some(&self.bullet)).then_some(self.indent + 1)?,
        };
        (bytes.get(marker_end) == Some(&b' ')).then_some(marker_end + 1)
    }

    /// Upstream's `startsItem`: a thematic break is one, not an item, even where an item
    /// could start (CommonMark 0.31 §4.1).
    fn starts_item(&self, line: &str) -> bool {
        self.content_start(line).is_some() && !is_horizontal_rule(line)
    }
}

/// Upstream `parseList`: the list that opens at `lines[start]`, and the line after it.
/// Blank lines between items or inside one make it loose (`spread`). A line where a
/// plugin's block syntax starts (`interrupts_lazy_continuation`) is no lazy continuation;
/// with `document_line_of`, items record their source lines.
pub(super) fn parse_list(
    lines: &[&str],
    start: usize,
    ordered: bool,
    opts: &ResolvedOptions,
    interrupts_lazy_continuation: &dyn Fn(usize) -> bool,
    document_line_of: Option<&LineResolver>,
) -> (Block, usize) {
    let first = lines[start];
    let indent = get_indent(first);
    let digits = first.as_bytes()[indent..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    let marker = ItemMarker {
        indent,
        delimiter: ordered.then(|| first.as_bytes()[indent + digits]),
        bullet: first.as_bytes()[indent],
    };
    let start_number = ordered.then(|| {
        first[indent..indent + digits]
            .bytes()
            .fold(0i64, |number, digit| {
                number
                    .saturating_mul(10)
                    .saturating_add(i64::from(digit - b'0'))
            })
    });

    let mut items = Vec::new();
    let mut loose = false;
    let mut index = start;
    while index < lines.len() && marker.starts_item(lines[index]) {
        let item_start = index;
        let line = lines[index];
        let marker_end = marker.content_start(line).unwrap_or(line.len());
        let content = &line[marker_end..];
        // Where the item's content starts: past the marker and one to four spaces; more
        // makes the content indented by one space.
        let spaces_after = get_indent(content);
        let content_column = if spaces_after >= 4 {
            marker_end
        } else {
            marker_end + spaces_after
        };

        // A task item: `/^\[([ xX])\] (.*)/`.
        let task = content
            .strip_prefix('[')
            .filter(|rest| matches!(rest.as_bytes(), [b' ' | b'x' | b'X', b']', b' ', ..]));
        let (checked, item_text) = match task {
            Some(rest) => (
                Some(rest.as_bytes()[0] != b' '),
                js::up_to_line_terminator(&rest[3..]),
            ),
            None => (None, content),
        };
        index += 1;

        // Lines indented past the marker are the item's content. A less indented nonblank
        // line still belongs to it while its deepest open block is a paragraph: a lazy
        // continuation line (CommonMark).
        let mut sub_lines: Vec<&str> = Vec::new();
        let mut lazy_lines = BTreeSet::new();
        let mut lazy_paragraph_open =
            can_continue_paragraph_lazily(&[item_text], 0, true).then_some(true);
        while index < lines.len() && !js::is_blank(lines[index]) {
            let next = lines[index];
            if get_indent(next) > indent {
                sub_lines.push(next);
                if lazy_paragraph_open == Some(true)
                    && !can_continue_paragraph_lazily(&[js::trim_start(next)], 0, false)
                {
                    lazy_paragraph_open = None;
                }
                index += 1;
                continue;
            }
            if interrupts_lazy_continuation(index)
                || !can_continue_paragraph_lazily(lines, index, true)
            {
                break;
            }
            let open = *lazy_paragraph_open.get_or_insert_with(|| {
                source_ends_in_paragraph(
                    &list_item_source(item_text, &sub_lines, &lazy_lines),
                    opts,
                )
            });
            if !open {
                break;
            }
            lazy_lines.insert(sub_lines.len());
            sub_lines.push(next);
            index += 1;
        }

        // After blank lines, lines indented to the item's content still belong to it
        // (CommonMark 0.31 §5.2): a nested list, code, or another paragraph. Lazy
        // continuation stops at a blank line.
        loop {
            let mut next = index;
            while next < lines.len() && js::is_blank(lines[next]) {
                next += 1;
            }
            if next == index || next >= lines.len() || get_indent(lines[next]) < content_column {
                break;
            }
            sub_lines.extend(std::iter::repeat_n("", next - index));
            index = next;
            while index < lines.len()
                && !js::is_blank(lines[index])
                && get_indent(lines[index]) >= content_column
            {
                sub_lines.push(lines[index]);
                index += 1;
            }
            loose = true;
        }

        let source = list_item_source(item_text, &sub_lines, &lazy_lines);
        match document_line_of {
            None => items.push(ListItem {
                checked,
                children: parse_markdown_impl(&source, &opts.nested()),
                ..ListItem::default()
            }),
            Some(document_line_of) => {
                // The item's source holds one line per line it consumed, from its marker
                // line to its last content line, so its lines map back one to one.
                let origin: Vec<usize> = (item_start..index)
                    .map(|line| document_line_of.resolve(line))
                    .collect();
                let line = |line: usize| Point {
                    line: Some(line + 1),
                    ..Point::default()
                };
                let position = Position {
                    start: line(origin[0]),
                    end: line(origin[origin.len() - 1]),
                };
                items.push(ListItem {
                    checked,
                    children: parse_markdown_impl(&source, &opts.nested_at(Some(origin))),
                    position: Some(position),
                    data: None,
                });
            }
        }

        // Blank lines between items of the same marker and indentation still make one
        // (loose) list.
        let mut lookahead = index;
        while lookahead < lines.len() && js::is_blank(lines[lookahead]) {
            lookahead += 1;
        }
        if lookahead > index && lookahead < lines.len() && marker.starts_item(lines[lookahead]) {
            loose = true;
            index = lookahead;
        }
    }

    let list = List {
        ordered,
        start: start_number,
        spread: loose.then_some(true),
        delimiter: marker.delimiter.map(char::from),
        children: items,
        position: None,
        data: None,
        marker_shape: ListMarkerShape(Some((
            marker.indent,
            char::from(marker.delimiter.unwrap_or(marker.bullet)),
        ))),
    };
    (Block::List(list), index)
}
