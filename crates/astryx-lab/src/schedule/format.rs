//! Schedule's date and time labels (upstream `shared.tsx`'s formatters, and Core's
//! `formatCalendarInstantRange`): dates in the provider's calendar through astryx-i18n,
//! times in the schedule's zone, and the ranges that write shared fields once.

use std::cell::OnceCell;
use std::cmp::Ordering;
use std::rc::Rc;

use astryx_i18n::date::{
    DatePart, DateStyle, PartKind, PlainDate, Width, calendar_month_of, format_calendar_date,
    format_calendar_date_to_parts, join_parts,
};
use astryx_i18n::{CalendarSystem, I18n, InstantFormat, TimeZoneChoice};
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu_datetime::fieldsets::zone::SpecificShort;
use icu_datetime::fieldsets::{MD, T};
use icu_datetime::input::{Date, DateTime, Time, TimeZone, UtcOffset};
use icu_datetime::options::TimePrecision;
use icu_datetime::{DateTimeFormatter, NoCalendarFormatter};
use icu_locale::Locale;
use icu_time::zone::ZoneNameTimestamp;
use writeable::Writeable as _;

use super::date_math::{get_minutes_since_start_of_day, is_all_day_span, plain_date_to_instant};
use super::month_layout::get_event_date_span;
use super::types::{CalendarEvent, Instant, ScheduleCategory, ScheduleEventColor};

/// The separator a range is written with (CLDR's English interval fallback, a thin-spaced
/// en dash).
pub(crate) const RANGE_SEPARATOR: &str = "\u{2009}\u{2013}\u{2009}";

/// What Schedule writes its labels with: the provider's locale and calendar.
#[derive(Clone)]
pub struct ScheduleFormat {
    /// The provider the labels come from.
    pub i18n: I18n,
    /// Its locale tag.
    pub locale: String,
    /// Its calendar, which writes the labels (the grids stay Gregorian).
    pub calendar: CalendarSystem,
    collator: Rc<OnceCell<Option<CollatorBorrowed<'static>>>>,
}

impl ScheduleFormat {
    /// The labels of `i18n`'s locale and calendar.
    pub fn new(i18n: I18n) -> Self {
        let locale = i18n.locale().to_string();
        let calendar = i18n.calendar_system();
        ScheduleFormat {
            i18n,
            locale,
            calendar,
            collator: Rc::default(),
        }
    }

    /// A title order (upstream's `localeCompare` with no options: the locale's collation,
    /// digits compared one by one, so "10 run" sorts before "9 run"). Upstream compares
    /// in the runtime's default locale; here, in the provider's.
    pub fn compare(&self, a: &str, b: &str) -> Ordering {
        let collator = self.collator.get_or_init(|| {
            let locale = parse_locale(&self.locale);
            Collator::try_new(CollatorPreferences::from(&locale), Default::default()).ok()
        });
        match collator {
            Some(collator) => collator.compare(a, b),
            None => a.cmp(b),
        }
    }

    /// Upstream `formatWithPlainDate`: Core's calendar label, which names the month and
    /// day Calendar shows.
    pub fn with_plain_date(&self, date: PlainDate, style: DateStyle) -> String {
        format_calendar_date(self.calendar, date, style, &self.locale, false)
    }

    fn month_label_date(&self, date: PlainDate) -> PlainDate {
        calendar_month_of(self.calendar, date).label_day()
    }

    /// A Gregorian month can span two months of another calendar, so ranges compare the
    /// calendar's own months (upstream `isSameCalendarMonth`).
    pub fn is_same_calendar_month(&self, a: PlainDate, b: PlainDate) -> bool {
        calendar_month_of(self.calendar, a).start == calendar_month_of(self.calendar, b).start
    }

    /// Upstream `isSameCalendarYear`.
    pub fn is_same_calendar_year(&self, a: PlainDate, b: PlainDate) -> bool {
        self.with_plain_date(self.month_label_date(a), DateStyle::YEAR)
            == self.with_plain_date(self.month_label_date(b), DateStyle::YEAR)
    }

    /// "May 2026" (upstream `formatMonthTitle`).
    pub fn month_title(&self, date: PlainDate) -> String {
        self.with_plain_date(self.month_label_date(date), DateStyle::MONTH_YEAR)
    }

