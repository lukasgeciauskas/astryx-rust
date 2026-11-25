//! Typed dates in every calendar, upstream `utils/calendarDateParser.ts`
//! (AST-111 FR9-FR14, DEC-4).
//!
//! One parser reads every calendar, Gregorian included. It learns what a
//! locale writes for a calendar from the formatter's own output (month and
//! era names in every width, the numeric field order, separators, weekday
//! words), so the formats a field shows are the formats it reads. Years are
//! read literally in the calendar.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use super::calendar::{
    CalendarDateFields, calendar_date_from_fields, calendar_era_year, calendar_month_of,
    calendar_months_in_year, calendar_year_of, format_calendar_date, format_calendar_date_to_parts,
};
use super::format::{
    DatePart, DateStyle, PartKind, SharedDateFormat, Width, format_date, format_date_to_parts,
    format_shared_date, join_parts,
};
use super::plain::{PlainDate, days_in_month};
use super::text::{
    is_letter, is_separator, is_space, is_spaceless, normalize_date_text, without_non_words,
    words_only,
};
use crate::CalendarSystem;

/// How typed text reads in a calendar (`CalendarDateParseResult`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateParse {
    Empty,
    /// Part of a date: nothing to report yet.
    Incomplete,
    /// Text that cannot become a date.
    Unreadable,
    /// A readable date that the calendar does not have.
    Nonexistent {
        is_complete: bool,
    },
    /// A date the constraints reject.
    OutOfRange {
        date: PlainDate,
        is_complete: bool,
    },
    /// A date; `is_complete` when more typing cannot change its meaning.
    Value {
        date: PlainDate,
        is_complete: bool,
    },
}

/// Options for [`parse_calendar_date`] (`CalendarDateParseOptions`).
#[derive(Default)]
pub struct DateParseOptions<'a> {
    /// The Gregorian date two-digit and omitted years resolve against.
    pub today: Option<PlainDate>,
    /// Reads ISO-shaped text as a Gregorian ISO date, as `system_date` shows.
    pub iso_is_gregorian: bool,
    /// True for a date that may not be chosen.
    pub is_disabled: Option<&'a dyn Fn(PlainDate) -> bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum DateField {
    Year,
    Month,
    Day,
}

#[derive(Clone, Copy, Debug)]
struct EraArithmetic {
    /// The Gregorian year before era year 1, or after it for a backward era.
    base: i32,
    is_forward: bool,
}

#[derive(Clone, Debug)]
struct MonthTemplate {
    pre: String,
    post: String,
    is_leap: bool,
}

/// What a locale writes for one calendar (`CalendarDateLexicon`).
pub struct CalendarDateLexicon {
    locale: String,
    calendar: CalendarSystem,
    order: Vec<DateField>,
    /// The order of the weekday format when it writes the month as a number
    /// in another order: ICU4X's data falls back to a month-first pattern
    /// there for some calendars, such as Arabic Coptic.
    weekday_order: Option<Vec<DateField>>,
    /// The separators between the numbers of each all-numeric format the
    /// field writes, with that format's field order: ICU4X's data orders a
    /// locale's numeric formats differently for some calendars (Finnish
    /// Chinese dates are `1905 5 17` and `17.5.1905`).
    numeric_forms: Vec<(Vec<String>, Vec<DateField>)>,
    name_order: Vec<DateField>,
    months: HashMap<String, BTreeSet<String>>,
    month_templates: Vec<MonthTemplate>,
    leap_markers: HashSet<String>,
    eras: HashMap<String, String>,
    year_aliases: HashMap<String, String>,
    trailers: HashSet<String>,
    year_markers: HashSet<String>,
    separators: HashSet<String>,
    weekdays: HashSet<String>,
    vocabulary: Vec<String>,
    is_numeric_month_ordinal: bool,
    era_arithmetic: HashMap<String, EraArithmetic>,
}

impl CalendarDateLexicon {
    /// The calendar the lexicon reads.
    pub fn calendar(&self) -> CalendarSystem {
        self.calendar
    }
}

const LUNISOLAR: [CalendarSystem; 2] = [CalendarSystem::Chinese, CalendarSystem::Dangi];

fn is_lunisolar(calendar: CalendarSystem) -> bool {
    LUNISOLAR.contains(&calendar)
}

fn without_era(calendar: CalendarSystem) -> bool {
    matches!(
        calendar,
        CalendarSystem::Gregory
            | CalendarSystem::Iso8601
            | CalendarSystem::Chinese
            | CalendarSystem::Dangi
    )
}

/// One date inside each era of a calendar, by era code (`ERA_SAMPLES`).
fn era_samples(calendar: CalendarSystem) -> &'static [(&'static str, (i32, u8, u8))] {
    match calendar {
        CalendarSystem::Gregory => &[("ce", (2000, 6, 15)), ("bce", (-100, 6, 15))],
        CalendarSystem::Japanese => &[
            ("meiji", (1900, 6, 15)),
            ("taisho", (1920, 6, 15)),
            ("showa", (1950, 6, 15)),
            ("heisei", (2000, 6, 15)),
            ("reiwa", (2026, 6, 15)),
        ],
        CalendarSystem::Roc => &[("roc", (2026, 6, 15)), ("broc", (1900, 6, 15))],
        CalendarSystem::Buddhist => &[("be", (2026, 6, 15))],
        CalendarSystem::Ethiopic => &[("am", (2026, 6, 15)), ("aa", (-5600, 6, 15))],
        CalendarSystem::Ethioaa => &[("aa", (2026, 6, 15))],
        CalendarSystem::Coptic => &[("am", (2026, 6, 15))],
        CalendarSystem::Persian => &[("ap", (2026, 6, 15))],
        CalendarSystem::Hebrew => &[("am", (2026, 6, 15))],
        CalendarSystem::Indian => &[("shaka", (2026, 6, 15))],
        CalendarSystem::IslamicUmalqura
        | CalendarSystem::IslamicCivil
        | CalendarSystem::IslamicTbla => &[("ah", (2026, 6, 15))],
        _ => &[],
    }
}

/// The calendar a text field reads and writes: `iso8601` writes as Gregorian.
fn text_calendar(calendar: CalendarSystem) -> CalendarSystem {
    if calendar == CalendarSystem::Iso8601 {
        CalendarSystem::Gregory
    } else {
        calendar
    }
}

