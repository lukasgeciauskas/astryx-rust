//! Streaming's trimming, upstream `Markdown/parser.ts` (`trimStreamingArtifacts`,
//! `trimOpenDisplayMath`, `trimUnsettledStructural`, and the display-math container
//! helpers the incremental parser's settlement shares): what a streamed snapshot parses
//! as, so partial syntax (an unclosed code span, emphasis, link, table header, list
//! marker or display expression) never shows while more of the document is on its way.

use std::borrow::Cow;

use super::inline::{find_inline_math_end, is_escaped, is_inline_math_start};
use super::lines::is_table_separator;
use super::link_definitions::{FenceEvent, TopLevelFences};
use crate::js::{self, get_indent};

/// Upstream `DisplayMathContainer`: where a standalone `$$` opener sits, so the line that
/// closes it is read inside the same container.
#[derive(Clone, Copy, Debug)]
pub(super) struct DisplayMathContainer {
    outer_quote_depth: usize,
    /// The opening list item's marker indentation, when a list item holds the opener.
    list_base_indent: Option<usize>,
    inner_quote_depth: usize,
}

/// Upstream `DisplayMathLineState`: what a line does to an open display expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DisplayMathLineState {
    Close,
    Inside,
    Outside,
}

/// `/^ {0,3}> ?/`: the length of the block quote marker that opens `text`.
fn quote_marker_len(text: &str) -> Option<usize> {
    let indent = get_indent(text);
    if indent > 3 || text.as_bytes().get(indent) != Some(&b'>') {
        return None;
    }
    Some(indent + 1 + usize::from(text.as_bytes().get(indent + 1) == Some(&b' ')))
}

/// Upstream `stripBlockquoteMarkers`: `line` without its trailing `\r` and every block
/// quote marker that opens it, and how many there were.
fn strip_blockquote_markers(line: &str) -> (&str, usize) {
    let mut content = line.strip_suffix('\r').unwrap_or(line);
    let mut depth = 0;
    while let Some(marker) = quote_marker_len(content) {
        content = &content[marker..];
        depth += 1;
    }
    (content, depth)
}

/// Upstream `stripExactBlockquoteDepth`: `line` without `depth` block quote markers, when
/// it has that many.
fn strip_exact_blockquote_depth(line: &str, depth: usize) -> Option<&str> {
    let mut content = line;
    for _ in 0..depth {
        content = &content[quote_marker_len(content)?..];
    }
    Some(content)
}

