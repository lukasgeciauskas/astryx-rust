//! Calendar months and years of every calendar system, upstream
//! `utils/calendarSystem.ts` (AST-111).
//!
//! Values stay Gregorian [`PlainDate`]s. ICU4X's `icu_calendar` does the
//! arithmetic that upstream leaves to `Temporal`, with Temporal's semantics
//! (month codes, `constrain` and `reject` overflow), and every calendar
//! always works: ICU4X compiles its data in, so upstream's Gregorian fallback
//! for a runtime without `Temporal` (FR5) never applies.

use icu_calendar::options::{
    DateAddOptions, DateDifferenceOptions, DateDurationUnit, DateFromFieldsOptions, Overflow,
};
use icu_calendar::types::{DateDuration, DateFields, YearInfo};
use icu_calendar::{AnyCalendar, AnyCalendarKind, Date, Iso};

use super::format::{
    DatePart, DateStyle, PartKind, format_date_to_parts, format_day_number,
    format_standalone_month_year, join_parts,
};
use super::plain::{PlainDate, days_in_month};
use crate::CalendarSystem;

impl CalendarSystem {
    /// Every calendar, in upstream's order (FR1).
    pub const ALL: [CalendarSystem; 16] = [
        CalendarSystem::Gregory,
        CalendarSystem::Iso8601,
        CalendarSystem::Buddhist,
        CalendarSystem::Roc,
        CalendarSystem::Japanese,
        CalendarSystem::Persian,
        CalendarSystem::Indian,
        CalendarSystem::Hebrew,
        CalendarSystem::Chinese,
        CalendarSystem::Dangi,
        CalendarSystem::Coptic,
        CalendarSystem::Ethiopic,
        CalendarSystem::Ethioaa,
        CalendarSystem::IslamicCivil,
        CalendarSystem::IslamicTbla,
        CalendarSystem::IslamicUmalqura,
    ];

    /// The calendar with this identifier, `None` for any other
    /// (`isCalendarSystem`).
    pub fn from_identifier(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.identifier() == id)
    }

    /// The calendar with this identifier; any other resolves to `gregory`
    /// and warns once in development (`resolveCalendarSystem`, FR1).
    pub fn resolve(id: &str, source: &str) -> Self {
        Self::from_identifier(id).unwrap_or_else(|| {
            rustryx_primitives::dev_warn_once(&format!("calendar-unsupported:{id}"), || {
                format!(
                    "{source}: The calendar \"{id}\" is not supported; showing Gregorian dates."
                )
            });
            CalendarSystem::Gregory
        })
    }

    /// Whether `<input type="date">` can show the calendar: `gregory` and
    /// `iso8601` (`isGregorianCalendar`).
    pub fn is_gregorian(self) -> bool {
        matches!(self, CalendarSystem::Gregory | CalendarSystem::Iso8601)
    }

    /// Whether the calendar's months and days are the Gregorian ones
    /// (`isGregorianAligned`).
    pub fn is_gregorian_aligned(self) -> bool {
        matches!(
            self,
            CalendarSystem::Gregory
                | CalendarSystem::Iso8601
                | CalendarSystem::Buddhist
                | CalendarSystem::Roc
                | CalendarSystem::Japanese
        )
    }

    fn kind(self) -> AnyCalendarKind {
        match self {
            CalendarSystem::Gregory => AnyCalendarKind::Gregorian,
            CalendarSystem::Iso8601 => AnyCalendarKind::Iso,
            CalendarSystem::Buddhist => AnyCalendarKind::Buddhist,
            CalendarSystem::Roc => AnyCalendarKind::Roc,
            CalendarSystem::Japanese => AnyCalendarKind::Japanese,
            CalendarSystem::Persian => AnyCalendarKind::Persian,
            CalendarSystem::Indian => AnyCalendarKind::Indian,
            CalendarSystem::Hebrew => AnyCalendarKind::Hebrew,
            CalendarSystem::Chinese => AnyCalendarKind::Chinese,
            CalendarSystem::Dangi => AnyCalendarKind::Dangi,
            CalendarSystem::Coptic => AnyCalendarKind::Coptic,
            CalendarSystem::Ethiopic => AnyCalendarKind::Ethiopian,
            CalendarSystem::Ethioaa => AnyCalendarKind::EthiopianAmeteAlem,
            CalendarSystem::IslamicCivil => AnyCalendarKind::HijriTabularTypeIIFriday,
            CalendarSystem::IslamicTbla => AnyCalendarKind::HijriTabularTypeIIThursday,
            CalendarSystem::IslamicUmalqura => AnyCalendarKind::HijriUmmAlQura,
        }
    }
}

