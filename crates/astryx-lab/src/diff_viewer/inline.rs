//! Upstream lab `DiffViewer/inlineDiff.ts`: the changed ranges of a removed line and
//! the added line that replaced it, for word- or character-level highlighting. The
//! longest common subsequence of the two lines' tokens (words, whitespace runs and
//! single punctuation characters, or single characters) is kept; what falls outside it
//! is changed. Lengths count UTF-16 code units, as upstream's do; ranges are byte
//! offsets into each line.

use std::ops::Range;

/// Upstream `InlineDiffGranularity`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineDiffGranularity {
    Word,
    Char,
}

/// Upstream `MAX_CELLS`: past this many token pairs a pair has no inline ranges.
const MAX_CELLS: usize = 250_000;
/// Upstream `MIN_SHARED_RATIO`.
const MIN_SHARED_RATIO: f64 = 0.5;

/// Upstream `InlineDiffResult`: the changed ranges of each line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InlineDiffResult {
    pub old: Vec<Range<usize>>,
    pub new: Vec<Range<usize>>,
}

struct Piece<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

/// JavaScript's `\s`.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// JavaScript's `\w` (ASCII, also with the `u` flag).
fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Upstream `split`: code points, or `/\w+|\s+|[^\w\s]/gu` matches.
fn split(line: &str, granularity: InlineDiffGranularity) -> Vec<Piece<'_>> {
    let mut pieces = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        let mut end = start + c.len_utf8();
        if granularity == InlineDiffGranularity::Word {
            let class = |c: char| {
                if is_word(c) {
                    0
                } else if is_js_space(c) {
                    1
                } else {
                    2
                }
            };
            let kind = class(c);
            if kind != 2 {
                while let Some(&(i, next)) = chars.peek() {
                    if class(next) != kind {
                        break;
                    }
                    end = i + next.len_utf8();
                    chars.next();
                }
            }
        }
        pieces.push(Piece {
            text: &line[start..end],
            start,
            end,
        });
    }
    pieces
}

fn is_blank(text: &str) -> bool {
    text.chars().all(is_js_space)
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Upstream `toRanges`: adjacent changed pieces join, and in word mode so do two
/// changes separated only by whitespace; ranges of only whitespace are dropped.
fn to_ranges(
    pieces: &[Piece],
    changed: &[bool],
    granularity: InlineDiffGranularity,
) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < pieces.len() {
        if !changed[i] {
            i += 1;
            continue;
        }
        let start = pieces[i].start;
        let mut end = pieces[i].end;
        i += 1;
        while i < pieces.len() {
            if changed[i] {
                end = pieces[i].end;
                i += 1;
                continue;
            }
            // Bridge one unchanged whitespace run between two changes.
            if granularity == InlineDiffGranularity::Word
                && is_blank(pieces[i].text)
                && i + 1 < pieces.len()
                && changed[i + 1]
            {
                end = pieces[i + 1].end;
                i += 2;
                continue;
            }
            break;
        }
        if pieces
            .iter()
            .any(|p| p.start >= start && p.end <= end && !is_blank(p.text))
        {
            ranges.push(start..end);
        }
    }
    ranges
}

fn content_length(pieces: &[Piece]) -> usize {
    pieces
        .iter()
        .filter(|p| !is_blank(p.text))
        .map(|p| utf16_len(p.text))
        .sum()
}

/// Upstream `diffInlineRanges`: no ranges when the lines are the same, mostly
/// different (less than half of the shorter line's non-whitespace text in common), or
/// too long to compare.
pub fn diff_inline_ranges(
    old_line: &str,
    new_line: &str,
    granularity: InlineDiffGranularity,
) -> InlineDiffResult {
    if old_line == new_line {
        return InlineDiffResult::default();
    }
    let a = split(old_line, granularity);
    let b = split(new_line, granularity);
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 || n * m > MAX_CELLS {
        return InlineDiffResult::default();
    }
    let width = m + 1;
    let mut lengths = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lengths[i * width + j] = if a[i].text == b[j].text {
                lengths[(i + 1) * width + j + 1] + 1
            } else {
                lengths[(i + 1) * width + j].max(lengths[i * width + j + 1])
            };
        }
    }
    let mut old_changed = vec![true; n];
    let mut new_changed = vec![true; m];
    let mut shared = 0usize;
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i].text == b[j].text {
            old_changed[i] = false;
            new_changed[j] = false;
            if !is_blank(a[i].text) {
                shared += utf16_len(a[i].text);
            }
            i += 1;
            j += 1;
        } else if lengths[(i + 1) * width + j] >= lengths[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    let shorter = content_length(&a).min(content_length(&b));
    if shared == 0 || (shared as f64) < shorter as f64 * MIN_SHARED_RATIO {
        return InlineDiffResult::default();
    }
    InlineDiffResult {
        old: to_ranges(&a, &old_changed, granularity),
        new: to_ranges(&b, &new_changed, granularity),
    }
}
