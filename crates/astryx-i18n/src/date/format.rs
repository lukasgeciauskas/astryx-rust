//! Dates written as `Intl.DateTimeFormat` writes them, from ICU4X's CLDR data:
//! upstream's `plainDateFormat`, `plainDateFormatToParts`, the shared date
//! formats, weekday names and the date-field and calendar display names.
//!
//! Upstream passes `Intl` option bags; [`DateStyle`] names the same fields,
//! and ICU4X's semantic field sets write them in the locale's pattern. A
//! locale's `-u-ca-` keyword never chooses the calendar (AST-111 FR2): every
//! formatter is built for the calendar it is given.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use icu_calendar::Gregorian;
use icu_datetime::fieldsets::enums::{CalendarPeriodFieldSet, DateFieldSet};
use icu_datetime::fieldsets::{self, Y, YM};
use icu_datetime::input::Date;
use icu_datetime::options::{Length, YearStyle};
use icu_datetime::pattern::{DateTimePattern, FixedCalendarDateTimeNames};
use icu_datetime::{DateTimeFormatter, DateTimeFormatterPreferences};
use icu_decimal::DecimalFormatter;
use icu_decimal::input::Decimal;
use icu_decimal::options::{DecimalFormatterOptions, GroupingStrategy};
use icu_locale::Locale;
use icu_locale::extensions::unicode::{Value, key};
use unicode_segmentation::UnicodeSegmentation as _;
use writeable::{Part, PartsWrite, TryWriteable, Writeable};

use super::PlainDate;
use crate::CalendarSystem;

#[rustfmt::skip]
#[allow(clippy::all)]
mod generated {
    include!("../generated/cldr.rs");
}

/// How wide a field is written, upstream's `'numeric' | 'narrow' | 'short' |
/// 'long'` option values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Width {
    Numeric,
    Narrow,
    Short,
    Long,
}

/// The fields a date is written with, upstream's `Intl.DateTimeFormatOptions`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DateStyle {
    pub year: bool,
    pub month: Option<Width>,
    pub day: bool,
    pub weekday: Option<Width>,
    pub era: Option<Width>,
}

impl DateStyle {
    /// "3/21/2026", upstream `{year, month, day: 'numeric'}`.
    pub const NUMERIC: Self = Self::ymd(Width::Numeric);
    /// "Mar 21, 2026", upstream `DATE_FORMAT_SHORT_WITH_YEAR` (`date`).
    pub const MEDIUM: Self = Self::ymd(Width::Short);
    /// "March 21, 2026", upstream `DATE_FORMAT_LONG` (`date_long`).
    pub const LONG: Self = Self::ymd(Width::Long);
    /// "Sat, Mar 21, 2026", upstream `DATE_FORMAT_SHORT_WITH_WEEKDAY`
    /// (`date_weekday`).
    pub const MEDIUM_WEEKDAY: Self = Self::MEDIUM.with_weekday(Width::Short);
    /// "Saturday, March 21, 2026", upstream `DATE_FORMAT_WITH_WEEKDAY`.
    pub const LONG_WEEKDAY: Self = Self::LONG.with_weekday(Width::Long);
    /// "March 2026", upstream `DATE_FORMAT_MONTH_YEAR`.
    pub const MONTH_YEAR: Self = Self {
        year: true,
        month: Some(Width::Long),
        day: false,
        weekday: None,
        era: None,
    };
    /// "March", upstream `DATE_FORMAT_MONTH_ONLY`.
    pub const MONTH: Self = Self::month(Width::Long);
    /// "Mar 21", upstream `DATE_FORMAT_SHORT`.
    pub const MONTH_DAY: Self = Self {
        year: false,
        month: Some(Width::Short),
        day: true,
        weekday: None,
        era: None,
    };
    /// "2026".
    pub const YEAR: Self = Self {
        year: true,
        month: None,
        day: false,
        weekday: None,
        era: None,
    };
    /// "Sun", upstream `DATE_FORMAT_WEEKDAY_ONLY`.
    pub const WEEKDAY: Self = Self::weekday(Width::Short);

