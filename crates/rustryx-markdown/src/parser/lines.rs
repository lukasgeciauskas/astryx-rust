//! Line tests the block parser and the link-definition scan share, upstream
//! `Markdown/parser.ts` ("Block parser helpers" and the fence and math helpers). Each
//! mirrors one of upstream's regexes, noted on it.

use crate::js::{self, get_indent};

/// `^ {0,max}`: the spaces that open `line`, when there are at most `max`.
fn indent_at_most(line: &str, max: usize) -> Option<usize> {
    let indent = get_indent(line);
    (indent <= max).then_some(indent)
}

/// `^ {0,3}#{1,6}`: the heading level an ATX marker opens `line` with, and the text after
/// the marker.
pub(super) fn atx_marker(line: &str) -> Option<(u8, &str)> {
    let indent = indent_at_most(line, 3)?;
    let rest = &line[indent..];
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    let depth = u8::try_from(hashes)
        .ok()
        .filter(|depth| (1..=6).contains(depth))?;
    Some((depth, &rest[hashes..]))
}

/// `/^ {0,3}#{1,6} /`: an ATX heading marker and a space.
pub(super) fn is_atx_heading_start(line: &str) -> bool {
    atx_marker(line).is_some_and(|(_, rest)| rest.starts_with(' '))
}

/// `/^ {0,3}#{1,6}(?:[ \t]|$)/`: an ATX heading marker, then a space, a tab or the end.
pub(super) fn is_atx_heading_line(line: &str) -> bool {
    atx_marker(line).is_some_and(|(_, rest)| rest.is_empty() || rest.starts_with([' ', '\t']))
}

/// Upstream `isHorizontalRule`: three or more of one marker (`-`, `*` or `_`), spaces
/// between allowed.
pub(super) fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = js::trim(line);
    let Some(marker) = trimmed.chars().next() else {
        return false;
    };
    if trimmed.len() < 3 || !matches!(marker, '-' | '*' | '_') || !trimmed.ends_with(marker) {
        return false;
    }
    let mut count = 0;
    for c in trimmed.chars() {
        if c == marker {
            count += 1;
        } else if c != ' ' {
            return false;
        }
    }
    count >= 3
}

/// Upstream `FENCE_LINE` (`/^( {0,3})(`{3,}|~{3,})/`): a code fence line's indentation and
/// its fence (CommonMark 0.31 §4.5).
pub(super) fn fence_line(line: &str) -> Option<(usize, &str)> {
    let indent = indent_at_most(line, 3)?;
    let rest = &line[indent..];
    let marker = *rest
        .as_bytes()
        .first()
        .filter(|&&b| b == b'`' || b == b'~')?;
    let run = rest.bytes().take_while(|&b| b == marker).count();
    (run >= 3).then(|| (indent, &rest[..run]))
}

/// Upstream `closesFence`: whether `line` closes a code block opened with `fence`: a
/// fence of the same character, at least as long, after up to three spaces, with only
/// spaces or tabs after it (`/^ {0,3}(`{3,}|~{3,})[ \t]*\r?$/`).
pub(super) fn closes_fence(line: &str, fence: &str) -> bool {
    let Some((indent, closing)) = fence_line(line) else {
        return false;
    };
    let after = &line[indent + closing.len()..];
    let after = after.strip_suffix('\r').unwrap_or(after);
    after.bytes().all(|b| b == b' ' || b == b'\t')
        && closing.as_bytes()[0] == fence.as_bytes()[0]
        && closing.len() >= fence.len()
}

/// Upstream `QUOTE_MARKER` (`/^ {0,3}>/`): a block quote marker (CommonMark 0.31 §5.1).
pub(super) fn is_quote_marker(line: &str) -> bool {
    indent_at_most(line, 3).is_some_and(|indent| line.as_bytes().get(indent) == Some(&b'>'))
}

/// `/^ {0,9}[-*+] /`: a bullet list item marker.
pub(super) fn is_bullet_item_start(line: &str) -> bool {
    indent_at_most(line, 9).is_some_and(|indent| {
        let rest = &line.as_bytes()[indent..];
        rest.len() >= 2 && matches!(rest[0], b'-' | b'*' | b'+') && rest[1] == b' '
    })
}

