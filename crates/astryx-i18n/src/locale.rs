//! A locale's formatting services and text direction, from ICU4X's CLDR
//! data: the same data behind upstream's `Intl.NumberFormat`,
//! `Intl.PluralRules` and `Intl.Locale.getTextInfo()`.

use icu_decimal::DecimalFormatter;
use icu_decimal::input::{Decimal, FloatPrecision};
use icu_decimal::options::{DecimalFormatterOptions, GroupingStrategy};
use std::cell::OnceCell;

use icu_experimental::dimension::percent::formatter::PercentFormatter;
use icu_experimental::dimension::units::categorized_formatter::CategorizedFormatter;
use icu_experimental::dimension::units::options::Width;
use icu_experimental::measure::category::Duration as DurationCategory;
use icu_list::ListFormatter;
use icu_list::options::{ListFormatterOptions, ListLength};
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

/// A unit of an elapsed duration, upstream `ElapsedDurationUnit`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurationUnit {
    Hour,
    Minute,
    Second,
}

type UnitFormatter = CategorizedFormatter<DurationCategory>;

/// Number and plural formatting for one locale.
pub struct LocaleInfo {
    locale: Locale,
    units: [OnceCell<Option<UnitFormatter>>; 3],
    unit_list: OnceCell<Option<ListFormatter>>,
    decimal: DecimalFormatter,
    decimal_ungrouped: DecimalFormatter,
    cardinal: PluralRules,
    ordinal: PluralRules,
    percent: Option<PercentFormatter<DecimalFormatter>>,
    symbols: OnceCell<crate::number::NumberSymbols>,
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
            units: Default::default(),
            unit_list: OnceCell::new(),
            decimal_ungrouped: decimal(GroupingStrategy::Never),
            cardinal: PluralRules::try_new_cardinal((&locale).into())
                .or_else(|_| PluralRules::try_new_cardinal(Default::default()))
                .expect("compiled plural data covers the root locale"),
            ordinal: PluralRules::try_new_ordinal((&locale).into())
                .or_else(|_| PluralRules::try_new_ordinal(Default::default()))
                .expect("compiled plural data covers the root locale"),
            percent: PercentFormatter::try_new((&locale).into(), Default::default()).ok(),
            decimal: decimal(GroupingStrategy::Auto),
            locale,
            symbols: OnceCell::new(),
        }
    }

    /// The locale's grouping and decimal separators and group sizes.
    pub fn number_symbols(&self) -> &crate::number::NumberSymbols {
        use crate::message::LocaleFormat as _;
        self.symbols.get_or_init(|| {
            crate::number::NumberSymbols::from_format(|value| {
                self.number(value, NumberStyle::default())
            })
        })
    }

    /// An elapsed duration in narrow units, upstream `formatElapsedDuration`:
    /// "5s", "1m 05s", "2h 03m". Parts after the first show two digits.
    pub fn elapsed_duration(&self, parts: &[(DurationUnit, u64)]) -> String {
        use writeable::Writeable as _;
        let texts: Vec<String> = parts
            .iter()
            .enumerate()
            .map(|(i, &(unit, value))| {
                let slot = match unit {
                    DurationUnit::Hour => 0,
                    DurationUnit::Minute => 1,
                    DurationUnit::Second => 2,
                };
                let formatter = self.units[slot].get_or_init(|| {
                    let category = match unit {
                        DurationUnit::Hour => DurationCategory::hour(),
                        DurationUnit::Minute => DurationCategory::minute(),
                        DurationUnit::Second => DurationCategory::second(),
                    };
                    UnitFormatter::try_new_core(
                        (&self.locale).into(),
                        category,
                        Width::Narrow.into(),
                    )
                    .ok()
                });
                let mut decimal = Decimal::from(value);
                if i > 0 {
                    decimal.absolute.pad_start(2);
                }
                match formatter {
                    Some(f) => f
                        .format_fixed_decimal(&decimal)
                        .write_to_string()
                        .into_owned(),
                    None => {
                        let suffix = match unit {
                            DurationUnit::Hour => "h",
                            DurationUnit::Minute => "m",
                            DurationUnit::Second => "s",
                        };
                        format!("{}{suffix}", decimal.write_to_string())
                    }
                }
            })
            .collect();
        let list = self.unit_list.get_or_init(|| {
            ListFormatter::try_new_unit(
                (&self.locale).into(),
                ListFormatterOptions::default().with_length(ListLength::Narrow),
            )
            .ok()
        });
        match list {
            Some(list) => list
                .format(texts.iter().map(String::as_str))
                .write_to_string()
                .into_owned(),
            None => texts.join(" "),
        }
    }

    /// A percentage in the locale's pattern, `Intl.NumberFormat(locale,
    /// {style: 'percent'})` of `value / 100`: 45 gives "45%" in English,
    /// "45 %" in French, "%45" in Turkish.
    pub fn percent(&self, value: f64) -> String {
        use writeable::Writeable as _;
        match (&self.percent, Self::decimal(value, NumberStyle::default())) {
            (Some(formatter), Some(decimal)) => {
                formatter.format(&decimal).write_to_string().into_owned()
            }
            _ => format!("{}%", self.number(value, NumberStyle::default())),
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
    fn percentages_follow_the_locale() {
        assert_eq!(LocaleInfo::new("en").percent(45.), "45%");
        let fr = LocaleInfo::new("fr-FR").percent(45.);
        assert!(
            fr.starts_with("45") && fr.ends_with('%') && fr.len() > 3,
            "{fr:?}"
        );
        assert!(LocaleInfo::new("tr-TR").percent(45.).starts_with('%'));
    }

    #[test]
    fn elapsed_durations_are_narrow_and_padded() {
        let en = LocaleInfo::new("en");
        assert_eq!(en.elapsed_duration(&[(DurationUnit::Second, 5)]), "5s");
        assert_eq!(
            en.elapsed_duration(&[(DurationUnit::Minute, 1), (DurationUnit::Second, 5)]),
            "1m 05s"
        );
        assert_eq!(
            en.elapsed_duration(&[(DurationUnit::Hour, 2), (DurationUnit::Minute, 3)]),
            "2h 03m"
        );
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