/// Whether `c` ends a line to JavaScript: what a regex `.` does not match.
fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `/^( {0,9})(?:[-*+]|\d+[.)]) +(.*)$/`: a list marker's indentation and the content after
/// the marker and its spaces.
fn list_marker_content(content: &str) -> Option<(usize, &str)> {
    let indent = get_indent(content);
    if indent > 9 {
        return None;
    }
    let bytes = content.as_bytes();
    let marker_end = match bytes.get(indent) {
        Some(b'-' | b'*' | b'+') => indent + 1,
        _ => {
            let digits = bytes[indent..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            if digits == 0 || !matches!(bytes.get(indent + digits), Some(b'.' | b')')) {
                return None;
            }
            indent + digits + 1
        }
    };
    let spaces = get_indent(&content[marker_end..]);
    if spaces == 0 {
        return None;
    }
    let rest = &content[marker_end + spaces..];
    (!rest.contains(is_line_terminator)).then_some((indent, rest))
}

/// `/^\[[ xX]\] +(.*)$/`: a task item's content after its box and spaces.
fn task_content(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('[')?;
    if !matches!(rest.as_bytes(), [b' ' | b'x' | b'X', b']', ..]) {
        return None;
    }
    let after = &rest[2..];
    let spaces = get_indent(after);
    (spaces > 0).then(|| &after[spaces..])
}

/// Upstream `displayMathContainer`: the container a standalone `$$` marker on `line` opens
/// display math in: the top level, a block quote, or one list item (with a block quote in
/// it). A list opener keeps its indentation, so an indented continuation marker closes
/// it; a block quote must keep its depth.
pub(super) fn display_math_container(line: &str) -> Option<DisplayMathContainer> {
    let (outer, outer_quote_depth) = strip_blockquote_markers(line);
    if let Some((indent, rest)) = list_marker_content(outer) {
        let (inner, inner_quote_depth) =
            strip_blockquote_markers(task_content(rest).unwrap_or(rest));
        return (js::trim(inner) == "$$").then_some(DisplayMathContainer {
            outer_quote_depth,
            list_base_indent: Some(indent),
            inner_quote_depth,
        });
    }
    (js::trim(outer) == "$$").then_some(DisplayMathContainer {
        outer_quote_depth,
        list_base_indent: None,
        inner_quote_depth: 0,
    })
}

/// Upstream `displayMathLineState`: whether `line` closes the display expression open in
/// `container`, is inside it, or has left the container.
pub(super) fn display_math_line_state(
    line: &str,
    container: &DisplayMathContainer,
) -> DisplayMathLineState {
    let Some(outer) = strip_exact_blockquote_depth(line, container.outer_quote_depth) else {
        return DisplayMathLineState::Outside;
    };
    let close_or_inside = |content: &str| {
        if js::trim(content) == "$$" {
            DisplayMathLineState::Close
        } else {
            DisplayMathLineState::Inside
        }
    };
    let Some(base_indent) = container.list_base_indent else {
        // A deeper quote is another container: it neither closes nor continues the math
        // the shallower quote owns.
        if quote_marker_len(outer).is_some() {
            return DisplayMathLineState::Outside;
        }
        return close_or_inside(outer);
    };
    let trimmed = js::trim_start(outer);
    // Upstream counts UTF-16 units; whitespace is one unit a character.
    if outer[..outer.len() - trimmed.len()].chars().count() <= base_indent {
        return DisplayMathLineState::Outside;
    }
    match strip_exact_blockquote_depth(trimmed, container.inner_quote_depth) {
        Some(inner) if quote_marker_len(inner).is_none() => close_or_inside(inner),
        _ => DisplayMathLineState::Outside,
    }
}

/// JavaScript's `hay.lastIndexOf(needle, from)`: the last match starting at or before
/// `from` (a negative `from` is 0).
fn last_index_of(hay: &[u8], needle: &[u8], from: isize) -> Option<usize> {
    if needle.len() > hay.len() {
        return None;
    }
    let start = usize::try_from(from.max(0))
        .unwrap_or(0)
        .min(hay.len() - needle.len());
    (0..=start).rev().find(|&at| hay[at..].starts_with(needle))
}

/// `text.length - 1`-style arithmetic: a byte length as a signed index.
fn signed(n: usize) -> isize {
    isize::try_from(n).unwrap_or(isize::MAX)
}

/// Upstream `trimStreamingArtifacts`: strips trailing incomplete inline syntax from a
/// streamed snapshot, and auto-closes an emphasis run that already has content so it
/// renders formatted as it streams. Only the last line changes, past an incomplete
/// display expression (with `math`) dropped as one unit, so it is safe on the whole
/// string.
pub(crate) fn trim_streaming_artifacts(input: &str, math: bool) -> String {
    // An incomplete display expression goes first, as one structural unit: only a scan
    // of the whole input tells a terminal nested closer from a new opener.
    let display_trimmed: Cow<str> = if math {
        trim_open_display_math(input)
    } else {
        Cow::Borrowed(input)
    };
    let (prefix, tail) = match display_trimmed.rfind('\n') {
        Some(newline) => display_trimmed.split_at(newline + 1),
        None => ("", &*display_trimmed),
    };
    let mut tail = tail.to_string();

    if math {
        // An unmatched inline opener is held back so raw TeX does not flash. When another
        // unescaped dollar follows but fails the closing rule, both stay text (currency).
        let mut index = 0;
        while index < tail.len() {
            let bytes = tail.as_bytes();
            if bytes[index] == b'$'
                && index + 1 == bytes.len()
                && (index == 0 || bytes[index - 1] != b'$')
                && !(index > 0 && bytes[index - 1].is_ascii_digit())
                && !is_escaped(bytes, index)
            {
                tail.truncate(index);
                break;
            }
            if !is_inline_math_start(&tail, index) {
                index += 1;
                continue;
            }
            if let Some(close) = find_inline_math_end(&tail, index) {
                index = close + 1;
                continue;
            }
            let later = (index + 1..bytes.len())
                .find(|&next| bytes[next] == b'$' && !is_escaped(bytes, next));
            match later {
                None => {
                    tail.truncate(index);
                    break;
                }
                Some(later) => index = later + 1,
            }
        }
    }

    // The last `[` or `![` that no `](…)` closes: an unfinished link or image.
    if let Some(last_bracket) = tail.rfind('[') {
        let after = &tail[last_bracket..];
        if !(after.contains("](") && after.contains(')')) {
            let trim_to = if last_bracket > 0 && tail.as_bytes()[last_bracket - 1] == b'!' {
                last_bracket - 1
            } else {
                last_bracket
            };
            tail.truncate(trim_to);
        }
    }

    // Trailing backticks no earlier run of the same length opens: an unclosed code span.
    let mut end = tail.len();
    while end > 0 && tail.as_bytes()[end - 1] == b'`' {
        end -= 1;
    }
    if end < tail.len() && end > 0 {
        let ticks = "`".repeat(tail.len() - end);
        if last_index_of(tail.as_bytes(), ticks.as_bytes(), signed(end) - 1).is_none() {
            tail.truncate(end);
        }
    }

    // Trailing stars with nothing after them yet, and no opener of their length.
    let mut end = tail.len();
    while end > 0 && tail.as_bytes()[end - 1] == b'*' {
        end -= 1;
    }
    if end < tail.len() && end > 0 {
        let stars = tail.len() - end;
        if stars <= 3 {
            let run = "*".repeat(stars);
            if last_index_of(tail.as_bytes(), run.as_bytes(), signed(end) - 1).is_none() {
                tail.truncate(end);
            }
        }
    }

    // Unclosed emphasis mid-line ("Hello **bold"): closed rather than hidden, so the text
    // renders formatted as it streams in.
    {
        let mut markers: Vec<(usize, usize)> = Vec::new();
        let mut search_from = 0;
        while search_from < tail.len() {
            let Some(at) = tail[search_from..].find('*').map(|at| at + search_from) else {
                break;
            };
            let bytes = tail.as_bytes();
            let mut len = 1;
            while at + len < bytes.len() && bytes[at + len] == b'*' {
                len += 1;
            }
            search_from = at + len;
            // Four or more stars are no emphasis.
            if len <= 3 {
                markers.push((at, len));
            }
        }
        // Markers pair greedily with the next of the same length.
        let mut paired = vec![false; markers.len()];
        for i in 0..markers.len() {
            if paired[i] {
                continue;
            }
            if let Some(j) =
                (i + 1..markers.len()).find(|&j| !paired[j] && markers[j].1 == markers[i].1)
            {
                paired[i] = true;
                paired[j] = true;
            }
        }
        // Each unpaired opener closes (last first) before any trailing whitespace, since a
        // closer must follow content (CommonMark's flanking rules): `**bold ` closes as
        // `**bold** `. An opener with only whitespace after it goes.
        for i in (0..markers.len()).rev() {
            if paired[i] {
                continue;
            }
            let (pos, len) = markers[i];
            let content = tail.get(pos + len..).unwrap_or("");
            let trailing = content.len() - js::trim_end(content).len();
            if trailing < content.len() {
                let at = tail.len() - trailing;
                tail.insert_str(at, &"*".repeat(len));
            } else {
                tail.truncate(pos);
            }
        }
    }

    // A trailing `~~` no earlier `~~` opens, or a lone `~` that may become one.
    if tail.len() >= 2 && tail.ends_with("~~") {
        if last_index_of(tail.as_bytes(), b"~~", signed(tail.len()) - 3).is_none() {
            tail.truncate(tail.len() - 2);
        }
    } else if tail.ends_with('~') && tail.len() >= 2 && tail.as_bytes()[tail.len() - 2] != b'~' {
        tail.truncate(tail.len() - 1);
    }

    // An odd number of `~~`: the last is unclosed, and the line is cut there.
    {
        let mut positions = Vec::new();
        let mut search_from = 0;
        while let Some(at) = tail.get(search_from..).and_then(|rest| rest.find("~~")) {
            positions.push(search_from + at);
            search_from += at + 2;
        }
        if positions.len() % 2 == 1 {
            tail.truncate(positions[positions.len() - 1]);
        }
    }

    let mut out = String::with_capacity(prefix.len() + tail.len());
    out.push_str(prefix);
    out.push_str(&tail);
    out
}

/// Upstream `trimOpenDisplayMath`: `text` without a display expression still open at its
/// end (from its opening line on, and the whitespace before it). Fences are followed as the
/// block parser reads them, and a container left before its closer arrives ends the
/// expression, as the parser then reads the opener as text.
pub(super) fn trim_open_display_math(text: &str) -> Cow<'_, str> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut fences = TopLevelFences::default();
    let mut math_container: Option<DisplayMathContainer> = None;
    let mut suppress_math_until_boundary = false;
    let mut math_start_line = None;

    for index in 0..lines.len() {
        let line = lines[index];
        if fences.is_open() {
            fences.read(&lines, index);
            continue;
        }
        if let Some(container) = &math_container {
            match display_math_line_state(line, container) {
                DisplayMathLineState::Close => {
                    math_container = None;
                    math_start_line = None;
                    continue;
                }
                DisplayMathLineState::Inside => continue,
                DisplayMathLineState::Outside => {
                    math_container = None;
                    math_start_line = None;
                    suppress_math_until_boundary = true;
                }
            }
        }
        if js::is_blank(line) {
            suppress_math_until_boundary = false;
        }
        if fences.read(&lines, index) == Some(FenceEvent::Open) {
            continue;
        }
        let container = if suppress_math_until_boundary {
            None
        } else {
            display_math_container(line)
        };
        if container.is_some() {
            math_container = container;
            math_start_line = Some(index);
        }
    }

    match (math_container, math_start_line) {
        (Some(_), Some(start)) => Cow::Owned(js::trim_end(&lines[..start].join("\n")).to_string()),
        _ => Cow::Borrowed(text),
    }
}