    /// A month title's parts, whose month the header writes bold (upstream
    /// `LocalizedMonthTitle`).
    pub fn month_title_parts(&self, date: PlainDate) -> Vec<DatePart> {
        format_calendar_date_to_parts(
            self.calendar,
            self.month_label_date(date),
            DateStyle::MONTH_YEAR,
            &self.locale,
            false,
        )
    }

    /// The month alone of a range's start (upstream's `{month: 'long'}`).
    pub fn month_name(&self, date: PlainDate) -> String {
        self.with_plain_date(self.month_label_date(date), DateStyle::MONTH)
    }

    /// A range's title, "May 2026" or "May - June 2026" (upstream `formatWeekTitle`).
    pub fn week_title(&self, start: PlainDate, end: PlainDate) -> String {
        if self.is_same_calendar_month(start, end) {
            return self.month_title(start);
        }
        let end_title = self.month_title(end);
        if self.is_same_calendar_year(start, end) {
            format!("{} - {end_title}", self.month_name(start))
        } else {
            format!("{} - {end_title}", self.month_title(start))
        }
    }

    /// "Wednesday, May 13, 2026" (upstream `formatFullDate`).
    pub fn full_date(&self, date: PlainDate) -> String {
        self.with_plain_date(date, DateStyle::LONG_WEEKDAY)
    }

    /// A week row's dates, "May 10 – 16, 2026" (upstream `formatWeekRange`).
    pub fn week_range(&self, start: PlainDate, end: PlainDate) -> String {
        let style = DateStyle::LONG;
        let a = format_calendar_date_to_parts(self.calendar, start, style, &self.locale, false);
        let b = format_calendar_date_to_parts(self.calendar, end, style, &self.locale, false);
        collapse_date_range(&a, &b)
    }

    /// "Wed" or "Wednesday" (upstream `formatWeekday`).
    pub fn weekday(&self, date: PlainDate, width: Width) -> String {
        self.with_plain_date(date, DateStyle::weekday(width))
    }

    /// "13" (upstream `formatDayNumber`).
    pub fn day_number(&self, date: PlainDate) -> String {
        self.with_plain_date(
            date,
            DateStyle {
                day: true,
                ..DateStyle::default()
            },
        )
    }

    /// "9 AM", an hour of the gutter (upstream `formatHour`).
    pub fn hour(&self, hour: f64) -> String {
        let locale = parse_locale(&self.locale);
        let hours = hour.rem_euclid(24.) as u8;
        let Ok(time) = Time::try_new(hours, 0, 0, 0) else {
            return String::new();
        };
        let fieldset = T::for_length(Default::default()).with_time_precision(TimePrecision::Hour);
        NoCalendarFormatter::try_new((&locale).into(), fieldset)
            .map(|f| intl_spaces(f.format(&time).write_to_string().into_owned()))
            .unwrap_or_default()
    }

    /// The zone's short name on a day ("PDT"), or the zone id (upstream
    /// `formatTimezoneAbbreviation`).
    pub fn timezone_abbreviation(&self, date: PlainDate, timezone_id: &str) -> String {
        let instant = plain_date_to_instant(date, timezone_id, 12., 0.);
        zone_short_name(instant, timezone_id, &self.locale)
            .unwrap_or_else(|| timezone_id.to_string())
    }

    /// "9:00 AM" in the zone (upstream's `{hour: 'numeric', minute: '2-digit'}`).
    pub fn time(&self, instant: Instant, timezone_id: &str) -> String {
        let seconds = if instant.is_finite() {
            (instant / 1000.).floor() as i64
        } else {
            0
        };
        let text = self.i18n.format_instant(
            seconds,
            InstantFormat::Time,
            &TimeZoneChoice::Iana(timezone_id.to_string()),
            false,
        );
        unpad_hour(&intl_spaces(text))
    }

    /// "9:00 AM - 10:00 AM" (upstream `formatEventTime` and `formatEventTimeRange`).
    pub fn event_time_range(&self, start: Instant, end: Instant, timezone_id: &str) -> String {
        format!(
            "{} - {}",
            self.time(start, timezone_id),
            self.time(end, timezone_id)
        )
    }

