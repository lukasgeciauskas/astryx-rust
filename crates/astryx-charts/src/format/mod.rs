//! Locale-aware tick, value and list formats (upstream `formatters.ts`).
//!
//! Each value formatter is a [`ChartValueFormat`]. A null or missing value returns
//! `None` to ask for the chart's default text. Non-finite numbers (NaN, ±Infinity)
//! and unparseable values pass through as-is rather than producing garbage like
//! "InfinityB", and no input panics. Numbers and dates are written with ICU4X's
//! CLDR data, the data behind upstream's `Intl` formatters, so output is
//! locale-aware and rounding is correct across the full magnitude range (K, M, B,
//! T and beyond).

mod date;

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use astryx_i18n::CalendarSystem;
use astryx_i18n::date::{DateStyle, Width, format_date};
use gpui::SharedString;
use icu_decimal::input::{Decimal, FloatPrecision};
use icu_decimal::{CompactDecimalFormatter, DecimalFormatter};
use icu_experimental::dimension::percent::formatter::PercentFormatter;
use icu_list::ListFormatter;
use icu_list::options::{ListFormatterOptions, ListLength};
use icu_locale::Locale;
use writeable::Writeable as _;

use crate::ChartValue;

type FormatFn = dyn Fn(Option<&ChartValue>, &str, CalendarSystem) -> Option<SharedString>;

/// Turns one value into its display text, upstream `ChartValueFormat`.
///
/// It receives the value (`None` for `undefined`, `Some(ChartValue::Null)` for
/// `null`), a locale and a calendar. It returns the text for the value, an empty
/// value included, or `None` to ask for the chart's default text. A format may
/// ignore the locale and calendar. The built-in formatters are formats:
///
/// ```ignore
/// let format = ChartValueFormat::compact_number();
/// let custom = ChartValueFormat::new(|value, _, _| Some(format!("{} ms", js_string(value)).into()));
/// ```
#[derive(Clone)]
pub struct ChartValueFormat(Rc<FormatFn>);

impl ChartValueFormat {
    /// A format from a function of the value, the locale and the calendar.
    pub fn new(
        format: impl Fn(Option<&ChartValue>, &str, CalendarSystem) -> Option<SharedString> + 'static,
    ) -> Self {
        Self(Rc::new(format))
    }

    /// The text for `value` in `locale` and `calendar`, or `None` for the chart's
    /// default text.
    pub fn format(
        &self,
        value: Option<&ChartValue>,
        locale: &str,
        calendar: CalendarSystem,
    ) -> Option<SharedString> {
        (self.0)(value, locale, calendar)
    }

    /// [`compact_number`] as a format, upstream `compactNumber`.
    pub fn compact_number() -> Self {
        Self::new(|value, locale, _| compact_number(value, locale))
    }

    /// [`percent`] as a format, upstream `percent`.
    pub fn percent() -> Self {
        Self::new(|value, locale, _| percent(value, locale))
    }

    /// [`short_date`] as a format, upstream `shortDate`.
    pub fn short_date() -> Self {
        Self::new(short_date)
    }

    /// [`month_year`] as a format, upstream `monthYear`.
    pub fn month_year() -> Self {
        Self::new(month_year)
    }

    /// [`currency`] with a symbol and an optional fixed locale, upstream
    /// `currency(symbol, locale)`.
    pub fn currency(symbol: impl Into<SharedString>, locale: Option<&str>) -> Self {
        currency(symbol, locale)
    }
}

impl<F> From<F> for ChartValueFormat
where
    F: Fn(Option<&ChartValue>, &str, CalendarSystem) -> Option<SharedString> + 'static,
{
    fn from(format: F) -> Self {
        Self::new(format)
    }
}

impl fmt::Debug for ChartValueFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChartValueFormat(..)")
    }
}

