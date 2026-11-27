//! Date math (upstream `dateMath.ts`, and the zone conversions of Core's `plainDate.ts` it
//! uses): ranges, which days an event covers, and how events sort. Instants are
//! milliseconds; the zone rules come from astryx-i18n.

use std::cmp::Ordering;

use astryx_i18n::TimeZoneChoice;
use astryx_i18n::date::{PlainDate, PlainDateTime, PlainTime};
use astryx_i18n::datetime::{day_of_instant, instant_in_zone, wall_clock_in_zone};

use super::types::{CalendarEvent, EventTimes, Instant, ScheduleRange};

const ALL_DAY_SPAN_MS: f64 = 24. * 60. * 60. * 1000.;

fn zone(timezone_id: &str) -> TimeZoneChoice {
    TimeZoneChoice::Iana(timezone_id.to_string())
}

fn seconds(instant: Instant) -> i64 {
    if instant.is_finite() {
        (instant / 1000.).floor() as i64
    } else {
        0
    }
}

/// The system's IANA zone, or `UTC` (upstream `getBrowserTimezoneID`).
pub fn get_browser_timezone_id() -> String {
    jiff::tz::TimeZone::system()
        .iana_name()
        .unwrap_or("UTC")
        .to_string()
}

/// The day an instant falls on in a zone (upstream `plainDateFromInstant`).
pub fn plain_date_from_instant(instant: Instant, timezone_id: &str) -> PlainDate {
    day_of_instant(seconds(instant), &zone(timezone_id))
}

/// The instant of a wall-clock time on a day in a zone (upstream `plainDateToInstant`): a
/// time that happens twice is its earlier occurrence, one that never happens is read in
/// the offset before the transition.
pub fn plain_date_to_instant(
    date: PlainDate,
    timezone_id: &str,
    hour: f64,
    minute: f64,
) -> Instant {
    let at = PlainDateTime::new(
        date,
        PlainTime {
            hour: hour as u8,
            minute: minute as u8,
            second: 0,
        },
    );
    instant_in_zone(at, &zone(timezone_id)) as f64 * 1000.
}

/// Minutes since midnight of an instant's wall clock in a zone (upstream
/// `getMinutesSinceStartOfDay`).
pub fn get_minutes_since_start_of_day(instant: Instant, timezone_id: &str) -> f64 {
    let at = wall_clock_in_zone(seconds(instant), &zone(timezone_id));
    at.time.hour as f64 * 60. + at.time.minute as f64
}

/// Whether the week and day views paint an event as a span in the all-day row: a
/// date-only event, or a timed event lasting 24 hours or more, measured on its own
/// instants (upstream `isAllDaySpan`, component:Schedule FR21).
pub fn is_all_day_span(event: &CalendarEvent) -> bool {
    match event.times {
        EventTimes::Day { .. } => true,
        EventTimes::Instant { start, end } => end - start >= ALL_DAY_SPAN_MS,
    }
}

/// A range from its first day to the day after its last (upstream
/// `getScheduleRangeFromDates`).
pub fn get_schedule_range_from_dates(
    start_date: PlainDate,
    end_date: PlainDate,
    timezone_id: &str,
) -> ScheduleRange {
    ScheduleRange {
        start_date,
        end_date,
        start: plain_date_to_instant(start_date, timezone_id, 0., 0.),
        end: plain_date_to_instant(end_date, timezone_id, 0., 0.),
    }
}

/// Every day from `start` up to `end_exclusive` (upstream `enumerateDates`).
pub fn enumerate_dates(start: PlainDate, end_exclusive: PlainDate) -> Vec<PlainDate> {
    let mut dates = Vec::new();
    let mut current = start;
    while current < end_exclusive {
        dates.push(current);
        current = current.add_days(1);
    }
    dates
}

/// Whether an event overlaps the range (upstream `eventOverlapsRange`).
pub fn event_overlaps_range(
    event: &CalendarEvent,
    range: &ScheduleRange,
    timezone_id: &str,
) -> bool {
    match event.times {
        EventTimes::Day { start, end } => {
            let event_start = plain_date_to_instant(start, timezone_id, 0., 0.);
            let event_end = plain_date_to_instant(end.add_days(1), timezone_id, 0., 0.);
            event_start < range.end && event_end > range.start
        }
        EventTimes::Instant { start, end } => start < range.end && end > range.start,
    }
}

/// Whether an event happens on a day in the zone (upstream `eventOccursOnDate`).
pub fn event_occurs_on_date(event: &CalendarEvent, date: PlainDate, timezone_id: &str) -> bool {
    match event.times {
        EventTimes::Day { start, end } => !(start > date) && !(end < date),
        EventTimes::Instant { start, end } => {
            let start_date = plain_date_from_instant(start, timezone_id);
            let end_date = plain_date_from_instant(js_max(end - 1., start), timezone_id);
            !(start_date > date) && !(end_date < date)
        }
    }
}

/// JavaScript's `Math.max` of two numbers: `NaN` wins.
pub(crate) fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// JavaScript's `Math.min` of two numbers: `NaN` wins.
pub(crate) fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

fn event_sort_start(event: &CalendarEvent, timezone_id: &str) -> Instant {
    match event.times {
        EventTimes::Day { start, .. } => plain_date_to_instant(start, timezone_id, 0., 0.),
        EventTimes::Instant { start, .. } => start,
    }
}

/// Events by start, then title in the locale's collation (upstream `sortEvents`).
pub fn sort_events(
    events: &[CalendarEvent],
    timezone_id: &str,
    compare: &dyn Fn(&str, &str) -> Ordering,
) -> Vec<CalendarEvent> {
    let mut sorted = events.to_vec();
    sorted.sort_by(|a, b| {
        let (a_start, b_start) = (
            event_sort_start(a, timezone_id),
            event_sort_start(b, timezone_id),
        );
        if a_start < b_start {
            Ordering::Less
        } else if a_start > b_start {
            Ordering::Greater
        } else {
            compare(&a.title, &b.title)
        }
    });
    sorted
}