/// The calendar's ICU4X implementation; built from compiled data, so cheap.
fn any(calendar: CalendarSystem) -> AnyCalendar {
    AnyCalendar::new(calendar.kind())
}

type CalendarDate = Date<AnyCalendar>;

fn in_calendar(calendar: CalendarSystem, pd: PlainDate) -> CalendarDate {
    let iso = Date::try_new_iso(pd.year, pd.month, pd.day)
        .unwrap_or_else(|_| Date::try_new_iso(1970, 1, 1).expect("1970-01-01 exists"));
    iso.to_calendar(any(calendar))
}

fn to_gregorian(date: &CalendarDate) -> PlainDate {
    let iso = date.to_calendar(Iso);
    PlainDate::new(
        iso.year().extended_year(),
        iso.month().ordinal,
        iso.day_of_month().0,
    )
    .unwrap_or(PlainDate {
        year: 1970,
        month: 1,
        day: 1,
    })
}

fn constrain() -> DateAddOptions {
    let mut options = DateAddOptions::default();
    options.overflow = Some(Overflow::Constrain);
    options
}

fn duration(years: i32, months: i32) -> DateDuration {
    if years != 0 {
        DateDuration::for_years(years)
    } else {
        DateDuration::for_months(months)
    }
}

fn added(date: CalendarDate, years: i32, months: i32) -> CalendarDate {
    let fallback = to_gregorian(&date);
    date.try_added_with_options(duration(years, months), constrain())
        .unwrap_or_else(|_| {
            Date::try_new_iso(fallback.year, fallback.month, fallback.day)
                .expect("a date converted from a calendar exists")
                .to_calendar(AnyCalendar::new(AnyCalendarKind::Iso))
        })
}

/// A month of a calendar, located by its first and last Gregorian days
/// (`CalendarMonth`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CalendarMonth {
    pub start: PlainDate,
    pub end: PlainDate,
    pub length: u8,
}

impl CalendarMonth {
    fn new(start: PlainDate, length: u8) -> Self {
        Self {
            start,
            end: start.add_days(length as i64 - 1),
            length,
        }
    }

    fn gregorian(year: i32, month: u8) -> Self {
        Self::new(
            PlainDate {
                year,
                month,
                day: 1,
            },
            days_in_month(year, month),
        )
    }

    fn of_first_day(first: &CalendarDate) -> Self {
        Self::new(to_gregorian(first), first.days_in_month())
    }

    /// Whether a Gregorian date falls inside the month (`isInCalendarMonth`).
    pub fn contains(&self, pd: PlainDate) -> bool {
        pd.is_in_range(self.start, self.end)
    }

    /// The day that names the month: its middle day, because a Japanese era
    /// can start inside a month (`calendarMonthLabelDay`).
    pub fn label_day(&self) -> PlainDate {
        self.start.add_days((self.length / 2) as i64)
    }
}

/// A calendar month by its year and its position in that year, as
/// `Temporal.PlainDate` numbers them: month 13 exists in a Hebrew leap year
/// (`CalendarYearMonth`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CalendarYearMonth {
    pub year: i32,
    pub month: u8,
}

/// The day of the calendar month a Gregorian date falls on
/// (`calendarDayOfMonth`).
pub fn calendar_day_of_month(calendar: CalendarSystem, pd: PlainDate) -> u8 {
    if calendar.is_gregorian_aligned() {
        pd.day
    } else {
        in_calendar(calendar, pd).day_of_month().0
    }
}

fn first_day(calendar: CalendarSystem, date: &CalendarDate) -> CalendarDate {
    let day = date.day_of_month().0 as i64;
    in_calendar(calendar, to_gregorian(date).add_days(1 - day))
}

