//! Upstream `calendarDateParser.test.ts`, in Rust.

use super::*;
use crate::date::calendar_display_name;
use crate::date::format::{SharedDateFormat, format_shared_date};

const TODAY: PlainDate = PlainDate {
    year: 2026,
    month: 11,
    day: 2,
};

fn parse(locale: &str, calendar: CalendarSystem, text: &str) -> DateParse {
    parse_calendar_date(
        text,
        &calendar_date_lexicon(locale, calendar),
        &DateParseOptions {
            today: Some(TODAY),
            ..Default::default()
        },
    )
}

/// The ISO date a text reads as, or the result kind.
fn iso(locale: &str, calendar: CalendarSystem, text: &str) -> String {
    match parse(locale, calendar, text) {
        DateParse::Value { date, .. } => date.to_iso(),
        DateParse::Empty => "empty".into(),
        DateParse::Incomplete => "incomplete".into(),
        DateParse::Unreadable => "unreadable".into(),
        DateParse::Nonexistent { .. } => "nonexistent".into(),
        DateParse::OutOfRange { .. } => "outOfRange".into(),
    }
}

fn complete(locale: &str, calendar: CalendarSystem, text: &str) -> Option<bool> {
    match parse(locale, calendar, text) {
        DateParse::Value { is_complete, .. }
        | DateParse::OutOfRange { is_complete, .. }
        | DateParse::Nonexistent { is_complete } => Some(is_complete),
        _ => None,
    }
}

use CalendarSystem::*;

#[test]
fn fr9_reads_any_unicode_digits_in_mixed_scripts() {
    assert_eq!(iso("fa-IR", Persian, "۱۴۰۵/۸/۱۱"), "2026-11-02");
    assert_eq!(iso("fa-IR", Persian, "١٤٠٥/٨/١١"), "2026-11-02");
    assert_eq!(iso("fa-IR", Persian, "۱۴۰۵/8/١١"), "2026-11-02");
    assert_eq!(iso("hi-IN", Indian, "११/८/१९४८ शक"), "2026-11-02");
}

#[test]
fn fr9_ignores_bidi_marks() {
    assert_eq!(
        iso(
            "ar-SA",
            IslamicUmalqura,
            "\u{200f}٢٢\u{200f}/٥\u{200f}/١٤٤٨ هـ"
        ),
        "2026-11-02"
    );
    assert_eq!(
        iso("ar-SA", IslamicUmalqura, "\u{202b}٢٢/٥/١٤٤٨ هـ\u{202c}"),
        "2026-11-02"
    );
}

#[test]
fn fr9_reads_an_apostrophe_for_the_okina() {
    for mark in ["'", "`", "\u{2019}", "\u{02bb}"] {
        assert_eq!(
            iso(
                "en",
                IslamicUmalqura,
                &format!("Dhu{mark}l-Hijjah 10, 1447 AH")
            ),
            "2026-05-27"
        );
    }
}

#[test]
fn fr9_follows_the_numeric_field_order() {
    assert_eq!(iso("fa-IR", Persian, "1405/8/11"), "2026-11-02");
    assert_eq!(iso("en-US", Persian, "8/11/1405"), "2026-11-02");
    assert_eq!(iso("he", Hebrew, "22/2/5787"), "2026-11-02");
    assert_eq!(iso("en-GB", Gregory, "12/01/2026"), "2026-01-12");
    assert_eq!(iso("en-US", Gregory, "12/01/2026"), "2026-12-01");
}

#[test]
fn fr9_a_long_number_beside_a_month_name_is_the_year() {
    assert_eq!(iso("fa-IR", Persian, "۱۱ آبان ۱۴۰۵"), "2026-11-02");
    assert_eq!(iso("fa-IR", Persian, "۱۴۰۵ آبان ۱۱"), "2026-11-02");
    assert_eq!(iso("en-US", Gregory, "2026 Nov 2"), "2026-11-02");
}

