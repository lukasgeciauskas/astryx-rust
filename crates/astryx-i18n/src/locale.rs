//! A locale's formatting services and text direction, from ICU4X's CLDR
//! data: the same data behind upstream's `Intl.NumberFormat`,
//! `Intl.PluralRules` and `Intl.Locale.getTextInfo()`.

use icu_decimal::DecimalFormatter;
use icu_decimal::input::{Decimal, FloatPrecision};
use icu_decimal::options::{DecimalFormatterOptions, GroupingStrategy};
use icu_locale::{Locale, LocaleDirectionality};
use icu_plurals::{PluralCategory as IcuCategory, PluralRules};

use crate::message::{LocaleFormat, NumberStyle, PluralCategory};

/// The direction text and layout flow in, upstream `'ltr' | 'rtl'`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Left to right.
    #[default]
    Ltr,
    /// Right to left: layout and directional icons mirror.
    Rtl,
}

impl Direction {
    /// Whether this is [`Direction::Rtl`].
    pub fn is_rtl(self) -> bool {
        self == Direction::Rtl
    }
}

/// Parses a BCP 47 tag, falling back to the root locale for malformed input,
/// as upstream falls back to the raw string.
pub fn parse_locale(tag: &str) -> Locale {
    tag.parse().unwrap_or(Locale::UNKNOWN)
}

/// The direction of a locale's script, upstream `getLocaleDirection`.
pub fn locale_direction(tag: &str) -> Direction {
    let locale = parse_locale(tag);
    if LocaleDirectionality::new_common().is_right_to_left(&locale.id) {
        Direction::Rtl
    } else {
        Direction::Ltr
    }
}

/// The tags a message lookup walks, most specific first: `pt-BR` gives
/// `pt-BR` then `pt`. Upstream `resolveLocaleChain`.
pub fn locale_chain(tag: &str) -> Vec<String> {
    let canonical = match tag.parse::<Locale>() {
        // The base name: language, script, region and variants, no extensions.
        Ok(locale) => locale.id.to_string(),
        Err(_) => tag.to_string(),
    };
    let parts: Vec<&str> = canonical.split('-').collect();
    (1..=parts.len())
        .rev()
        .map(|n| parts[..n].join("-"))
        .collect()
}

/// Number and plural formatting for one locale.
pub struct LocaleInfo {
    decimal: DecimalFormatter,
    decimal_ungrouped: DecimalFormatter,
    cardinal: PluralRules,
    ordinal: PluralRules,
}

impl LocaleInfo {
    /// Loads the formatters for `tag`. Locales without data of their own use
    /// CLDR's fallback (`de-CH` → `de` → root).
    pub fn new(tag: &str) -> Self {
        let locale = parse_locale(tag);
        let decimal = |grouping| {
            DecimalFormatter::try_new((&locale).into(), DecimalFormatterOptions::from(grouping))
                .or_else(|_| {
                    DecimalFormatter::try_new(
                        Default::default(),
                        DecimalFormatterOptions::from(grouping),
                    )
                })
                .expect("compiled decimal data covers the root locale")
        };
        Self {
            decimal: decimal(GroupingStrategy::Auto),
            decimal_ungrouped: decimal(GroupingStrategy::Never),
            cardinal: PluralRules::try_new_cardinal((&locale).into())
                .or_else(|_| PluralRules::try_new_cardinal(Default::default()))
                .expect("compiled plural data covers the root locale"),
            ordinal: PluralRules::try_new_ordinal((&locale).into())
                .or_else(|_| PluralRules::try_new_ordinal(Default::default()))
                .expect("compiled plural data covers the root locale"),
        }
    }

    fn decimal(value: f64, style: NumberStyle) -> Option<Decimal> {
        let mut decimal = Decimal::try_from_f64(value, FloatPrecision::RoundTrip).ok()?;
        if let Some(digits) = style.fraction_digits {
            let position = -(digits as i16);
            decimal.round(position);
            decimal.absolute.pad_end(position);
        }
        Some(decimal)
    }
}

impl LocaleFormat for LocaleInfo {
    fn number(&self, value: f64, style: NumberStyle) -> String {
        let Some(decimal) = Self::decimal(value, style) else {
            // NaN and infinities: what `Intl.NumberFormat` prints.
            return if value.is_nan() {
                "NaN".into()
            } else if value > 0.0 {
                "∞".into()
            } else {
                "-∞".into()
            };
        };
        let formatter = if style.no_grouping {
            &self.decimal_ungrouped
        } else {
            &self.decimal
        };
        formatter.format_to_string(&decimal)
    }

    fn plural(&self, value: f64, ordinal: bool) -> PluralCategory {
        let Some(decimal) = Self::decimal(value, NumberStyle::default()) else {
            return PluralCategory::Other;
        };
        let rules = if ordinal {
            &self.ordinal
        } else {
            &self.cardinal
        };
        match rules.category_for(&decimal) {
            IcuCategory::Zero => PluralCategory::Zero,
            IcuCategory::One => PluralCategory::One,
            IcuCategory::Two => PluralCategory::Two,
            IcuCategory::Few => PluralCategory::Few,
            IcuCategory::Many => PluralCategory::Many,
            IcuCategory::Other => PluralCategory::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chains_walk_from_specific_to_general() {
        assert_eq!(locale_chain("pt-BR"), ["pt-BR", "pt"]);
        assert_eq!(locale_chain("zh-Hant-TW"), ["zh-Hant-TW", "zh-Hant", "zh"]);
        assert_eq!(locale_chain("en"), ["en"]);
    }

    #[test]
    fn direction_follows_the_script() {
        assert_eq!(locale_direction("ar-SA"), Direction::Rtl);
        assert_eq!(locale_direction("he-IL"), Direction::Rtl);
        assert_eq!(locale_direction("fa"), Direction::Rtl);
        assert_eq!(locale_direction("en"), Direction::Ltr);
        assert_eq!(locale_direction("ja-JP"), Direction::Ltr);
        // Azerbaijani in Arabic script, not Latin.
        assert_eq!(locale_direction("az-Arab"), Direction::Rtl);
    }

    #[test]
    fn numbers_use_locale_separators_and_digits() {
        let style = NumberStyle::default();
        assert_eq!(LocaleInfo::new("en").number(1234.5, style), "1,234.5");
        assert_eq!(LocaleInfo::new("de-DE").number(1234.5, style), "1.234,5");
        let ungrouped = NumberStyle {
            no_grouping: true,
            fraction_digits: Some(1),
        };
        assert_eq!(LocaleInfo::new("en").number(1536.04, ungrouped), "1536.0");
        assert_eq!(LocaleInfo::new("ar-SA").number(3.0, style), "٣");
    }

    #[test]
    fn plural_categories_follow_cldr() {
        let ru = LocaleInfo::new("ru-RU");
        assert_eq!(ru.plural(1.0, false), PluralCategory::One);
        assert_eq!(ru.plural(3.0, false), PluralCategory::Few);
        assert_eq!(ru.plural(5.0, false), PluralCategory::Many);
        let ar = LocaleInfo::new("ar-SA");
        assert_eq!(ar.plural(0.0, false), PluralCategory::Zero);
        assert_eq!(ar.plural(2.0, false), PluralCategory::Two);
        let en = LocaleInfo::new("en");
        assert_eq!(en.plural(1.0, false), PluralCategory::One);
        assert_eq!(en.plural(1.5, false), PluralCategory::Other);
        assert_eq!(en.plural(2.0, true), PluralCategory::Two);
    }
}
