//! Dates, times and relative times, upstream's `Intl.DateTimeFormat` and
//! `Intl.RelativeTimeFormat` use in `Timestamp/formatInstant.ts` and
//! `formatRelativeTime.ts`.
//!
//! Time zones come from jiff (IANA rules and the system zone, bundled on
//! Windows and wasm); the words, patterns and calendars from ICU4X's CLDR
//! data, the data behind `Intl`.

use icu_datetime::fieldsets::zone::{SpecificLong, SpecificShort};
use icu_datetime::fieldsets::{T, YMD, YMDE};
use icu_datetime::input::{Date, DateTime, Time, TimeZone, UtcOffset, ZonedDateTime};
use icu_datetime::{DateTimeFormatter, NoCalendarFormatter};
use icu_decimal::input::Decimal;
use icu_experimental::relativetime::options::Numeric;
use icu_experimental::relativetime::{RelativeTimeFormatter, RelativeTimeFormatterOptions};
use icu_locale::Locale;
use icu_time::zone::ZoneNameTimestamp;
use writeable::Writeable as _;

use crate::CalendarSystem;

/// The absolute formats of upstream `TimestampFormat` (and its tooltip's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstantFormat {
    /// "Mar 21, 2025".
    Date,
    /// "March 21, 2025".
    DateLong,
    /// "Fri, Mar 21, 2025".
    DateWeekday,
    /// "Mar 21, 2025, 2:51 PM".
    DateTime,
    /// "2:51 PM".
    Time,
    /// "March 21, 2025 at 2:51:07 PM PDT": the tooltip's full time.
    Full,
    /// The full time with the zone spelled out, for accessible names:
    /// "March 21, 2025 at 2:51:07 PM Pacific Daylight Time".
    FullLongZone,
    /// "2025-03-21".
    SystemDate,
    /// "2025-03-21 14:51:07".
    SystemDateTime,
    /// "14:51:07".
    SystemTime,
    /// "1742593867".
    UnixSeconds,
}

/// The zone an absolute time is written in, upstream `timezoneID`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TimeZoneChoice {
    /// The system's zone (`'local'`, or omitted).
    #[default]
    Local,
    /// An IANA zone, such as `"UTC"` or `"America/Los_Angeles"`.
    Iana(String),
}

/// An instant's wall clock in a zone.
struct WallClock {
    zoned: jiff::ZonedDateTime,
    iana: Option<String>,
}

fn wall_clock(unix_seconds: i64, zone: &TimeZoneChoice) -> WallClock {
    let tz = match zone {
        TimeZoneChoice::Local => jiff::tz::TimeZone::system(),
        TimeZoneChoice::Iana(id) => jiff::tz::TimeZone::get(id).unwrap_or(jiff::tz::TimeZone::UTC),
    };
    let timestamp =
        jiff::Timestamp::from_second(unix_seconds).unwrap_or(jiff::Timestamp::UNIX_EPOCH);
    let iana = tz.iana_name().map(str::to_string);
    WallClock {
        zoned: timestamp.to_zoned_datetime(tz),
        iana,
    }
}

fn icu_datetime_of(clock: &WallClock) -> Option<DateTime<icu_calendar::Iso>> {
    let dt = clock.zoned.datetime();
    Some(DateTime {
        date: Date::try_new_iso(dt.year() as i32, dt.month() as u8, dt.day() as u8).ok()?,
        time: Time::try_new(dt.hour() as u8, dt.minute() as u8, dt.second() as u8, 0).ok()?,
    })
}

fn icu_zone_of(
    clock: &WallClock,
    unix_seconds: i64,
) -> icu_datetime::input::TimeZoneInfo<icu_time::zone::models::AtTime> {
    let zone = clock
        .iana
        .as_deref()
        .map(TimeZone::from_iana_id)
        .unwrap_or(TimeZone::UNKNOWN);
    zone.with_offset(UtcOffset::try_from_seconds(clock.zoned.offset().seconds()).ok())
        .with_zone_name_timestamp(ZoneNameTimestamp::from_epoch_seconds(unix_seconds))
}

