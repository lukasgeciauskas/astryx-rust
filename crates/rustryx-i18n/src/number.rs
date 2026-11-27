//! Reading the numbers people type, upstream `NumberInput/numberParser.ts`
//! and `utils/unicodeDigits.ts`.
//!
//! A number reads in the locale's own form first (its grouping, decimal
//! separator and digits), then as a plain machine number (`1234.5`), then
//! under a separator the text itself makes unambiguous (`1,234,567` in a
//! locale that groups with periods). Signs may lead or trail, accounting
//! parentheses negate, a currency symbol at either end is ignored, and an
//! exponent (`1.5e3`) scales.

use std::borrow::Cow;

use icu_normalizer::ComposingNormalizerBorrowed;
use icu_properties::CodePointMapData;
use icu_properties::props::GeneralCategory;

/// A locale's number symbols, read from how it formats numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberSymbols {
    /// The grouping separator, NFKC-normalized as typed text is.
    pub group: String,
    pub decimal: String,
    /// The size of the group nearest the decimal separator (3 in `1,234`).
    pub primary_group: usize,
    /// The size of the groups above it (2 in Indian `12,34,567`).
    pub secondary_group: usize,
}

impl NumberSymbols {
    /// Reads the symbols from the locale's formatting of 12345.6 and of
    /// 11111111111, as upstream reads `Intl.NumberFormat`'s parts.
    pub(crate) fn from_format(format: impl Fn(f64) -> String) -> Self {
        let runs = |text: &str| -> Vec<(bool, String)> {
            let plain = to_ascii_digits(&strip_invisible(&nfkc(text))).unwrap_or_default();
            let mut runs: Vec<(bool, String)> = Vec::new();
            for ch in plain.chars() {
                let digit = ch.is_ascii_digit();
                match runs.last_mut() {
                    Some((is_digit, run)) if *is_digit == digit => run.push(ch),
                    _ => runs.push((digit, ch.to_string())),
                }
            }
            runs
        };
        let sample = runs(&format(12345.6));
        let separators: Vec<&String> = sample
            .iter()
            .filter(|(digit, _)| !digit)
            .map(|(_, s)| s)
            .collect();
        let (group, decimal) = match separators.as_slice() {
            [group, decimal] => ((*group).clone(), (*decimal).clone()),
            [decimal] => (",".to_string(), (*decimal).clone()),
            _ => (",".to_string(), ".".to_string()),
        };
        let sizes: Vec<usize> = runs(&format(11111111111.))
            .iter()
            .filter(|(digit, _)| *digit)
            .map(|(_, s)| s.len())
            .collect();
        let (primary_group, secondary_group) = if sizes.len() > 1 {
            (sizes[sizes.len() - 1], sizes[sizes.len() - 2])
        } else {
            (3, 3)
        };
        NumberSymbols {
            group,
            decimal,
            primary_group,
            secondary_group,
        }
    }
}

pub(crate) fn nfkc(text: &str) -> Cow<'_, str> {
    ComposingNormalizerBorrowed::new_nfkc().normalize(text)
}

/// Upstream `INVISIBLE_CHARS`: zero-width and bidirectional marks that
/// copied numbers carry.
pub(crate) fn is_invisible(ch: char) -> bool {
    matches!(
        ch,
        '\u{200B}'..='\u{200F}' | '\u{061C}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
    )
}

pub(crate) fn strip_invisible(text: &str) -> String {
    text.chars().filter(|&ch| !is_invisible(ch)).collect()
}

fn is_decimal_digit(ch: char) -> bool {
    CodePointMapData::<GeneralCategory>::new().get(ch) == GeneralCategory::DecimalNumber
}

fn is_currency(ch: char) -> bool {
    CodePointMapData::<GeneralCategory>::new().get(ch) == GeneralCategory::CurrencySymbol
}

/// A decimal digit's value in any script: its distance from the zero that
/// starts its run of ten, upstream `digitValue`.
pub fn digit_value(ch: char) -> Option<u32> {
    if !is_decimal_digit(ch) {
        return None;
    }
    let code = ch as u32;
    let mut zero = code;
    while code - zero < 10 {
        let before = zero.checked_sub(1).and_then(char::from_u32);
        if !before.is_some_and(is_decimal_digit) {
            return Some(code - zero);
        }
        zero -= 1;
    }
    None
}