fn pd(year: i32, month: u8, day: u8) -> PlainDate {
    PlainDate { year, month, day }
}

fn parts_of(
    locale: &str,
    calendar: CalendarSystem,
    style: DateStyle,
    date: PlainDate,
) -> Vec<DatePart> {
    format_date_to_parts(date, style, locale, calendar)
}

fn find_part(parts: &[DatePart], kind: PartKind) -> Option<&str> {
    parts
        .iter()
        .find(|p| p.kind == kind)
        .map(|p| p.value.as_str())
}

fn field_order(parts: &[DatePart]) -> Vec<DateField> {
    let mut order = Vec::new();
    for part in parts {
        let field = match part.kind {
            PartKind::Year | PartKind::RelatedYear | PartKind::YearName => DateField::Year,
            PartKind::Month => DateField::Month,
            PartKind::Day => DateField::Day,
            _ => continue,
        };
        if !order.contains(&field) {
            order.push(field);
        }
    }
    order
}

thread_local! {
    static LEXICONS: RefCell<HashMap<(String, CalendarSystem), Rc<CalendarDateLexicon>>> =
        RefCell::new(HashMap::new());
    static DISPLAY_STYLES: RefCell<HashMap<(String, CalendarSystem, DateStyle), DateStyle>> =
        RefCell::new(HashMap::new());
    static ERA_CHANGES: RefCell<HashMap<CalendarSystem, bool>> = RefCell::new(HashMap::new());
}

/// The lexicon for a locale and calendar, built on first use and cached
/// (`getCalendarDateLexicon`). Building one formats a few hundred dates; a
/// field asks for it when it gains focus.
pub fn calendar_date_lexicon(locale: &str, calendar: CalendarSystem) -> Rc<CalendarDateLexicon> {
    let calendar = text_calendar(calendar);
    let key = (locale.to_string(), calendar);
    if let Some(lexicon) = LEXICONS.with(|c| c.borrow().get(&key).cloned()) {
        return lexicon;
    }
    let lexicon = Rc::new(build_lexicon(locale, calendar));
    LEXICONS.with(|c| c.borrow_mut().insert(key, lexicon.clone()));
    lexicon
}

struct SampledMonth {
    code: String,
    ordinal: u8,
    mid: PlainDate,
}

/// Months to learn names from: a few calendar years, so leap months appear.
fn sample_months(calendar: CalendarSystem) -> Vec<SampledMonth> {
    if calendar.is_gregorian_aligned() {
        return (1..=12)
            .map(|m| SampledMonth {
                code: format!("M{m:02}"),
                ordinal: m,
                mid: pd(2026, m, 15),
            })
            .collect();
    }
    let reference = calendar_year_of(calendar, pd(2026, 6, 15));
    let mut sampled = Vec::new();
    for year in reference - 1..=reference + 3 {
        let mut starts: Vec<(String, PlainDate)> = Vec::new();
        for n in 1..=13u8 {
            for leap in ["", "L"] {
                let code = format!("M{n:02}{leap}");
                let fields = CalendarDateFields {
                    year: Some(year),
                    month_code: Some(code.clone()),
                    day: 1,
                    ..Default::default()
                };
                if let Some(start) = calendar_date_from_fields(calendar, &fields) {
                    starts.push((code, start));
                }
            }
        }
        starts.sort_by_key(|(_, start)| *start);
        for (i, (code, start)) in starts.into_iter().enumerate() {
            let month = calendar_month_of(calendar, start);
            sampled.push(SampledMonth {
                code,
                ordinal: i as u8 + 1,
                mid: month.start.add_days((month.length / 2) as i64),
            });
        }
    }
    sampled
}

fn digits_split(name: &str) -> Option<(&str, &str, &str)> {
    // ^(\D*)(\d+)(\D*)$
    let start = name.find(|c: char| c.is_ascii_digit())?;
    let end = start
        + name[start..]
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(name.len() - start);
    if name[end..].chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((&name[..start], &name[start..end], &name[end..]))
}

