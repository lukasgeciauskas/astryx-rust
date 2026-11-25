//! Gregorian calendar dates, upstream `utils/plainDate.ts` and `dateTypes.ts`.
//!
//! Every date component reads and writes these (AST-111 FR3): a calendar
//! system changes only how a date is shown and typed.

use std::cell::Cell;
use std::fmt;
use std::str::FromStr;

/// A calendar date in the proleptic Gregorian calendar, upstream `PlainDate`
/// (and the `ISODateString` it reads and writes). Ordered by time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlainDate {
    pub year: i32,
    /// 1 (January) through 12.
    pub month: u8,
    pub day: u8,
}

/// The earliest and latest years a date can have, ICU4X's range.
pub(crate) const MIN_YEAR: i32 = -9999;
pub(crate) const MAX_YEAR: i32 = 9999;

impl PlainDate {
    /// A date, or `None` when the month or day does not exist
    /// (`plainDateCreate`).
    pub fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        ((MIN_YEAR..=MAX_YEAR).contains(&year)
            && (1..=12).contains(&month)
            && day >= 1
            && day <= days_in_month(year, month))
        .then_some(Self { year, month, day })
    }

    /// Reads `YYYY-MM-DD` (`plainDateFromISO`): a positive year and a real
    /// month and day. `None` otherwise.
    pub fn from_iso(text: &str) -> Option<Self> {
        let mut fields = text.trim().split('-');
        let (year, month, day) = (fields.next()?, fields.next()?, fields.next()?);
        if fields.next().is_some()
            || [year, month, day]
                .iter()
                .any(|f| f.is_empty() || !f.bytes().all(|b| b.is_ascii_digit()))
        {
            return None;
        }
        let year: i32 = year.parse().ok()?;
        if year < 1 {
            return None;
        }
        Self::new(year, month.parse().ok()?, day.parse().ok()?)
    }

    /// `YYYY-MM-DD` (`plainDateToISO`).
    pub fn to_iso(self) -> String {
        self.to_string()
    }

    /// Today in the system's time zone (`plainDateToday`), or the date
    /// [`set_today`] holds.
    pub fn today() -> Self {
        if let Some(today) = TODAY.with(Cell::get) {
            return today;
        }
        let now = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let timestamp = jiff::Timestamp::from_second(now).unwrap_or(jiff::Timestamp::UNIX_EPOCH);
        let date = timestamp
            .to_zoned_datetime(jiff::tz::TimeZone::system())
            .date();
        Self {
            year: date.year() as i32,
            month: date.month() as u8,
            day: date.day() as u8,
        }
    }

    /// Days since 1970-01-01 (negative before it).
    pub fn to_epoch_days(self) -> i64 {
        // Howard Hinnant's days_from_civil.
        let y = self.year as i64 - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let m = self.month as i64;
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + self.day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// The date `days` after 1970-01-01, clamped to ICU4X's year range.
    pub fn from_epoch_days(days: i64) -> Self {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let year = yoe + era * 400 + i64::from(month <= 2);
        if year < MIN_YEAR as i64 {
            return Self {
                year: MIN_YEAR,
                month: 1,
                day: 1,
            };
        }
        if year > MAX_YEAR as i64 {
            return Self {
                year: MAX_YEAR,
                month: 12,
                day: 31,
            };
        }
        Self {
            year: year as i32,
            month,
            day,
        }
    }

    /// The day of the week (`plainDateDayOfWeek`).
    pub fn weekday(self) -> Weekday {
        // 1970-01-01 was a Thursday.
        Weekday::from_index((self.to_epoch_days() + 4).rem_euclid(7) as u8)
    }

    /// The date `n` days later, or earlier when negative (`plainDateAddDays`).
    pub fn add_days(self, n: i64) -> Self {
        Self::from_epoch_days(self.to_epoch_days() + n)
    }

    /// The date `n` months later with its day clamped to the month's length
    /// (`plainDateAddMonths`, Temporal's `add`): January 31 plus a month is
    /// February 28 or 29.
    pub fn add_months(self, n: i32) -> Self {
        let total = self.year as i64 * 12 + (self.month as i64 - 1) + n as i64;
        let year = total.div_euclid(12).clamp(MIN_YEAR as i64, MAX_YEAR as i64) as i32;
        let month = total.rem_euclid(12) as u8 + 1;
        Self {
            year,
            month,
            day: self.day.min(days_in_month(year, month)),
        }
    }

    /// Whole days from `self` to `other`: positive when `other` is later
    /// (`plainDateDiffDays(self, other)`).
    pub fn days_until(self, other: Self) -> i64 {
        other.to_epoch_days() - self.to_epoch_days()
    }

    /// The first day of the month (`plainDateSetFirstOfMonth`).
    pub fn first_of_month(self) -> Self {
        Self { day: 1, ..self }
    }

    /// The first day of the week holding this date (`plainDateSetStartOfWeek`).
    pub fn start_of_week(self, week_starts_on: Weekday) -> Self {
        let delta = (self.weekday().index() as i64 - week_starts_on.index() as i64).rem_euclid(7);
        self.add_days(-delta)
    }

    /// The day after the last day of the week (`plainDateSetEndOfWeekExclusive`).
    pub fn end_of_week_exclusive(self, week_starts_on: Weekday) -> Self {
        self.start_of_week(week_starts_on).add_days(7)
    }

    /// The ISO 8601 week number: the week of the year that holds this week's
    /// Thursday (`plainDateGetWeekNumber`).
    pub fn iso_week_number(self) -> u8 {
        let iso_day = match self.weekday() {
            Weekday::Sunday => 7,
            day => day.index() as i64,
        };
        let thursday = self.add_days(4 - iso_day);
        let year_start = Self {
            year: thursday.year,
            month: 1,
            day: 1,
        };
        (year_start.days_until(thursday) / 7 + 1) as u8
    }

    /// Whether the date lies in `start..=end` (`plainDateIsInRange`).
    pub fn is_in_range(self, start: Self, end: Self) -> bool {
        self >= start && self <= end
    }
}