/// The locale with its calendar keyword set to `calendar`: a `-u-ca-`
/// keyword the locale carries never chooses the calendar (AST-111 FR2).
fn with_calendar(locale: &Locale, calendar: CalendarSystem) -> Locale {
    let mut locale = locale.clone();
    let id = if calendar == CalendarSystem::Iso8601 {
        "gregory"
    } else {
        calendar.identifier()
    };
    if let Ok(value) = icu_locale::extensions::unicode::Value::try_from_str(id) {
        locale
            .extensions
            .unicode
            .keywords
            .set(icu_locale::extensions::unicode::key!("ca"), value);
    }
    locale
}

/// Formats an instant, upstream `formatInstant`.
pub fn format_instant(
    locale: &Locale,
    unix_seconds: i64,
    format: InstantFormat,
    zone: &TimeZoneChoice,
    show_zone: bool,
    calendar: CalendarSystem,
) -> String {
    let clock = wall_clock(unix_seconds, zone);
    let dt = clock.zoned.datetime();
    let pad = |n: i8| format!("{n:02}");
    match format {
        InstantFormat::SystemDate => {
            return format!("{:04}-{}-{}", dt.year(), pad(dt.month()), pad(dt.day()));
        }
        InstantFormat::SystemDateTime => {
            return format!(
                "{:04}-{}-{} {}:{}:{}",
                dt.year(),
                pad(dt.month()),
                pad(dt.day()),
                pad(dt.hour()),
                pad(dt.minute()),
                pad(dt.second())
            );
        }
        InstantFormat::SystemTime => {
            return format!(
                "{}:{}:{}",
                pad(dt.hour()),
                pad(dt.minute()),
                pad(dt.second())
            );
        }
        InstantFormat::UnixSeconds => return unix_seconds.to_string(),
        _ => {}
    }
    let Some(datetime) = icu_datetime_of(&clock) else {
        return String::new();
    };
    let prefs = with_calendar(locale, calendar);
    let zoned = || ZonedDateTime {
        date: datetime.date,
        time: datetime.time,
        zone: icu_zone_of(&clock, unix_seconds),
    };
    let fallback = || format!("{} {}", dt.date(), dt.time());
    match format {
        InstantFormat::Date => DateTimeFormatter::try_new((&prefs).into(), YMD::medium())
            .map(|f| f.format(&datetime.date).write_to_string().into_owned()),
        InstantFormat::DateLong => DateTimeFormatter::try_new((&prefs).into(), YMD::long())
            .map(|f| f.format(&datetime.date).write_to_string().into_owned()),
        InstantFormat::DateWeekday => DateTimeFormatter::try_new((&prefs).into(), YMDE::medium())
            .map(|f| f.format(&datetime.date).write_to_string().into_owned()),
        InstantFormat::DateTime if show_zone => DateTimeFormatter::try_new(
            (&prefs).into(),
            YMD::medium().with_time_hm().with_zone(SpecificShort),
        )
        .map(|f| f.format(&zoned()).write_to_string().into_owned()),
        InstantFormat::DateTime => {
            DateTimeFormatter::try_new((&prefs).into(), YMD::medium().with_time_hm())
                .map(|f| f.format(&datetime).write_to_string().into_owned())
        }
        InstantFormat::Time if show_zone => {
            // A calendar-free formatter takes no zoned input, so the zone
            // name follows the time as CLDR's "{time} {zone}" puts it.
            let time = NoCalendarFormatter::try_new((&prefs).into(), T::hm())
                .map(|f| f.format(&datetime.time).write_to_string().into_owned());
            let zone = NoCalendarFormatter::try_new((&prefs).into(), SpecificShort).map(|f| {
                f.format(&icu_zone_of(&clock, unix_seconds))
                    .write_to_string()
                    .into_owned()
            });
            time.map(|time| match zone {
                Ok(zone) => format!("{time} {zone}"),
                Err(_) => time,
            })
        }
        InstantFormat::Time => NoCalendarFormatter::try_new((&prefs).into(), T::hm())
            .map(|f| f.format(&datetime.time).write_to_string().into_owned()),
        InstantFormat::Full => DateTimeFormatter::try_new(
            (&prefs).into(),
            YMD::long().with_time_hms().with_zone(SpecificShort),
        )
        .map(|f| f.format(&zoned()).write_to_string().into_owned()),
        InstantFormat::FullLongZone => DateTimeFormatter::try_new(
            (&prefs).into(),
            YMD::long().with_time_hms().with_zone(SpecificLong),
        )
        .map(|f| f.format(&zoned()).write_to_string().into_owned()),
        _ => unreachable!("handled above"),
    }
    .unwrap_or_else(|_| fallback())
}