/// Every decimal digit as its ASCII digit, upstream `toAsciiDigits`;
/// `None` when a digit's value is ambiguous.
pub fn to_ascii_digits(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_digit() || !is_decimal_digit(ch) {
            out.push(ch);
        } else {
            out.push(char::from_digit(digit_value(ch)?, 10)?);
        }
    }
    Some(out)
}

const MINUS_SIGNS: [char; 4] = ['\u{2212}', '\u{2012}', '\u{2013}', '-'];
/// The separators a typed number may use, upstream `SEPARATOR_CHARS`.
const SEPARATORS: [char; 7] = [',', '.', '\'', ' ', '\u{2019}', '\u{066B}', '\u{066C}'];

fn all_digits(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii_digit())
}

/// `^(?:\d+\.?\d*|\.\d+)$`
fn is_machine_number(text: &str) -> bool {
    match text.split_once('.') {
        None => !text.is_empty() && all_digits(text),
        Some((int, frac)) => {
            all_digits(int) && all_digits(frac) && !(int.is_empty() && frac.is_empty())
        }
    }
}

/// Reads `core` under one grouping and decimal separator, upstream
/// `readUnder`: the plain number, or `None` when the grouping is off.
fn read_under(
    core: &str,
    group: &str,
    decimal: &str,
    primary: usize,
    secondary: usize,
    min_groups: usize,
) -> Option<String> {
    let (int_text, frac_text) = match (!decimal.is_empty()).then(|| core.find(decimal)).flatten() {
        Some(at) => {
            let frac = &core[at + decimal.len()..];
            if frac.contains(decimal) || !all_digits(frac) {
                return None;
            }
            (&core[..at], Some(frac))
        }
        None => (core, None),
    };
    let digits = if !group.is_empty() && int_text.contains(group) {
        let chunks: Vec<&str> = int_text.split(group).collect();
        if chunks.len() - 1 < min_groups || !chunks.iter().all(|c| !c.is_empty() && all_digits(c)) {
            return None;
        }
        let (first, last) = (chunks[0], chunks[chunks.len() - 1]);
        if last.len() != primary || first.len() > secondary {
            return None;
        }
        if chunks[1..chunks.len() - 1]
            .iter()
            .any(|c| c.len() != secondary)
        {
            return None;
        }
        chunks.concat()
    } else {
        if !all_digits(int_text) {
            return None;
        }
        int_text.to_string()
    };
    if digits.is_empty() && frac_text.is_none_or(str::is_empty) {
        return None;
    }
    Some(match frac_text {
        None => digits,
        Some(frac) => format!("{digits}.{frac}"),
    })
}

fn is_group_only(separator: char) -> bool {
    separator.is_whitespace() || separator == '\'' || separator == '\u{2019}'
}

/// The digits and separators as a plain number, upstream `toPlainNumber`.
fn to_plain_number(core: &str, symbols: &NumberSymbols) -> Option<String> {
    if core.is_empty()
        || !core
            .chars()
            .all(|ch| ch.is_ascii_digit() || SEPARATORS.contains(&ch))
    {
        return None;
    }
    if let Some(read) = read_under(
        core,
        &symbols.group,
        &symbols.decimal,
        symbols.primary_group,
        symbols.secondary_group,
        1,
    ) {
        return Some(read);
    }
    if is_machine_number(core) {
        return Some(core.to_string());
    }
    let mut separators: Vec<char> = Vec::new();
    for ch in core.chars().filter(|ch| !ch.is_ascii_digit()) {
        if !separators.contains(&ch) {
            separators.push(ch);
        }
    }
    if separators.is_empty() || separators.len() > 2 {
        return None;
    }
    for &candidate in &separators {
        let occurrences = core.matches(candidate).count();
        let min_groups = if is_group_only(candidate) { 1 } else { 2 };
        if occurrences < min_groups {
            continue;
        }
        let other = separators.iter().find(|&&s| s != candidate);
        if let Some(&other) = other
            && core.matches(other).count() != 1
        {
            continue;
        }
        let other = other.map(|c| c.to_string()).unwrap_or_default();
        if let Some(read) = read_under(core, &candidate.to_string(), &other, 3, 3, min_groups) {
            return Some(read);
        }
    }
    None
}