fn build_lexicon(locale: &str, calendar: CalendarSystem) -> CalendarDateLexicon {
    let locales: Vec<&str> = if locale == "en" {
        vec!["en"]
    } else {
        vec![locale, "en"]
    };
    let norm = |text: &str| normalize_date_text(text, locale).trim().to_string();
    let mut months: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut templates: Vec<(String, MonthTemplate)> = Vec::new();
    let mut leap_markers = HashSet::new();
    let mut eras: HashMap<String, String> = HashMap::new();
    let mut year_aliases: HashMap<String, String> = HashMap::new();
    let mut skip: HashSet<String> = HashSet::new();
    let mut trailers = HashSet::new();
    let mut year_markers = HashSet::new();
    let mut separators = HashSet::new();
    let mut weekdays = HashSet::new();

    let add_skip = |text: &str, words: &mut HashSet<String>| {
        for word in words_only(&norm(text)).split_whitespace() {
            words.insert(word.to_string());
        }
    };

    let sampled = sample_months(calendar);
    let month_widths = [
        DateStyle::LONG,
        DateStyle::MEDIUM,
        DateStyle::NUMERIC,
        DateStyle::month(Width::Long),
        DateStyle::month(Width::Short),
    ];
    let mut base_text: HashMap<(String, usize, String), String> = HashMap::new();
    for month in &sampled {
        for loc in &locales {
            for (width, style) in month_widths.iter().enumerate() {
                let parts = parts_of(loc, calendar, *style, month.mid);
                let Some(text) = find_part(&parts, PartKind::Month) else {
                    continue;
                };
                let base_key = (loc.to_string(), width, month.code[..3].to_string());
                if month.code.ends_with('L') {
                    // A leap month written as its common month plus a marker.
                    if let Some(base) = base_text.get(&base_key)
                        && text.contains(base.as_str())
                        && !base.chars().any(|c| c.is_ascii_digit())
                    {
                        let marker = norm(&text.replacen(base.as_str(), "", 1));
                        if !marker.is_empty() {
                            leap_markers.insert(marker);
                            continue;
                        }
                    }
                } else {
                    base_text.insert(base_key, text.to_string());
                }
                // addMonth
                let name = norm(text);
                let split = digits_split(&name);
                let is_own_number = split.is_some_and(|(_, n, _)| {
                    let n: u32 = n.parse().unwrap_or(0);
                    n == month.ordinal as u32 || n == month.code[1..3].parse::<u32>().unwrap_or(0)
                });
                let has_digit = name.chars().any(|c| c.is_ascii_digit());
                if name.is_empty() || (has_digit && (!name.chars().any(is_letter) || is_own_number))
                {
                    if let Some((pre, _, post)) = split
                        && (!pre.is_empty() || !post.is_empty())
                    {
                        let key = format!("{pre}#{post}");
                        let template = MonthTemplate {
                            pre: pre.to_string(),
                            post: post.to_string(),
                            is_leap: month.code.ends_with('L'),
                        };
                        match templates.iter_mut().find(|(k, _)| *k == key) {
                            Some(slot) => slot.1 = template,
                            None => templates.push((key, template)),
                        }
                    }
                    continue;
                }
                months.entry(name).or_default().insert(month.code.clone());
            }
        }
    }

    let mut literal_formats = Vec::new();
    for style in [
        DateStyle::NUMERIC,
        DateStyle::LONG,
        DateStyle::MEDIUM,
        DateStyle::MEDIUM_WEEKDAY,
        DateStyle::LONG.with_weekday(Width::Short),
    ] {
        literal_formats.push(style);
        literal_formats.push(display_style(locale, calendar, style));
    }
    let literal_dates: Vec<PlainDate> = sampled.iter().take(14).map(|m| m.mid).collect();
    for loc in &locales {
        for style in &literal_formats {
            for date in &literal_dates {
                let parts = parts_of(loc, calendar, *style, *date);
                for (k, part) in parts.iter().enumerate() {
                    if part.kind == PartKind::Literal || part.kind == PartKind::YearName {
                        add_skip(&part.value, &mut skip);
                    }
                    if part.kind == PartKind::Literal
                        && k > 0
                        && parts[k - 1].kind == PartKind::Year
                    {
                        add_skip(&part.value, &mut year_markers);
                    }
                }
                if *loc == locale
                    && let Some(tail) = parts.last()
                    && tail.kind == PartKind::Literal
                {
                    let trailer: String = norm(&tail.value)
                        .chars()
                        .filter(|c| !is_space(*c))
                        .collect();
                    if !trailer.is_empty() && trailer.chars().all(is_separator) {
                        trailers.insert(trailer);
                    }
                }
            }
        }
        for day in 0..7u8 {
            let date = pd(2026, 11, 1 + day);
            for style in [
                DateStyle::weekday(Width::Long),
                DateStyle::weekday(Width::Short),
                DateStyle::LONG_WEEKDAY,
                DateStyle::LONG.with_weekday(Width::Short),
            ] {
                let parts = parts_of(loc, calendar, style, date);
                if let Some(text) = find_part(&parts, PartKind::Weekday) {
                    add_skip(text, &mut skip);
                    add_skip(text, &mut weekdays);
                    // Vietnamese writes a weekday as a word and a number: "Thứ 2".
                    if norm(text).chars().any(|c| c.is_ascii_digit()) {
                        skip.insert(norm(text));
                    }
                }
            }
        }
    }
    let hint = display_style(locale, calendar, DateStyle::NUMERIC);
    if let Some(first) = literal_dates.first() {
        for part in parts_of(locale, calendar, hint, *first) {
            let separator: String = norm(&part.value)
                .chars()
                .filter(|c| !is_space(*c))
                .collect();
            if part.kind == PartKind::Literal && !separator.is_empty() {
                separators.insert(separator);
            }
        }
    }
    if is_lunisolar(calendar) {
        // Year names cycle every 60 years. ICU4X writes them in some formats
        // and not others (a long Chinese date has only the related year), so
        // they are learned from every format a field writes.
        for year in 1996..2057 {
            for loc in &locales {
                for style in [
                    DateStyle::LONG,
                    DateStyle::MEDIUM,
                    DateStyle::MEDIUM_WEEKDAY,
                    DateStyle::NUMERIC,
                ] {
                    for part in parts_of(loc, calendar, style, pd(year, 6, 15)) {
                        if part.kind == PartKind::YearName {
                            add_skip(&part.value, &mut skip);
                        }
                    }
                }
            }
        }
    }

    let mut era_arithmetic = HashMap::new();
    let era_widths: &[Width] = if calendar == CalendarSystem::Japanese {
        &[Width::Long, Width::Short, Width::Narrow]
    } else {
        &[Width::Long, Width::Short]
    };
    for (code, (y, m, d)) in era_samples(calendar) {
        let sample = pd(*y, *m, *d);
        for loc in &locales {
            let styles = era_widths
                .iter()
                .map(|w| DateStyle::YEAR.with_era(*w))
                .chain(literal_formats.iter().copied())
                .chain([
                    DateStyle::LONG.with_era(Width::Long),
                    DateStyle::NUMERIC.with_era(Width::Short),
                ]);
            for style in styles {
                let parts = parts_of(loc, calendar, style, sample);
                if let Some(text) = find_part(&parts, PartKind::Era)
                    && !norm(text).is_empty()
                {
                    eras.insert(norm(text), code.to_string());
                }
            }
        }
        let era_year = |date: PlainDate| calendar_era_year(calendar, date).1;
        let year = era_year(sample);
        let is_forward = era_year(pd(sample.year + 1, sample.month, sample.day)) > year;
        let base = if is_forward {
            sample.year - year
        } else {
            sample.year + year
        };
        era_arithmetic.insert(code.to_string(), EraArithmetic { base, is_forward });
        let first_year = pd(if is_forward { base + 1 } else { base - 1 }, 12, 31);
        // Some locales write era year 1 as a word, such as Japanese 元.
        if calendar.is_gregorian_aligned() && first_year.year > 0 {
            let parts = parts_of(
                locale,
                calendar,
                DateStyle::LONG.with_era(Width::Long),
                first_year,
            );
            if let Some(text) = find_part(&parts, PartKind::Year)
                && !norm(text).chars().any(|c| c.is_ascii_digit())
                && !norm(text).is_empty()
            {
                year_aliases.insert(norm(text), "1".to_string());
            }
        }
    }
    if calendar == CalendarSystem::Japanese {
        era_arithmetic.insert(
            "ce".into(),
            EraArithmetic {
                base: 0,
                is_forward: true,
            },
        );
    }
    for marker in &leap_markers {
        skip.remove(marker);
    }
    for word in months.keys().chain(eras.keys()).chain(year_aliases.keys()) {
        skip.remove(word);
    }

    // Whether a typed month number is the month's position in its year
    // (Hebrew) or its code number (Chinese). Probed with a locale that writes
    // numbers.
    let mut is_numeric_month_ordinal = false;
    if let Some(shifted) = sampled
        .iter()
        .find(|m| m.ordinal as u32 != m.code[1..3].parse::<u32>().unwrap_or(0))
    {
        let parts = parts_of("sv", calendar, DateStyle::NUMERIC, shifted.mid);
        let text = normalize_date_text(find_part(&parts, PartKind::Month).unwrap_or(""), "sv");
        let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
        is_numeric_month_ordinal = digits.parse::<u32>().ok() == Some(shifted.ordinal as u32);
    }

    let mut vocabulary: Vec<String> = months
        .keys()
        .chain(eras.keys())
        .chain(skip.iter())
        .chain(leap_markers.iter())
        .chain(year_aliases.keys())
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    vocabulary.sort_by(|a, b| {
        b.chars()
            .count()
            .cmp(&a.chars().count())
            .then_with(|| a.cmp(b))
    });
    let sample = pd(2026, 11, 2);
    let order = field_order(&parts_of(locale, calendar, DateStyle::NUMERIC, sample));
    let weekday_parts = parts_of(
        locale,
        calendar,
        display_style(locale, calendar, DateStyle::MEDIUM_WEEKDAY),
        sample,
    );
    let weekday_order = find_part(&weekday_parts, PartKind::Month)
        .is_some_and(|m| {
            let m = normalize_date_text(m, locale);
            !m.is_empty() && m.chars().all(|c| c.is_ascii_digit())
        })
        .then(|| field_order(&weekday_parts))
        .filter(|o| *o != order);
    let mut numeric_forms: Vec<(Vec<String>, Vec<DateField>)> = Vec::new();
    for style in [
        DateStyle::NUMERIC,
        DateStyle::MEDIUM,
        DateStyle::LONG,
        DateStyle::MEDIUM_WEEKDAY,
    ] {
        let parts = committed_parts(sample, style, locale, calendar);
        let numeric_month = find_part(&parts, PartKind::Month).is_some_and(|m| {
            let m = normalize_date_text(m, locale);
            !m.is_empty() && m.chars().all(|c| c.is_ascii_digit())
        });
        if !numeric_month {
            continue;
        }
        let is_field = |p: &DatePart| {
            matches!(
                p.kind,
                PartKind::Year
                    | PartKind::RelatedYear
                    | PartKind::YearName
                    | PartKind::Month
                    | PartKind::Day
            )
        };
        let fields: Vec<usize> = parts
            .iter()
            .enumerate()
            .filter(|(_, p)| is_field(p))
            .map(|(k, _)| k)
            .collect();
        let separators: Vec<String> = fields
            .windows(2)
            .map(|w| {
                let between: String = parts[w[0] + 1..w[1]]
                    .iter()
                    .map(|p| p.value.as_str())
                    .collect();
                normalize_date_text(&between, locale)
                    .chars()
                    .filter(|c| !is_space(*c))
                    .collect()
            })
            .collect();
        let form = (separators, field_order(&parts));
        if form.1.len() == 3 && !numeric_forms.contains(&form) {
            numeric_forms.push(form);
        }
    }
    let mut month_templates: Vec<MonthTemplate> = templates.into_iter().map(|(_, t)| t).collect();
    month_templates
        .sort_by_key(|t| std::cmp::Reverse(t.pre.chars().count() + t.post.chars().count()));
    CalendarDateLexicon {
        locale: locale.to_string(),
        calendar,
        order,
        weekday_order,
        numeric_forms,
        name_order: field_order(&parts_of(locale, calendar, DateStyle::LONG, sample)),
        months,
        month_templates,
        leap_markers,
        eras,
        year_aliases,
        trailers,
        year_markers,
        separators,
        weekdays,
        vocabulary,
        is_numeric_month_ordinal,
        era_arithmetic,
    }
}

