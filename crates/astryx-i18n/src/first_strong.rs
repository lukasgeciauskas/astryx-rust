//! The direction of a text's first strongly directional character, upstream
//! `i18n/firstStrongDirection.ts`: what gives a Markdown block the direction of
//! its own text. Upstream approximates the Unicode Bidi_Class from scripts and
//! categories; this reads the same sets.

use icu_properties::CodePointMapData;
use icu_properties::props::{GeneralCategory, GeneralCategoryGroup, Script};

use crate::Direction;

/// Bidi_Class `AN`, `EN`, `ET` and `ON` characters the scan passes over:
/// digits, the Arabic number signs and marks, and the digits and marks inside
/// the supplementary right-to-left ranges (upstream `WEAK_NUMBER_OR_NEUTRAL`).
const WEAK_NUMBER_OR_NEUTRAL: &[(u32, u32)] = &[
    (0x30, 0x39),
    (0x0600, 0x0605),
    (0x0609, 0x060A),
    (0x0660, 0x0669),
    (0x066A, 0x066C),
    (0x06DD, 0x06DD),
    (0x06F0, 0x06F9),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0xFF10, 0xFF19),
    (0x1091F, 0x1091F),
    (0x10B39, 0x10B3F),
    (0x10D30, 0x10D39),
    (0x10D40, 0x10D49),
    (0x10D6E, 0x10D6E),
    (0x10E60, 0x10E7E),
    (0x10ED0, 0x10ED8),
    (0x1CCF0, 0x1CCF9),
    (0x1D7CE, 0x1D7FF),
    (0x1EEF0, 0x1EEF1),
    (0x1FBF0, 0x1FBF9),
];

fn in_ranges(c: char, ranges: &[(u32, u32)]) -> bool {
    let c = c as u32;
    ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&c))
}

/// Upstream `STRONG_RTL`: Bidi_Class `R` and `AL`, from the living
/// right-to-left scripts, four `Common` characters, and the historic
/// supplementary ranges.
fn strong_rtl(c: char) -> bool {
    if matches!(
        c,
        '\u{061B}' | '\u{061C}' | '\u{061F}' | '\u{0640}' | '\u{200F}'
    ) || in_ranges(c, &[(0x10800, 0x10FFF), (0x1E800, 0x1EFFF)])
    {
        return true;
    }
    matches!(
        CodePointMapData::<Script>::new().get(c),
        Script::Adlam
            | Script::Arabic
            | Script::Hebrew
            | Script::Mandaic
            | Script::Nko
            | Script::Samaritan
            | Script::Syriac
            | Script::Thaana
    )
}

/// Upstream `STRONG_LTR`: letters, letter numbers, decimal digits and the
/// left-to-right mark.
fn strong_ltr(c: char, gc: GeneralCategory) -> bool {
    c == '\u{200E}'
        || GeneralCategoryGroup::Letter.contains(gc)
        || gc == GeneralCategory::LetterNumber
        || gc == GeneralCategory::DecimalNumber
}

/// The direction of `text`'s first strongly directional character, or `None`
/// when it has none. Marks, digits and neutrals are passed over.
pub fn first_strong_direction(text: &str) -> Option<Direction> {
    let categories = CodePointMapData::<GeneralCategory>::new();
    for c in text.chars() {
        let gc = categories.get(c);
        if GeneralCategoryGroup::Mark.contains(gc) || in_ranges(c, WEAK_NUMBER_OR_NEUTRAL) {
            continue;
        }
        if strong_rtl(c) {
            return Some(Direction::Rtl);
        }
        if strong_ltr(c, gc) {
            return Some(Direction::Ltr);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_first_strong_character() {
        assert_eq!(first_strong_direction("hello"), Some(Direction::Ltr));
        assert_eq!(first_strong_direction("123 שלום"), Some(Direction::Rtl));
        assert_eq!(first_strong_direction("  ١٢٣ مرحبا"), Some(Direction::Rtl));
        assert_eq!(first_strong_direction("\u{0640}abc"), Some(Direction::Rtl));
        assert_eq!(first_strong_direction("(!) 42"), None);
        assert_eq!(first_strong_direction("\u{0301}é"), Some(Direction::Ltr));
    }
}
