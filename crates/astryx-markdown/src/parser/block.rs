//! The block parser, upstream `Markdown/parser.ts` ("Main block parser",
//! `parseMarkdownImpl`, with its table, blockquote and position helpers): a document read
//! line by line into blocks, each block's text then read inline.

use std::rc::Rc;

use super::inline::{InlineContext, destination_url, image_alt, parse_inline_entry};
use super::lines::{
    atx_marker, can_continue_paragraph_lazily, closes_fence, fence_line, is_block_start,
    is_bullet_item_start, is_horizontal_rule, is_ordered_item_start, is_quote_marker,
    is_table_separator, match_display_math_block,
};
use super::link_definitions::extract_link_definitions;
use super::list::parse_list;
use super::{MAX_BLOCK_NESTING, ResolvedOptions};
use crate::ast::{
    Block, Blockquote, Code, Heading, Image, Literal, Paragraph, Phrasing, Position, Table,
    TableAlign, TableCell, TableRow, Void,
};
use crate::js::{self, get_indent};
use crate::url::is_safe_markdown_parser_url;

/// The blocks a parse produced, with the line each started on and, for a block that
/// consumes blank lines as content, the line it ended on.
#[derive(Default)]
struct Blocks {
    nodes: Vec<Block>,
    start_lines: Vec<usize>,
    end_lines: Vec<Option<usize>>,
}

impl Blocks {
    fn push(&mut self, node: Block, start_line: usize, end_line: Option<usize>) {
        self.nodes.push(node);
        self.start_lines.push(start_line);
        self.end_lines.push(end_line);
    }
}

/// Upstream `quotedContent`: a quoted line's content, without the marker and the one space
/// or tab after it (`/^ {0,3}>[ \t]?/`).
fn quoted_content(line: &str) -> &str {
    let after = &line[get_indent(line) + 1..];
    after.strip_prefix([' ', '\t']).unwrap_or(after)
}

/// The standalone-image regex (`/^!\[([^\]]*)\]\(([^)]+)\)/`): the alt, the destination,
/// and the whole match.
fn standalone_image(line: &str) -> Option<(&str, &str, &str)> {
    let rest = line.strip_prefix("![")?;
    let alt_end = rest.find(']')?;
    let after = rest[alt_end + 1..].strip_prefix('(')?;
    let destination_end = after.find(')').filter(|&end| end > 0)?;
    let matched = 2 + alt_end + 2 + destination_end + 1;
    Some((
        &rest[..alt_end],
        &after[..destination_end],
        &line[..matched],
    ))
}

/// Upstream `endsInParagraph`: whether the deepest last block of `blocks` is a paragraph.
fn ends_in_paragraph(blocks: &[Block]) -> bool {
    match blocks.last() {
        Some(Block::Paragraph(_)) => true,
        Some(Block::Blockquote(quote)) => ends_in_paragraph(&quote.children),
        Some(Block::List(list)) => list
            .children
            .last()
            .is_some_and(|item| ends_in_paragraph(&item.children)),
        _ => false,
    }
}

/// Upstream `sourceEndsInParagraph`: whether `source`, parsed as nested content, ends in a
/// paragraph. A parse at the next depth may already have read it: the probe at each
/// nesting level then costs a lookup, not another parse of every level below.
pub(super) fn source_ends_in_paragraph(source: &str, opts: &ResolvedOptions) -> bool {
    let key = (opts.block_depth + 1, source.to_string());
    if let Some(&known) = opts.nested_parses.borrow().get(&key) {
        return known;
    }
    ends_in_paragraph(&parse_markdown_impl(
        source,
        &ResolvedOptions {
            probing: true,
            ..opts.nested()
        },
    ))
}