impl fmt::Display for PlainDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.year < 0 {
            write!(f, "-{:06}-{:02}-{:02}", -self.year, self.month, self.day)
        } else {
            write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
        }
    }
}

impl FromStr for PlainDate {
    type Err = InvalidDate;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_iso(s).ok_or(InvalidDate)
    }
}

/// Text that is not an ISO date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidDate;

impl fmt::Display for InvalidDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a YYYY-MM-DD date")
    }
}

impl std::error::Error for InvalidDate {}

/// The number of days in a Gregorian month (`getDaysInMonth`).
pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// A day of the week, upstream `DayOfWeek` (0 is Sunday) and `DayOfWeekName`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Weekday {
    #[default]
    Sunday,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
}

impl Weekday {
    const ALL: [Weekday; 7] = [
        Weekday::Sunday,
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
        Weekday::Saturday,
    ];

    /// 0 for Sunday through 6 for Saturday; other numbers wrap.
    pub fn from_index(index: u8) -> Self {
        Self::ALL[(index % 7) as usize]
    }

    /// 0 for Sunday through 6 for Saturday.
    pub fn index(self) -> u8 {
        self as u8
    }

    /// A three-letter name, `"sun"` through `"sat"`, in any case
    /// (`normalizeDayOfWeek`); anything else is Sunday.
    pub fn from_name(name: &str) -> Self {
        const NAMES: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
        NAMES
            .iter()
            .position(|n| n.eq_ignore_ascii_case(name))
            .map_or(Weekday::Sunday, |i| Self::ALL[i])
    }
}

impl From<u8> for Weekday {
    fn from(index: u8) -> Self {
        if index < 7 {
            Self::from_index(index)
        } else {
            Weekday::Sunday
        }
    }
}

impl From<&str> for Weekday {
    fn from(name: &str) -> Self {
        Self::from_name(name)
    }
}

/// A range of dates, both ends included, upstream `DateRange`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DateRange {
    pub start: PlainDate,
    pub end: PlainDate,
}

impl DateRange {
    pub fn new(start: PlainDate, end: PlainDate) -> Self {
        Self { start, end }
    }

    /// Whether a date lies in the range.
    pub fn contains(&self, date: PlainDate) -> bool {
        date.is_in_range(self.start, self.end)
    }
}

/// Bounds and a rejection test for [`nearest_accepted_date`].
#[derive(Default)]
pub struct AcceptedDates<'a> {
    pub min: Option<PlainDate>,
    pub max: Option<PlainDate>,
    /// Rejects a date inside the bounds, such as a date constraint.
    pub is_disabled: Option<&'a dyn Fn(PlainDate) -> bool>,
    /// How many days to search on each side of the clamped date (366 when 0).
    pub search_days: u32,
}

/// The date nearest to `target` that a field accepts (`nearestAcceptedDate`):
/// `target` clamped into the bounds, else the nearest allowed date within the
/// search window, the later one on a tie, else the clamped date.
pub fn nearest_accepted_date(target: PlainDate, options: &AcceptedDates<'_>) -> PlainDate {
    let mut clamped = target;
    if let Some(min) = options.min {
        clamped = clamped.max(min);
    }
    if let Some(max) = options.max {
        clamped = clamped.min(max);
    }
    let Some(is_disabled) = options.is_disabled else {
        return clamped;
    };
    if !is_disabled(clamped) {
        return clamped;
    }
    let accepted = |date: PlainDate| {
        options.min.is_none_or(|min| date >= min)
            && options.max.is_none_or(|max| date <= max)
            && !is_disabled(date)
    };
    let search = if options.search_days == 0 {
        366
    } else {
        options.search_days
    };
    for offset in 1..=search as i64 {
        let later = clamped.add_days(offset);
        if accepted(later) {
            return later;
        }
        let earlier = clamped.add_days(-offset);
        if accepted(earlier) {
            return earlier;
        }
    }
    clamped
}