    /// An event's start and end with their dates, "May 11 at 9:00 AM – May 13 at 9:00 AM",
    /// for a timed span of a day or more (upstream `formatEventDateTimeRange`, AR2).
    pub fn event_date_time_range(&self, start: Instant, end: Instant, timezone_id: &str) -> String {
        let first = self.date_time(start, timezone_id);
        let last = self.date_time(end, timezone_id);
        if first == last {
            first
        } else {
            format!("{first}{RANGE_SEPARATOR}{last}")
        }
    }

    /// "May 11 at 9:00 AM" in the zone and calendar.
    fn date_time(&self, instant: Instant, timezone_id: &str) -> String {
        let seconds = if instant.is_finite() {
            (instant / 1000.).floor() as i64
        } else {
            0
        };
        let wall = astryx_i18n::datetime::wall_clock_in_zone(
            seconds,
            &TimeZoneChoice::Iana(timezone_id.to_string()),
        );
        let (Ok(date), Ok(time)) = (
            Date::try_new_iso(wall.date.year, wall.date.month, wall.date.day),
            Time::try_new(wall.time.hour, wall.time.minute, 0, 0),
        ) else {
            return String::new();
        };
        let prefs = with_calendar(&parse_locale(&self.locale), self.calendar);
        DateTimeFormatter::try_new((&prefs).into(), MD::long().with_time_hm())
            .map(|f| {
                f.format(&DateTime { date, time })
                    .write_to_string()
                    .into_owned()
            })
            .unwrap_or_default()
    }

    /// The days an event covers: one full date, or a range (upstream `formatEventDays`).
    pub fn event_days(&self, event: &CalendarEvent, timezone_id: &str) -> String {
        let (first, last) = get_event_date_span(event, timezone_id);
        if first == last {
            self.full_date(first)
        } else {
            self.week_range(first, last)
        }
    }

    /// "all day", the dates of a long span, or the times (upstream
    /// `formatTimeGridEventTime`, AR2).
    pub fn time_grid_event_time(&self, event: &CalendarEvent, timezone_id: &str) -> String {
        match event.instants() {
            None => "all day".into(),
            Some((start, end)) if is_all_day_span(event) => {
                self.event_date_time_range(start, end, timezone_id)
            }
            Some((start, end)) => self.event_time_range(start, end, timezone_id),
        }
    }

    /// "Title, Category, 9:00 AM - 10:00 AM" (upstream `formatEventAccessibilityLabel`).
    pub fn event_accessibility_label(
        &self,
        event: &CalendarEvent,
        timezone_id: &str,
        categories: &[ScheduleCategory],
    ) -> String {
        let category = get_event_category(event, categories);
        let time = match event.instants() {
            None => "all day".into(),
            Some((start, end)) => self.event_time_range(start, end, timezone_id),
        };
        format!("{}, {}, {time}", event.title, category.label)
    }

    /// The minutes of an instant's wall clock (upstream `getMinutesSinceStartOfDay`).
    pub fn minutes(&self, instant: Instant, timezone_id: &str) -> f64 {
        get_minutes_since_start_of_day(instant, timezone_id)
    }
}

/// Browsers' `Intl.DateTimeFormat.format` writes the narrow no-break space CLDR puts in
/// times ("9:00\u{202f}AM") as a plain space ("9:00 AM"); `formatRange` keeps it.
fn intl_spaces(text: String) -> String {
    if text.contains('\u{202f}') {
        text.replace('\u{202f}', " ")
    } else {
        text
    }
}

/// `Intl`'s `hour: 'numeric'` beside minutes: the hour unpadded ("7:05" where CLDR's
/// pattern writes "07:05"). The hour is the time's first number.
fn unpad_hour(text: &str) -> String {
    let Some(start) = text.find(|c: char| astryx_i18n::number::digit_value(c).is_some()) else {
        return text.to_string();
    };
    let mut digits = text[start..]
        .chars()
        .take_while(|c| astryx_i18n::number::digit_value(*c).is_some());
    match (digits.next(), digits.next(), digits.next()) {
        (Some(first), Some(_), None) if astryx_i18n::number::digit_value(first) == Some(0) => {
            format!("{}{}", &text[..start], &text[start + first.len_utf8()..])
        }
        _ => text.to_string(),
    }
}

fn parse_locale(locale: &str) -> Locale {
    locale.parse().unwrap_or(Locale::UNKNOWN)
}