#[test]
fn fr9_reads_month_and_era_names_in_every_width() {
    assert_eq!(iso("fa-IR", Persian, "11 Aban 1405"), "2026-11-02");
    assert_eq!(iso("en-US", Persian, "Aban 11, 1405 AP"), "2026-11-02");
    assert_eq!(iso("th-TH", Buddhist, "2 พ.ย. 2569"), "2026-11-02");
    assert_eq!(iso("th-TH", Buddhist, "2 พฤศจิกายน พ.ศ. 2569"), "2026-11-02");
    assert_eq!(iso("ja-JP", Japanese, "R8/11/2"), "2026-11-02");
    assert_eq!(iso("ja-JP", Japanese, "Reiwa 8/11/2"), "2026-11-02");
    assert_eq!(iso("de-DE", Gregory, "Mo., 2. Nov. 2026"), "2026-11-02");
    // A weekday word that is also a month name.
    assert_eq!(iso("fr-FR", Gregory, "mar. 3 nov. 2026"), "2026-11-03");
    assert_eq!(iso("it-IT", Gregory, "mar 3 mar 2026"), "2026-03-03");
    let tuesday = format_calendar_date_text(
        PlainDate { day: 3, ..TODAY },
        SharedDateFormat::DateWeekday,
        "vi-VN",
        Gregory,
    );
    assert_eq!(iso("vi-VN", Gregory, &tuesday), "2026-11-03", "{tuesday}");
}

#[test]
fn fr9_ignores_case_and_latin_diacritics() {
    assert_eq!(iso("en-US", Japanese, "Jan 7, 64 Shōwa"), "1989-01-07");
    assert_eq!(iso("en-US", Japanese, "JAN 7, 64 showa"), "1989-01-07");
}

#[test]
fn fr9_reads_an_abbreviation_of_one_month() {
    assert_eq!(iso("en-US", Gregory, "Sept 1, 2026"), "2026-09-01");
    assert_eq!(iso("en-US", Gregory, "Ma 1, 2026"), "unreadable");
}

#[test]
fn fr9_hebrew_month_numbers_are_positions_in_the_year() {
    assert_eq!(iso("he", Hebrew, "1/6/5784"), "2024-02-10");
    assert_eq!(iso("he", Hebrew, "1/7/5784"), "2024-03-11");
    assert_eq!(iso("he", Hebrew, "1/6/5785"), "2025-03-01");
}

#[test]
fn fr9_a_plain_adar_in_a_leap_year_is_adar_ii() {
    assert_eq!(iso("en-US", Hebrew, "1 Adar 5784"), "2024-03-11");
    assert_eq!(iso("en-US", Hebrew, "1 Adar II 5784"), "2024-03-11");
    assert_eq!(iso("en-US", Hebrew, "1 Adar I 5784"), "2024-02-10");
    assert_eq!(iso("he", Hebrew, "1 אדר 5784"), "2024-03-11");
    assert_eq!(iso("he", Hebrew, "1 אדר 5785"), "2025-03-01");
}

#[test]
fn fr9_hebrew_names_with_the_prefix_and_a_typed_apostrophe() {
    assert_eq!(iso("he", Hebrew, "22 בחשוון 5787"), "2026-11-02");
    assert_eq!(iso("he", Hebrew, "1 אדר א' 5784"), "2024-02-10");
}

#[test]
fn fr9_chinese_and_dangi_month_numbers_with_the_leap_marker() {
    assert_eq!(iso("zh-CN", Chinese, "2025年闰六月1"), "2025-07-25");
    assert_eq!(iso("zh-CN", Chinese, "2025年六月1"), "2025-06-25");
    assert_eq!(iso("zh-CN", Chinese, "2025/6/1"), "2025-06-25");
    assert_eq!(iso("ko-KR", Dangi, "2025. 윤6. 1."), "2025-07-25");
    assert_eq!(iso("ko-KR", Dangi, "2025. 6. 1."), "2025-06-25");
}

#[test]
fn fr9_rejects_traditional_numerals_and_mixed_separators() {
    assert_eq!(iso("zh-CN", Chinese, "2026丙午年九月廿四"), "unreadable");
    assert_eq!(iso("he", Hebrew, "כ״ב בחשוון תשפ״ז"), "unreadable");
    assert_eq!(iso("en-US", Gregory, "1/25.2026"), "unreadable");
}