// =============================================================================
// Tokens
// =============================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token {
    Num {
        v: String,
        is_alias: bool,
    },
    Month {
        codes: BTreeSet<String>,
        is_weekday: bool,
    },
    MonthNum {
        v: String,
        is_leap: bool,
    },
    Era {
        code: String,
        is_weekday: bool,
    },
    Leap,
    Word {
        is_year_marker: bool,
        is_weekday: bool,
    },
    Sep {
        v: String,
    },
}

enum Tokens {
    List(Vec<Token>),
    Incomplete,
    Unreadable,
}

fn tokenize(s: &str, lex: &CalendarDateLexicon, digit_names: bool) -> Tokens {
    let chars: Vec<char> = s.chars().collect();
    let text = |from: usize, to: usize| chars[from..to].iter().collect::<String>();
    let end = s.trim_end().chars().count();
    let mut out = Vec::new();
    let mut has_digit_name = false;
    let mut i = 0;
    while i < end {
        let ch = chars[i];
        if is_space(ch) {
            i += 1;
            continue;
        }
        if let Some((token, next)) = match_template(&chars, i, lex) {
            out.push(token);
            i = next;
            continue;
        }
        if digit_names
            && ch.is_ascii_digit()
            && let Some(word) = match_word(&chars, i, lex)
        {
            i += word.chars().count();
            out.push(word_token(&word, lex));
            has_digit_name = true;
            continue;
        }
        if ch.is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            out.push(Token::Num {
                v: text(i, j),
                is_alias: false,
            });
            i = j;
            continue;
        }
        if let Some(word) = match_word(&chars, i, lex) {
            i += word.chars().count();
            out.push(word_token(&word, lex));
            continue;
        }
        if is_separator(ch) {
            let mut j = i;
            while j < end && (is_separator(chars[j]) || is_space(chars[j])) {
                j += 1;
            }
            let v: String = chars[i..j].iter().filter(|c| !is_space(**c)).collect();
            out.push(Token::Sep { v });
            i = j;
            continue;
        }
        let mut j = i;
        while j < end && is_letter(chars[j]) {
            j += 1;
        }
        let run = text(i, j);
        if let Some(joined) = match_joined_word(&chars, i, lex) {
            i += joined.chars().count();
            out.push(word_token(&joined, lex));
            continue;
        }
        if let Some(codes) = month_by_prefix(&run, lex) {
            out.push(Token::Month {
                codes,
                is_weekday: false,
            });
            i = j;
            continue;
        }
        if j >= end && !run.is_empty() && lex.vocabulary.iter().any(|w| w.starts_with(&run)) {
            return Tokens::Incomplete;
        }
        return Tokens::Unreadable;
    }
    // Latvian "1. adars" is a month, and also day 1 of "adars". It is the day
    // when no other number can be the day. A number before the year word, as
    // in "26. g.", is the year.
    let can_be_day = |k: usize, token: &Token| {
        let Token::Num { v, .. } = token else {
            return false;
        };
        let next = out[k + 1..]
            .iter()
            .find(|t| !matches!(t, Token::Sep { .. }));
        let is_year = matches!(
            next,
            Some(Token::Word {
                is_year_marker: true,
                ..
            })
        );
        v.parse::<u32>().is_ok_and(|n| n <= 31) && !is_year
    };
    if has_digit_name && !out.iter().enumerate().any(|(k, t)| can_be_day(k, t)) {
        return tokenize(s, lex, false);
    }
    Tokens::List(out)
}

