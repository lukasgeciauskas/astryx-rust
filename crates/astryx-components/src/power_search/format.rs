//! How filters read as text, upstream `formatFilterValue.ts`,
//! `formatTokenValue.ts` and `relativeDateOptions.ts`.

use astryx_i18n::{Arg, I18n};
use gpui::SharedString;
use unicode_segmentation::UnicodeSegmentation;

use super::model::{EnumItem, FilterValue, OperatorValue};

/// Upstream `truncateCharacters`: at most `max` user-perceived characters,
/// the ellipsis included.
pub(crate) fn truncate_characters(text: &str, max: usize, ellipsis: &str) -> String {
    let characters: Vec<&str> = text.graphemes(true).collect();
    if characters.len() <= max {
        return text.to_string();
    }
    let keep = max.saturating_sub(ellipsis.graphemes(true).count());
    let mut out: String = characters[..keep].concat();
    out.push_str(ellipsis);
    out
}

fn truncate(text: &str, max: usize) -> String {
    truncate_characters(text, max, "…")
}

/// A token's value cut: three dots past the limit (upstream `truncateString`).
fn truncate_token(text: &str, max: usize) -> String {
    truncate_characters(text, max + 3, "...")
}

fn enum_label(values: &[EnumItem], value: &str) -> SharedString {
    values
        .iter()
        .find(|v| v.value.as_ref() == value)
        .map_or_else(
            || SharedString::from(value.to_string()),
            |v| v.label.clone(),
        )
}

fn count_text(t18: &I18n, key: &str, count: usize) -> String {
    t18.format(key, &[("count", Arg::Num(count as f64))])
        .to_string()
}

/// Upstream `RELATIVE_DATE_UNITS`.
const RELATIVE_UNITS: [(&str, &[u32], &str, &str); 3] = [
    (
        "d",
        &[1, 3, 7, 14, 30],
        "@astryx.powersearch.sheet.relativeDaysAgo",
        "@astryx.powersearch.sheet.relativeDaysFromNow",
    ),
    (
        "w",
        &[1, 2, 4],
        "@astryx.powersearch.sheet.relativeWeeksAgo",
        "@astryx.powersearch.sheet.relativeWeeksFromNow",
    ),
    (
        "m",
        &[1, 3, 6, 12],
        "@astryx.powersearch.sheet.relativeMonthsAgo",
        "@astryx.powersearch.sheet.relativeMonthsFromNow",
    ),
];

/// Upstream `buildRelativeDateOptions`: (value, label) pairs such as
/// `("3d_ago", "3 days ago")`.
pub fn relative_date_options(
    past: Option<bool>,
    future: Option<bool>,
    t18: &I18n,
) -> Vec<(SharedString, SharedString)> {
    let mut out = Vec::new();
    for (unit, amounts, ago, from_now) in RELATIVE_UNITS {
        for amount in amounts {
            let args = [("count", Arg::Num(*amount as f64))];
            if past != Some(false) {
                out.push((format!("{amount}{unit}_ago").into(), t18.format(ago, &args)));
            }
            if future != Some(false) {
                out.push((
                    format!("{amount}{unit}_from_now").into(),
                    t18.format(from_now, &args),
                ));
            }
        }
    }
    out
}

/// Upstream `relativeDateLabel`.
pub fn relative_date_label(op: &OperatorValue, value: &str, t18: &I18n) -> String {
    match op {
        OperatorValue::DateRelative {
            past_allowed,
            future_allowed,
        } => relative_date_options(*past_allowed, *future_allowed, t18)
            .into_iter()
            .find(|(v, _)| v.as_ref() == value)
            .map_or_else(|| value.to_string(), |(_, l)| l.to_string()),
        _ => value.to_string(),
    }
}

/// A Unix time as its UTC civil date and time (Howard Hinnant's algorithm).
fn civil(unix_seconds: i64) -> (i64, u32, u32, u32, u32) {
    let days = unix_seconds.div_euclid(86_400);
    let secs = unix_seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        month,
        day,
        (secs / 3_600) as u32,
        ((secs % 3_600) / 60) as u32,
    )
}