#[test]
fn fr9_iso_shaped_text() {
    assert_eq!(iso("sv-SE", Persian, "1405-08-11 AP"), "2026-11-02");
    assert_eq!(iso("en-US", Persian, "1405-08-11"), "2026-11-02");
    let result = parse_calendar_date(
        "2026-11-02",
        &calendar_date_lexicon("en-US", Persian),
        &DateParseOptions {
            today: Some(TODAY),
            iso_is_gregorian: true,
            ..Default::default()
        },
    );
    assert_eq!(
        result,
        DateParse::Value {
            date: TODAY,
            is_complete: true
        }
    );
}

#[test]
fn fr10_buddhist_years_read_literally() {
    assert_eq!(iso("th-TH", Buddhist, "2/11/2569"), "2026-11-02");
    assert_eq!(iso("th-TH", Buddhist, "2/11/2026"), "1483-11-02");
}

#[test]
fn fr10_japanese_years() {
    assert_eq!(iso("ja-JP", Japanese, "8/11/25"), "2026-11-25");
    assert_eq!(iso("ja-JP", Japanese, "2026/11/2"), "2026-11-02");
    assert_eq!(iso("ja-JP", Japanese, "令和元年5月1日"), "2019-05-01");
}

#[test]
fn fr10_rejects_a_japanese_era_year_outside_its_era() {
    assert_eq!(iso("ja-JP", Japanese, "平成31年4月30日"), "2019-04-30");
    assert_eq!(iso("ja-JP", Japanese, "平成31年5月1日"), "nonexistent");
    assert_eq!(iso("ja-JP", Japanese, "令和1年4月30日"), "nonexistent");
    assert_eq!(iso("ja-JP", Japanese, "S64/1/8"), "nonexistent");
    assert_eq!(iso("en-US", Japanese, "May 1, 31 Heisei"), "nonexistent");
}

#[test]
fn fr10_minguo_years() {
    assert_eq!(iso("zh-TW", Roc, "115/11/2"), "2026-11-02");
    assert_eq!(iso("zh-TW", Roc, "民國115/11/2"), "2026-11-02");
    assert_eq!(iso("zh-TW", Roc, "民國前12年1月1日"), "1900-01-01");
}

#[test]
fn fr10_ethiopic_years() {
    // ICU4X's CLDR data writes Amharic Ethiopic dates month first, where
    // V8's writes them day first; the field reads the order it writes.
    assert_eq!(iso("am-ET", Ethiopic, "2/23/2019"), "2026-11-02");
    let early = format_calendar_date_text(
        PlainDate {
            year: 1,
            month: 1,
            day: 1,
        },
        SharedDateFormat::Date,
        "en",
        Ethiopic,
    );
    assert!(early.starts_with("Ter 8, 5493 "), "{early}");
}

#[test]
fn fr10_two_digit_years_window() {
    assert_eq!(iso("en-US", Gregory, "1/2/45"), "2045-01-02");
    assert_eq!(iso("en-US", Gregory, "1/2/46"), "1946-01-02");
    assert_eq!(iso("th-TH", Buddhist, "2/11/69"), "2026-11-02");
    assert_eq!(iso("fa-IR", Persian, "05/8/11"), "2026-11-02");
}

#[test]
fn fr10_the_era_shows_in_the_hint_and_the_committed_text() {
    let hint = format_calendar_date_hint(TODAY, "th-TH", Buddhist);
    assert!(hint.contains("พ.ศ."), "{hint}");
    assert_eq!(
        format_calendar_date_text(TODAY, SharedDateFormat::DateLong, "th-TH", Buddhist),
        "2 พฤศจิกายน พ.ศ. 2569"
    );
    assert!(format_calendar_date_hint(TODAY, "zh-TW", Roc).contains("民國"));
    assert!(
        format_calendar_date_text(TODAY, SharedDateFormat::Date, "zh-TW", Roc).contains("民國")
    );
}