/// The calendar month that holds a Gregorian date (`calendarMonthOf`).
pub fn calendar_month_of(calendar: CalendarSystem, pd: PlainDate) -> CalendarMonth {
    if calendar.is_gregorian_aligned() {
        return CalendarMonth::gregorian(pd.year, pd.month);
    }
    CalendarMonth::of_first_day(&first_day(calendar, &in_calendar(calendar, pd)))
}

/// How many years a Gregorian-aligned calendar counts ahead of Gregorian ones:
/// 2026 is 2569 in `buddhist` and 115 in `roc`.
fn gregorian_year_offset(calendar: CalendarSystem) -> i32 {
    match calendar {
        CalendarSystem::Buddhist => 543,
        CalendarSystem::Roc => -1911,
        _ => 0,
    }
}

/// The year and month numbers of a calendar month (`calendarYearMonthOf`).
pub fn calendar_year_month_of(
    calendar: CalendarSystem,
    month: &CalendarMonth,
) -> CalendarYearMonth {
    if calendar.is_gregorian_aligned() {
        return CalendarYearMonth {
            year: month.start.year + gregorian_year_offset(calendar),
            month: month.start.month,
        };
    }
    let date = in_calendar(calendar, month.start);
    CalendarYearMonth {
        year: date.year().extended_year(),
        month: date.month().ordinal,
    }
}

fn from_ordinal(
    calendar: CalendarSystem,
    year: i32,
    month: u8,
    day: u8,
    overflow: Overflow,
) -> Option<CalendarDate> {
    let mut fields = DateFields::default();
    fields.extended_year = Some(year);
    fields.ordinal_month = Some(month);
    fields.day = Some(day);
    let mut options = DateFromFieldsOptions::default();
    options.overflow = Some(overflow);
    Date::try_from_fields(fields, options, any(calendar)).ok()
}

/// The calendar month with these year and month numbers; a month past the
/// end of its year is the year's last month, as Temporal constrains it
/// (`calendarMonthFromYearMonth`).
pub fn calendar_month_from_year_month(
    calendar: CalendarSystem,
    ym: CalendarYearMonth,
) -> CalendarMonth {
    if calendar.is_gregorian_aligned() {
        return CalendarMonth::gregorian(
            ym.year - gregorian_year_offset(calendar),
            ym.month.clamp(1, 12),
        );
    }
    match from_ordinal(calendar, ym.year, ym.month.max(1), 1, Overflow::Constrain) {
        Some(first) => CalendarMonth::of_first_day(&first),
        None => calendar_month_of(
            calendar,
            PlainDate {
                year: 1970,
                month: 1,
                day: 1,
            },
        ),
    }
}

/// The calendar month `n` months after `month`, before it when negative
/// (`calendarAddMonths`).
pub fn calendar_add_months(
    calendar: CalendarSystem,
    month: &CalendarMonth,
    n: i32,
) -> CalendarMonth {
    if n == 0 {
        return *month;
    }
    if calendar.is_gregorian_aligned() {
        let moved = month.start.add_months(n);
        return CalendarMonth::gregorian(moved.year, moved.month);
    }
    CalendarMonth::of_first_day(&added(in_calendar(calendar, month.start), 0, n))
}

/// A date `n` calendar months away, its day clamped to the target month
/// (`calendarShiftMonths`).
pub fn calendar_shift_months(calendar: CalendarSystem, pd: PlainDate, n: i32) -> PlainDate {
    if calendar.is_gregorian_aligned() {
        return pd.add_months(n);
    }
    to_gregorian(&added(in_calendar(calendar, pd), 0, n))
}

/// A date `n` calendar years away, in the same month of that year: a leap
/// month the target year lacks becomes its common counterpart (Adar I becomes
/// Adar), and the day clamps (`calendarShiftYears`).
pub fn calendar_shift_years(calendar: CalendarSystem, pd: PlainDate, n: i32) -> PlainDate {
    if calendar.is_gregorian_aligned() {
        return pd.add_months(n * 12);
    }
    to_gregorian(&added(in_calendar(calendar, pd), n, 0))
}