fn starts_with_at(chars: &[char], i: usize, word: &str) -> bool {
    (i..)
        .zip(word.chars())
        .all(|(k, w)| chars.get(k) == Some(&w))
}

fn match_template(chars: &[char], i: usize, lex: &CalendarDateLexicon) -> Option<(Token, usize)> {
    for template in &lex.month_templates {
        if template.pre.is_empty() && template.post.is_empty() {
            continue;
        }
        if !starts_with_at(chars, i, &template.pre) {
            continue;
        }
        let start = i + template.pre.chars().count();
        let mut after = start;
        while after < chars.len() && chars[after].is_ascii_digit() {
            after += 1;
        }
        if after == start {
            continue;
        }
        if !template.post.is_empty() && !starts_with_at(chars, after, &template.post) {
            continue;
        }
        let v: String = chars[start..after].iter().collect();
        return Some((
            Token::MonthNum {
                v,
                is_leap: template.is_leap,
            },
            after + template.post.chars().count(),
        ));
    }
    None
}

/// The longest known word at `i` that ends where a word may end.
fn match_word(chars: &[char], i: usize, lex: &CalendarDateLexicon) -> Option<String> {
    for word in &lex.vocabulary {
        if !starts_with_at(chars, i, word) {
            continue;
        }
        let next = chars.get(i + word.chars().count()).copied();
        let last = word.chars().last()?;
        let next_is_letter = next.is_some_and(is_letter);
        if !next_is_letter
            || next.is_some_and(is_spaceless)
            || is_spaceless(last)
            || !is_letter(last)
        {
            return Some(word.clone());
        }
    }
    None
}

/// A known word joined to the next with no space, such as Hebrew ב before a
/// month name or a leap marker after it ("Eighth Monthbis").
fn match_joined_word(chars: &[char], i: usize, lex: &CalendarDateLexicon) -> Option<String> {
    for word in &lex.vocabulary {
        if !starts_with_at(chars, i, word) {
            continue;
        }
        let next = i + word.chars().count();
        if lex
            .vocabulary
            .iter()
            .any(|w| starts_with_at(chars, next, w))
        {
            return Some(word.clone());
        }
    }
    None
}

fn word_token(word: &str, lex: &CalendarDateLexicon) -> Token {
    let is_weekday = lex.weekdays.contains(&without_non_words(word));
    if let Some(codes) = lex.months.get(word) {
        return Token::Month {
            codes: codes.clone(),
            is_weekday,
        };
    }
    if let Some(code) = lex.eras.get(word) {
        return Token::Era {
            code: code.clone(),
            is_weekday,
        };
    }
    if lex.leap_markers.contains(word) {
        return Token::Leap;
    }
    if let Some(alias) = lex.year_aliases.get(word) {
        return Token::Num {
            v: alias.clone(),
            is_alias: true,
        };
    }
    Token::Word {
        is_year_marker: lex.year_markers.contains(word),
        is_weekday,
    }
}

/// The month a typed abbreviation names when it starts only one month name.
fn month_by_prefix(run: &str, lex: &CalendarDateLexicon) -> Option<BTreeSet<String>> {
    if run.chars().count() < 3 {
        return None;
    }
    let mut found: Option<&BTreeSet<String>> = None;
    for (name, codes) in &lex.months {
        if name.starts_with(run) {
            if found.is_some_and(|f| f != codes) {
                return None;
            }
            found = Some(codes);
        }
    }
    found.cloned()
}

/// A weekday word can also be a month or era name, such as French "mar.".
/// When the text has another month or era, the first such word is the weekday.
fn weekdays_as_words(mut tokens: Vec<Token>) -> Vec<Token> {
    for months in [true, false] {
        let is_kind = |t: &Token| {
            if months {
                matches!(t, Token::Month { .. } | Token::MonthNum { .. })
            } else {
                matches!(t, Token::Era { .. })
            }
        };
        let named = tokens.iter().filter(|t| is_kind(t)).count();
        let weekday = tokens.iter().position(|t| {
            matches!(
                t,
                Token::Month {
                    is_weekday: true,
                    ..
                } | Token::Era {
                    is_weekday: true,
                    ..
                }
            ) && is_kind(t)
        });
        if named > 1
            && let Some(k) = weekday
        {
            tokens[k] = Token::Word {
                is_year_marker: false,
                is_weekday: true,
            };
        }
    }
    tokens
}

// =============================================================================
// Parsing
// =============================================================================