/// Labels as a conjunction list in the locale, upstream `formatList`
/// (`Intl.ListFormat`, long conjunction): "a, b, and c" in English.
pub fn format_list<I>(values: I, locale: &str) -> String
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let values: Vec<I::Item> = values.into_iter().collect();
    let formats = formats(locale);
    match formats.list() {
        Some(list) => list
            .format(values.iter().map(AsRef::as_ref))
            .write_to_string()
            .into_owned(),
        None => values
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// A compact number, upstream `compactNumber` (1200 → "1.2K", 1500000 → "1.5M",
/// 1e12 → "1T").
///
/// Non-numeric values (e.g. category labels) and non-finite numbers pass through
/// unchanged, so it is safe to use as a general-purpose tick formatter. A null or
/// missing value returns `None`.
pub fn compact_number(value: Option<&ChartValue>, locale: &str) -> Option<SharedString> {
    let value = present(value)?;
    let n = number_of(value);
    if !n.is_finite() {
        return Some(string_of(value));
    }
    Some(compact_text(locale, n).into())
}

/// A currency format, upstream `currency(symbol, locale)`.
///
/// The symbol prefixes the magnitude and the sign leads the symbol, so negatives
/// read as "-$1.5K" rather than "$-1.5K". Values of 1000 and above use compact
/// notation. The format uses the chart's locale unless `locale` is given, which
/// wins over it. A null or missing value returns `None`.
///
/// ```ignore
/// let dollars = currency("$", None); // $1.5K
/// let euros = currency("€", None); // €1.5K
/// let yen = currency("¥", None); // ¥1.5K
/// ```
pub fn currency(symbol: impl Into<SharedString>, locale: Option<&str>) -> ChartValueFormat {
    let symbol: SharedString = symbol.into();
    let locale: Option<SharedString> = locale.map(|l| SharedString::from(l.to_string()));
    ChartValueFormat::new(move |value, call_locale, _| {
        let value = present(value)?;
        let n = number_of(value);
        if !n.is_finite() {
            return Some(string_of(value));
        }
        let resolved = locale.as_deref().unwrap_or(call_locale);
        let sign = if n < 0. { "-" } else { "" };
        let abs = n.abs();
        let body = if abs >= 1000. {
            compact_text(resolved, abs)
        } else {
            locale_number_text(resolved, abs)
        };
        Some(format!("{sign}{symbol}{body}").into())
    })
}

/// A percentage, upstream `percent` (0.45 → "45%", 1.2 → "120%", 0.005 → "0.5%").
///
/// The input is a ratio; it is multiplied by 100 and localized. Whole percents
/// render without a decimal, and up to one fractional digit is shown otherwise.
/// A null or missing value returns `None`.
pub fn percent(value: Option<&ChartValue>, locale: &str) -> Option<SharedString> {
    let value = present(value)?;
    let n = number_of(value);
    if !n.is_finite() {
        return Some(string_of(value));
    }
    Some(percent_text(locale, n).into())
}

/// A short date, upstream `shortDate` ("Jan 5", "Mar 12"), in `calendar`: pass the
/// application's calendar to match the date components (upstream defaults to
/// Gregorian). A value that is not a date passes through; a null or missing
/// value returns `None`.
pub fn short_date(
    value: Option<&ChartValue>,
    locale: &str,
    calendar: CalendarSystem,
) -> Option<SharedString> {
    date_text(value, locale, calendar, DateStyle::MONTH_DAY)
}

/// A month and year, upstream `monthYear` ("Jan 2024", "Mar 2025"), in
/// `calendar`: pass the application's calendar to match the date components
/// (upstream defaults to Gregorian). A value that is not a date passes through; a
/// null or missing value returns `None`.
pub fn month_year(
    value: Option<&ChartValue>,
    locale: &str,
    calendar: CalendarSystem,
) -> Option<SharedString> {
    date_text(value, locale, calendar, SHORT_MONTH_YEAR)
}

/// Upstream's `{month: 'short', year: 'numeric'}`.
const SHORT_MONTH_YEAR: DateStyle = DateStyle {
    year: true,
    month: Some(Width::Short),
    day: false,
    weekday: None,
    era: None,
};

fn date_text(
    value: Option<&ChartValue>,
    locale: &str,
    calendar: CalendarSystem,
    style: DateStyle,
) -> Option<SharedString> {
    let value = present(value)?;
    let time = date::to_date(value);
    if time.is_nan() {
        return Some(string_of(value));
    }
    let day = date::local_plain_date(time);
    let gregorian = matches!(calendar, CalendarSystem::Gregory | CalendarSystem::Iso8601);
    if gregorian
        && style == SHORT_MONTH_YEAR
        && let Some(text) = date::gregorian_month_year(day, locale)
    {
        return Some(text.into());
    }
    Some(format_date(day, style, locale, calendar).into())
}

/// A value that is neither `null` nor `undefined`.
fn present(value: Option<&ChartValue>) -> Option<&ChartValue> {
    value.filter(|v| !matches!(v, ChartValue::Null))
}

/// `Number(value)`, a date being the time value a JavaScript `Date` holds.
fn number_of(value: &ChartValue) -> f64 {
    match value {
        ChartValue::Date(ms) => date::time_clip(*ms),
        other => other.to_number(),
    }
}

/// `String(value)`: what passes through when a value cannot be formatted.
fn string_of(value: &ChartValue) -> SharedString {
    match value {
        ChartValue::Date(ms) if date::time_clip(*ms).is_nan() => "Invalid Date".into(),
        other => other.to_js_string(),
    }
}

/// One locale's formatters, built on first use: upstream's `byLocale` cache.
struct LocaleFormats {
    locale: Locale,
    compact: OnceCell<Option<CompactDecimalFormatter>>,
    percent: OnceCell<Option<PercentFormatter<DecimalFormatter>>>,
    decimal: OnceCell<Option<DecimalFormatter>>,
    list: OnceCell<Option<ListFormatter>>,
}

impl LocaleFormats {
    /// `Intl.NumberFormat(locale, {notation: 'compact'})`: CLDR's short compact
    /// patterns, grouped only from five digits.
    fn compact(&self) -> Option<&CompactDecimalFormatter> {
        self.compact
            .get_or_init(|| {
                CompactDecimalFormatter::try_new_short((&self.locale).into(), Default::default())
                    .or_else(|_| {
                        CompactDecimalFormatter::try_new_short(
                            Default::default(),
                            Default::default(),
                        )
                    })
                    .ok()
            })
            .as_ref()
    }

    /// `Intl.NumberFormat(locale, {style: 'percent'})`.
    fn percent(&self) -> Option<&PercentFormatter<DecimalFormatter>> {
        self.percent
            .get_or_init(|| {
                PercentFormatter::try_new((&self.locale).into(), Default::default())
                    .or_else(|_| PercentFormatter::try_new(Default::default(), Default::default()))
                    .ok()
            })
            .as_ref()
    }

    /// `Intl.NumberFormat(locale)`.
    fn decimal(&self) -> Option<&DecimalFormatter> {
        self.decimal
            .get_or_init(|| {
                DecimalFormatter::try_new((&self.locale).into(), Default::default())
                    .or_else(|_| DecimalFormatter::try_new(Default::default(), Default::default()))
                    .ok()
            })
            .as_ref()
    }

    /// `Intl.ListFormat(locale, {style: 'long', type: 'conjunction'})`.
    fn list(&self) -> Option<&ListFormatter> {
        self.list
            .get_or_init(|| {
                let options = ListFormatterOptions::default().with_length(ListLength::Wide);
                ListFormatter::try_new_and((&self.locale).into(), options)
                    .or_else(|_| ListFormatter::try_new_and(Default::default(), options))
                    .ok()
            })
            .as_ref()
    }
}

thread_local! {
    static FORMATS: RefCell<HashMap<String, Rc<LocaleFormats>>> = RefCell::new(HashMap::new());
}

/// The formatters of a locale tag, one set per tag. A malformed tag reads as the
/// root locale.
fn formats(locale: &str) -> Rc<LocaleFormats> {
    FORMATS.with(|cache| {
        cache
            .borrow_mut()
            .entry(locale.to_string())
            .or_insert_with(|| {
                Rc::new(LocaleFormats {
                    locale: locale.parse().unwrap_or(Locale::UNKNOWN),
                    compact: OnceCell::new(),
                    percent: OnceCell::new(),
                    decimal: OnceCell::new(),
                    list: OnceCell::new(),
                })
            })
            .clone()
    })
}

/// A finite number as an exact decimal: the shortest digits that read back as it,
/// as `Intl.NumberFormat` takes a number.
fn decimal_of(n: f64) -> Option<Decimal> {
    Decimal::try_from_f64(n, FloatPrecision::RoundTrip).ok()
}

/// Rounds to at most `digits` fraction digits, halves away from zero (`Intl`'s
/// default `halfExpand`), and drops the trailing zeros (a minimum of none).
fn round_fraction(d: &mut Decimal, digits: i16) {
    let position = -digits;
    if d.absolute.digit_at(position - 1) >= 5 {
        d.absolute.expand(position);
    } else {
        d.absolute.trunc(position);
    }
    d.absolute.trim_start();
    d.absolute.trim_end();
}

/// `Intl.NumberFormat(locale, {notation: 'compact', maximumFractionDigits: 1})`.
///
/// The number is scaled to the compact pattern its magnitude takes and rounded to
/// one fraction digit; when rounding carries it into a magnitude with another
/// pattern (999.95K), it is scaled and rounded again (1M), as ICU does.
fn compact_text(locale: &str, n: f64) -> String {
    let formats = formats(locale);
    let (Some(formatter), Some(mut d)) = (formats.compact(), decimal_of(n)) else {
        return plain_text(n);
    };
    let mut exponent = 0;
    if d.absolute.is_zero() {
        round_fraction(&mut d, 1);
    } else {
        let magnitude = d.absolute.nonzero_magnitude_start();
        exponent = formatter.compact_exponent_for_magnitude(magnitude);
        d.absolute.multiply_pow10(-i16::from(exponent));
        round_fraction(&mut d, 1);
        if !d.absolute.is_zero()
            && d.absolute.nonzero_magnitude_start() != magnitude - i16::from(exponent)
        {
            let next = formatter.compact_exponent_for_magnitude(magnitude + 1);
            if next != exponent {
                d.absolute
                    .multiply_pow10(i16::from(exponent) - i16::from(next));
                round_fraction(&mut d, 1);
                exponent = next;
            }
        }
    }
    match formatter.format_with_exponent(&d, exponent) {
        Ok(text) => text.write_to_string().into_owned(),
        Err(_) => formatter.format(&d).write_to_string().into_owned(),
    }
}

/// `Intl.NumberFormat(locale, {style: 'percent', maximumFractionDigits: 1})`: the
/// ratio times 100, rounded to one fraction digit.
fn percent_text(locale: &str, n: f64) -> String {
    let formats = formats(locale);
    let (Some(formatter), Some(mut d)) = (formats.percent(), decimal_of(n)) else {
        return format!("{}%", plain_text(n * 100.));
    };
    d.absolute.multiply_pow10(2);
    round_fraction(&mut d, 1);
    formatter.format(&d).write_to_string().into_owned()
}

/// `n.toLocaleString(locale)`: up to three fraction digits, grouped as the locale
/// groups.
fn locale_number_text(locale: &str, n: f64) -> String {
    let formats = formats(locale);
    let (Some(formatter), Some(mut d)) = (formats.decimal(), decimal_of(n)) else {
        return plain_text(n);
    };
    round_fraction(&mut d, 3);
    formatter.format_to_string(&d)
}

/// `String(n)`, for a formatter that could not be built.
fn plain_text(n: f64) -> String {
    ChartValue::Number(n).to_js_string().to_string()
}

#[cfg(test)]
mod tests;