/// The fields of a date written in a calendar (`CalendarDateFields`): either
/// `year` or `era` with `era_year`, and either `month_code` (`M05L` for a
/// leap month) or `month`, the month's position in its year.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CalendarDateFields {
    pub year: Option<i32>,
    pub era: Option<String>,
    pub era_year: Option<i32>,
    pub month_code: Option<String>,
    pub month: Option<u8>,
    pub day: u8,
}

/// The Gregorian date for fields written in a calendar, or `None` when the
/// calendar lacks that date: the fields are never adjusted, so 30 Esfand in a
/// common Persian year is `None` (`calendarDateFromFields`).
pub fn calendar_date_from_fields(
    calendar: CalendarSystem,
    fields: &CalendarDateFields,
) -> Option<PlainDate> {
    let mut icu = DateFields::default();
    icu.era = fields.era.as_deref().map(str::as_bytes);
    icu.era_year = fields.era_year;
    icu.extended_year = fields.year;
    icu.month_code = fields.month_code.as_deref().map(str::as_bytes);
    icu.ordinal_month = fields.month;
    icu.day = Some(fields.day);
    let mut options = DateFromFieldsOptions::default();
    options.overflow = Some(Overflow::Reject);
    Date::try_from_fields(icu, options, any(calendar))
        .ok()
        .map(|date| to_gregorian(&date))
}

/// The number of months in the calendar year holding a date
/// (`calendarMonthsInYear`).
pub fn calendar_months_in_year(calendar: CalendarSystem, pd: PlainDate) -> u8 {
    if calendar.is_gregorian_aligned() {
        12
    } else {
        in_calendar(calendar, pd).months_in_year()
    }
}

/// Calendar months from `from` to `to`, positive when `to` is later
/// (`calendarMonthsBetween`).
pub fn calendar_months_between(
    calendar: CalendarSystem,
    from: &CalendarMonth,
    to: &CalendarMonth,
) -> i32 {
    if calendar.is_gregorian_aligned() {
        return (to.start.year - from.start.year) * 12
            + (to.start.month as i32 - from.start.month as i32);
    }
    let mut options = DateDifferenceOptions::default();
    options.largest_unit = Some(DateDurationUnit::Months);
    let a = in_calendar(calendar, from.start);
    let b = in_calendar(calendar, to.start);
    match a.try_until_with_options(&b, options) {
        Ok(d) => {
            let months = (d.years * 12 + d.months) as i32;
            if d.is_negative { -months } else { months }
        }
        Err(_) => 0,
    }
}

/// The calendar month at a position in a calendar year; a Gregorian-aligned
/// calendar counts Gregorian years here (`calendarMonthOfYear`).
pub fn calendar_month_of_year(calendar: CalendarSystem, year: i32, month: u8) -> CalendarMonth {
    if calendar.is_gregorian_aligned() {
        CalendarMonth::gregorian(year, month.clamp(1, 12))
    } else {
        calendar_month_from_year_month(calendar, CalendarYearMonth { year, month })
    }
}

/// The number of months in a year of the calendar
/// (`calendarMonthsInCalendarYear`).
pub fn calendar_months_in_calendar_year(calendar: CalendarSystem, year: i32) -> u8 {
    if calendar.is_gregorian_aligned() {
        12
    } else {
        calendar_months_in_year(calendar, calendar_month_of_year(calendar, year, 1).start)
    }
}

/// The calendar's own year number of a Gregorian date, as Temporal's `year`.
pub fn calendar_year_of(calendar: CalendarSystem, pd: PlainDate) -> i32 {
    if calendar.is_gregorian_aligned() {
        pd.year + gregorian_year_offset(calendar)
    } else {
        in_calendar(calendar, pd).year().extended_year()
    }
}

/// The era code and era year of a date, such as `("reiwa", 8)`.
pub fn calendar_era_year(calendar: CalendarSystem, pd: PlainDate) -> (String, i32) {
    match in_calendar(calendar, pd).year() {
        YearInfo::Era(era) => (era.era.to_string(), era.year),
        other => (String::new(), other.extended_year()),
    }
}

/// The month code of a date, such as `M05L` for a leap month.
pub fn calendar_month_code(calendar: CalendarSystem, pd: PlainDate) -> String {
    let month = in_calendar(calendar, pd).month();
    let code = month.to_input();
    format!(
        "M{:02}{}",
        code.number(),
        if code.is_leap() { "L" } else { "" }
    )
}