    const fn ymd(month: Width) -> Self {
        Self {
            year: true,
            month: Some(month),
            day: true,
            weekday: None,
            era: None,
        }
    }

    /// The month alone.
    pub const fn month(width: Width) -> Self {
        Self {
            year: false,
            month: Some(width),
            day: false,
            weekday: None,
            era: None,
        }
    }

    /// The weekday alone.
    pub const fn weekday(width: Width) -> Self {
        Self {
            year: false,
            month: None,
            day: false,
            weekday: Some(width),
            era: None,
        }
    }

    /// The same fields with a weekday.
    pub const fn with_weekday(mut self, width: Width) -> Self {
        self.weekday = Some(width);
        self
    }

    /// The same fields with the era written.
    pub const fn with_era(mut self, width: Width) -> Self {
        self.era = Some(width);
        self
    }

    fn length(&self) -> Length {
        let width = self.month.or(self.weekday).or(self.era);
        match width {
            Some(Width::Long) => Length::Long,
            Some(Width::Numeric | Width::Narrow)
                if self.month.is_some() || self.weekday.is_none() =>
            {
                Length::Short
            }
            _ => Length::Medium,
        }
    }

    /// A numeric date needs ICU4X's `Full` for the whole year ("11/2/2026");
    /// elsewhere `Auto` picks the patterns `Intl` picks, and a year it still
    /// shortens is written out in full (`with_decimal_fields`).
    fn year_style(&self) -> YearStyle {
        if self.era.is_some() {
            YearStyle::WithEra
        } else if self.month == Some(Width::Numeric) {
            YearStyle::Full
        } else {
            YearStyle::Auto
        }
    }
}

/// What a piece of formatted date text is, upstream's
/// `Intl.DateTimeFormatPart` types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartKind {
    Literal,
    Era,
    Year,
    RelatedYear,
    YearName,
    Month,
    Day,
    Weekday,
    DayPeriod,
    Hour,
    Minute,
    Second,
    TimeZoneName,
}

/// One piece of formatted date text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatePart {
    pub kind: PartKind,
    pub value: String,
}

impl DatePart {
    fn new(kind: PartKind, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: value.into(),
        }
    }
}

/// The text of a list of parts.
pub fn join_parts(parts: &[DatePart]) -> String {
    parts.iter().map(|p| p.value.as_str()).collect()
}

fn part_kind(part: Part) -> Option<PartKind> {
    use icu_datetime::parts;
    if part.category != parts::ERA.category {
        return None;
    }
    Some(match part {
        p if p == parts::ERA => PartKind::Era,
        p if p == parts::YEAR => PartKind::Year,
        p if p == parts::RELATED_YEAR => PartKind::RelatedYear,
        p if p == parts::YEAR_NAME => PartKind::YearName,
        p if p == parts::MONTH => PartKind::Month,
        p if p == parts::DAY => PartKind::Day,
        p if p == parts::WEEKDAY => PartKind::Weekday,
        p if p == parts::DAY_PERIOD => PartKind::DayPeriod,
        p if p == parts::HOUR => PartKind::Hour,
        p if p == parts::MINUTE => PartKind::Minute,
        p if p == parts::SECOND => PartKind::Second,
        p if p == parts::TIME_ZONE_NAME => PartKind::TimeZoneName,
        _ => return None,
    })
}

/// ICU4X's numeric Hebrew month: digits and an optional `a` or `b`.
fn is_month_code_number(text: &str) -> bool {
    let digits = text
        .chars()
        .take_while(|c| crate::number::digit_value(*c).is_some())
        .count();
    let rest: String = text.chars().skip(digits).collect();
    digits > 0 && matches!(rest.as_str(), "" | "a" | "b")
}

/// Collects formatted text with the outermost date-field spans.
#[derive(Default)]
struct PartsSink {
    out: String,
    spans: Vec<(usize, usize, PartKind)>,
    depth: usize,
}

impl fmt::Write for PartsSink {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.out.push_str(s);
        Ok(())
    }
}

impl PartsWrite for PartsSink {
    type SubPartsWrite = PartsSink;