/// The long name of the zone an instant falls in ("Pacific Daylight
/// Time"), upstream `formatZoneName`.
pub fn format_zone_name(locale: &Locale, unix_seconds: i64, zone: &TimeZoneChoice) -> String {
    let clock = wall_clock(unix_seconds, zone);
    NoCalendarFormatter::try_new(locale.into(), SpecificLong)
        .map(|f| {
            f.format(&icu_zone_of(&clock, unix_seconds))
                .write_to_string()
                .into_owned()
        })
        .unwrap_or_default()
}

/// Unix seconds from an ISO 8601 instant ("2025-03-21T21:51:07Z"), upstream
/// `value` as a string.
pub fn parse_iso(value: &str) -> Option<i64> {
    value.parse::<jiff::Timestamp>().ok().map(|t| t.as_second())
}

/// The relative unit a difference is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RelativeUnit {
    Second,
    Minute,
    Hour,
    Day,
    Month,
    Year,
}

const MINUTE: i64 = 60;
const HOUR: i64 = 3600;
const DAY: i64 = 86_400;
const MONTH: i64 = 30 * DAY;
const YEAR: i64 = 365 * DAY;
/// Upstream `FUTURE_SKEW_TOLERANCE`: a time a little ahead reads as now.
const FUTURE_SKEW: i64 = 30;

fn relative_formatter(
    locale: &Locale,
    unit: RelativeUnit,
    narrow: bool,
    numeric: Numeric,
) -> Option<RelativeTimeFormatter> {
    let prefs = locale.into();
    let options = RelativeTimeFormatterOptions::default().with_numeric(numeric);
    let formatter = match (unit, narrow) {
        (RelativeUnit::Second, false) => RelativeTimeFormatter::try_new_long_second(prefs, options),
        (RelativeUnit::Minute, false) => RelativeTimeFormatter::try_new_long_minute(prefs, options),
        (RelativeUnit::Hour, false) => RelativeTimeFormatter::try_new_long_hour(prefs, options),
        (RelativeUnit::Day, false) => RelativeTimeFormatter::try_new_long_day(prefs, options),
        (RelativeUnit::Month, false) => RelativeTimeFormatter::try_new_long_month(prefs, options),
        (RelativeUnit::Year, false) => RelativeTimeFormatter::try_new_long_year(prefs, options),
        (RelativeUnit::Second, true) => {
            RelativeTimeFormatter::try_new_narrow_second(prefs, options)
        }
        (RelativeUnit::Minute, true) => {
            RelativeTimeFormatter::try_new_narrow_minute(prefs, options)
        }
        (RelativeUnit::Hour, true) => RelativeTimeFormatter::try_new_narrow_hour(prefs, options),
        (RelativeUnit::Day, true) => RelativeTimeFormatter::try_new_narrow_day(prefs, options),
        (RelativeUnit::Month, true) => RelativeTimeFormatter::try_new_narrow_month(prefs, options),
        (RelativeUnit::Year, true) => RelativeTimeFormatter::try_new_narrow_year(prefs, options),
    };
    formatter.ok()
}