/// The stand-alone month and year label of a calendar month, such as "Aban
/// 1405 AP", named by its middle day. With `day`, a month that spans an era
/// change is named by that day instead, clamped to the month, so January 1989
/// reads 昭和64年1月 on the 3rd and 平成元年1月 on the 22nd
/// (`formatCalendarMonthLabel`).
pub fn format_calendar_month_label(
    calendar: CalendarSystem,
    month: &CalendarMonth,
    locale: &str,
    day: Option<u8>,
) -> String {
    let label_day = match day {
        Some(day) if month_spans_eras(calendar, month, locale) => {
            month.start.add_days(day.clamp(1, month.length) as i64 - 1)
        }
        _ => month.label_day(),
    };
    format_standalone_month_year(label_day, locale, calendar)
}

fn month_spans_eras(calendar: CalendarSystem, month: &CalendarMonth, locale: &str) -> bool {
    !calendar.is_gregorian()
        && era_year_at(calendar, month.start, locale).era
            != era_year_at(calendar, month.end, locale).era
}

/// Formats a Gregorian date in the calendar, such as "Friday, Aban 8, 1405
/// AP" (`formatCalendarDate`). With `related_year`, a Chinese or Dangi year
/// the locale names only by its place in the 60-year cycle becomes the
/// related Gregorian year.
pub fn format_calendar_date(
    calendar: CalendarSystem,
    pd: PlainDate,
    style: DateStyle,
    locale: &str,
    related_year: bool,
) -> String {
    join_parts(&format_calendar_date_to_parts(
        calendar,
        pd,
        style,
        locale,
        related_year,
    ))
}

/// The parts of [`format_calendar_date`]'s label.
pub fn format_calendar_date_to_parts(
    calendar: CalendarSystem,
    pd: PlainDate,
    style: DateStyle,
    locale: &str,
    related_year: bool,
) -> Vec<DatePart> {
    let parts = format_date_to_parts(pd, style, locale, calendar);
    if !related_year || !matches!(calendar, CalendarSystem::Chinese | CalendarSystem::Dangi) {
        return parts;
    }
    let has_related = parts.iter().any(|p| p.kind == PartKind::RelatedYear);
    let year = format_day_number(calendar_year_of(calendar, pd) as i64, locale);
    parts
        .into_iter()
        .map(|p| match p.kind {
            PartKind::Year | PartKind::YearName if !has_related => DatePart {
                kind: p.kind,
                value: year.clone(),
            },
            _ => p,
        })
        .collect()
}

/// The name of a calendar month, such as "Adar I", from its middle day
/// (`formatCalendarMonthName`).
pub fn format_calendar_month_name(
    calendar: CalendarSystem,
    month: &CalendarMonth,
    locale: &str,
) -> String {
    format_calendar_date(calendar, month.label_day(), DateStyle::MONTH, locale, false)
}

struct EraYearLabel {
    era: Option<String>,
    label: Option<String>,
}

/// The era and year parts of a date as the locale writes them in a long date.
fn era_year_at(calendar: CalendarSystem, pd: PlainDate, locale: &str) -> EraYearLabel {
    let parts = format_date_to_parts(pd, DateStyle::LONG.with_era(Width::Short), locale, calendar);
    let era = parts
        .iter()
        .find(|p| p.kind == PartKind::Era)
        .map(|p| p.value.clone());
    let is_year = |p: &DatePart| {
        matches!(
            p.kind,
            PartKind::Era | PartKind::Year | PartKind::RelatedYear
        )
    };
    let Some(first) = parts.iter().position(is_year) else {
        return EraYearLabel { era, label: None };
    };
    let last = parts.iter().rposition(is_year).unwrap_or(first);
    let label: String = parts[first..=last]
        .iter()
        .filter(|p| is_year(p) || p.kind == PartKind::Literal)
        .map(|p| p.value.as_str())
        .collect();
    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
    EraYearLabel {
        era,
        label: Some(label),
    }
}

use super::format::Width;