/// `^ {0,9}\d+[.)]`: where an ordered list item marker ends, past its delimiter.
fn ordered_marker_end(line: &str) -> Option<usize> {
    let indent = indent_at_most(line, 9)?;
    let digits = line.as_bytes()[indent..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    let delimiter = indent + digits;
    (digits > 0 && matches!(line.as_bytes().get(delimiter), Some(b'.' | b')')))
        .then_some(delimiter + 1)
}

/// `/^ {0,9}\d+[.)] /`: an ordered list item marker.
pub(super) fn is_ordered_item_start(line: &str) -> bool {
    ordered_marker_end(line).is_some_and(|end| line.as_bytes().get(end) == Some(&b' '))
}

/// `/^ {0,9}0*1[.)]/`: an ordered marker numbered 1, the only one that may interrupt a
/// paragraph.
pub(super) fn is_ordered_one_marker(line: &str) -> bool {
    ordered_marker_end(line).is_some_and(|end| {
        let number = js::trim_start(&line[..end - 1]);
        number.trim_start_matches('0') == "1"
    })
}

/// `/^ {0,9}0*1[.)] /`: an ordered item marker numbered 1, then a space.
fn is_ordered_one_item_start(line: &str) -> bool {
    is_ordered_one_marker(line) && is_ordered_item_start(line)
}

/// Upstream `isTableSeparator`: a GFM delimiter row, its cells only dashes and colons.
pub(super) fn is_table_separator(line: &str) -> bool {
    if !line.contains('|') {
        return false;
    }
    let mut cells = line
        .split('|')
        .map(js::trim)
        .filter(|cell| !cell.is_empty())
        .peekable();
    cells.peek().is_some() && cells.all(is_delimiter_cell)
}

/// `/^:?-+:?$/`.
fn is_delimiter_cell(cell: &str) -> bool {
    let cell = cell.strip_prefix(':').unwrap_or(cell);
    let cell = cell.strip_suffix(':').unwrap_or(cell);
    !cell.is_empty() && cell.bytes().all(|b| b == b'-')
}

/// Upstream `isBlockStart`: whether `line` could start a new block, which ends a
/// paragraph.
pub(super) fn is_block_start(line: &str) -> bool {
    is_atx_heading_start(line)
        || fence_line(line).is_some()
        || is_horizontal_rule(line)
        || is_quote_marker(line)
        || is_bullet_item_start(line)
        || is_ordered_one_item_start(line)
        || line.contains('|')
}

/// Upstream `canContinueParagraphLazily`: whether `lines[index]` may continue a paragraph
/// without its owning quote marker or list indentation. A blank line or a block start
/// ends that opportunity; inside a list, any ordered marker does.
pub(super) fn can_continue_paragraph_lazily(
    lines: &[&str],
    index: usize,
    inside_list: bool,
) -> bool {
    let Some(&line) = lines.get(index) else {
        return false;
    };
    if js::is_blank(line)
        || is_atx_heading_start(line)
        || fence_line(line).is_some()
        || is_horizontal_rule(line)
        || is_quote_marker(line)
        || is_bullet_item_start(line)
        || if inside_list {
            is_ordered_item_start(line)
        } else {
            is_ordered_one_item_start(line)
        }
    {
        return false;
    }
    !(line.contains('|')
        && lines
            .get(index + 1)
            .is_some_and(|next| is_table_separator(next)))
}

/// A complete `$$…$$` display-math block.
pub(super) struct DisplayMath {
    pub(super) value: String,
    pub(super) next_index: usize,
    pub(super) end_line: usize,
}

/// Upstream `matchDisplayMathBlock`: the display math that opens at `lines[index]`, on
/// one line (`$$x$$`) or between two `$$` lines; never a partial or empty one.
pub(super) fn match_display_math_block(lines: &[&str], index: usize) -> Option<DisplayMath> {
    let trimmed = js::trim(lines[index]);
    if trimmed.len() > 4 && trimmed.starts_with("$$") && trimmed.ends_with("$$") {
        let value = &trimmed[2..trimmed.len() - 2];
        return (!js::is_blank(value)).then(|| DisplayMath {
            value: value.to_string(),
            next_index: index + 1,
            end_line: index,
        });
    }
    if trimmed != "$$" {
        return None;
    }
    let close = (index + 1..lines.len()).find(|&line| js::trim(lines[line]) == "$$")?;
    let value = lines[index + 1..close].join("\n");
    (!js::is_blank(&value)).then_some(DisplayMath {
        value,
        next_index: close + 1,
        end_line: close,
    })
}