/// Upstream `hasUnescapedPipe`: whether `line` holds a pipe that could delimit cells; an
/// escaped `\|` is text in one cell.
fn has_unescaped_pipe(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'|' => return true,
            _ => index += 1,
        }
    }
    false
}

/// `/^ {0,9}[-*+] $/` or `/^ {0,9}\d+[.)] $/`: a list marker with nothing after it yet.
fn is_bare_list_marker(line: &str) -> bool {
    let indent = get_indent(line);
    if indent > 9 {
        return false;
    }
    let rest = &line.as_bytes()[indent..];
    match rest {
        [b'-' | b'*' | b'+', b' '] => true,
        _ => {
            let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
            digits > 0 && matches!(&rest[digits..], [b'.' | b')', b' '])
        }
    }
}

/// Upstream `trimUnsettledStructural`: the unsettled tail without trailing lines that
/// start a block not complete yet: blank lines, a bare list marker (`- `, `1. `), a lone
/// table header (no separator row under it, no table established above), and a
/// separator with no header. Once a table has its header and separator, its rows show
/// as they arrive.
pub(super) fn trim_unsettled_structural(text: &str) -> String {
    let mut lines: Vec<&str> = text.split('\n').collect();
    while let Some(&last) = lines.last() {
        let trimmed = js::trim(last);
        if trimmed.is_empty() || is_bare_list_marker(last) {
            lines.pop();
            continue;
        }
        // A line with an unescaped pipe that is no separator waits for its separator,
        // unless a header and separator above already make it a row. (The header test is
        // the block parser's own `includes('|')`.)
        if has_unescaped_pipe(trimmed) && !is_table_separator(last) {
            let established = (1..lines.len().saturating_sub(1))
                .rev()
                .any(|i| is_table_separator(lines[i]) && lines[i - 1].contains('|'));
            if !established {
                lines.pop();
                continue;
            }
        }
        if is_table_separator(last) && (lines.len() < 2 || !lines[lines.len() - 2].contains('|')) {
            lines.pop();
            continue;
        }
        break;
    }
    lines.join("\n")
}
