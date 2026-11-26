//! What the inline scan of one text asks about it again and again, upstream
//! `Markdown/parser.ts` (`inlineIndexOf`, `angleDestinationClose`): code-span closers,
//! bracket and parenthesis partners, and where links close, each indexed once, when first
//! asked, so no question rescans the text.

use std::collections::HashMap;

use super::link_definitions::normalize_link_label;
use super::{LinkDefinitions, MAX_LINK_LABEL_LENGTH};
use crate::js;
use crate::url::is_safe_markdown_parser_url;

/// Where an inline link's or image's destination ends when it opens with `<`.
enum AngleDestination {
    /// The content after the `(` does not open with `<`.
    None,
    /// It opens with `<` but is no angle-bracket destination: no link forms.
    Refused,
    /// The index of the `)` that closes the link.
    Close(usize),
}

/// The length of the run of `byte` at `index`.
fn run_length(bytes: &[u8], index: usize, byte: u8) -> usize {
    bytes[index..].iter().take_while(|&&b| b == byte).count()
}

/// Upstream `angleDestinationClose`. Inside the brackets parentheses are plain
/// characters, so `<b(c>` is the destination `b(c`. A destination that opens with `<`
/// must be one whole angle-bracket destination (no line ending, even escaped, and no
/// unescaped `<` inside), then only spaces, an optional title, and the `)`
/// (CommonMark 0.31 §6.3).
fn angle_destination_close(text: &str, open: usize) -> AngleDestination {
    let bytes = text.as_bytes();
    let skip_whitespace = |mut index: usize| {
        while let Some(c) = js::char_at(text, index).filter(|&c| js::is_whitespace(c)) {
            index += c.len_utf8();
        }
        index
    };
    let mut index = open + 1;
    while matches!(bytes.get(index), Some(b' ' | b'\t')) {
        index += 1;
    }
    if bytes.get(index) != Some(&b'<') {
        return AngleDestination::None;
    }
    index += 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if !matches!(bytes.get(index + 1), Some(b'\n' | b'\r')) => index += 1,
            b'\n' | b'\r' | b'<' => return AngleDestination::Refused,
            b'>' => break,
            _ => {}
        }
        index += 1;
    }
    if index >= bytes.len() {
        return AngleDestination::Refused;
    }
    // After the brackets: spaces, an optional title, spaces, and the `)`.
    index = skip_whitespace(index + 1);
    if let Some(&quote @ (b'"' | b'\'' | b'(')) = bytes.get(index) {
        let closer = if quote == b'(' { b')' } else { quote };
        index += 1;
        while index < bytes.len() && bytes[index] != closer {
            if bytes[index] == b'\\' {
                index += 1;
            } else if quote == b'(' && bytes[index] == b'(' {
                // A title in parentheses holds no unescaped `(`.
                return AngleDestination::Refused;
            }
            index += 1;
        }
        index = skip_whitespace(index + 1);
    }
    if bytes.get(index) == Some(&b')') {
        AngleDestination::Close(index)
    } else {
        AngleDestination::Refused
    }
}

/// Upstream `InlineIndex`, for one text.
pub(super) struct InlineIndex<'a> {
    text: &'a str,
    link_defs: Option<&'a LinkDefinitions>,
    /// The starts of the backtick strings of each length, in order.
    backtick_starts: Option<HashMap<usize, Vec<usize>>>,
    /// For each index, the first `]` at or after it that can close.
    next_closing_bracket: Option<Vec<Option<usize>>>,
    /// For each `(`, the `)` that closes it.
    paren_partners: Option<Vec<Option<usize>>>,
    /// For each `[` that opens a link or image, the `]` that closes it.
    opener_closes: Option<HashMap<usize, usize>>,
}

impl<'a> InlineIndex<'a> {
    pub(super) fn new(text: &'a str, link_defs: Option<&'a LinkDefinitions>) -> Self {
        InlineIndex {
            text,
            link_defs,
            backtick_starts: None,
            next_closing_bracket: None,
            paren_partners: None,
            opener_closes: None,
        }
    }

    /// Where the backtick string of `length` that closes a code span opened before `from`
    /// starts: the first backtick string of exactly that length at or after `from`
    /// (CommonMark 0.31 §6.1).
    pub(super) fn backtick_closer(&mut self, from: usize, length: usize) -> Option<usize> {
        let text = self.text;
        let starts = self.backtick_starts.get_or_insert_with(|| {
            let bytes = text.as_bytes();
            let mut starts: HashMap<usize, Vec<usize>> = HashMap::new();
            let mut next = text.find('`');
            while let Some(start) = next {
                let end = start + run_length(bytes, start, b'`');
                starts.entry(end - start).or_default().push(start);
                next = text[end..].find('`').map(|i| end + i);
            }
            starts
        });
        let same_length = starts.get(&length)?;
        same_length
            .get(same_length.partition_point(|&start| start < from))
            .copied()
    }

    /// Where the scan resumes past the backtick string at `index`: past its code span when
    /// one closes, else past the string.
    fn past_backticks(&mut self, index: usize) -> usize {
        let length = run_length(self.text.as_bytes(), index, b'`');
        let run_end = index + length;
        match self.backtick_closer(run_end, length) {
            Some(closer) => closer + length,
            None => run_end,
        }
    }