#[test]
fn fr11_dates_the_calendar_lacks_are_not_adjusted() {
    assert_eq!(iso("fa-IR", Persian, "1404/12/30"), "nonexistent");
    assert_eq!(iso("fa-IR", Persian, "1403/12/30"), "2025-03-20");
    assert_eq!(iso("en-US", Hebrew, "1 Adar I 5785"), "nonexistent");
    assert_eq!(iso("am-ET", Ethiopic, "13/6/2016"), "nonexistent");
    assert_eq!(iso("am-ET", Ethiopic, "13/6/2015"), "2023-09-11");
    assert_eq!(iso("ar-SA", IslamicUmalqura, "30/1/1448"), "nonexistent");
    assert_eq!(iso("zh-CN", Chinese, "2026年闰六月1"), "nonexistent");
    assert_eq!(iso("en-US", Gregory, "2/30/2026"), "nonexistent");
}

#[test]
fn fr11_unreadable_and_partial_text_apart() {
    assert_eq!(iso("en-US", Gregory, "not a date"), "unreadable");
    assert_eq!(iso("en-US", Gregory, "25/1/2026"), "unreadable");
    assert_eq!(iso("en-US", Gregory, "No"), "incomplete");
    assert_eq!(iso("fa-IR", Persian, "۱۴۰۵/۸"), "incomplete");
    assert_eq!(iso("en-US", Gregory, "   "), "empty");
}

#[test]
fn fr11_constraints_report_out_of_range() {
    let in_2026 = |d: PlainDate| d.year == 2026;
    let result = parse_calendar_date(
        "1405/8/11",
        &calendar_date_lexicon("fa-IR", Persian),
        &DateParseOptions {
            today: Some(TODAY),
            is_disabled: Some(&in_2026),
            ..Default::default()
        },
    );
    assert_eq!(
        result,
        DateParse::OutOfRange {
            date: TODAY,
            is_complete: true
        }
    );
}

