//! The JavaScript string semantics upstream's parser (`Markdown/parser.ts`) relies on:
//! what `\s`, `trim()` and `.` mean in its regexes, and lengths counted in UTF-16
//! units. The port indexes strings by UTF-8 bytes; these helpers keep its character tests
//! equal to upstream's.

/// Whether `c` is whitespace to JavaScript: what `\s` matches and `trim()` removes
/// (ECMAScript WhiteSpace and LineTerminator). Unlike `char::is_whitespace`, it takes
/// U+FEFF and leaves U+0085.
pub(crate) fn is_whitespace(c: char) -> bool {
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

/// [`is_whitespace`] as the inside of a regex character class, for the regexes upstream
/// writes with `\s` or `\S`.
pub(crate) const WHITESPACE_CLASS: &str =
    r"\t\n\x0B\x0C\r \xA0\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

/// Whether `c` ends a line to JavaScript: what a regex `.` does not match.
fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `s.trim()`.
pub(crate) fn trim(s: &str) -> &str {
    s.trim_matches(is_whitespace)
}

/// `s.trimEnd()`.
pub(crate) fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_whitespace)
}

/// `s.trimStart()`.
pub(crate) fn trim_start(s: &str) -> &str {
    s.trim_start_matches(is_whitespace)
}

/// `s.trim() === ''`.
pub(crate) fn is_blank(s: &str) -> bool {
    trim(s).is_empty()
}

/// What a regex `(.*)` captures at the start of `s`: the text before its first line
/// terminator.
pub(crate) fn up_to_line_terminator(s: &str) -> &str {
    &s[..s.find(is_line_terminator).unwrap_or(s.len())]
}

/// The character that ends just before byte `index`, if any.
pub(crate) fn char_before(text: &str, index: usize) -> Option<char> {
    text[..index].chars().next_back()
}

/// The character that starts at byte `index`, if any.
pub(crate) fn char_at(text: &str, index: usize) -> Option<char> {
    text.get(index..)?.chars().next()
}

/// Whether `s` is longer than `max` UTF-16 code units, as upstream's `.length` counts
/// it. A UTF-16 unit takes one to three UTF-8 bytes, so only lengths in between are
/// counted.
pub(crate) fn longer_than_utf16(s: &str, max: usize) -> bool {
    if s.len() <= max {
        false
    } else if s.len() > max * 3 {
        true
    } else {
        s.encode_utf16().count() > max
    }
}

/// Upstream `getIndent`: how many spaces open `line`.
pub(crate) fn get_indent(line: &str) -> usize {
    line.bytes().take_while(|&b| b == b' ').count()
}
