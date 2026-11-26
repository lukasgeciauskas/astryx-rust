//! Character references, upstream `Markdown/characterReferences.ts`: what an `&name;`,
//! `&#N;` or `&#xH;` reference names, and the decoders built on it. The named table is
//! generated from upstream's (`spec/markdown/character-references.json`).

use std::borrow::Cow;

use crate::generated::character_references::NAMED;

/// The longest HTML reference name, in characters, after its first letter.
const MAX_NAME_TAIL: usize = 31;

/// The characters a named reference (without `&` and `;`) stands for.
fn named_reference(name: &str) -> Option<&'static str> {
    NAMED
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| NAMED[i].1)
}

/// How many bytes of `text` from the start satisfy `test`, up to `max`.
fn run_length(text: &[u8], max: usize, test: impl Fn(u8) -> bool) -> usize {
    text.iter().take(max + 1).take_while(|&&b| test(b)).count()
}

/// Upstream `matchCharacterReference`: the reference that starts at byte `start` of
/// `text` (at its `&`), as the characters it names and the byte index just past it; or
/// `None` when the text there is no reference: an unknown name, a missing `;`, or too many
/// digits. It reads `&(?:#[xX]([0-9a-fA-F]{1,6})|#([0-9]{1,7})|([A-Za-z][A-Za-z0-9]{0,31}));`
/// (CommonMark 0.31). NUL, surrogates and values past Unicode name U+FFFD.
pub fn match_character_reference(text: &str, start: usize) -> Option<(Cow<'static, str>, usize)> {
    let rest = text.as_bytes().get(start..)?.strip_prefix(b"&")?;
    if let Some(number) = rest.strip_prefix(b"#") {
        let (digits, radix, max, prefix) = match number.first() {
            Some(b'x' | b'X') => (&number[1..], 16, 6, 3),
            _ => (number, 10, 7, 2),
        };
        let test: fn(u8) -> bool = if radix == 16 {
            |b| b.is_ascii_hexdigit()
        } else {
            |b| b.is_ascii_digit()
        };
        let length = run_length(digits, max, test);
        if length == 0 || length > max || digits.get(length) != Some(&b';') {
            return None;
        }
        // At most seven ASCII digits: always a valid u32.
        let digits = std::str::from_utf8(&digits[..length]).ok()?;
        let code_point = u32::from_str_radix(digits, radix).ok()?;
        let value = match char::from_u32(code_point) {
            Some(c) if code_point != 0 => c,
            _ => '\u{FFFD}',
        };
        return Some((Cow::Owned(value.to_string()), start + prefix + length + 1));
    }
    if !rest.first().is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let length = 1 + run_length(&rest[1..], MAX_NAME_TAIL, |b| b.is_ascii_alphanumeric());
    if length > MAX_NAME_TAIL + 1 || rest.get(length) != Some(&b';') {
        return None;
    }
    let name = std::str::from_utf8(&rest[..length]).ok()?;
    named_reference(name).map(|value| (Cow::Borrowed(value), start + 1 + length + 1))
}

/// Upstream `isAsciiPunctuation`: the characters a backslash escapes (CommonMark 0.31).
pub fn is_ascii_punctuation(character: char) -> bool {
    character.is_ascii_punctuation()
}

/// Upstream `decodeLiteralText`: `text` with backslash escapes and character references
/// resolved the way inline parsing resolves them, for text shown as written rather than
/// parsed as Markdown, such as an image's alt text. An escaped `&` stays literal.
pub fn decode_literal_text(text: &str) -> Cow<'_, str> {
    if !text.contains(['\\', '&']) {
        return Cow::Borrowed(text);
    }
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if bytes[index] == b'\\'
            && bytes
                .get(index + 1)
                .is_some_and(|&b| is_ascii_punctuation(b.into()))
        {
            output.push(bytes[index + 1] as char);
            index += 2;
            continue;
        }
        if bytes[index] == b'&'
            && let Some((value, end)) = match_character_reference(text, index)
        {
            output.push_str(&value);
            index = end;
            continue;
        }
        let c = text[index..].chars().next().unwrap_or_default();
        output.push(c);
        index += c.len_utf8();
    }
    Cow::Owned(output)
}

/// Upstream `decodeMarkdownCharacterReferences`: `text` with every valid named or numeric
/// character reference replaced by the characters it names, as Markdown renders it:
/// `&copy;`, `&#169;` and `&#xA9;` all become `©`. An unknown name, a reference without
/// its `;`, and all other text stay as written. It knows nothing of Markdown, so the caller
/// keeps escaped references and code literal by not passing them in.
pub fn decode_markdown_character_references(text: &str) -> Cow<'_, str> {
    let mut output = String::new();
    let mut index = 0;
    let mut ampersand = text.find('&');
    while let Some(at) = ampersand {
        match match_character_reference(text, at) {
            None => ampersand = text[at + 1..].find('&').map(|i| at + 1 + i),
            Some((value, end)) => {
                output.push_str(&text[index..at]);
                output.push_str(&value);
                index = end;
                ampersand = text[index..].find('&').map(|i| index + i);
            }
        }
    }
    if index == 0 {
        Cow::Borrowed(text)
    } else {
        output.push_str(&text[index..]);
        Cow::Owned(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_and_numeric_references_decode() {
        assert_eq!(
            decode_markdown_character_references("&amp; &copy; &#65;&#x42; &bogus; &copy"),
            "& © AB &bogus; &copy"
        );
    }

    #[test]
    fn invalid_code_points_become_the_replacement_character() {
        assert_eq!(
            decode_markdown_character_references("&#0;&#xD800;&#x110000;"),
            "\u{FFFD}\u{FFFD}\u{FFFD}"
        );
    }

    #[test]
    fn references_are_bounded() {
        // Seven hex digits, eight decimal digits, a name starting with a digit.
        assert_eq!(match_character_reference("&#x1234567;", 0), None);
        assert_eq!(match_character_reference("&#12345678;", 0), None);
        assert_eq!(match_character_reference("&1amp;", 0), None);
        assert_eq!(
            match_character_reference("a&amp;", 1),
            Some((Cow::Borrowed("&"), 6))
        );
    }

    #[test]
    fn literal_text_resolves_escapes_and_references() {
        assert_eq!(
            decode_literal_text(r"\*a\* \&amp; &amp; \q"),
            r"*a* &amp; & \q"
        );
    }
}