/// Upstream `splitTableRow`: a row's cells, trimmed. Indentation and a leading or trailing
/// pipe open no cell; an escaped `\|` stays in its cell, for the inline parser.
fn split_table_row(line: &str) -> Vec<String> {
    let bytes = line.as_bytes();
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && matches!(bytes[start], b' ' | b'\t') {
        start += 1;
    }
    if bytes.get(start) == Some(&b'|') {
        start += 1;
        while start < end && bytes[start] == b' ' {
            start += 1;
        }
    }
    while end > start && bytes[end - 1] == b' ' {
        end -= 1;
    }
    if end > start && bytes[end - 1] == b'|' {
        end -= 1;
    }
    let content = &line[start..end];
    let mut cells = Vec::new();
    let mut cell_start = 0;
    let content_bytes = content.as_bytes();
    let mut i = 0;
    while i < content_bytes.len() {
        if content_bytes[i] == b'\\' && content_bytes.get(i + 1) == Some(&b'|') {
            i += 2;
        } else if content_bytes[i] == b'|' {
            cells.push(js::trim(&content[cell_start..i]).to_string());
            i += 1;
            cell_start = i;
        } else {
            i += 1;
        }
    }
    cells.push(js::trim(&content[cell_start..]).to_string());
    cells
}

/// A row's cells, read inline.
fn table_cells(line: &str, opts: &ResolvedOptions) -> Vec<TableCell> {
    split_table_row(line)
        .iter()
        .map(|cell| TableCell {
            children: parse_inline_entry(cell, opts, InlineContext::TableCell),
            position: None,
        })
        .collect()
}

/// Upstream `parseTable`: the table whose header is `lines[index]`, and the line after it.
/// Body rows run while lines hold a pipe.
fn parse_table(lines: &[&str], index: usize, opts: &ResolvedOptions) -> (Block, usize) {
    let header = TableRow {
        children: table_cells(lines[index], opts),
        position: None,
    };
    let align = split_table_row(lines[index + 1])
        .iter()
        .map(|cell| {
            let cell = js::trim(cell);
            match (cell.starts_with(':'), cell.ends_with(':')) {
                (true, true) => Some(TableAlign::Center),
                (_, true) => Some(TableAlign::Right),
                (true, false) => Some(TableAlign::Left),
                (false, false) => None,
            }
        })
        .collect();
    let mut children = vec![header];
    let mut row = index + 2;
    while row < lines.len() && lines[row].contains('|') && !js::is_blank(lines[row]) {
        children.push(TableRow {
            children: table_cells(lines[row], opts),
            position: None,
        });
        row += 1;
    }
    (
        Block::Table(Table {
            align,
            children,
            position: None,
        }),
        row,
    )
}

/// Upstream `stampSourceRanges`: gives each top-level block the offsets it occupies in
/// `input`. A block runs from its first line to the line before the next block starts,
/// less the blank lines between, unless it states its own end. Offsets address `input`,
/// not the text the block loop read: link definition lines were removed from that, and
/// `line_map` says which input line each remaining line came from. A CRLF line's `\r` is
/// part of the line.
fn stamp_source_ranges(
    blocks: &mut Blocks,
    lines: &[&str],
    line_map: Option<&[usize]>,
    input: &str,
) {
    let mut input_line_starts = vec![0];
    input_line_starts.extend(input.match_indices('\n').map(|(at, _)| at + 1));
    let line_start = |line: usize| input_line_starts[line_map.map_or(line, |map| map[line])];
    let count = blocks.nodes.len();
    for i in 0..count {
        let start_line = blocks.start_lines[i];
        let end_line = blocks.end_lines[i].unwrap_or_else(|| {
            let next_start = if i + 1 < count {
                blocks.start_lines[i + 1]
            } else {
                lines.len()
            };
            let mut end_line = next_start - 1;
            while end_line > start_line && js::is_blank(lines[end_line]) {
                end_line -= 1;
            }
            end_line
        });
        let position = Position::offsets(
            line_start(start_line),
            line_start(end_line) + lines[end_line].len(),
        );
        set_position(&mut blocks.nodes[i], position);
    }
}

