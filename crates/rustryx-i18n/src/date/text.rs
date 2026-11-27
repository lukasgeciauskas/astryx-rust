//! Character classes and the text normalization the date and time parsers
//! share: upstream's `\p{...}` regular expressions, from ICU4X's Unicode
//! properties.

use icu_normalizer::{ComposingNormalizerBorrowed, DecomposingNormalizerBorrowed};
use icu_properties::CodePointMapData;
use icu_properties::props::{GeneralCategory, GeneralCategoryGroup, Script};

use crate::number::{is_invisible, nfkc, to_ascii_digits};

fn category(ch: char) -> GeneralCategory {
    CodePointMapData::<GeneralCategory>::new().get(ch)
}

/// `[\p{L}\p{M}]`.
pub(crate) fn is_letter(ch: char) -> bool {
    let gc = category(ch);
    GeneralCategoryGroup::Letter.contains(gc) || GeneralCategoryGroup::Mark.contains(gc)
}

/// `[\p{P}\p{S}]`.
pub(crate) fn is_separator(ch: char) -> bool {
    let gc = category(ch);
    GeneralCategoryGroup::Punctuation.contains(gc) || GeneralCategoryGroup::Symbol.contains(gc)
}

/// JavaScript's `\s`.
pub(crate) fn is_space(ch: char) -> bool {
    ch.is_whitespace() || ch == '\u{feff}'
}

/// The scripts written without spaces between words.
pub(crate) fn is_spaceless(ch: char) -> bool {
    matches!(
        CodePointMapData::<Script>::new().get(ch),
        Script::Han
            | Script::Hiragana
            | Script::Katakana
            | Script::Hangul
            | Script::Thai
            | Script::Lao
            | Script::Khmer
            | Script::Myanmar
    )
}

fn is_latin(ch: char) -> bool {
    CodePointMapData::<Script>::new().get(ch) == Script::Latin
}

/// Lower case as `toLocaleLowerCase(locale)` writes it: Turkish and
/// Azerbaijani keep the dotless i.
pub(crate) fn locale_lowercase(text: &str, locale: &str) -> String {
    let language = locale
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(language.as_str(), "tr" | "az") {
        text.chars()
            .map(|ch| match ch {
                'I' => "ı".to_string(),
                '\u{130}' => "i".to_string(),
                ch => ch.to_lowercase().collect(),
            })
            .collect()
    } else {
        text.to_lowercase()
    }
}

/// Text in one comparable form, upstream `normalizeDateText`: ASCII digits,
/// lower case, no Latin accents, no invisible or bidi marks, apostrophes and
/// quotes in their ASCII forms.
pub fn normalize_date_text(text: &str, locale: &str) -> String {
    if text.chars().all(|ch| (' '..='~').contains(&ch)) {
        let ascii = text.replace('`', "'");
        return if ascii.chars().any(|ch| ch.is_ascii_uppercase()) {
            locale_lowercase(&ascii, locale)
        } else {
            ascii
        };
    }
    let folded: String = nfkc(text)
        .chars()
        .filter(|&ch| !is_invisible(ch) && !('\u{202a}'..='\u{202e}').contains(&ch))
        .map(|ch| match ch {
            '\u{05f3}' | '\u{2018}' | '\u{2019}' | '\u{02bb}' | '\u{02bc}' | '\u{02bf}' | '`' => {
                '\''
            }
            '\u{05f4}' | '\u{201c}' | '\u{201d}' => '"',
            ch => ch,
        })
        .collect();
    let ascii = to_ascii_digits(&folded).unwrap_or(folded);
    let result = locale_lowercase(&ascii, locale);
    if !result.chars().any(|ch| {
        ('\u{00c0}'..='\u{024f}').contains(&ch) || ('\u{1e00}'..='\u{1eff}').contains(&ch)
    }) {
        return result;
    }
    // Fold Latin accents only: in other scripts a mark is part of the letter.
    let decomposed = DecomposingNormalizerBorrowed::new_nfd().normalize(&result);
    let mut out = String::with_capacity(decomposed.len());
    let mut after_latin = false;
    for ch in decomposed.chars() {
        let is_mark = category(ch) == GeneralCategory::NonspacingMark;
        if is_mark && after_latin {
            continue;
        }
        after_latin = is_latin(ch) || (after_latin && is_mark);
        out.push(ch);
    }
    ComposingNormalizerBorrowed::new_nfc()
        .normalize(&out)
        .into_owned()
}

/// Every run of digits, punctuation and symbols replaced by a space
/// (upstream `NOT_WORDS`).
pub(crate) fn words_only(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    for ch in text.chars() {
        if ch.is_ascii_digit() || is_separator(ch) {
            if !in_run {
                out.push(' ');
                in_run = true;
            }
        } else {
            out.push(ch);
            in_run = false;
        }
    }
    out
}

/// Every run of digits, punctuation and symbols removed.
pub(crate) fn without_non_words(text: &str) -> String {
    text.chars()
        .filter(|&ch| !(ch.is_ascii_digit() || is_separator(ch)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_folds_what_people_type() {
        assert_eq!(normalize_date_text("Mar 21, 2026", "en"), "mar 21, 2026");
        assert_eq!(normalize_date_text("Dhuʻl-Hijjah", "en"), "dhu'l-hijjah");
        assert_eq!(normalize_date_text("۱۴۰۵/۸/۸", "fa"), "1405/8/8");
        assert_eq!(normalize_date_text("Février", "fr"), "fevrier");
        assert_eq!(normalize_date_text("ISTANBUL", "tr"), "ıstanbul");
        assert_eq!(normalize_date_text("\u{200f}12", "ar"), "12");
    }
}