/// Upstream `formatDateAbsolute`, for now as an ISO date (and time) in UTC.
/// The locale's calendar, its formats and `timezoneID` come with M8's
/// calendar systems.
pub fn format_date_absolute(unix_seconds: i64, date_only: bool) -> String {
    let (y, m, d, hh, mm) = civil(unix_seconds);
    if date_only {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}")
    }
}

fn date_only(op: &OperatorValue) -> bool {
    !matches!(
        op,
        OperatorValue::DateAbsolute {
            date_only: Some(false)
        }
    )
}

fn format_number(value: f64, units: Option<&SharedString>, t18: &I18n) -> String {
    let formatted = t18.number(value);
    match units {
        Some(units) => format!("{formatted} {units}"),
        None => formatted,
    }
}

fn units(op: &OperatorValue) -> Option<&SharedString> {
    match op {
        OperatorValue::Integer { units, .. } | OperatorValue::Float { units, .. } => units.as_ref(),
        _ => None,
    }
}

/// Upstream `formatFilterValue`: a value as the editor and the sheet's
/// rows read it.
pub fn format_filter_value(
    op: &OperatorValue,
    value: &FilterValue,
    max_length: usize,
    t18: &I18n,
) -> String {
    let list = |labels: Vec<String>, count_key: &str| -> String {
        match labels.len() {
            0 => String::new(),
            1 => truncate(&labels[0], max_length),
            n => {
                let joined = t18.conjunction(labels.iter().map(|s| s.as_str()));
                if joined.chars().count() <= max_length {
                    joined
                } else {
                    count_text(t18, count_key, n)
                }
            }
        }
    };
    match value {
        FilterValue::Empty => String::new(),
        FilterValue::String(s) => truncate(s, max_length),
        FilterValue::Integer(n) => format_number(*n as f64, units(op), t18),
        FilterValue::Float(n) => format_number(*n, units(op), t18),
        FilterValue::Enum(v) => match op.enum_values() {
            Some(values) if matches!(op, OperatorValue::Enum { .. }) => {
                truncate(&enum_label(values, v), max_length)
            }
            _ => truncate(v, max_length),
        },
        FilterValue::StringList(items) => list(
            items.iter().map(|s| s.to_string()).collect(),
            "@astryx.powersearch.valueEditor.itemsCount",
        ),
        FilterValue::EnumList(items) => match op {
            OperatorValue::EnumList { values } => list(
                items
                    .iter()
                    .map(|v| enum_label(values, v).to_string())
                    .collect(),
                "@astryx.powersearch.valueEditor.itemsCount",
            ),
            _ => match items.len() {
                0 => String::new(),
                1 => truncate(&items[0], max_length),
                n => count_text(t18, "@astryx.powersearch.valueEditor.itemsCount", n),
            },
        },
        FilterValue::EntityList(entities) => list(
            entities.iter().map(|e| e.label.to_string()).collect(),
            "@astryx.powersearch.valueEditor.entitiesCount",
        ),
        FilterValue::Time(v) => v.to_string(),
        FilterValue::DateAbsolute { unix_seconds } => truncate(
            &format_date_absolute(*unix_seconds, date_only(op)),
            max_length,
        ),
        FilterValue::DateRelative(v) => relative_date_label(op, v, t18),
        FilterValue::DateRange(_) => t18
            .t("@astryx.powersearch.valueEditor.dateRange")
            .to_string(),
        FilterValue::Custom(v) => match op {
            OperatorValue::Custom { text, .. } => truncate(&text(v), max_length),
            _ => v.to_string(),
        },
        FilterValue::Nested(filters) => count_text(
            t18,
            "@astryx.powersearch.valueEditor.filtersCount",
            filters.len(),
        ),
    }
}

/// Upstream `formatTokenLabel`: "Field: operator", or the field alone.
pub fn format_token_label(field: &str, operator: &str, t18: &I18n) -> String {
    if operator.is_empty() {
        return field.to_string();
    }
    t18.format(
        "@astryx.powersearch.token.fieldOperator",
        &[("field", field.into()), ("operator", operator.into())],
    )
    .to_string()
}

