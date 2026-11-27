//! The lab's tick formatters (upstream lab `formatters.ts`): compact numbers, currency,
//! percents and dates.

use std::rc::Rc;

use astryx_charts::ChartValue;
use astryx_i18n::CalendarSystem;
use astryx_i18n::date::{DateStyle, PlainDate, Width, format_date};
use astryx_theme::authoring::js::{js_number_to_string, js_round, js_to_fixed};
use gpui::SharedString;

use super::axis::TickFormat;

/// Upstream's `strip`: one decimal, without a trailing ".0".
fn strip(n: f64) -> String {
    let s = js_to_fixed(n, 1);
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

fn abridged(n: f64) -> Option<String> {
    let a = n.abs();
    if a >= 1_000_000_000. {
        Some(format!("{}B", strip(n / 1_000_000_000.)))
    } else if a >= 1_000_000. {
        Some(format!("{}M", strip(n / 1_000_000.)))
    } else if a >= 1_000. {
        Some(format!("{}K", strip(n / 1_000.)))
    } else {
        None
    }
}

/// Upstream `compactNumber`: 1200 is "1.2K", 1500000 "1.5M"; what is not a number is written
/// as it is.
pub fn compact_number(value: &ChartValue) -> SharedString {
    let n = value.to_number();
    if n.is_nan() {
        return value.to_js_string();
    }
    abridged(n).unwrap_or_else(|| js_number_to_string(n)).into()
}

/// Upstream `currency(symbol, locale)`: a tick format with `symbol` before the abridged value
/// ("$1.5K"), or before the value in `locale`'s digits under a thousand (English by default).
pub fn currency(symbol: impl Into<SharedString>, locale: Option<&str>) -> TickFormat {
    let symbol: SharedString = symbol.into();
    let i18n = astryx_i18n::I18n::new(locale.unwrap_or("en").to_string());
    Rc::new(move |value: &ChartValue| {
        let n = value.to_number();
        if n.is_nan() {
            return value.to_js_string();
        }
        match abridged(n) {
            Some(text) => format!("{symbol}{text}").into(),
            None => format!("{symbol}{}", i18n.number(n)).into(),
        }
    })
}

/// Upstream `percent`: 0.45 is "45%".
pub fn percent(value: &ChartValue) -> SharedString {
    let n = value.to_number();
    if n.is_nan() {
        return value.to_js_string();
    }
    format!("{}%", js_number_to_string(js_round(n * 100.))).into()
}

/// The local day of a value as upstream's `new Date(Number(value))` reads it (`getFullYear`,
/// `getMonth`, `getDate`); `None` for an invalid date.
fn local_day(value: &ChartValue) -> Option<PlainDate> {
    let ms = value.to_number();
    if !ms.is_finite() || ms.abs() > 8.64e15 {
        return None;
    }
    let ts = jiff::Timestamp::from_millisecond(ms.trunc() as i64).ok()?;
    let dt = jiff::tz::TimeZone::system().to_datetime(ts);
    Some(PlainDate {
        year: i32::from(dt.year()),
        month: dt.month() as u8,
        day: dt.day() as u8,
    })
}

/// Upstream `shortDate(value, locale, calendar)`: "Jan 5", in `calendar` (Gregorian) and `locale`
/// (English). A value that is no date is written as it is.
pub fn short_date(
    value: &ChartValue,
    locale: Option<&str>,
    calendar: Option<CalendarSystem>,
) -> SharedString {
    match local_day(value) {
        Some(day) => format_date(
            day,
            DateStyle::MONTH_DAY,
            locale.unwrap_or("en"),
            calendar.unwrap_or_default(),
        )
        .into(),
        None => value.to_js_string(),
    }
}

/// Upstream `monthYear(value, locale, calendar)`: "Jan 2024", in `calendar` and `locale`.
pub fn month_year(
    value: &ChartValue,
    locale: Option<&str>,
    calendar: Option<CalendarSystem>,
) -> SharedString {
    let style = DateStyle {
        year: true,
        month: Some(Width::Short),
        day: false,
        weekday: None,
        era: None,
    };
    match local_day(value) {
        Some(day) => format_date(
            day,
            style,
            locale.unwrap_or("en"),
            calendar.unwrap_or_default(),
        )
        .into(),
        None => value.to_js_string(),
    }
}
