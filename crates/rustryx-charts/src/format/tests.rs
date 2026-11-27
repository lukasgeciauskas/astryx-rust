//! Upstream `formatters.test.ts`, then outputs checked against Node's `Intl` (full
//! ICU) for the locales and magnitudes charts meet, and the date coercion's
//! parsing rules. Expected strings come from upstream's formatters run under Node.

use rustryx_i18n::CalendarSystem::{self, Buddhist, Gregory, Hebrew, Iso8601, Persian};

use super::*;
use crate::js_string;

fn num(n: f64) -> ChartValue {
    ChartValue::Number(n)
}

fn text(s: &str) -> ChartValue {
    ChartValue::Text(SharedString::from(s.to_string()))
}

fn compact(value: ChartValue, locale: &str) -> String {
    compact_number(Some(&value), locale)
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn pct(value: ChartValue, locale: &str) -> String {
    percent(Some(&value), locale)
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn money(format: &ChartValueFormat, value: ChartValue, locale: &str) -> String {
    format
        .format(Some(&value), locale, Gregory)
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn short(value: &str, locale: &str, calendar: CalendarSystem) -> String {
    short_date(Some(&text(value)), locale, calendar)
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn month(value: &str, locale: &str, calendar: CalendarSystem) -> String {
    month_year(Some(&text(value)), locale, calendar)
        .map(|s| s.to_string())
        .unwrap_or_default()
}

#[test]
fn formats_numbers_with_the_requested_locale_and_keeps_caches_locale_keyed() {
    let en = compact(num(1_500_000.), "en-US");
    let de = compact(num(1_500_000.), "de-DE");
    assert_eq!(en, "1.5M");
    assert_eq!(de, "1,5\u{a0}Mio.");
    assert_ne!(de, en);
    assert_eq!(compact(num(1_500_000.), "en-US"), en);
}

#[test]
fn formats_currency_and_percentages_with_the_requested_locale() {
    assert_eq!(
        money(&currency("€", Some("de-DE")), num(1234.5), "en-US"),
        "€1234,5"
    );
    assert_eq!(pct(num(0.125), "de-DE"), "12,5\u{a0}%");
}

#[test]
fn keeps_the_symbol_only_currency_call() {
    assert!(money(&currency("€", None), num(1234.5), "en-US").starts_with('€'));
}

#[test]
fn formats_dates_with_the_requested_locale() {
    assert_eq!(short("2024-01-05", "de-DE", Gregory), "5. Jan.");
    assert_eq!(short("2024-01-05", "en-US", Gregory), "Jan 5");
    assert_ne!(
        short("2024-01-05", "de-DE", Gregory),
        short("2024-01-05", "en-US", Gregory)
    );
    assert_eq!(month("2024-01-05", "de-DE", Gregory), "Jan. 2024");
    assert_eq!(month("2024-01-05", "fr-FR", Gregory), "janv. 2024");
}

#[test]
fn month_year_names_the_month_where_intl_does() {
    assert_eq!(month("2024-01-05", "ja-JP", Gregory), "2024年1月");
    assert_eq!(month("2024-01-05", "ko-KR", Gregory), "2024년 1월");
    assert_eq!(
        month("2024-01-05", "ar-EG", Gregory),
        "\u{64a}\u{646}\u{627}\u{64a}\u{631} \u{662}\u{660}\u{662}\u{664}"
    );
    assert_eq!(month("2024-01-05", "de-AT", Gregory), "Jän. 2024");
    assert_eq!(month("2026-10-30", "es-ES", Gregory), "oct 2026");
    assert_eq!(month("2026-10-30", "vi-VN", Gregory), "thg 10 2026");
    // Lithuanian's yMMM is numeric.
    assert_eq!(month("2024-01-05", "lt", Gregory), "2024-01");
    // yMMM writes no era.
    assert_eq!(month("0100-01-01", "en-US", Gregory), "Jan 100");
    assert_eq!(month("2024-01-05", "ja-JP", Iso8601), "2024年1月");
}

#[test]
fn keeps_gregorian_dates_for_locales_that_default_to_another_calendar() {
    assert_eq!(
        short("2026-08-22", "fa-IR", Gregory),
        "\u{6f2}\u{6f2} \u{627}\u{648}\u{62a}"
    );
    assert_eq!(
        month("2026-08-22", "th-TH", Gregory),
        "\u{e2a}.\u{e04}. 2026"
    );
}

#[test]
fn formats_dates_in_an_explicit_calendar() {
    let cases: [(CalendarSystem, &str, &str, &str, &str); 6] = [
        (Persian, "en", "1405", "Aban 8", "Aban 1405 AP"),
        (
            Persian,
            "fa",
            "\u{6f1}\u{6f4}\u{6f0}\u{6f5}",
            "\u{6f8} \u{622}\u{628}\u{627}\u{646}",
            "\u{622}\u{628}\u{627}\u{646} \u{6f1}\u{6f4}\u{6f0}\u{6f5}",
        ),
        (Hebrew, "en", "5787", "19 Heshvan", "Heshvan 5787"),
        (
            Hebrew,
            "he",
            "5787",
            "19 \u{5d1}\u{5d7}\u{5e9}\u{5d5}\u{5d5}\u{5df}",
            "\u{5d7}\u{5e9}\u{5d5}\u{5d5}\u{5df} 5787",
        ),
        (Buddhist, "en", "2569", "Oct 30", "Oct 2569 BE"),
        (
            Buddhist,
            "th",
            "2569",
            "30 \u{e15}.\u{e04}.",
            "\u{e15}.\u{e04}. 2569",
        ),
    ];
    for (calendar, locale, year, short_label, month_label) in cases {
        assert_eq!(
            short("2026-10-30", locale, calendar),
            short_label,
            "{calendar:?} {locale}"
        );
        let label = month("2026-10-30", locale, calendar);
        assert_eq!(label, month_label, "{calendar:?} {locale}");
        assert!(label.contains(year), "{label}");
        assert!(!label.contains("2026"), "{label}");
    }
}

#[test]
fn defaults_to_gregorian_and_writes_iso8601_dates_as_gregorian_ones() {
    assert_eq!(month("2026-10-30", "en", Gregory), "Oct 2026");
    assert_eq!(month("2026-10-30", "en", Iso8601), "Oct 2026");
    assert_eq!(
        short("2026-10-30", "en", Iso8601),
        short("2026-10-30", "en", Gregory)
    );
}

#[test]
fn passes_through_non_finite_and_unparseable_values() {
    assert_eq!(compact(num(f64::INFINITY), "en-US"), "Infinity");
    assert_eq!(pct(text("not-a-number"), "en-US"), "not-a-number");
    assert_eq!(short("not-a-date", "en-US", Gregory), "not-a-date");
    assert_eq!(month("not-a-date", "en-US", Gregory), "not-a-date");
}

fn built_ins() -> [(&'static str, ChartValueFormat); 6] {
    [
        ("compactNumber", ChartValueFormat::compact_number()),
        ("percent", ChartValueFormat::percent()),
        ("shortDate", ChartValueFormat::short_date()),
        ("monthYear", ChartValueFormat::month_year()),
        ("currency('$')", currency("$", None)),
        (
            "currency('$', 'en-US')",
            ChartValueFormat::currency("$", Some("en-US")),
        ),
    ]
}

#[test]
fn every_built_in_returns_none_for_null_and_undefined() {
    for (name, format) in built_ins() {
        assert_eq!(
            format.format(Some(&ChartValue::Null), "en-US", Gregory),
            None,
            "{name}"
        );
        assert_eq!(format.format(None, "en-US", Gregory), None, "{name}");
    }
}

#[test]
fn keeps_the_output_for_present_values_zero_included() {
    assert_eq!(compact(num(0.), "en-US"), "0");
    assert_eq!(pct(num(0.), "en-US"), "0%");
    assert_eq!(money(&currency("$", Some("en-US")), num(0.), "en-US"), "$0");
    assert_eq!(month("2024-01-05", "en-US", Gregory), "Jan 2024");
}

#[test]
fn a_closure_is_a_format() {
    let format: ChartValueFormat =
        (|value: Option<&ChartValue>, _: &str, _: CalendarSystem| Some(js_string(value))).into();
    assert_eq!(
        format.format(Some(&num(3.)), "en", Gregory).as_deref(),
        Some("3")
    );
    assert_eq!(
        format.format(None, "en", Gregory).as_deref(),
        Some("undefined")
    );
    let custom =
        ChartValueFormat::new(|value, _, _| Some(format!("{} ms", js_string(value)).into()));
    assert_eq!(
        custom.format(Some(&num(5.)), "en", Gregory).as_deref(),
        Some("5 ms")
    );
}

#[test]
fn currency_uses_the_factory_locale_when_only_the_factory_has_one() {
    assert_eq!(
        money(&currency("€", Some("de-DE")), num(1234.5), "en-US"),
        "€1234,5"
    );
}

#[test]
fn currency_uses_the_call_locale_when_the_factory_has_none() {
    assert_eq!(money(&currency("€", None), num(1234.5), "de-DE"), "€1234,5");
    assert_eq!(money(&currency("€", None), num(12.5), "de-DE"), "€12,5");
}

#[test]
fn currency_keeps_the_factory_locale_when_both_are_given() {
    assert_eq!(
        money(&currency("€", Some("en-US")), num(1234.5), "de-DE"),
        "€1.2K"
    );
    assert_eq!(
        money(&currency("€", Some("de-DE")), num(1234.5), "en-US"),
        "€1234,5"
    );
}

/// compactNumber, percent and currency('$') of 0, -0, 0.005, -0.005, 999, 999.95,
/// 1200, 1.5e6, 1e12, 1e15, -1500, -2.5e6, NaN, Infinity and -Infinity.
const MAGNITUDES: [f64; 15] = [
    0.,
    -0.,
    0.005,
    -0.005,
    999.,
    999.95,
    1200.,
    1_500_000.,
    1e12,
    1e15,
    -1500.,
    -2_500_000.,
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
];

const NON_FINITE: [[&str; 3]; 3] = [
    ["NaN", "NaN", "NaN"],
    ["Infinity", "Infinity", "Infinity"],
    ["-Infinity", "-Infinity", "-Infinity"],
];

fn assert_magnitudes<S: AsRef<str>>(locale: &str, expected: &[[S; 3]; 12], percent_matches: bool) {
    let dollars = currency("$", None);
    for (i, n) in MAGNITUDES.iter().copied().enumerate() {
        let [c, p, m] = match expected.get(i) {
            Some(row) => row.each_ref().map(AsRef::as_ref),
            None => NON_FINITE[i - expected.len()],
        };
        assert_eq!(compact(num(n), locale), c, "compactNumber({n}, {locale})");
        if percent_matches {
            assert_eq!(pct(num(n), locale), p, "percent({n}, {locale})");
        }
        assert_eq!(
            money(&dollars, num(n), locale),
            m,
            "currency('$')({n}, {locale})"
        );
    }
}

#[test]
fn numbers_match_intl_across_magnitudes_in_english() {
    assert_magnitudes(
        "en-US",
        &[
            ["0", "0%", "$0"],
            ["-0", "-0%", "$0"],
            ["0", "0.5%", "$0.005"],
            ["-0", "-0.5%", "-$0.005"],
            ["999", "99,900%", "$999"],
            ["1K", "99,995%", "$999.95"],
            ["1.2K", "120,000%", "$1.2K"],
            ["1.5M", "150,000,000%", "$1.5M"],
            ["1T", "100,000,000,000,000%", "$1T"],
            ["1000T", "100,000,000,000,000,000%", "$1000T"],
            ["-1.5K", "-150,000%", "-$1.5K"],
            ["-2.5M", "-250,000,000%", "-$2.5M"],
        ],
        true,
    );
}

#[test]
fn numbers_match_intl_across_magnitudes_in_german_and_french() {
    assert_magnitudes(
        "de-DE",
        &[
            ["0", "0\u{a0}%", "$0"],
            ["-0", "-0\u{a0}%", "$0"],
            ["0", "0,5\u{a0}%", "$0,005"],
            ["-0", "-0,5\u{a0}%", "-$0,005"],
            ["999", "99.900\u{a0}%", "$999"],
            ["1000", "99.995\u{a0}%", "$999,95"],
            ["1200", "120.000\u{a0}%", "$1200"],
            ["1,5\u{a0}Mio.", "150.000.000\u{a0}%", "$1,5\u{a0}Mio."],
            ["1\u{a0}Bio.", "100.000.000.000.000\u{a0}%", "$1\u{a0}Bio."],
            [
                "1000\u{a0}Bio.",
                "100.000.000.000.000.000\u{a0}%",
                "$1000\u{a0}Bio.",
            ],
            ["-1500", "-150.000\u{a0}%", "-$1500"],
            ["-2,5\u{a0}Mio.", "-250.000.000\u{a0}%", "-$2,5\u{a0}Mio."],
        ],
        true,
    );
    assert_magnitudes(
        "fr-FR",
        &[
            ["0", "0\u{a0}%", "$0"],
            ["-0", "-0\u{a0}%", "$0"],
            ["0", "0,5\u{a0}%", "$0,005"],
            ["-0", "-0,5\u{a0}%", "-$0,005"],
            ["999", "99\u{202f}900\u{a0}%", "$999"],
            ["1\u{a0}k", "99\u{202f}995\u{a0}%", "$999,95"],
            ["1,2\u{a0}k", "120\u{202f}000\u{a0}%", "$1,2\u{a0}k"],
            [
                "1,5\u{a0}M",
                "150\u{202f}000\u{202f}000\u{a0}%",
                "$1,5\u{a0}M",
            ],
            [
                "1\u{a0}Bn",
                "100\u{202f}000\u{202f}000\u{202f}000\u{202f}000\u{a0}%",
                "$1\u{a0}Bn",
            ],
            [
                "1000\u{a0}Bn",
                "100\u{202f}000\u{202f}000\u{202f}000\u{202f}000\u{202f}000\u{a0}%",
                "$1000\u{a0}Bn",
            ],
            ["-1,5\u{a0}k", "-150\u{202f}000\u{a0}%", "-$1,5\u{a0}k"],
            [
                "-2,5\u{a0}M",
                "-250\u{202f}000\u{202f}000\u{a0}%",
                "-$2,5\u{a0}M",
            ],
        ],
        true,
    );
}

#[test]
fn numbers_match_intl_across_magnitudes_in_japanese_hindi_and_portuguese() {
    assert_magnitudes(
        "ja-JP",
        &[
            ["0", "0%", "$0"],
            ["-0", "-0%", "$0"],
            ["0", "0.5%", "$0.005"],
            ["-0", "-0.5%", "-$0.005"],
            ["999", "99,900%", "$999"],
            ["1000", "99,995%", "$999.95"],
            ["1200", "120,000%", "$1200"],
            ["150\u{4e07}", "150,000,000%", "$150\u{4e07}"],
            ["1\u{5146}", "100,000,000,000,000%", "$1\u{5146}"],
            ["1000\u{5146}", "100,000,000,000,000,000%", "$1000\u{5146}"],
            ["-1500", "-150,000%", "-$1500"],
            ["-250\u{4e07}", "-250,000,000%", "-$250\u{4e07}"],
        ],
        true,
    );
    let k = "\u{a0}\u{939}\u{91c}\u{93c}\u{93e}\u{930}";
    let lakh = "\u{a0}\u{932}\u{93e}\u{916}";
    let rows: [[String; 3]; 12] = [
        ["0", "0%", "$0"].map(String::from),
        ["-0", "-0%", "$0"].map(String::from),
        ["0", "0.5%", "$0.005"].map(String::from),
        ["-0", "-0.5%", "-$0.005"].map(String::from),
        ["999", "99,900%", "$999"].map(String::from),
        [format!("1{k}"), "99,995%".into(), "$999.95".into()],
        [format!("1.2{k}"), "1,20,000%".into(), format!("$1.2{k}")],
        [
            format!("15{lakh}"),
            "15,00,00,000%".into(),
            format!("$15{lakh}"),
        ],
        [
            "10\u{a0}\u{916}\u{970}",
            "10,00,00,00,00,00,000%",
            "$10\u{a0}\u{916}\u{970}",
        ]
        .map(String::from),
        [
            "100\u{a0}\u{928}\u{940}\u{932}",
            "1,00,00,00,00,00,00,00,000%",
            "$100\u{a0}\u{928}\u{940}\u{932}",
        ]
        .map(String::from),
        [format!("-1.5{k}"), "-1,50,000%".into(), format!("-$1.5{k}")],
        [
            format!("-25{lakh}"),
            "-25,00,00,000%".into(),
            format!("-$25{lakh}"),
        ],
    ];
    assert_magnitudes("hi-IN", &rows, true);
    assert_magnitudes(
        "pt-BR",
        &[
            ["0", "0%", "$0"],
            ["-0", "-0%", "$0"],
            ["0", "0,5%", "$0,005"],
            ["-0", "-0,5%", "-$0,005"],
            ["999", "99.900%", "$999"],
            ["1\u{a0}mil", "99.995%", "$999,95"],
            ["1,2\u{a0}mil", "120.000%", "$1,2\u{a0}mil"],
            ["1,5\u{a0}mi", "150.000.000%", "$1,5\u{a0}mi"],
            ["1\u{a0}tri", "100.000.000.000.000%", "$1\u{a0}tri"],
            [
                "1000\u{a0}tri",
                "100.000.000.000.000.000%",
                "$1000\u{a0}tri",
            ],
            ["-1,5\u{a0}mil", "-150.000%", "-$1,5\u{a0}mil"],
            ["-2,5\u{a0}mi", "-250.000.000%", "-$2,5\u{a0}mi"],
        ],
        true,
    );
}

#[test]
fn numbers_match_intl_in_egyptian_arabic_digits() {
    // ICU4X writes the Latin percent sign (U+200E % U+200E) with Arabic digits,
    // where Intl writes U+066A U+061C, so only the compact and currency columns
    // are compared.
    let thousand = "\u{a0}\u{623}\u{644}\u{641}";
    let million = "\u{a0}\u{645}\u{644}\u{64a}\u{648}\u{646}";
    let trillion = "\u{a0}\u{62a}\u{631}\u{644}\u{64a}\u{648}\u{646}";
    let rows: [[String; 3]; 12] = [
        ["\u{660}".into(), String::new(), "$\u{660}".into()],
        ["\u{61c}-\u{660}".into(), String::new(), "$\u{660}".into()],
        [
            "\u{660}".into(),
            String::new(),
            "$\u{660}\u{66b}\u{660}\u{660}\u{665}".into(),
        ],
        [
            "\u{61c}-\u{660}".into(),
            String::new(),
            "-$\u{660}\u{66b}\u{660}\u{660}\u{665}".into(),
        ],
        [
            "\u{669}\u{669}\u{669}".into(),
            String::new(),
            "$\u{669}\u{669}\u{669}".into(),
        ],
        [
            format!("\u{661}{thousand}"),
            String::new(),
            "$\u{669}\u{669}\u{669}\u{66b}\u{669}\u{665}".into(),
        ],
        [
            format!("\u{661}\u{66b}\u{662}{thousand}"),
            String::new(),
            format!("$\u{661}\u{66b}\u{662}{thousand}"),
        ],
        [
            format!("\u{661}\u{66b}\u{665}{million}"),
            String::new(),
            format!("$\u{661}\u{66b}\u{665}{million}"),
        ],
        [
            format!("\u{661}{trillion}"),
            String::new(),
            format!("$\u{661}{trillion}"),
        ],
        [
            format!("\u{661}\u{660}\u{660}\u{660}{trillion}"),
            String::new(),
            format!("$\u{661}\u{660}\u{660}\u{660}{trillion}"),
        ],
        [
            format!("\u{61c}-\u{661}\u{66b}\u{665}{thousand}"),
            String::new(),
            format!("-$\u{661}\u{66b}\u{665}{thousand}"),
        ],
        [
            format!("\u{61c}-\u{662}\u{66b}\u{665}{million}"),
            String::new(),
            format!("-$\u{662}\u{66b}\u{665}{million}"),
        ],
    ];
    assert_magnitudes("ar-EG", &rows, false);
}

#[test]
fn values_coerce_as_number_and_string_do() {
    assert_eq!(compact(text("1200"), "en-US"), "1.2K");
    assert_eq!(compact(ChartValue::Bool(true), "en-US"), "1");
    assert_eq!(compact(ChartValue::Date(f64::NAN), "en-US"), "Invalid Date");
    assert_eq!(pct(text(""), "en-US"), "0%");
    assert_eq!(pct(text("  "), "en-US"), "0%");
    assert_eq!(compact(text("0x10"), "en-US"), "16");
    assert_eq!(compact(text("12px"), "en-US"), "12px");
    assert_eq!(pct(ChartValue::Bool(false), "en-US"), "0%");
}

#[test]
fn lists_join_as_a_long_conjunction() {
    let lists: [(&str, [&str; 4]); 6] = [
        ("en-US", ["", "a", "a and b", "a, b, and c"]),
        ("de-DE", ["", "a", "a und b", "a, b und c"]),
        ("fr-FR", ["", "a", "a et b", "a, b et c"]),
        ("ja-JP", ["", "a", "a\u{3001}b", "a\u{3001}b\u{3001}c"]),
        ("ar-EG", ["", "a", "a \u{648}b", "a \u{648}b \u{648}c"]),
        ("pt-BR", ["", "a", "a e b", "a, b e c"]),
    ];
    for (locale, [none, one, two, three]) in lists {
        assert_eq!(format_list(Vec::<&str>::new(), locale), none);
        assert_eq!(format_list(["a"], locale), one);
        assert_eq!(format_list(["a", "b"], locale), two);
        assert_eq!(format_list(["a", "b", "c"], locale), three);
    }
}

#[test]
fn date_strings_coerce_as_upstream_reads_them() {
    let cases: [(&str, &str, &str); 12] = [
        // A month and day alone take V8's default year, 2001.
        ("Jan 5", "Jan 5", "Jan 2001"),
        // Rolled-over and pre-100 date-only strings pass through.
        ("2024-02-31", "2024-02-31", "2024-02-31"),
        ("0050-01-05", "0050-01-05", "0050-01-05"),
        ("2023-02-29", "2023-02-29", "2023-02-29"),
        ("  2024-01-05  ", "Jan 5", "Jan 2024"),
        ("Fri, 05 Jan 2024 10:00:00 GMT", "Jan 5", "Jan 2024"),
        ("1/5/2024", "Jan 5", "Jan 2024"),
        ("January 5, 2024", "Jan 5", "Jan 2024"),
        ("2024-06", "Jun 1", "Jun 2024"),
        // A numeric string is epoch milliseconds.
        ("1704412800000", "Jan 5", "Jan 2024"),
        ("not-a-date", "not-a-date", "not-a-date"),
        ("", "", ""),
    ];
    for (value, short_label, month_label) in cases {
        assert_eq!(short(value, "en-US", Gregory), short_label, "{value:?}");
        assert_eq!(month(value, "en-US", Gregory), month_label, "{value:?}");
    }
}

#[test]
fn date_parse_reads_what_v8_reads() {
    let cases: [(&str, f64); 18] = [
        ("2024-01-05T10:00:00Z", 1_704_448_800_000.),
        ("2024-01-05T23:30:00-05:00", 1_704_515_400_000.),
        ("Fri, 05 Jan 2024 10:00:00 GMT", 1_704_448_800_000.),
        ("Tue, 1 Nov 2016 12:00:00 +0530", 1_477_981_800_000.),
        ("Jan 5 2024 10:00 GMT+0100", 1_704_445_200_000.),
        ("2024-01-05T24:00:00.000Z", 1_704_499_200_000.),
        ("2024-01-05T24:01Z", f64::NAN),
        ("+275760-09-13T00:00:00Z", 8.64e15),
        ("+275760-09-13T00:00:00.001Z", f64::NAN),
        ("-000000-01-01T00:00:00Z", f64::NAN),
        // A date-only ISO string is UTC midnight.
        ("2024-01-05", 1_704_412_800_000.),
        ("2024-02-31", 1_709_337_600_000.),
        ("Sat, 13 Sep 275760 00:00:00 GMT", 8.64e15),
        ("Jan 5 2024 10:00 GMT-8", 1_704_477_600_000.),
        ("2024-01-05T10:00:00.123456Z", 1_704_448_800_123.),
        ("2024-01-05foo", f64::NAN),
        ("Jan 5 2024 10:00 -12345", f64::NAN),
        ("Thu, 01 Jan 1970 00:00:00 GMT (comment)", 0.),
    ];
    for (s, expected) in cases {
        let got = date::date_parse(s);
        assert!(
            got == expected || (got.is_nan() && expected.is_nan()),
            "Date.parse({s:?}) = {got}, want {expected}"
        );
    }
}

#[test]
fn a_date_value_is_a_javascript_time_value() {
    assert_eq!(date::to_date(&num(1.7)), 1.);
    assert_eq!(date::to_date(&num(-1.7)), -1.);
    assert!(date::to_date(&num(8.64e15 + 1.)).is_nan());
    assert_eq!(date::to_date(&ChartValue::Date(5.)), 5.);
    assert!(date::to_date(&ChartValue::Bool(true)).is_nan());
}