    fn with_part(
        &mut self,
        part: Part,
        mut f: impl FnMut(&mut Self::SubPartsWrite) -> fmt::Result,
    ) -> fmt::Result {
        let start = self.out.len();
        let kind = part_kind(part).filter(|_| self.depth == 0);
        if kind.is_some() {
            self.depth += 1;
        }
        let result = f(self);
        if let Some(kind) = kind {
            self.depth -= 1;
            self.spans.push((start, self.out.len(), kind));
        }
        result
    }
}

/// Formatted text split into parts, with literals between the fields.
pub(crate) fn parts_of(writeable: &impl Writeable) -> Vec<DatePart> {
    let mut sink = PartsSink::default();
    if writeable.write_to_parts(&mut sink).is_err() {
        return Vec::new();
    }
    let mut spans = sink.spans;
    spans.sort_by_key(|s| s.0);
    let mut parts = Vec::new();
    let mut at = 0;
    for (start, end, kind) in spans {
        if start < at {
            continue;
        }
        if start > at {
            parts.push(DatePart::new(PartKind::Literal, &sink.out[at..start]));
        }
        parts.push(DatePart::new(kind, &sink.out[start..end]));
        at = end;
    }
    if at < sink.out.len() {
        parts.push(DatePart::new(PartKind::Literal, &sink.out[at..]));
    }
    parts
}

/// The calendar a formatter writes in: `iso8601` has no names of its own, so
/// its labels use `gregory`, which has the same months and days.
pub(crate) fn label_calendar(calendar: CalendarSystem) -> CalendarSystem {
    if calendar == CalendarSystem::Iso8601 {
        CalendarSystem::Gregory
    } else {
        calendar
    }
}

/// A locale tag parsed once, with its `-u-ca-` keyword replaced.
pub(crate) fn locale_with_calendar(tag: &str, calendar: CalendarSystem) -> Locale {
    let mut locale = crate::locale::parse_locale(tag);
    if let Ok(value) = Value::try_from_str(label_calendar(calendar).identifier()) {
        locale.extensions.unicode.keywords.set(key!("ca"), value);
    }
    locale
}

enum Formatter {
    Date(DateTimeFormatter<DateFieldSet>),
    Period(DateTimeFormatter<CalendarPeriodFieldSet>),
}

impl Formatter {
    fn new(locale: &Locale, style: DateStyle) -> Option<Self> {
        let prefs = DateTimeFormatterPreferences::from(locale);
        let length = style.length();
        let year_style = style.year_style();
        let (y, m, d, e) = (
            style.year,
            style.month.is_some(),
            style.day,
            style.weekday.is_some(),
        );
        macro_rules! date {
            ($variant:ident, $set:expr) => {
                DateTimeFormatter::try_new(prefs, DateFieldSet::$variant($set))
                    .ok()
                    .map(Formatter::Date)
            };
        }
        match (y, m, d, e) {
            (true, true, true, true) => date!(
                YMDE,
                fieldsets::YMDE::for_length(length).with_year_style(year_style)
            ),
            (true, true, true, false) => date!(
                YMD,
                fieldsets::YMD::for_length(length).with_year_style(year_style)
            ),
            (false, true, true, true) => date!(MDE, fieldsets::MDE::for_length(length)),
            (false, true, true, false) => date!(MD, fieldsets::MD::for_length(length)),
            (false, false, true, true) => date!(DE, fieldsets::DE::for_length(length)),
            (false, false, true, false) => date!(D, fieldsets::D::for_length(length)),
            (false, false, false, true) => date!(E, fieldsets::E::for_length(length)),
            (true, true, false, _) => DateTimeFormatter::try_new(
                prefs,
                CalendarPeriodFieldSet::YM(YM::for_length(length).with_year_style(year_style)),
            )
            .ok()
            .map(Formatter::Period),
            (true, false, _, _) => DateTimeFormatter::try_new(
                prefs,
                CalendarPeriodFieldSet::Y(Y::for_length(length).with_year_style(year_style)),
            )
            .ok()
            .map(Formatter::Period),
            (false, true, false, _) => DateTimeFormatter::try_new(
                prefs,
                CalendarPeriodFieldSet::M(fieldsets::M::for_length(length)),
            )
            .ok()
            .map(Formatter::Period),
            (false, false, false, false) => None,
        }
    }