fn set_position(block: &mut Block, position: Position) {
    let slot = match block {
        Block::Heading(node) => &mut node.position,
        Block::Paragraph(node) => &mut node.position,
        Block::Code(node) => &mut node.position,
        Block::Math(node) => &mut node.position,
        Block::Blockquote(node) => &mut node.position,
        Block::List(node) => &mut node.position,
        Block::Table(node) => &mut node.position,
        Block::ThematicBreak(node) => &mut node.position,
        Block::Image(node) => &mut node.position,
        Block::Extension(node) => &mut node.position,
    };
    *slot = Some(position);
}

fn paragraph(children: Vec<Phrasing>) -> Block {
    Block::Paragraph(Paragraph {
        children,
        position: None,
    })
}

/// Upstream `parseMarkdownImpl`: the blocks of `input`.
pub(crate) fn parse_markdown_impl(input: &str, caller: &ResolvedOptions) -> Vec<Block> {
    // Content at depth 100 sits inside 100 lists or blockquotes: the cap. Deeper, it is
    // one paragraph of text.
    if caller.block_depth >= MAX_BLOCK_NESTING {
        let text = js::trim(input);
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![paragraph(parse_inline_entry(
                text,
                caller,
                InlineContext::Default,
            ))]
        };
    }
    // This input's link reference definitions, merged with those inherited from an
    // enclosing parse; inherited ones win, as the first definition in the document does.
    let extracted = extract_link_definitions(input, caller.math);
    let link_defs = if extracted.defs.is_empty() {
        caller.link_defs.clone()
    } else {
        let mut defs = extracted.defs;
        if let Some(inherited) = &caller.link_defs {
            defs.extend(
                inherited
                    .iter()
                    .map(|(label, href)| (label.clone(), href.clone())),
            );
        }
        Some(Rc::new(defs))
    };
    let opts = ResolvedOptions {
        link_defs,
        ..caller.clone()
    };
    let cleaned: &str = &extracted.cleaned;
    let lines: Vec<&str> = cleaned.split('\n').collect();
    let mut blocks = Blocks::default();
    let mut index = 0;

    while index < lines.len() {
        let start = index;
        let line = lines[index];
        if js::is_blank(line) {
            index += 1;
            continue;
        }

        // Fenced code. The rest of the opening line is the info string; its first word is
        // the language (CommonMark 0.31 §4.5).
        if let Some((indentation, fence)) = fence_line(line) {
            let info = js::trim(&line[indentation + fence.len()..]);
            let lang = (!info.is_empty())
                .then(|| &info[..info.find(js::is_whitespace).unwrap_or(info.len())]);
            let meta = lang
                .map(|lang| js::trim(&info[lang.len()..]))
                .filter(|meta| !meta.is_empty());
            let mut code_lines = Vec::new();
            index += 1;
            while index < lines.len() && !closes_fence(lines[index], fence) {
                // Each code line loses as much indentation as the opening fence has.
                let code_line = lines[index];
                code_lines.push(&code_line[get_indent(code_line).min(indentation)..]);
                index += 1;
            }
            let closed = index < lines.len();
            // A fence left open runs to the document's end, whose final line ending starts
            // no line of code. A container's content is rebuilt from its lines, so its last
            // line is a real one.
            if opts.block_depth == 0
                && !closed
                && !code_lines.is_empty()
                && lines.last() == Some(&"")
            {
                code_lines.pop();
            }
            index += 1;
            let code = Code {
                lang: lang.map(str::to_string),
                meta: meta.map(str::to_string),
                value: code_lines.join("\n"),
                position: None,
            };
            // A fence owns its blank lines, so it states its own end.
            blocks.push(Block::Code(code), start, Some(index.min(lines.len()) - 1));
            continue;
        }

        // Display math (opt-in; fenced code takes precedence).
        if opts.math
            && let Some(math) = match_display_math_block(&lines, index)
        {
            blocks.push(
                Block::Math(Literal {
                    value: math.value,
                    position: None,
                }),
                start,
                Some(math.end_line),
            );
            index = math.next_index;
            continue;
        }

        // ATX heading, indented up to three spaces (`/^ {0,3}(#{1,6}) +(.*)/`).
        if let Some((depth, rest)) = atx_marker(line)
            && rest.starts_with(' ')
        {
            let text = js::up_to_line_terminator(rest.trim_start_matches(' '));
            let children = parse_inline_entry(text, &opts, InlineContext::Default);
            blocks.push(
                Block::Heading(Heading {
                    depth,
                    children,
                    position: None,
                }),
                start,
                None,
            );
            index += 1;
            continue;
        }

        // Thematic break, before lists, for `- - -` and `* * *`.
        if is_horizontal_rule(line) {
            blocks.push(Block::ThematicBreak(Void::default()), start, None);
            index += 1;
            continue;
        }

        // A standalone image. An unsafe destination falls through to a paragraph, as text.
        if let Some((alt, destination, matched)) = standalone_image(line) {
            let url = destination_url(destination);
            if js::trim(line) == matched && is_safe_markdown_parser_url(&url) {
                let alt = image_alt(alt, &opts, InlineContext::Default);
                blocks.push(
                    Block::Image(Image {
                        url,
                        alt,
                        position: None,
                    }),
                    start,
                    None,
                );
                index += 1;
                continue;
            }
        }

        // Table, with or without a leading pipe.
        if index + 1 < lines.len() && line.contains('|') && is_table_separator(lines[index + 1]) {
            let (table, next) = parse_table(&lines, index, &opts);
            blocks.push(table, start, None);
            index = next;
            continue;
        }

        // Blockquote, with lazy continuation lines while its content ends in a paragraph.
        if is_quote_marker(line) {
            let mut quote_lines: Vec<&str> = Vec::new();
            let mut lazy_paragraph_open: Option<bool> = None;
            while index < lines.len() {
                let quote_line = lines[index];
                if is_quote_marker(quote_line) {
                    let content = quoted_content(quote_line);
                    quote_lines.push(content);
                    if js::is_blank(content) {
                        lazy_paragraph_open = Some(false);
                    } else if lazy_paragraph_open == Some(false)
                        || (lazy_paragraph_open == Some(true)
                            && !can_continue_paragraph_lazily(&[content], 0, false))
                    {
                        lazy_paragraph_open = None;
                    }
                    index += 1;
                    continue;
                }
                // Upstream also ends the quote where a plugin's block syntax starts.
                if quote_lines.last().is_some_and(|last| js::is_blank(last))
                    || !can_continue_paragraph_lazily(&lines, index, false)
                {
                    break;
                }
                let open = *lazy_paragraph_open.get_or_insert_with(|| {
                    source_ends_in_paragraph(&quote_lines.join("\n"), &opts)
                });
                if !open {
                    break;
                }
                quote_lines.push(quote_line);
                index += 1;
            }
            let children = parse_markdown_impl(&quote_lines.join("\n"), &opts.nested());
            blocks.push(
                Block::Blockquote(Blockquote {
                    children,
                    position: None,
                }),
                start,
                None,
            );
            continue;
        }

        // Lists.
        if is_bullet_item_start(line) || is_ordered_item_start(line) {
            let ordered = !is_bullet_item_start(line);
            let (list, next) = parse_list(&lines, index, ordered, &opts);
            blocks.push(list, start, None);
            index = next;
            continue;
        }

        // Upstream tries plugins' block extension syntax here; built-in blocks win.

        // Paragraph: up to a blank line or a block start.
        let mut paragraph_lines = vec![line];
        index += 1;
        while index < lines.len() {
            let next = lines[index];
            if is_block_start(next)
                || (opts.math && match_display_math_block(&lines, index).is_some())
                || js::is_blank(next)
            {
                break;
            }
            paragraph_lines.push(next);
            index += 1;
        }
        let children =
            parse_inline_entry(&paragraph_lines.join("\n"), &opts, InlineContext::Default);
        blocks.push(paragraph(children), start, None);
    }

    if opts.ast_positions {
        stamp_source_ranges(&mut blocks, &lines, extracted.line_map.as_deref(), input);
    }
    if opts.probing {
        let ends = ends_in_paragraph(&blocks.nodes);
        opts.nested_parses
            .borrow_mut()
            .insert((opts.block_depth, input.to_string()), ends);
    }
    blocks.nodes
}