fn strip_currency(text: &str) -> &str {
    let mut out = text.trim();
    if out.chars().next().is_some_and(is_currency) {
        let first = out.chars().next().map_or(0, char::len_utf8);
        out = out[first..].trim();
    }
    if out.chars().next_back().is_some_and(is_currency) {
        let last = out.chars().next_back().map_or(0, char::len_utf8);
        out = out[..out.len() - last].trim();
    }
    out
}

/// Splits a trailing exponent (`e3`, `E-2`, with optional spaces).
fn split_exponent(text: &str) -> (&str, i32) {
    let digits_start = text.trim_end_matches(|c: char| c.is_ascii_digit());
    if digits_start.len() == text.len() {
        return (text, 0);
    }
    let digits = &text[digits_start.len()..];
    let (before_sign, signed) = match digits_start.strip_suffix(['+', '-']) {
        Some(rest) => (rest, &text[rest.len()..]),
        None => (digits_start, digits),
    };
    let before_e = before_sign.trim_end();
    let Some(mantissa) = before_e.strip_suffix(['e', 'E']) else {
        return (text, 0);
    };
    match signed.parse::<i32>() {
        Ok(exponent) => (mantissa.trim_end(), exponent),
        Err(_) => (text, 0),
    }
}

/// Reads a typed number in a locale, upstream `parseLocaleNumber`: `None`
/// when the text is not one number.
pub fn parse_number(text: &str, symbols: &NumberSymbols) -> Option<f64> {
    let normalized = to_ascii_digits(&strip_invisible(&nfkc(text)))?;
    let mut rest = normalized.trim();
    let mut sign = 1.0;
    let accounting = rest.len() >= 2 && rest.starts_with('(') && rest.ends_with(')');
    if accounting {
        sign = -1.0;
        rest = rest[1..rest.len() - 1].trim();
    }
    rest = strip_currency(rest);
    let mut written_sign = false;
    if let Some(first) = rest.chars().next()
        && (first == '+' || MINUS_SIGNS.contains(&first))
    {
        written_sign = true;
        if first != '+' {
            sign = -sign;
        }
        rest = rest[first.len_utf8()..].trim_start();
    } else if let Some(last) = rest.chars().next_back()
        && (last == '+' || MINUS_SIGNS.contains(&last))
    {
        written_sign = true;
        if last != '+' {
            sign = -sign;
        }
        rest = rest[..rest.len() - last.len_utf8()].trim_end();
    }
    if accounting && written_sign {
        return None;
    }
    rest = strip_currency(rest);
    let (rest, exponent) = split_exponent(rest);
    let core = rest.trim();
    if core.is_empty() {
        return None;
    }
    let magnitude: f64 = to_plain_number(core, symbols)?.parse().ok()?;
    if !magnitude.is_finite() {
        return None;
    }
    let scaled = if exponent == 0 {
        magnitude
    } else {
        // Round away binary noise: 1.23e3 is 1230, not 1229.9999999999998.
        format!("{:.14e}", magnitude * 10f64.powi(exponent))
            .parse()
            .ok()?
    };
    if !f64::is_finite(scaled) {
        return None;
    }
    let result = sign * scaled;
    Some(if result == 0.0 { 0.0 } else { result })
}