    fn parts(&self, date: &Date<icu_calendar::Iso>) -> Vec<DatePart> {
        match self {
            Formatter::Date(f) => parts_of(&f.format(date)),
            Formatter::Period(f) => parts_of(&f.format(date)),
        }
    }
}

type FormatterKey = (String, CalendarSystem, DateStyle);

thread_local! {
    static FORMATTERS: RefCell<HashMap<FormatterKey, Option<Rc<Formatter>>>> =
        RefCell::new(HashMap::new());
    static DECIMALS: RefCell<HashMap<String, Option<Rc<DecimalFormatter>>>> =
        RefCell::new(HashMap::new());
}

/// One formatter per locale, calendar and style, built on first use: a
/// calendar formats a month's 42 day labels with the same style every render
/// (upstream `dateTimeFormatFor`).
fn formatter(tag: &str, calendar: CalendarSystem, style: DateStyle) -> Option<Rc<Formatter>> {
    let key = (tag.to_string(), label_calendar(calendar), style);
    FORMATTERS.with(|cache| {
        cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                Formatter::new(&locale_with_calendar(tag, calendar), style).map(Rc::new)
            })
            .clone()
    })
}

fn iso(pd: PlainDate) -> Option<Date<icu_calendar::Iso>> {
    Date::try_new_iso(pd.year, pd.month, pd.day).ok()
}

/// The parts of a date written in a calendar, upstream
/// `plainDateFormatToParts` with `calendar`. The calendar's own month and day:
/// ICU4X does the arithmetic and the names, so they always agree (upstream's
/// `withTemporalDay` mends Intl and Temporal disagreeing).
pub fn format_date_to_parts(
    pd: PlainDate,
    style: DateStyle,
    locale: &str,
    calendar: CalendarSystem,
) -> Vec<DatePart> {
    match (formatter(locale, calendar, style), iso(pd)) {
        (Some(f), Some(date)) => with_decimal_fields(f.parts(&date), pd, locale, calendar),
        _ => vec![DatePart::new(PartKind::Literal, pd.to_iso())],
    }
}

/// Day and year numbers in the locale's digits. CLDR gives some calendars'
/// patterns a traditional numbering (Hebrew letters for Hebrew dates, Han
/// numerals for Chinese days), which ICU4X applies; `Intl` writes the
/// locale's digits there, and so do the grids and the text typed entry reads
/// back (AST-111 FR6, FR13). Japanese first years (元年) stay, as in `Intl`.
/// A numeric Hebrew month is its position in the year, as `Intl` writes it
/// and typed entry reads it (FR9); ICU4X writes month codes (`06a`, `06b`).
fn with_decimal_fields(
    mut parts: Vec<DatePart>,
    pd: PlainDate,
    locale: &str,
    calendar: CalendarSystem,
) -> Vec<DatePart> {
    let has_digit = |text: &str| {
        text.chars()
            .any(|c| crate::number::digit_value(c).is_some())
    };
    for part in &mut parts {
        match part.kind {
            PartKind::Day if !has_digit(&part.value) => {
                let day = super::calendar::calendar_day_of_month(calendar, pd);
                part.value = format_day_number(day as i64, locale);
            }
            PartKind::Year if calendar != CalendarSystem::Japanese && !has_digit(&part.value) => {
                let (_, year) = super::calendar::calendar_era_year(calendar, pd);
                part.value = format_day_number(year as i64, locale);
            }
            // `Intl`'s `year: 'numeric'` writes the whole year; some ICU4X
            // patterns keep two digits (`03.12.00 Minguo` for Minguo 100).
            PartKind::Year
                if !matches!(calendar, CalendarSystem::Chinese | CalendarSystem::Dangi) =>
            {
                let digits: String = part
                    .value
                    .chars()
                    .filter_map(|c| {
                        crate::number::digit_value(c).and_then(|d| char::from_digit(d, 10))
                    })
                    .collect();
                let (_, year) = super::calendar::calendar_era_year(calendar, pd);
                let full = year.unsigned_abs().to_string();
                if !digits.is_empty() && digits.len() < full.len() && full.ends_with(&digits) {
                    part.value = format_day_number(year.abs() as i64, locale);
                }
            }
            PartKind::Month
                if calendar == CalendarSystem::Hebrew && is_month_code_number(&part.value) =>
            {
                let width = part
                    .value
                    .chars()
                    .take_while(|c| crate::number::digit_value(*c).is_some())
                    .count();
                let month = super::calendar::calendar_month_of(calendar, pd);
                let ordinal = super::calendar::calendar_year_month_of(calendar, &month).month;
                let digits = format_day_number(ordinal as i64, locale);
                let zero = format_day_number(0, locale);
                let pad = width.saturating_sub(digits.chars().count());
                part.value = zero.repeat(pad) + &digits;
            }
            _ => {}
        }
    }
    parts
}