#[test]
fn fr12_only_finished_day_first_text_is_complete() {
    let typed = "12/01/2026";
    let done: Vec<&str> = (1..=typed.len())
        .map(|n| &typed[..n])
        .filter(|text| {
            matches!(
                parse("en-GB", Gregory, text),
                DateParse::Value {
                    is_complete: true,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(done, vec!["12/01/2026"]);
}

#[test]
fn fr12_short_and_omitted_years_wait_for_enter_or_blur() {
    assert_eq!(complete("en-GB", Gregory, "12/01/90"), Some(false));
    assert_eq!(iso("en-GB", Gregory, "12/01/90"), "1990-01-12");
    assert_eq!(complete("en-US", Gregory, "Jan 25"), Some(false));
    assert_eq!(iso("en-US", Gregory, "Jan 25"), "2026-01-25");
}

#[test]
fn fr12_waits_while_a_field_can_grow() {
    assert_eq!(complete("en-US", Gregory, "11/2/2026/"), Some(false));
    assert_eq!(complete("fa-IR", Persian, "1405/8/3"), Some(false));
    assert_eq!(complete("fa-IR", Persian, "1405/8/4"), Some(true));
    assert_eq!(complete("ja-JP", Japanese, "令和8年11月2"), Some(false));
    assert_eq!(complete("ja-JP", Japanese, "令和8年11月2日"), Some(true));
    assert_eq!(complete("ko-KR", Dangi, "2026. 9. 2"), Some(false));
    assert_eq!(complete("ko-KR", Dangi, "2026. 9. 2."), Some(true));
    assert_eq!(complete("fa-IR", Persian, "1404/12/3"), Some(false));
    assert_eq!(complete("sv-SE", Gregory, "2026-02-3"), Some(false));
}

#[test]
fn fr12_an_era_year_with_its_era_is_complete() {
    assert_eq!(complete("ja-JP", Japanese, "令和8年11月25日"), Some(true));
    assert_eq!(
        complete("en-US", Japanese, "November 25, 8 Reiwa"),
        Some(true)
    );
}

#[test]
fn fr13_gregorian_text_is_the_shared_text() {
    for format in [
        SharedDateFormat::Date,
        SharedDateFormat::DateLong,
        SharedDateFormat::DateWeekday,
    ] {
        assert_eq!(
            format_calendar_date_text(TODAY, format, "en-US", Gregory),
            format_shared_date(TODAY, format, "en-US", Gregory)
        );
    }
    assert_eq!(
        format_calendar_date_text(TODAY, SharedDateFormat::SystemDate, "fa-IR", Persian),
        "2026-11-02"
    );
}

#[test]
fn fr13_named_formats_in_the_calendar() {
    assert_eq!(
        format_calendar_date_text(TODAY, SharedDateFormat::DateLong, "en-US", Persian),
        "Aban 11, 1405 AP"
    );
    assert_eq!(
        format_calendar_date_text(TODAY, SharedDateFormat::DateLong, "en-US", Hebrew),
        "22 Heshvan 5787 AM"
    );
}

#[test]
fn fr13_chinese_and_dangi_years_round_trip() {
    for locale in ["de-DE", "ja"] {
        for calendar in [Chinese, Dangi] {
            let text =
                format_calendar_date_text(TODAY, SharedDateFormat::DateLong, locale, calendar);
            assert!(text.contains("2026"), "{text}");
            assert_eq!(
                iso(locale, calendar, &text),
                "2026-11-02",
                "{locale} {calendar:?} {text}"
            );
        }
    }
}

#[test]
fn fr13_the_hint_is_the_numeric_pattern() {
    assert_eq!(
        format_calendar_date_hint(TODAY, "fa-IR", Persian),
        "۱۴۰۵/۸/۱۱"
    );
    assert_eq!(
        format_calendar_date_hint(TODAY, "en-US", Gregory),
        "11/2/2026"
    );
}

#[test]
fn ar2_calendars_go_by_their_cldr_names() {
    assert_eq!(calendar_display_name(Persian, "en"), "Persian Calendar");
    assert_eq!(calendar_display_name(Hebrew, "he"), "לוח השנה העברי");
}

fn round_trip_failures(locale: &str, calendar: CalendarSystem, dates: &[PlainDate]) -> Vec<String> {
    let lexicon = calendar_date_lexicon(locale, calendar);
    let mut failed = Vec::new();
    for date in dates {
        let texts = [
            format_calendar_date_hint(*date, locale, calendar),
            format_calendar_date_text(*date, SharedDateFormat::Date, locale, calendar),
            format_calendar_date_text(*date, SharedDateFormat::DateLong, locale, calendar),
            format_calendar_date_text(*date, SharedDateFormat::DateWeekday, locale, calendar),
        ];
        for text in texts {
            let read = parse_calendar_date(
                &text,
                &lexicon,
                &DateParseOptions {
                    today: Some(TODAY),
                    ..Default::default()
                },
            );
            if !matches!(read, DateParse::Value { date: d, .. } if d == *date) && failed.len() < 5 {
                failed.push(format!(
                    "{calendar:?} {locale} {date}: {text:?} -> {read:?}"
                ));
            }
        }
    }
    failed
}

fn sample_days(step: i64, offset: i64) -> Vec<PlainDate> {
    let first = PlainDate {
        year: 1900,
        month: 1,
        day: 1,
    };
    let days = first.days_until(PlainDate {
        year: 2100,
        month: 12,
        day: 31,
    });
    (0..)
        .map(|k| offset + k * step)
        .take_while(|d| *d <= days)
        .map(|d| first.add_days(d))
        .collect()
}

/// FR13: committed text and the format hint read back as the same date for
/// every calendar, in its native locale, English and Swedish, sampled from
/// 1900 through 2100 as upstream's round-trip matrix samples them.
#[test]
fn fr13_committed_text_reads_back_as_the_same_date() {
    let native = [
        (Gregory, "en-GB"),
        (Iso8601, "en-CA"),
        (Buddhist, "th-TH"),
        (Roc, "zh-TW"),
        (Japanese, "ja-JP"),
        (Persian, "fa-IR"),
        (Indian, "hi-IN"),
        (Hebrew, "he"),
        (Chinese, "zh-CN"),
        (Dangi, "ko-KR"),
        (Coptic, "ar-EG"),
        (Ethiopic, "am-ET"),
        (Ethioaa, "am-ET"),
        (IslamicCivil, "ar-EG"),
        (IslamicTbla, "ar"),
        (IslamicUmalqura, "ar-SA"),
    ];
    let mut failed = Vec::new();
    for (calendar, locale) in native {
        let step = if matches!(calendar, Chinese | Dangi) {
            42
        } else {
            14
        };
        failed.extend(round_trip_failures(locale, calendar, &sample_days(step, 0)));
        failed.extend(round_trip_failures(
            "en",
            calendar,
            &sample_days(step * 3, 2),
        ));
        failed.extend(round_trip_failures(
            "sv-SE",
            calendar,
            &sample_days(step * 3, 4),
        ));
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Weekday words that name months or eras, digit month names, cycle years,
/// eras left out of a numeric pattern, numeric patterns with two separators.
#[test]
fn fr13_more_locales_read_back() {
    let more: &[(CalendarSystem, &str)] = &[
        (Gregory, "fr-FR it-IT es-ES ro-RO vi-VN"),
        (Buddhist, "fr-FR hi-IN"),
        (Roc, "de-DE am-ET"),
        (Japanese, "it-IT hu-HU am-ET"),
        (Persian, "nb-NO"),
        (Hebrew, "lv-LV"),
        (
            Chinese,
            "ca-ES cs-CZ de-DE fi-FI fr-FR it-IT nl-NL pl-PL pt-PT ru-RU",
        ),
        (Dangi, "pt-BR fi-FI"),
        (Ethiopic, "pt-BR"),
        (IslamicCivil, "fr-FR hu-HU sr-RS lv-LV"),
        (IslamicTbla, "lv-LV"),
        (IslamicUmalqura, "lv-LV"),
    ];
    let mut failed = Vec::new();
    for (calendar, locales) in more {
        for locale in locales.split(' ') {
            failed.extend(round_trip_failures(locale, *calendar, &sample_days(997, 1)));
        }
    }
    type Edge = (&'static str, CalendarSystem, &'static [(i32, u8, u8)]);
    let edges: &[Edge] = &[
        (
            "lv-LV",
            Hebrew,
            &[(1900, 2, 1), (1900, 3, 2), (1901, 2, 20)],
        ),
        ("lv-LV", IslamicCivil, &[(1900, 7, 1), (1900, 8, 26)]),
        (
            "am-ET",
            Roc,
            &[(1900, 1, 1), (1911, 12, 31), (1912, 1, 1), (2026, 1, 1)],
        ),
        ("ksh", Roc, &[(1900, 1, 1), (1912, 1, 1)]),
        (
            "am-ET",
            Japanese,
            &[
                (1900, 1, 1),
                (1950, 6, 15),
                (2000, 6, 15),
                (2019, 5, 1),
                (2026, 1, 1),
            ],
        ),
        ("hi-IN", Buddhist, &[(1900, 1, 1), (2026, 11, 2)]),
    ];
    for (locale, calendar, days) in edges {
        let days: Vec<PlainDate> = days
            .iter()
            .map(|(y, m, d)| PlainDate {
                year: *y,
                month: *m,
                day: *d,
            })
            .collect();
        failed.extend(round_trip_failures(locale, *calendar, &days));
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn fr10_a_two_digit_year_before_a_latvian_month_name() {
    let lexicon = calendar_date_lexicon("lv-LV", Hebrew);
    let options = DateParseOptions {
        today: Some(TODAY),
        ..Default::default()
    };
    for (text, expected) in [
        ("86. g. 1. adars", (2026, 2, 18)),
        ("26. g. 1. adars", (1966, 2, 21)),
    ] {
        let read = parse_calendar_date(text, &lexicon, &options);
        let expected = PlainDate {
            year: expected.0,
            month: expected.1,
            day: expected.2,
        };
        assert!(
            matches!(read, DateParse::Value { date, .. } if date == expected),
            "{text}: {read:?}"
        );
    }
}