/// Upstream `formatTokenText`: the token's whole text.
pub fn format_token_text(label: &str, value: &str, t18: &I18n) -> String {
    t18.format(
        "@astryx.powersearch.token.labelValue",
        &[("label", label.into()), ("value", value.into())],
    )
    .to_string()
}

/// Upstream `tokenTextKeepsParts`: the locale's token text is the label, a
/// space, then the value, so the token can style the two parts apart.
pub fn token_text_keeps_parts(t18: &I18n) -> bool {
    const LABEL: &str = "\u{E000}";
    const VALUE: &str = "\u{E001}";
    let pattern = format_token_text(LABEL, VALUE, t18);
    pattern
        .strip_prefix(LABEL)
        .and_then(|rest| rest.strip_suffix(VALUE))
        .is_some_and(|gap| !gap.is_empty() && gap.chars().all(char::is_whitespace))
}

/// JavaScript's `String(number)` for the values a token shows.
fn js_number(value: f64) -> String {
    if value.fract() == 0. && value.abs() < 1e21 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// Upstream `formatNumberAsToken`: plain digits where the locale writes
/// them so, else its digits without grouping.
fn token_number(value: f64, t18: &I18n) -> String {
    let symbols = t18.number_symbols();
    if astryx_i18n::number::editable_number(1_234_567.5, symbols) == "1234567.5" {
        js_number(value)
    } else {
        astryx_i18n::number::editable_number(value, symbols)
    }
}

/// Upstream `formatTokenValueText`: the value a token shows, or none.
pub fn format_token_value(
    op: &OperatorValue,
    value: &FilterValue,
    max_length: usize,
    t18: &I18n,
) -> Option<String> {
    let list = |labels: Vec<String>| -> Option<String> {
        match labels.len() {
            0 => None,
            1 => Some(truncate_token(&labels[0], max_length)),
            n => {
                let total: usize = labels.iter().map(|s| s.encode_utf16().count()).sum();
                if total > max_length {
                    Some(count_text(
                        t18,
                        "@astryx.powersearch.valueEditor.itemsCount",
                        n,
                    ))
                } else {
                    Some(t18.conjunction(labels.iter().map(|s| s.as_str())))
                }
            }
        }
    };
    match value {
        FilterValue::Empty => None,
        FilterValue::String(s) => Some(truncate_token(s, max_length)),
        FilterValue::Integer(n) => Some(token_number(*n as f64, t18)),
        FilterValue::Float(n) => Some(token_number(*n, t18)),
        FilterValue::Enum(v) => Some(match op {
            OperatorValue::Enum { values } => truncate_token(&enum_label(values, v), max_length),
            _ => truncate_token(v, max_length),
        }),
        FilterValue::StringList(items) => list(items.iter().map(|s| s.to_string()).collect()),
        FilterValue::EnumList(items) => match op {
            OperatorValue::EnumList { values } => list(
                items
                    .iter()
                    .map(|v| enum_label(values, v).to_string())
                    .collect(),
            ),
            _ => match items.len() {
                0 => None,
                1 => Some(truncate_token(&items[0], max_length)),
                n => Some(count_text(
                    t18,
                    "@astryx.powersearch.valueEditor.itemsCount",
                    n,
                )),
            },
        },
        FilterValue::EntityList(entities) => {
            list(entities.iter().map(|e| e.label.to_string()).collect())
        }
        FilterValue::Time(v) => Some(v.to_string()),
        FilterValue::DateAbsolute { unix_seconds } => Some(truncate_token(
            &format_date_absolute(*unix_seconds, date_only(op)),
            max_length,
        )),
        FilterValue::DateRelative(v) => {
            Some(truncate_token(&relative_date_label(op, v, t18), max_length))
        }
        FilterValue::DateRange(_) => Some(
            t18.t("@astryx.powersearch.valueEditor.dateRange")
                .to_string(),
        ),
        FilterValue::Custom(v) => Some(match op {
            OperatorValue::Custom { text, .. } => truncate_token(&text(v), max_length),
            _ => v.to_string(),
        }),
        FilterValue::Nested(filters) => Some(count_text(
            t18,
            "@astryx.powersearch.valueEditor.filtersCount",
            filters.len(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power_search::model::PowerSearchEntity;

    fn t18(cx: &mut gpui::TestAppContext) -> I18n {
        cx.update(|cx| astryx_i18n::current(cx))
    }

    #[test]
    fn truncation_counts_characters_and_keeps_the_ellipsis_inside() {
        assert_eq!(truncate_characters("abcdef", 6, "…"), "abcdef");
        assert_eq!(truncate_characters("abcdefg", 6, "…"), "abcde…");
        assert_eq!(truncate_characters("👍🏽👍🏽👍🏽", 2, "…"), "👍🏽…");
    }

    #[test]
    fn civil_dates_follow_the_gregorian_calendar() {
        assert_eq!(format_date_absolute(0, true), "1970-01-01");
        assert_eq!(format_date_absolute(951_782_400, true), "2000-02-29");
        assert_eq!(
            format_date_absolute(1_700_000_000, false),
            "2023-11-14 22:13"
        );
        assert_eq!(format_date_absolute(-86_400, true), "1969-12-31");
    }

    #[gpui::test]
    fn values_read_as_text(cx: &mut gpui::TestAppContext) {
        let t = t18(cx);
        let genres = OperatorValue::Enum {
            values: vec![EnumItem::new("fiction", "Fiction")],
        };
        assert_eq!(
            format_filter_value(&genres, &FilterValue::Enum("fiction".into()), 40, &t),
            "Fiction"
        );
        let list = FilterValue::StringList(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(
            format_filter_value(&OperatorValue::string_list(), &list, 40, &t),
            "a, b, c"
        );
        assert_eq!(
            format_filter_value(&OperatorValue::string_list(), &list, 4, &t),
            "3 items"
        );
        let pages = OperatorValue::Float {
            min: None,
            max: None,
            units: Some("pages".into()),
        };
        assert_eq!(
            format_filter_value(&pages, &FilterValue::Float(1200.), 40, &t),
            "1,200 pages"
        );
        let people = FilterValue::EntityList(vec![
            PowerSearchEntity {
                id: "1".into(),
                label: "Ada".into(),
                photo: None,
            },
            PowerSearchEntity {
                id: "2".into(),
                label: "Grace".into(),
                photo: None,
            },
        ]);
        let op = OperatorValue::EntityList {
            search_source: None,
            arbitrary_allowed: false,
            tokenization: None,
            render_item: None,
        };
        assert_eq!(format_filter_value(&op, &people, 4, &t), "2 entities");
        let relative = OperatorValue::DateRelative {
            past_allowed: None,
            future_allowed: Some(false),
        };
        assert_eq!(
            format_filter_value(
                &relative,
                &FilterValue::DateRelative("3d_ago".into()),
                40,
                &t
            ),
            "3 days ago"
        );
        assert_eq!(relative_date_options(None, Some(false), &t).len(), 12);
    }

    #[gpui::test]
    fn tokens_read_field_operator_and_value(cx: &mut gpui::TestAppContext) {
        let t = t18(cx);
        assert_eq!(
            format_token_label("Title", "contains", &t),
            "Title: contains"
        );
        assert_eq!(format_token_label("Starred", "", &t), "Starred");
        assert_eq!(
            format_token_text("Title: contains", "dune", &t),
            "Title: contains dune"
        );
        assert!(token_text_keeps_parts(&t));
        assert_eq!(
            format_token_value(&OperatorValue::float(), &FilterValue::Float(1234.5), 40, &t)
                .as_deref(),
            Some("1234.5")
        );
        assert_eq!(
            format_token_value(
                &OperatorValue::string(),
                &FilterValue::String("abcdefghij".into()),
                4,
                &t
            )
            .as_deref(),
            Some("abcd...")
        );
        assert_eq!(
            format_token_value(&OperatorValue::Empty, &FilterValue::Empty, 40, &t),
            None
        );
    }
}