/// The locale with its calendar keyword set to `calendar`.
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

/// The short name of the zone an instant falls in (Intl's `timeZoneName: 'short'`).
fn zone_short_name(instant: Instant, timezone_id: &str, locale: &str) -> Option<String> {
    let seconds = if instant.is_finite() {
        (instant / 1000.).floor() as i64
    } else {
        0
    };
    let tz = jiff::tz::TimeZone::get(timezone_id).ok()?;
    let timestamp = jiff::Timestamp::from_second(seconds).ok()?;
    let offset = tz.to_offset(timestamp).seconds();
    let zone = TimeZone::from_iana_id(tz.iana_name().unwrap_or(timezone_id))
        .with_offset(UtcOffset::try_from_seconds(offset).ok())
        .with_zone_name_timestamp(ZoneNameTimestamp::from_epoch_seconds(seconds));
    let locale = parse_locale(locale);
    NoCalendarFormatter::try_new((&locale).into(), SpecificShort)
        .ok()
        .map(|f| f.format(&zone).write_to_string().into_owned())
}

/// Two dates of one pattern written as a range: the fields above the greatest difference
/// once, the rest at each end around [`RANGE_SEPARATOR`] (Intl's `formatRange` for the
/// English patterns; ICU4X ships no interval patterns).
fn collapse_date_range(a: &[DatePart], b: &[DatePart]) -> String {
    let (first, last) = (join_parts(a), join_parts(b));
    if first == last {
        return first;
    }
    let same_shape = a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.kind == y.kind);
    if !same_shape {
        return format!("{first}{RANGE_SEPARATOR}{last}");
    }
    let differs = |kind: fn(PartKind) -> bool| {
        a.iter()
            .zip(b)
            .any(|(x, y)| kind(x.kind) && x.value != y.value)
    };
    let year_differs = differs(|k| {
        matches!(
            k,
            PartKind::Era | PartKind::Year | PartKind::RelatedYear | PartKind::YearName
        )
    });
    if year_differs {
        return format!("{first}{RANGE_SEPARATOR}{last}");
    }
    let month_differs = differs(|k| k == PartKind::Month);
    // The day varies, and the month with it when the months differ.
    let varies = |kind: PartKind| match kind {
        PartKind::Day | PartKind::Weekday => true,
        PartKind::Month => month_differs,
        _ => false,
    };
    let positions: Vec<usize> = a
        .iter()
        .enumerate()
        .filter(|(_, p)| varies(p.kind))
        .map(|(i, _)| i)
        .collect();
    let (Some(&lo), Some(&hi)) = (positions.first(), positions.last()) else {
        return format!("{first}{RANGE_SEPARATOR}{last}");
    };
    let mut out = join_parts(&a[..lo]);
    out.push_str(&join_parts(&a[lo..=hi]));
    out.push_str(RANGE_SEPARATOR);
    out.push_str(&join_parts(&b[lo..=hi]));
    out.push_str(&join_parts(&a[hi + 1..]));
    out
}

const DEFAULT_EVENT_CATEGORY_LABEL: &str = "Event";

/// An event's category (upstream `getEventCategory`): the matching entry, else its own
/// name in blue, else "Event" in blue.
pub fn get_event_category(
    event: &CalendarEvent,
    categories: &[ScheduleCategory],
) -> ScheduleCategory {
    if let Some(category) = categories
        .iter()
        .find(|c| Some(&c.label) == event.category.as_ref())
    {
        return category.clone();
    }
    ScheduleCategory::new(
        event
            .category
            .clone()
            .unwrap_or_else(|| DEFAULT_EVENT_CATEGORY_LABEL.into()),
        ScheduleEventColor::Blue,
    )
}

/// Whether an event is over (upstream `isEventInPast`).
pub fn is_event_in_past(event: &CalendarEvent, current_time: Instant, timezone_id: &str) -> bool {
    match event.times {
        super::types::EventTimes::Day { end, .. } => {
            end < super::date_math::plain_date_from_instant(current_time, timezone_id)
        }
        super::types::EventTimes::Instant { end, .. } => end <= current_time,
    }
}

/// Upstream `clamp`: `Math.min(max, Math.max(min, value))`.
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    super::date_math::js_min(max, super::date_math::js_max(min, value))
}