    /// The unescaped `]` at or after `from` that closes link text or a label. A code span
    /// hides its brackets: code spans bind tighter than links (CommonMark 0.31 §6.1).
    pub(super) fn closing_bracket(&mut self, from: usize) -> Option<usize> {
        if self.next_closing_bracket.is_none() {
            let bytes = self.text.as_bytes();
            let mut closes = vec![false; bytes.len()];
            let mut index = 0;
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' => index += 1,
                    b'`' => index = self.past_backticks(index) - 1,
                    b']' => closes[index] = true,
                    _ => {}
                }
                index += 1;
            }
            let mut next = vec![None; bytes.len() + 1];
            for index in (0..bytes.len()).rev() {
                next[index] = if closes[index] {
                    Some(index)
                } else {
                    next[index + 1]
                };
            }
            self.next_closing_bracket = Some(next);
        }
        let next = self.next_closing_bracket.as_ref()?;
        if from < self.text.len() {
            next[from]
        } else {
            None
        }
    }

    /// The `)` that closes the `(` at `open`, by nesting and skipping escaped
    /// parentheses: where an inline link's or image's destination ends.
    pub(super) fn closing_paren(&mut self, open: usize) -> Option<usize> {
        let bytes = self.text.as_bytes();
        let partners = self.paren_partners.get_or_insert_with(|| {
            let mut partners = vec![None; bytes.len()];
            let mut opens = Vec::new();
            let mut index = 0;
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' => index += 1,
                    b'(' => opens.push(index),
                    b')' => {
                        if let Some(opened) = opens.pop() {
                            partners[opened] = Some(index);
                        }
                    }
                    _ => {}
                }
                index += 1;
            }
            partners
        });
        if bytes.get(open) == Some(&b'(') {
            partners[open]
        } else {
            None
        }
    }

    /// The `)` that closes an inline link's or image's destination opened by the `(` at
    /// `open`, with or without angle brackets.
    pub(super) fn destination_close(&mut self, open: usize) -> Option<usize> {
        match angle_destination_close(self.text, open) {
            AngleDestination::Refused => None,
            AngleDestination::Close(close) => Some(close),
            AngleDestination::None => self.closing_paren(open),
        }
    }

    /// Where the link or image whose text opens at `open` and closes at `close` ends (past
    /// its destination or label), or `None` when nothing after the `]` makes one: the same
    /// checks the inline scan makes when it builds the node.
    fn link_end(&mut self, open: usize, close: usize) -> Option<usize> {
        let text = self.text;
        let bytes = text.as_bytes();
        if bytes.get(close + 1) == Some(&b'(')
            && let Some(url_close) = self.destination_close(close + 1)
        {
            return Some(url_close + 1);
        }
        let link_defs = self.link_defs?;
        let link_text = &text[open + 1..close];
        if js::longer_than_utf16(link_text, MAX_LINK_LABEL_LENGTH) {
            return None;
        }
        let defines = |label: &str| {
            link_defs
                .get(&normalize_link_label(label))
                .is_some_and(|href| is_safe_markdown_parser_url(href))
        };
        if bytes.get(close + 1) == Some(&b'[')
            && let Some(label_close) = self.closing_bracket(close + 2)
        {
            let label = &text[close + 2..label_close];
            if !js::longer_than_utf16(label, MAX_LINK_LABEL_LENGTH)
                && defines(if label.is_empty() { link_text } else { label })
            {
                return Some(label_close + 1);
            }
        }
        (!js::is_blank(link_text) && defines(link_text)).then_some(close + 1)
    }

    /// The `]` that closes the link or image opened by the `[` at `open`, or `None` when
    /// no link or image forms there. Brackets pair as CommonMark 0.31 §6.3 pairs them:
    /// each `]` closes the nearest open `[`, so link text may hold balanced brackets and
    /// `[a [b](u)` links only `b`; a link inside link text wins, so the outer brackets
    /// stay text and links never nest. Images may hold links.
    pub(super) fn opener_close(&mut self, open: usize) -> Option<usize> {
        if self.opener_closes.is_none() {
            // One pass in text order, as CommonMark's bracket stack runs: a `]` closes the
            // nearest open `[`; when that makes a link, every `[` still open before it can
            // no longer make one.
            let bytes = self.text.as_bytes();
            let mut closes = HashMap::new();
            let mut openers: Vec<(usize, bool)> = Vec::new();
            let mut inactive_below = 0;
            let mut index = 0;
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' => index += 1,
                    b'`' => index = self.past_backticks(index) - 1,
                    b'!' if bytes.get(index + 1) == Some(&b'[') => {
                        openers.push((index + 1, true));
                        index += 1;
                    }
                    b'[' => openers.push((index, false)),
                    b']' if !openers.is_empty() => {
                        let position = openers.len() - 1;
                        let (opener, image) = openers[position];
                        openers.pop();
                        let active = image || position >= inactive_below;
                        inactive_below = inactive_below.min(position);
                        if active && let Some(end) = self.link_end(opener, index) {
                            closes.insert(opener, index);
                            if !image {
                                inactive_below = openers.len();
                            }
                            // The destination or label is no part of any bracket pairing.
                            index = end - 1;
                        }
                    }
                    _ => {}
                }
                index += 1;
            }
            self.opener_closes = Some(closes);
        }
        self.opener_closes.as_ref()?.get(&open).copied()
    }
}