/// Whether a calendar year's first and last days fall in different eras, as
/// the Japanese year 2019 does (`calendarYearSpansEras`).
pub fn calendar_year_spans_eras(calendar: CalendarSystem, year: i32, locale: &str) -> bool {
    if calendar.is_gregorian() {
        return false;
    }
    let months = calendar_months_in_calendar_year(calendar, year);
    let start = era_year_at(
        calendar,
        calendar_month_of_year(calendar, year, 1).start,
        locale,
    );
    let end = era_year_at(
        calendar,
        calendar_month_of_year(calendar, year, months).end,
        locale,
    );
    start.era != end.era && start.label.is_some() && end.label.is_some()
}

/// The label of a calendar year with its era where the locale writes one,
/// such as "令和8" in `ja-JP` or "1405 AP" in English. A year an era change
/// splits takes its era from `at` (a month position and a day), so 2019
/// reads "平成31" in January and "令和元" in May; Gregorian years are plain
/// numbers (`formatCalendarYearLabel`).
pub fn format_calendar_year_label(
    calendar: CalendarSystem,
    year: i32,
    locale: &str,
    at: Option<(u8, u8)>,
) -> String {
    if calendar.is_gregorian() {
        return year.to_string();
    }
    let months = calendar_months_in_calendar_year(calendar, year);
    if let Some((month, day)) = at
        && calendar_year_spans_eras(calendar, year, locale)
    {
        let month = calendar_month_of_year(calendar, year, month.clamp(1, months));
        let day = day.clamp(1, month.length);
        return era_year_at(calendar, month.start.add_days(day as i64 - 1), locale)
            .label
            .unwrap_or_else(|| year.to_string());
    }
    era_year_at(
        calendar,
        calendar_month_of_year(calendar, year, months / 2 + 1).start,
        locale,
    )
    .label
    .unwrap_or_else(|| year.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> PlainDate {
        PlainDate::new(y, m, day).unwrap()
    }

    #[test]
    fn identifiers_resolve_with_a_gregorian_fallback() {
        for calendar in CalendarSystem::ALL {
            assert_eq!(
                CalendarSystem::from_identifier(calendar.identifier()),
                Some(calendar)
            );
        }
        assert_eq!(
            CalendarSystem::resolve("julian", "test"),
            CalendarSystem::Gregory
        );
        assert_eq!(
            CalendarSystem::resolve("islamic-civil", "test"),
            CalendarSystem::IslamicCivil
        );
    }

    #[test]
    fn years_count_as_temporal_counts_them() {
        assert_eq!(
            calendar_year_of(CalendarSystem::Buddhist, d(2026, 6, 15)),
            2569
        );
        assert_eq!(calendar_year_of(CalendarSystem::Roc, d(2026, 6, 15)), 115);
        assert_eq!(
            calendar_year_of(CalendarSystem::Japanese, d(2026, 6, 15)),
            2026
        );
        assert_eq!(
            calendar_year_of(CalendarSystem::Persian, d(2026, 10, 30)),
            1405
        );
        assert_eq!(
            calendar_year_of(CalendarSystem::Hebrew, d(2024, 3, 15)),
            5784
        );
        assert_eq!(
            calendar_year_of(CalendarSystem::Chinese, d(2026, 6, 15)),
            2026
        );
    }

    #[test]
    fn persian_months_follow_the_calendar() {
        let month = calendar_month_of(CalendarSystem::Persian, d(2026, 10, 30));
        // Aban 1405 runs from October 23 to November 21.
        assert_eq!(month.start, d(2026, 10, 23));
        assert_eq!(month.length, 30);
        assert_eq!(
            calendar_day_of_month(CalendarSystem::Persian, d(2026, 10, 30)),
            8
        );
        let ym = calendar_year_month_of(CalendarSystem::Persian, &month);
        assert_eq!(
            ym,
            CalendarYearMonth {
                year: 1405,
                month: 8
            }
        );
        assert_eq!(
            calendar_month_from_year_month(CalendarSystem::Persian, ym),
            month
        );
        assert_eq!(
            format_calendar_month_label(CalendarSystem::Persian, &month, "en", None),
            "Aban 1405 AP"
        );
    }

    #[test]
    fn hebrew_leap_years_have_thirteen_months() {
        // 5784 is a leap year: Adar I is month 6 and Adar II month 7.
        assert_eq!(
            calendar_months_in_calendar_year(CalendarSystem::Hebrew, 5784),
            13
        );
        assert_eq!(
            calendar_months_in_calendar_year(CalendarSystem::Hebrew, 5785),
            12
        );
        let adar1 = calendar_month_from_year_month(
            CalendarSystem::Hebrew,
            CalendarYearMonth {
                year: 5784,
                month: 6,
            },
        );
        assert_eq!(
            format_calendar_month_name(CalendarSystem::Hebrew, &adar1, "en"),
            "Adar I"
        );
        // A year step from Adar I lands on Adar of the common year.
        let next = calendar_shift_years(CalendarSystem::Hebrew, adar1.start, 1);
        assert_eq!(calendar_month_code(CalendarSystem::Hebrew, next), "M06");
        assert_eq!(
            format_calendar_month_name(
                CalendarSystem::Hebrew,
                &calendar_month_of(CalendarSystem::Hebrew, next),
                "en"
            ),
            "Adar"
        );
        // Month 13 past a common year's end constrains to its last month.
        let last = calendar_month_from_year_month(
            CalendarSystem::Hebrew,
            CalendarYearMonth {
                year: 5785,
                month: 13,
            },
        );
        assert_eq!(
            calendar_year_month_of(CalendarSystem::Hebrew, &last).month,
            12
        );
    }

    #[test]
    fn fields_are_rejected_not_adjusted() {
        // 1404 is a common Persian year: Esfand has 29 days.
        let esfand30 = CalendarDateFields {
            year: Some(1404),
            month: Some(12),
            day: 30,
            ..Default::default()
        };
        assert_eq!(
            calendar_date_from_fields(CalendarSystem::Persian, &esfand30),
            None
        );
        let adar1_common = CalendarDateFields {
            year: Some(5785),
            month_code: Some("M05L".into()),
            day: 1,
            ..Default::default()
        };
        assert_eq!(
            calendar_date_from_fields(CalendarSystem::Hebrew, &adar1_common),
            None
        );
        let reiwa = CalendarDateFields {
            era: Some("reiwa".into()),
            era_year: Some(8),
            month: Some(6),
            day: 15,
            ..Default::default()
        };
        assert_eq!(
            calendar_date_from_fields(CalendarSystem::Japanese, &reiwa),
            Some(d(2026, 6, 15))
        );
    }

    #[test]
    fn steps_move_by_calendar_months() {
        let pd = d(2026, 10, 30);
        let next = calendar_shift_months(CalendarSystem::Persian, pd, 1);
        assert_eq!(calendar_day_of_month(CalendarSystem::Persian, next), 8);
        let a = calendar_month_of(CalendarSystem::Persian, pd);
        let b = calendar_add_months(CalendarSystem::Persian, &a, 5);
        assert_eq!(calendar_months_between(CalendarSystem::Persian, &a, &b), 5);
        assert_eq!(calendar_months_between(CalendarSystem::Persian, &b, &a), -5);
        // The Ethiopic thirteenth month has five or six days.
        let pagume = calendar_month_of(CalendarSystem::Ethiopic, d(2026, 9, 9));
        assert!(pagume.length == 5 || pagume.length == 6, "{pagume:?}");
    }

    #[test]
    fn japanese_years_take_the_era_of_the_day() {
        assert!(calendar_year_spans_eras(
            CalendarSystem::Japanese,
            2019,
            "ja-JP"
        ));
        assert_eq!(
            format_calendar_year_label(CalendarSystem::Japanese, 2019, "ja-JP", Some((1, 15))),
            "平成31"
        );
        assert_eq!(
            format_calendar_year_label(CalendarSystem::Japanese, 2019, "ja-JP", Some((7, 1))),
            "令和元"
        );
        assert_eq!(
            format_calendar_year_label(CalendarSystem::Gregory, 2019, "ja-JP", None),
            "2019"
        );
    }

    #[test]
    fn lunisolar_dates_use_the_related_year() {
        let text = format_calendar_date(
            CalendarSystem::Chinese,
            d(2026, 6, 15),
            DateStyle::LONG,
            "zh",
            true,
        );
        assert!(text.contains("2026"), "{text}");
    }
}