thread_local! {
    static TODAY: Cell<Option<PlainDate>> = const { Cell::new(None) };
}

/// Holds the date [`PlainDate::today`] answers on this thread, or releases it
/// with `None`: for tests and screenshots, as upstream's tests fake `Date`.
pub fn set_today(today: Option<PlainDate>) {
    TODAY.with(|cell| cell.set(today));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> PlainDate {
        PlainDate::new(y, m, day).unwrap()
    }

    #[test]
    fn iso_round_trips_and_rejects_bad_dates() {
        assert_eq!(PlainDate::from_iso("2026-01-28"), Some(d(2026, 1, 28)));
        assert_eq!(d(2026, 1, 28).to_iso(), "2026-01-28");
        assert_eq!(d(33, 3, 4).to_iso(), "0033-03-04");
        assert_eq!(PlainDate::from_iso("2026-02-29"), None);
        assert_eq!(PlainDate::from_iso("2024-02-29"), Some(d(2024, 2, 29)));
        assert_eq!(PlainDate::from_iso("2026-13-01"), None);
        assert_eq!(PlainDate::from_iso("0000-01-01"), None);
        assert_eq!(PlainDate::from_iso("2026-1-5"), Some(d(2026, 1, 5)));
        assert_eq!(PlainDate::from_iso("2026/01/05"), None);
    }

    #[test]
    fn arithmetic_matches_temporal() {
        assert_eq!(d(2026, 1, 31).add_months(1), d(2026, 2, 28));
        assert_eq!(d(2024, 1, 31).add_months(1), d(2024, 2, 29));
        assert_eq!(d(2026, 1, 15).add_months(-1), d(2025, 12, 15));
        assert_eq!(d(2026, 12, 31).add_days(1), d(2027, 1, 1));
        assert_eq!(d(2024, 3, 1).add_days(-1), d(2024, 2, 29));
        assert_eq!(d(2026, 3, 1).days_until(d(2026, 3, 31)), 30);
        assert_eq!(PlainDate::from_epoch_days(0), d(1970, 1, 1));
        assert_eq!(d(1970, 1, 4).weekday(), Weekday::Sunday);
        assert_eq!(d(2026, 11, 25).weekday(), Weekday::Wednesday);
        for days in [-800_000, -1, 0, 1, 19_000, 2_900_000] {
            assert_eq!(PlainDate::from_epoch_days(days).to_epoch_days(), days);
        }
    }

    #[test]
    fn iso_week_numbers() {
        assert_eq!(d(2026, 1, 1).iso_week_number(), 1);
        assert_eq!(d(2021, 1, 1).iso_week_number(), 53);
        assert_eq!(d(2024, 12, 30).iso_week_number(), 1);
        // A year that starts on a Friday: every Thursday is a multiple of 7.
        assert_eq!(d(2021, 4, 8).iso_week_number(), 14);
    }

    #[test]
    fn weeks_start_where_asked() {
        let wed = d(2026, 11, 25);
        assert_eq!(wed.start_of_week(Weekday::Sunday), d(2026, 11, 22));
        assert_eq!(wed.start_of_week(Weekday::Monday), d(2026, 11, 23));
        assert_eq!(wed.end_of_week_exclusive(Weekday::Monday), d(2026, 11, 30));
        assert_eq!(Weekday::from_name("MON"), Weekday::Monday);
        assert_eq!(Weekday::from_name("x"), Weekday::Sunday);
    }

    #[test]
    fn nearest_accepted_date_prefers_later_on_ties() {
        let weekend =
            |date: PlainDate| matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday);
        let options = AcceptedDates {
            is_disabled: Some(&weekend),
            ..Default::default()
        };
        // Saturday 2026-11-28: Friday and Monday are 1 and 2 days away.
        assert_eq!(
            nearest_accepted_date(d(2026, 11, 28), &options),
            d(2026, 11, 27)
        );
        // Sunday: Monday and Friday are 1 and 2 away.
        assert_eq!(
            nearest_accepted_date(d(2026, 11, 29), &options),
            d(2026, 11, 30)
        );
        let bounded = AcceptedDates {
            min: Some(d(2026, 12, 1)),
            ..Default::default()
        };
        assert_eq!(
            nearest_accepted_date(d(2026, 11, 1), &bounded),
            d(2026, 12, 1)
        );
    }
}