/// A time relative to now, upstream `formatRelativeTime`: `ago_seconds` is
/// how long ago (negative for the future). Long style says "2 hours ago"
/// and "yesterday"; narrow says "2h ago".
pub fn format_relative(locale: &Locale, ago_seconds: i64, narrow: bool) -> String {
    let abs = ago_seconds.abs();
    let (value, unit, numeric) = if abs < 10 || (ago_seconds < 0 && abs <= FUTURE_SKEW) {
        (0, RelativeUnit::Second, Numeric::Auto)
    } else {
        let (count, unit) = if abs < MINUTE {
            (abs, RelativeUnit::Second)
        } else if abs < HOUR {
            (abs / MINUTE, RelativeUnit::Minute)
        } else if abs < DAY {
            (abs / HOUR, RelativeUnit::Hour)
        } else if abs < MONTH {
            (abs / DAY, RelativeUnit::Day)
        } else if abs < YEAR {
            (abs / MONTH, RelativeUnit::Month)
        } else {
            (abs / YEAR, RelativeUnit::Year)
        };
        let value = if ago_seconds < 0 { count } else { -count };
        let numeric = if !narrow && unit == RelativeUnit::Day && value == -1 {
            Numeric::Auto
        } else {
            Numeric::Always
        };
        (value, unit, numeric)
    };
    match relative_formatter(locale, unit, narrow, numeric) {
        Some(f) => f
            .format(Decimal::from(value))
            .write_to_string()
            .into_owned(),
        None => format!("{value} {unit:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn en() -> Locale {
        "en".parse().unwrap()
    }

    /// 2025-03-21T21:51:07Z, 2:51:07 PM in Los Angeles (PDT).
    const T0: i64 = 1_742_593_867;

    fn la() -> TimeZoneChoice {
        TimeZoneChoice::Iana("America/Los_Angeles".into())
    }

    #[test]
    fn absolute_formats_match_upstreams_examples() {
        let f = |format, zone: &TimeZoneChoice, show| {
            format_instant(&en(), T0, format, zone, show, CalendarSystem::Gregory)
        };
        assert_eq!(f(InstantFormat::Date, &la(), false), "Mar 21, 2025");
        assert_eq!(f(InstantFormat::DateLong, &la(), false), "March 21, 2025");
        assert_eq!(
            f(InstantFormat::DateWeekday, &la(), false),
            "Fri, Mar 21, 2025"
        );
        assert_eq!(
            f(InstantFormat::DateTime, &la(), false),
            "Mar 21, 2025, 2:51\u{202f}PM"
        );
        assert_eq!(f(InstantFormat::Time, &la(), false), "2:51\u{202f}PM");
        assert_eq!(f(InstantFormat::Time, &la(), true), "2:51\u{202f}PM PDT");
        assert_eq!(
            f(InstantFormat::SystemDateTime, &la(), false),
            "2025-03-21 14:51:07"
        );
        let utc = TimeZoneChoice::Iana("UTC".into());
        assert_eq!(f(InstantFormat::SystemTime, &utc, false), "21:51:07");
        assert_eq!(f(InstantFormat::UnixSeconds, &utc, false), "1742593867");
    }

    #[test]
    fn a_locale_keyword_never_chooses_the_calendar() {
        let text = format_instant(
            &"th-TH-u-ca-buddhist".parse().unwrap(),
            T0,
            InstantFormat::Date,
            &la(),
            false,
            CalendarSystem::Gregory,
        );
        assert!(text.contains("2025"), "{text}");
    }

    #[test]
    fn calendars_come_from_the_keyword() {
        let persian = format_instant(
            &en(),
            T0,
            InstantFormat::Date,
            &la(),
            false,
            CalendarSystem::Persian,
        );
        assert!(
            persian.contains("1404") || persian.contains("1403"),
            "{persian}"
        );
    }

    #[test]
    fn relative_times_follow_upstream() {
        let r = |ago, narrow| format_relative(&en(), ago, narrow);
        assert_eq!(r(5, false), "now");
        assert_eq!(r(-20, false), "now");
        assert_eq!(r(45, false), "45 seconds ago");
        assert_eq!(r(2 * 3600 + 5, false), "2 hours ago");
        assert_eq!(r(DAY + 10, false), "yesterday");
        assert_eq!(r(3 * DAY, false), "3 days ago");
        assert_eq!(r(-2 * DAY, false), "in 2 days");
        assert_eq!(r(2 * 3600, true), "2h ago");
    }
}