/// Reads typed text as a date in the lexicon's calendar; values are Gregorian
/// (`parseCalendarDate`).
pub fn parse_calendar_date(
    text: &str,
    lex: &CalendarDateLexicon,
    options: &DateParseOptions<'_>,
) -> DateParse {
    let s = normalize_date_text(text, &lex.locale).trim().to_string();
    if s.is_empty() {
        return DateParse::Empty;
    }
    let is_disabled = options.is_disabled;
    if options.iso_is_gregorian
        && let Some((y, m, d)) = iso_shape(&s)
    {
        return finish(gregorian_date(y, m, d), true, is_disabled);
    }
    let tokens = match tokenize(&s, lex, true) {
        Tokens::List(tokens) => weekdays_as_words(tokens),
        Tokens::Incomplete => return DateParse::Incomplete,
        Tokens::Unreadable => return DateParse::Unreadable,
    };
    let nums: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, Token::Num { .. }))
        .map(|(k, _)| k)
        .collect();
    let month_tokens: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, Token::Month { .. } | Token::MonthNum { .. }))
        .map(|(k, _)| k)
        .collect();
    let era_codes: BTreeSet<&str> = tokens
        .iter()
        .filter_map(|t| match t {
            Token::Era { code, .. } => Some(code.as_str()),
            _ => None,
        })
        .collect();
    let is_leap = tokens
        .iter()
        .any(|t| matches!(t, Token::Leap | Token::MonthNum { is_leap: true, .. }));
    if month_tokens.len() > 1 || era_codes.len() > 1 {
        return DateParse::Unreadable;
    }
    if nums.is_empty() && month_tokens.is_empty() {
        return if era_codes.is_empty() {
            DateParse::Unreadable
        } else {
            DateParse::Incomplete
        };
    }
    let typed_era = era_codes.iter().next().map(|s| s.to_string());
    let num_value = |k: usize| match &tokens[k] {
        Token::Num { v, .. } => v.clone(),
        _ => String::new(),
    };
    let num_alias = |k: usize| matches!(&tokens[k], Token::Num { is_alias: true, .. });

    let mut assigned: HashMap<usize, DateField> = HashMap::new();
    let mut month_code: Option<String> = None;
    let mut month_ordinal: Option<u32> = None;
    if let Some(&mk) = month_tokens.first() {
        match &tokens[mk] {
            Token::Month { codes, .. } => {
                if codes.len() != 1 {
                    return DateParse::Unreadable;
                }
                month_code = codes.iter().next().cloned();
            }
            Token::MonthNum { v, .. } => {
                if lex.is_numeric_month_ordinal {
                    month_ordinal = v.parse().ok();
                } else {
                    month_code = Some(format!("M{v:0>2}"));
                }
            }
            _ => {}
        }
        if nums.is_empty() || nums.len() > 2 {
            return if nums.is_empty() {
                DateParse::Incomplete
            } else {
                DateParse::Unreadable
            };
        }
        let mut rest: Vec<DateField> = lex
            .name_order
            .iter()
            .copied()
            .filter(|f| *f != DateField::Month)
            .collect();
        let long: Vec<usize> = nums
            .iter()
            .copied()
            .filter(|&k| num_value(k).len() >= 3 && !num_alias(k))
            .collect();
        if nums.len() == 1 {
            rest = if long.len() == 1 {
                vec![DateField::Year]
            } else {
                vec![DateField::Day]
            };
        } else if long.len() == 1 {
            // With a month name, a number of three or more digits is the year.
            rest = if nums[0] == long[0] {
                vec![DateField::Year, DateField::Day]
            } else {
                vec![DateField::Day, DateField::Year]
            };
        }
        for (k, &n) in nums.iter().enumerate() {
            if let Some(field) = rest.get(k) {
                assigned.insert(n, *field);
            }
        }
    } else {
        if nums.len() < 2 {
            return DateParse::Incomplete;
        }
        let typed_form = numeric_separators(&tokens);
        let form = typed_form
            .as_ref()
            .and_then(|seps| lex.numeric_forms.iter().find(|(s, _)| s == seps))
            .map(|(_, order)| order);
        if nums.len() > 3 || (form.is_none() && !same_separators(&tokens, lex)) {
            return DateParse::Unreadable;
        }
        let has_weekday = tokens.iter().any(|t| {
            matches!(
                t,
                Token::Word {
                    is_weekday: true,
                    ..
                }
            )
        });
        let base = match (&lex.weekday_order, form) {
            (Some(order), _) if has_weekday => order,
            (_, Some(order)) if nums.len() == 3 => order,
            _ => &lex.order,
        };
        let mut order: Vec<DateField> = if nums.len() == 2 {
            base.iter()
                .copied()
                .filter(|f| *f != DateField::Year)
                .collect()
        } else {
            base.clone()
        };
        if nums.len() == 3
            && order.first() != Some(&DateField::Year)
            && num_value(nums[0]).len() >= 3
        {
            // ISO-shaped text: a leading long number is the year.
            order = vec![DateField::Year, DateField::Month, DateField::Day];
        }
        if nums.len() == 2 && nums.iter().any(|&k| num_value(k).len() >= 3) {
            return DateParse::Incomplete;
        }
        for (k, &n) in nums.iter().enumerate() {
            if let Some(field) = order.get(k) {
                assigned.insert(n, *field);
            }
        }
    }
    let token_of = |field: DateField| {
        nums.iter()
            .copied()
            .find(|k| assigned.get(k) == Some(&field))
    };
    let day_token = token_of(DateField::Day);
    let year_token = token_of(DateField::Year);
    let month_num_token = token_of(DateField::Month);
    let Some(day_token) = day_token else {
        return DateParse::Incomplete;
    };
    if let Some(k) = month_num_token {
        let n: u32 = num_value(k).parse().unwrap_or(0);
        if lex.is_numeric_month_ordinal {
            month_ordinal = Some(n);
        } else {
            month_code = Some(format!("M{n:02}"));
        }
    }
    if let Some(code) = &mut month_code
        && is_leap
        && !code.ends_with('L')
    {
        code.push('L');
    }
    let day: u32 = num_value(day_token).parse().unwrap_or(0);
    let month_number = month_ordinal.unwrap_or_else(|| {
        month_code
            .as_deref()
            .and_then(|c| c.get(1..3))
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    });
    if !(1..=31).contains(&day) || !(1..=13).contains(&month_number) {
        // A lone 0 starts a two-digit day or month such as 07.
        let starts_field = [Some(day_token), month_num_token]
            .into_iter()
            .flatten()
            .any(|k| num_value(k) == "0");
        return if starts_field {
            DateParse::Incomplete
        } else {
            DateParse::Unreadable
        };
    }

    let today = options.today.unwrap_or_else(PlainDate::today);
    let mut fields = CalendarDateFields {
        day: day as u8,
        ..Default::default()
    };
    if let Some(code) = &month_code {
        fields.month_code = Some(code.clone());
    } else {
        fields.month = month_ordinal.map(|m| m as u8);
    }
    let mut is_year_complete = false;
    match year_token {
        None => apply_default_year(&mut fields, lex, today),
        Some(k) => {
            let digits = if num_alias(k) { 1 } else { num_value(k).len() };
            let year: i32 = num_value(k).parse().unwrap_or(0);
            let is_year_last = last_number(&tokens) == Some(k);
            let calendar = lex.calendar;
            if let Some(era) = &typed_era {
                fields.era = Some(era.clone());
                fields.era_year = Some(year);
                is_year_complete = true;
            } else if calendar == CalendarSystem::Japanese && digits <= 2 {
                let (era, _) = calendar_era_year(calendar, today);
                if !lex.eras.values().any(|code| *code == era) {
                    return DateParse::Unreadable;
                }
                fields.era = Some(era);
                fields.era_year = Some(year);
                is_year_complete = !is_year_last;
            } else if calendar == CalendarSystem::Japanese && digits == 4 {
                fields.era = Some("ce".into());
                fields.era_year = Some(year);
                is_year_complete = true;
            } else if calendar == CalendarSystem::Roc {
                fields.era = Some("roc".into());
                fields.era_year = Some(year);
                is_year_complete = digits >= 4 || !is_year_last;
            } else if digits == 2 {
                fields.year = Some(windowed_year(year, calendar_year_of(calendar, today)));
            } else if digits >= 4 {
                fields.year = Some(year);
                is_year_complete = true;
            } else {
                return DateParse::Incomplete;
            }
        }
    }
    let date = resolve_date(lex, &fields);
    let is_complete = year_token.is_some()
        && is_year_complete
        && is_last_field_closed(&tokens, lex, &assigned, date);
    finish(date, is_complete, is_disabled)
}