/// A date written in a calendar, upstream `plainDateFormat`.
pub fn format_date(
    pd: PlainDate,
    style: DateStyle,
    locale: &str,
    calendar: CalendarSystem,
) -> String {
    join_parts(&format_date_to_parts(pd, style, locale, calendar))
}

/// A number in the locale's digits with no grouping, upstream
/// `plainDateFormatDayNumber`: `15` in English, `۱۵` in Persian.
pub fn format_day_number(value: i64, locale: &str) -> String {
    let decimal = DECIMALS.with(|cache| {
        cache
            .borrow_mut()
            .entry(locale.to_string())
            .or_insert_with(|| {
                let parsed = crate::locale::parse_locale(locale);
                DecimalFormatter::try_new(
                    (&parsed).into(),
                    DecimalFormatterOptions::from(GroupingStrategy::Never),
                )
                .ok()
                .map(Rc::new)
            })
            .clone()
    });
    match decimal {
        Some(f) => f
            .format(&Decimal::from(value))
            .write_to_string()
            .into_owned(),
        None => value.to_string(),
    }
}

/// The first user-perceived character of a string.
pub(crate) fn first_character(text: &str) -> &str {
    text.graphemes(true).next().unwrap_or("")
}

/// The number of user-perceived characters in a string.
pub(crate) fn character_count(text: &str) -> usize {
    text.graphemes(true).count()
}

fn language(tag: &str) -> String {
    crate::locale::parse_locale(tag).id.language.to_string()
}

/// The month and year as a label that stands alone, such as a calendar
/// heading (upstream `plainDateFormatStandaloneMonthYear`): it starts with a
/// capital where CLDR titlecases a stand-alone month name.
pub fn format_standalone_month_year(
    pd: PlainDate,
    locale: &str,
    calendar: CalendarSystem,
) -> String {
    let label = format_date(pd, DateStyle::MONTH_YEAR, locale, calendar);
    capitalize_standalone(label, locale)
}

pub(crate) fn capitalize_standalone(label: String, locale: &str) -> String {
    if generated::STANDALONE_MONTH_CAPITALS
        .binary_search(&language(locale).as_str())
        .is_err()
    {
        return label;
    }
    let first = first_character(&label);
    let upper = if matches!(language(locale).as_str(), "tr" | "az") && first == "i" {
        "İ".to_string()
    } else {
        first.to_uppercase()
    };
    format!("{upper}{}", &label[first.len()..])
}

fn weekday_names_for_pattern(locale: &str, pattern: &str) -> Option<[String; 7]> {
    let parsed = crate::locale::parse_locale(locale);
    let mut names =
        FixedCalendarDateTimeNames::<Gregorian, DateFieldSet>::try_new((&parsed).into()).ok()?;
    let pattern: DateTimePattern = pattern.parse().ok()?;
    let formatter = names.include_for_pattern(&pattern).ok()?;
    let mut out: [String; 7] = Default::default();
    for (day, slot) in out.iter_mut().enumerate() {
        // January 4, 1970 was a Sunday.
        let date = Date::try_new_gregorian(1970, 1, 4 + day as u8).ok()?;
        *slot = formatter
            .format(&date)
            .try_write_to_string()
            .ok()?
            .into_owned();
    }
    Some(out)
}