/// A number as a person edits it, upstream `formatEditableNumber`: plain
/// digits with the locale's decimal separator, no grouping.
pub fn editable_number(value: f64, symbols: &NumberSymbols) -> String {
    let raw = format!("{value}");
    if symbols.decimal == "." {
        raw
    } else {
        raw.replacen('.', &symbols.decimal, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en() -> NumberSymbols {
        NumberSymbols {
            group: ",".into(),
            decimal: ".".into(),
            primary_group: 3,
            secondary_group: 3,
        }
    }

    fn de() -> NumberSymbols {
        NumberSymbols {
            group: ".".into(),
            decimal: ",".into(),
            primary_group: 3,
            secondary_group: 3,
        }
    }

    #[test]
    fn reads_the_locale_form_first() {
        assert_eq!(parse_number("1,234.5", &en()), Some(1234.5));
        assert_eq!(parse_number("1.234,5", &de()), Some(1234.5));
        assert_eq!(parse_number("1,234,234,234", &en()), Some(1234234234.));
    }

    #[test]
    fn reads_machine_numbers_and_unambiguous_grouping() {
        assert_eq!(parse_number("1234.5", &de()), Some(1234.5));
        // Two commas can only be grouping, even where commas are decimals.
        assert_eq!(parse_number("1,234,567", &de()), Some(1234567.));
        assert_eq!(parse_number(".5", &en()), Some(0.5));
        assert_eq!(parse_number("5.", &en()), Some(5.));
    }

    #[test]
    fn rejects_broken_grouping_and_stray_text() {
        assert_eq!(parse_number("1,23", &en()), None);
        assert_eq!(parse_number("12a", &en()), None);
        assert_eq!(parse_number("1.2.3", &en()), None);
        assert_eq!(parse_number("-", &en()), None);
        assert_eq!(parse_number("", &en()), None);
    }

    #[test]
    fn reads_signs_currency_and_exponents() {
        assert_eq!(parse_number("-12", &en()), Some(-12.));
        assert_eq!(parse_number("\u{2212}12", &en()), Some(-12.));
        assert_eq!(parse_number("12-", &en()), Some(-12.));
        assert_eq!(parse_number("(12)", &en()), Some(-12.));
        assert_eq!(parse_number("(-12)", &en()), None);
        assert_eq!(parse_number("$1,200", &en()), Some(1200.));
        assert_eq!(parse_number("1.23e3", &en()), Some(1230.));
        assert_eq!(parse_number("5 E -1", &en()), Some(0.5));
        assert_eq!(parse_number("-0", &en()), Some(0.));
    }

    #[test]
    fn reads_other_digit_scripts() {
        // Arabic-Indic and Devanagari digits.
        assert_eq!(to_ascii_digits("\u{0661}\u{0662}").as_deref(), Some("12"));
        assert_eq!(parse_number("\u{0967}\u{0968}\u{0969}", &en()), Some(123.));
        // Fullwidth digits normalize under NFKC.
        assert_eq!(parse_number("\u{FF14}\u{FF12}", &en()), Some(42.));
    }

    #[test]
    fn indian_grouping_reads_under_its_own_sizes() {
        let hi = NumberSymbols {
            group: ",".into(),
            decimal: ".".into(),
            primary_group: 3,
            secondary_group: 2,
        };
        assert_eq!(parse_number("12,34,567", &hi), Some(1234567.));
    }

    #[test]
    fn symbols_come_from_the_format() {
        let symbols = NumberSymbols::from_format(|v| {
            if v == 12345.6 {
                "12.345,6".into()
            } else {
                "11.111.111.111".into()
            }
        });
        assert_eq!(symbols, de());
    }

    #[test]
    fn real_locales_read_their_own_numbers() {
        let de = crate::I18n::new("de");
        assert_eq!(de.parse_number("1.234,5"), Some(1234.5));
        assert_eq!(de.editable_number(0.5), "0,5");
        // French groups with a narrow no-break space; a typed space works too.
        let fr = crate::I18n::new("fr");
        assert_eq!(fr.parse_number(&fr.number(1234.5)), Some(1234.5));
        assert_eq!(fr.parse_number("1 234,5"), Some(1234.5));
        let hi = crate::I18n::new("hi");
        assert_eq!(hi.number_symbols().secondary_group, 2);
        assert_eq!(hi.parse_number("12,34,567"), Some(1234567.));
        let ar = crate::I18n::new("ar-EG");
        assert_eq!(ar.parse_number(&ar.number(1234.5)), Some(1234.5));
    }

    #[test]
    fn lists_join_in_the_locale() {
        let en = crate::I18n::new("en");
        assert_eq!(en.conjunction(["a", "b", "c"]), "a, b, c");
        assert_eq!(en.conjunction(["a"]), "a");
        let de = crate::I18n::new("de");
        assert_eq!(de.conjunction(["a", "b"]), "a und b");
    }

    #[test]
    fn editable_numbers_use_the_decimal_separator() {
        assert_eq!(editable_number(1234.5, &de()), "1234,5");
        assert_eq!(editable_number(7., &en()), "7");
    }
}