fn iso_shape(s: &str) -> Option<(i32, u8, u8)> {
    let mut parts = s.split('-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some()
        || y.len() != 4
        || !(1..=2).contains(&m.len())
        || !(1..=2).contains(&d.len())
        || ![y, m, d]
            .iter()
            .all(|p| p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some((y.parse().ok()?, m.parse().ok()?, d.parse().ok()?))
}

fn finish(
    date: Option<PlainDate>,
    is_complete: bool,
    is_disabled: Option<&dyn Fn(PlainDate) -> bool>,
) -> DateParse {
    match date {
        None => DateParse::Nonexistent { is_complete },
        Some(date) if is_disabled.is_some_and(|f| f(date)) => {
            DateParse::OutOfRange { date, is_complete }
        }
        Some(date) => DateParse::Value { date, is_complete },
    }
}

/// Whether the numbers share one separator; two separators of the locale's
/// own numeric pattern also match.
fn same_separators(tokens: &[Token], lex: &CalendarDateLexicon) -> bool {
    let mut previous: Option<String> = None;
    let mut seen_number = false;
    let mut pending: Option<String> = None;
    for token in tokens {
        match token {
            Token::Num { .. } => {
                if seen_number {
                    let separator = pending.clone().unwrap_or_default();
                    if let Some(previous) = &previous
                        && separator != *previous
                        && !(lex.separators.contains(&separator)
                            && lex.separators.contains(previous))
                    {
                        return false;
                    }
                    previous = Some(separator);
                }
                seen_number = true;
                pending = None;
            }
            Token::Sep { v } => pending = Some(v.clone()),
            _ => {
                pending = None;
                seen_number = false;
                previous = None;
            }
        }
    }
    true
}

/// The separators between consecutive numbers, when only separators (or
/// nothing) stand between them.
fn numeric_separators(tokens: &[Token]) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut pending: Option<String> = None;
    let mut seen = false;
    for token in tokens {
        match token {
            Token::Num { .. } => {
                if seen {
                    out.push(pending.take().unwrap_or_default());
                }
                seen = true;
                pending = None;
            }
            Token::Sep { v } if seen => pending = Some(pending.unwrap_or_default() + v),
            Token::Sep { .. } => {}
            Token::Word { .. } if !seen => {}
            _ => return None,
        }
    }
    Some(out)
}

fn last_number(tokens: &[Token]) -> Option<usize> {
    let (k, last) = tokens
        .iter()
        .enumerate()
        .rfind(|(_, t)| !matches!(t, Token::Sep { .. }))?;
    matches!(last, Token::Num { .. }).then_some(k)
}

/// Whether the text has stopped growing: no separator trails it and its last
/// field cannot take another digit.
fn is_last_field_closed(
    tokens: &[Token],
    lex: &CalendarDateLexicon,
    assigned: &HashMap<usize, DateField>,
    date: Option<PlainDate>,
) -> bool {
    let Some(last) = tokens.last() else {
        return false;
    };
    let k = tokens.len() - 1;
    match last {
        Token::Sep { v } => lex.trailers.contains(v),
        Token::Num { v, .. } => {
            let field = assigned.get(&k);
            if field == Some(&DateField::Year) || v.len() >= 2 {
                return true;
            }
            let value: u32 = v.parse().unwrap_or(0);
            if field == Some(&DateField::Day) {
                // Not the month's own length: a 3 can start a 30 the month lacks.
                return value * 10 > 31;
            }
            let months = date.map_or(13, |d| calendar_months_in_year(lex.calendar, d));
            value * 10 > months as u32
        }
        _ => true,
    }
}

/// A two-digit year in the century that puts it at most 80 years before and
/// 19 years after the current year.
fn windowed_year(two_digits: i32, current: i32) -> i32 {
    let mut year = current - current.rem_euclid(100) + two_digits;
    if year > current + 19 {
        year -= 100;
    } else if year < current - 80 {
        year += 100;
    }
    year
}

fn apply_default_year(
    fields: &mut CalendarDateFields,
    lex: &CalendarDateLexicon,
    today: PlainDate,
) {
    if !lex.era_arithmetic.is_empty() && !without_era(lex.calendar) {
        let (era, year) = calendar_era_year(lex.calendar, today);
        if lex.eras.values().any(|code| *code == era) {
            fields.era = Some(era);
            fields.era_year = Some(year);
            return;
        }
    }
    fields.year = Some(calendar_year_of(lex.calendar, today));
}

fn gregorian_date(year: i32, month: u8, day: u8) -> Option<PlainDate> {
    ((1..=9999).contains(&year)
        && (1..=12).contains(&month)
        && day >= 1
        && day <= days_in_month(year, month))
    .then_some(PlainDate { year, month, day })
}

/// The Gregorian date for calendar fields, or `None` when it does not exist.
fn resolve_date(lex: &CalendarDateLexicon, fields: &CalendarDateFields) -> Option<PlainDate> {
    let calendar = lex.calendar;
    if !calendar.is_gregorian_aligned() {
        return calendar_date_from_fields(calendar, fields)
            .filter(|d| (1..=9999).contains(&d.year));
    }
    if fields
        .month_code
        .as_deref()
        .is_some_and(|c| c.ends_with('L'))
    {
        return None;
    }
    let month = fields.month.or_else(|| {
        fields
            .month_code
            .as_deref()
            .and_then(|c| c.get(1..3))
            .and_then(|n| n.parse().ok())
    })?;
    let gregorian_year = match &fields.era {
        Some(era) => {
            let arithmetic = lex.era_arithmetic.get(era)?;
            let era_year = fields.era_year.filter(|y| *y >= 1)?;
            if arithmetic.is_forward {
                arithmetic.base + era_year
            } else {
                arithmetic.base - era_year
            }
        }
        None => {
            let default_era = if calendar == CalendarSystem::Buddhist {
                "be"
            } else {
                "ce"
            };
            lex.era_arithmetic.get(default_era).map_or(0, |a| a.base) + fields.year.unwrap_or(0)
        }
    };
    let date = gregorian_date(gregorian_year, month, fields.day)?;
    if calendar != CalendarSystem::Japanese || fields.era.as_deref() == Some("ce") {
        return Some(date);
    }
    // Each Japanese era ends on a set day; a year past it is not in that era.
    let (era, year) = calendar_era_year(calendar, date);
    (Some(era.as_str()) == fields.era.as_deref() && Some(year) == fields.era_year).then_some(date)
}

// =============================================================================
// Display
// =============================================================================

/// Whether the era changes between 1900 and 2100; only then can a year
/// without its era read as two dates.
fn has_era_change(calendar: CalendarSystem) -> bool {
    if let Some(result) = ERA_CHANGES.with(|c| c.borrow().get(&calendar).copied()) {
        return result;
    }
    let result = calendar_era_year(calendar, pd(1900, 1, 1)).0
        != calendar_era_year(calendar, pd(2100, 12, 31)).0;
    ERA_CHANGES.with(|c| c.borrow_mut().insert(calendar, result));
    result
}

/// Adds the era to a style that omits it, so a year never reads as a
/// Gregorian year, keeping the locale's own field order (`displayOptions`).
fn display_style(locale: &str, calendar: CalendarSystem, style: DateStyle) -> DateStyle {
    if without_era(calendar) {
        return style;
    }
    let key = (locale.to_string(), calendar, style);
    if let Some(cached) = DISPLAY_STYLES.with(|c| c.borrow().get(&key).copied()) {
        return cached;
    }
    let sample = pd(2026, 11, 2);
    let parts = parts_of(locale, calendar, style, sample);
    let mut result = style;
    let order = field_order(&parts);
    if !parts.iter().any(|p| p.kind == PartKind::Era) {
        let widths: &[Width] = if has_era_change(calendar) {
            &[Width::Short, Width::Narrow]
        } else {
            &[Width::Short]
        };
        for width in widths {
            let with_era = style.with_era(*width);
            if field_order(&parts_of(locale, calendar, with_era, sample)) == order {
                result = with_era;
                break;
            }
        }
    }
    DISPLAY_STYLES.with(|c| c.borrow_mut().insert(key, result));
    result
}

/// The parts of the text a field commits for a style, before an era is
/// added at an edge.
fn committed_parts(
    date: PlainDate,
    style: DateStyle,
    locale: &str,
    calendar: CalendarSystem,
) -> Vec<DatePart> {
    if is_lunisolar(calendar) {
        format_calendar_date_to_parts(calendar, date, style, locale, true)
    } else {
        parts_of(
            locale,
            calendar,
            display_style(locale, calendar, style),
            date,
        )
    }
}

fn format_in_calendar(
    date: PlainDate,
    style: DateStyle,
    locale: &str,
    calendar: CalendarSystem,
) -> String {
    if is_lunisolar(calendar) {
        return format_calendar_date(calendar, date, style, locale, true);
    }
    let display = display_style(locale, calendar, style);
    let text = format_date(date, display, locale, calendar);
    // When every era width reorders the fields, the text has no era, and text
    // without one reads in the latest era: a date of an earlier era adds it,
    // at the start or the end, as in the era format.
    let era_of = |d: PlainDate| calendar_era_year(calendar, d).0;
    if !has_era_change(calendar)
        || era_of(date) == era_of(pd(9999, 12, 31))
        || parts_of(locale, calendar, display, date)
            .iter()
            .any(|p| p.kind == PartKind::Era)
    {
        return text;
    }
    let with_era = parts_of(locale, calendar, style.with_era(Width::Short), date);
    let Some(at) = with_era.iter().position(|p| p.kind == PartKind::Era) else {
        return text;
    };
    let range = if at == 0 {
        0..2.min(with_era.len())
    } else {
        at - 1..at + 1
    };
    let era = join_parts(&with_era[range]);
    if at == 0 { era + &text } else { text + &era }
}

/// The committed text of a date field in a calendar
/// (`formatCalendarDateText`): Gregorian text is the shared Timestamp date
/// text, and `system_date` is always the ISO date.
pub fn format_calendar_date_text(
    date: PlainDate,
    format: SharedDateFormat,
    locale: &str,
    calendar: CalendarSystem,
) -> String {
    let resolved = text_calendar(calendar);
    match format.style() {
        Some(style) if resolved != CalendarSystem::Gregory => {
            format_in_calendar(date, style, locale, resolved)
        }
        _ => format_shared_date(date, format, locale, CalendarSystem::Gregory),
    }
}

/// An example date in the calendar's numeric pattern with its era
/// (`formatCalendarDateHint`, AST-111 AR2).
pub fn format_calendar_date_hint(
    date: PlainDate,
    locale: &str,
    calendar: CalendarSystem,
) -> String {
    format_in_calendar(date, DateStyle::NUMERIC, locale, text_calendar(calendar))
}

#[cfg(test)]
mod tests;