/// The platform's stand-alone weekday names, Sunday first, upstream
/// `plainDateWeekdayNames`: abbreviated (`Width::Short`) or narrow.
pub fn weekday_names(locale: &str, width: Width) -> [String; 7] {
    let pattern = match width {
        Width::Narrow => "ccccc",
        Width::Long => "cccc",
        _ => "ccc",
    };
    weekday_names_for_pattern(locale, pattern)
        .unwrap_or_else(|| ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"].map(String::from))
}

fn chain_lookup<T: Copy>(table: &[(&str, T)], locale: &str) -> Option<T> {
    let find = |tag: &str| {
        table
            .binary_search_by(|(t, _)| t.cmp(&tag))
            .ok()
            .map(|i| table[i].1)
    };
    crate::locale_chain(locale).iter().find_map(|tag| find(tag))
}

/// The longest abbreviated weekday name a header row takes.
pub const MAX_ABBREVIATED_HEADER_LENGTH: usize = 4;

/// A locale's weekday header names, Sunday first (upstream
/// `getWeekdayHeaderNames`'s rows, Calendar DD1): the generated CLDR row for
/// its tag or language, else its platform's abbreviated names when all seven
/// fit in four characters, else its narrow names.
pub fn weekday_header_names(locale: &str) -> [String; 7] {
    if let Some(row) = chain_lookup(generated::WEEKDAY_HEADER_NAMES, locale) {
        return row.map(String::from);
    }
    let abbreviated = weekday_names(locale, Width::Short);
    if abbreviated
        .iter()
        .all(|n| character_count(n) <= MAX_ABBREVIATED_HEADER_LENGTH)
    {
        return abbreviated;
    }
    weekday_names(locale, Width::Narrow)
}

/// The locale's name for the week-of-year field ("week", "Woche", "週"),
/// upstream `plainDateWeekFieldName`; English's for a locale without one.
pub fn week_field_name(locale: &str) -> String {
    chain_lookup(generated::WEEK_OF_YEAR, locale)
        .unwrap_or("week")
        .to_string()
}

/// The calendar's name in the locale, such as "Persian Calendar", upstream
/// `calendarDisplayName` (AST-111 DEC-8). English's for a locale without
/// one, then the identifier.
pub fn calendar_display_name(calendar: CalendarSystem, locale: &str) -> String {
    let id = calendar.identifier();
    let find = |names: &[(&str, &'static str)]| {
        names
            .binary_search_by(|(c, _)| c.cmp(&id))
            .ok()
            .map(|i| names[i].1)
    };
    chain_lookup(generated::CALENDAR_NAMES, locale)
        .and_then(find)
        .or_else(|| chain_lookup(generated::CALENDAR_NAMES, "en").and_then(find))
        .unwrap_or(id)
        .to_string()
}

/// The date-only members of Timestamp's `format` vocabulary that a date
/// field shows, upstream `SharedDateFormat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SharedDateFormat {
    /// "Mar 21, 2026".
    Date,
    /// "March 21, 2026".
    #[default]
    DateLong,
    /// "Wed, Mar 21, 2026".
    DateWeekday,
    /// "2026-03-21", the ISO date in every calendar.
    SystemDate,
}

impl SharedDateFormat {
    /// The style a named format writes, `None` for `system_date`
    /// (`SHARED_DATE_FORMAT_OPTIONS`).
    pub fn style(self) -> Option<DateStyle> {
        match self {
            SharedDateFormat::Date => Some(DateStyle::MEDIUM),
            SharedDateFormat::DateLong => Some(DateStyle::LONG),
            SharedDateFormat::DateWeekday => Some(DateStyle::MEDIUM_WEEKDAY),
            SharedDateFormat::SystemDate => None,
        }
    }
}

/// A date in a shared format, upstream `formatSharedDate`: in `calendar`,
/// except `system_date`, which is the Gregorian ISO date.
pub fn format_shared_date(
    pd: PlainDate,
    format: SharedDateFormat,
    locale: &str,
    calendar: CalendarSystem,
) -> String {
    match format.style() {
        None => pd.to_iso(),
        Some(style) => format_date(pd, style, locale, calendar),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> PlainDate {
        PlainDate::new(y, m, day).unwrap()
    }

    #[test]
    fn shared_formats_match_upstreams_examples() {
        let pd = d(2026, 3, 21);
        let f = |format| format_shared_date(pd, format, "en", CalendarSystem::Gregory);
        assert_eq!(f(SharedDateFormat::Date), "Mar 21, 2026");
        assert_eq!(f(SharedDateFormat::DateLong), "March 21, 2026");
        assert_eq!(f(SharedDateFormat::DateWeekday), "Sat, Mar 21, 2026");
        assert_eq!(f(SharedDateFormat::SystemDate), "2026-03-21");
        assert_eq!(
            format_shared_date(
                pd,
                SharedDateFormat::SystemDate,
                "en",
                CalendarSystem::Persian
            ),
            "2026-03-21"
        );
        assert_eq!(
            format_date(
                d(2026, 11, 2),
                DateStyle::NUMERIC,
                "en",
                CalendarSystem::Gregory
            ),
            "11/2/2026"
        );
    }

    #[test]
    fn a_locale_keyword_never_chooses_the_calendar() {
        let gregorian = format_date(
            d(2026, 6, 15),
            DateStyle::LONG,
            "th-TH-u-ca-buddhist",
            CalendarSystem::Gregory,
        );
        assert!(gregorian.contains("2026"), "{gregorian}");
        let buddhist = format_date(
            d(2026, 6, 15),
            DateStyle::LONG,
            "th-TH",
            CalendarSystem::Buddhist,
        );
        assert!(buddhist.contains("2569"), "{buddhist}");
    }

    #[test]
    fn parts_name_each_field() {
        let parts = format_date_to_parts(
            d(2026, 10, 30),
            DateStyle::LONG,
            "en",
            CalendarSystem::Persian,
        );
        let kinds: Vec<PartKind> = parts.iter().map(|p| p.kind).collect();
        assert_eq!(join_parts(&parts), "Aban 8, 1405 AP");
        assert!(kinds.contains(&PartKind::Era));
        assert_eq!(parts[0], DatePart::new(PartKind::Month, "Aban"));
    }

    #[test]
    fn standalone_labels_capitalize_where_cldr_does() {
        assert_eq!(
            format_standalone_month_year(d(2026, 6, 1), "fr", CalendarSystem::Gregory),
            "Juin 2026"
        );
        assert_eq!(
            format_standalone_month_year(d(2026, 6, 1), "de", CalendarSystem::Gregory),
            "Juni 2026"
        );
        assert_eq!(
            format_standalone_month_year(d(2026, 6, 1), "en", CalendarSystem::Iso8601),
            "June 2026"
        );
    }

    #[test]
    fn weekday_headers_follow_dd1() {
        assert_eq!(weekday_header_names("en-US")[0], "Sun");
        assert_eq!(weekday_header_names("pt-PT")[1], "seg.");
        assert_eq!(weekday_header_names("pl-PL")[1], "pon");
        // No shipped row: the platform's names.
        let sw = weekday_header_names("sw-KE");
        assert!(sw.iter().all(|n| character_count(n) <= 4), "{sw:?}");
    }

    #[test]
    fn display_names_come_from_cldr() {
        assert_eq!(
            calendar_display_name(CalendarSystem::Persian, "en"),
            "Persian Calendar"
        );
        assert_eq!(
            calendar_display_name(CalendarSystem::Persian, "xx"),
            "Persian Calendar"
        );
        assert_eq!(week_field_name("de-DE"), "Woche");
        assert_eq!(week_field_name("en"), "week");
        assert_eq!(format_day_number(15, "fa"), "۱۵");
        assert_eq!(format_day_number(1234, "en"), "1234");
    }
}
