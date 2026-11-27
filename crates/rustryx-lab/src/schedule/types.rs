//! Schedule's public types (upstream `types.ts` and `CalendarEvent.ts`): events, categories
//! and their colors, the rendered range, views, header plugins and the event source.

use std::rc::Rc;

use gpui::{AnyElement, App, SharedString, Task, Window};
use rustryx_i18n::date::{InvalidDate, PlainDate};

use super::context::ScheduleContextValue;
use super::zoned::{ZonedDateTime, ZonedDateTimeRange};

/// Unix epoch milliseconds (upstream `Instant`). A JavaScript number, so an unparsable
/// ISO string is `NaN` as `Date.parse` makes it.
pub type Instant = f64;

/// The date a Schedule renders around (upstream `ScheduleDate`).
pub type ScheduleDate = Instant;

/// An event's color (upstream `ScheduleEventColor`, Token's colors but `default`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ScheduleEventColor {
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Cyan,
    /// The color of an event whose category matches no entry.
    #[default]
    Blue,
    Purple,
    Pink,
    Gray,
}

/// A category events are colored and named by (upstream `ScheduleCategory`), matched to
/// an event by `label == event.category`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleCategory {
    /// The name, announced with the event.
    pub label: SharedString,
    /// The color its events are painted in.
    pub color: ScheduleEventColor,
}

impl ScheduleCategory {
    /// A category named `label` in `color`.
    pub fn new(label: impl Into<SharedString>, color: ScheduleEventColor) -> Self {
        ScheduleCategory {
            label: label.into(),
            color,
        }
    }
}

/// When an event happens: whole days, or instants (upstream's `CalendarDayEvent` and
/// `CalendarInstantEvent`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventTimes {
    /// A date-only event from its first to its last day, both included.
    Day { start: PlainDate, end: PlainDate },
    /// A timed event from its start to its end instant.
    Instant { start: Instant, end: Instant },
}

/// One event (upstream `CalendarEvent`: `CalendarDayEvent` or `CalendarInstantEvent`, with
/// `CalendarEventBase`'s `id`, `title` and `category`).
#[derive(Clone, Debug, PartialEq)]
pub struct CalendarEvent {
    /// A stable identity.
    pub id: SharedString,
    /// What the event is called.
    pub title: SharedString,
    /// The label of the category it belongs to.
    pub category: Option<SharedString>,
    /// When it happens.
    pub times: EventTimes,
}

impl CalendarEvent {
    /// A date-only event over `start..=end` (upstream `CalendarDayEvent`).
    pub fn day(
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        start: PlainDate,
        end: PlainDate,
    ) -> Self {
        CalendarEvent {
            id: id.into(),
            title: title.into(),
            category: None,
            times: EventTimes::Day { start, end },
        }
    }

    /// A timed event from `start` to `end` (upstream `CalendarInstantEvent`).
    pub fn instant(
        id: impl Into<SharedString>,
        title: impl Into<SharedString>,
        start: Instant,
        end: Instant,
    ) -> Self {
        CalendarEvent {
            id: id.into(),
            title: title.into(),
            category: None,
            times: EventTimes::Instant { start, end },
        }
    }

    /// Upstream `category`.
    pub fn category(mut self, category: impl Into<SharedString>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Whether it is a date-only event (upstream `isDayEvent`).
    pub fn is_day_event(&self) -> bool {
        matches!(self.times, EventTimes::Day { .. })
    }

    /// A timed event's start and end.
    pub(crate) fn instants(&self) -> Option<(Instant, Instant)> {
        match self.times {
            EventTimes::Instant { start, end } => Some((start, end)),
            EventTimes::Day { .. } => None,
        }
    }
}

fn is_date_only(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// An event from ISO strings (upstream `createEventFromISO`): two `YYYY-MM-DD` dates make a
/// date-only event, anything else two instants read as `Date.parse` reads them (`NaN` when
/// unparsable). A date-only string that names no real date is an error, as upstream's
/// `plainDateCreate` throws.
pub fn create_event_from_iso(
    id: impl Into<SharedString>,
    title: impl Into<SharedString>,
    category: Option<SharedString>,
    start: &str,
    end: &str,
) -> Result<CalendarEvent, InvalidDate> {
    let times = if is_date_only(start) && is_date_only(end) {
        EventTimes::Day {
            start: plain_date_from_iso_strict(start)?,
            end: plain_date_from_iso_strict(end)?,
        }
    } else {
        EventTimes::Instant {
            start: date_parse(start),
            end: date_parse(end),
        }
    };
    Ok(CalendarEvent {
        id: id.into(),
        title: title.into(),
        category,
        times,
    })
}

fn plain_date_from_iso_strict(text: &str) -> Result<PlainDate, InvalidDate> {
    PlainDate::from_iso(text).ok_or(InvalidDate)
}

/// `Date.parse` for ISO 8601 text: a date alone is UTC midnight, a date and time with no
/// offset is local time, one with `Z` or an offset is that instant; anything else is
/// `NaN`.
pub(crate) fn date_parse(text: &str) -> Instant {
    let text = text.trim();
    if is_date_only(text) {
        return PlainDate::from_iso(text)
            .map_or(f64::NAN, |d| d.to_epoch_days() as f64 * 86_400_000.);
    }
    if let Ok(timestamp) = text.parse::<jiff::Timestamp>() {
        return timestamp.as_millisecond() as f64;
    }
    if let Ok(civil) = text.parse::<jiff::civil::DateTime>()
        && let Ok(zoned) = civil.to_zoned_datetime(jiff::tz::TimeZone::system())
    {
        return zoned.timestamp().as_millisecond() as f64;
    }
    f64::NAN
}

/// The rendered range (upstream `ScheduleRange`): its first day and the day after its last,
/// and their midnights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScheduleRange {
    /// Upstream `startDate`: the first day rendered.
    pub start_date: PlainDate,
    /// The day after the last day rendered.
    pub end_date: PlainDate,
    /// Upstream `start`: the first day's midnight.
    pub start: Instant,
    /// Upstream `end`: the midnight after the last day.
    pub end: Instant,
}

/// Where the previous or next page goes, and its control's name (upstream
/// `ScheduleNavigationRange`).
#[derive(Clone, Debug, PartialEq)]
pub struct ScheduleNavigationRange {
    /// Upstream `label`: the control's name ("Next month").
    pub label: SharedString,
    /// Upstream `range`: the range it pages to.
    pub range: ZonedDateTimeRange,
}

/// Content of a time-grid or month view's event popover (upstream `renderPopover`):
/// `None` for an event that has nothing to open. Called while the view renders, for every
/// painted event, so keep it pure and cheap.
pub type RenderPopover = Rc<dyn Fn(&CalendarEvent, &mut Window, &mut App) -> Option<AnyElement>>;

/// A view: what range a date covers, where paging goes, and what it draws (upstream
/// `ScheduleView`: `getDateRange`, `getPreviousDateRange`, `getNextDateRange`, and its
/// `component` with its `options`). Implement it for a view of your own.
pub trait ScheduleViewImpl: 'static {
    /// Upstream `getDateRange`: the range around `date`.
    fn date_range(&self, date: &ZonedDateTime) -> ZonedDateTimeRange;
    /// Upstream `getPreviousDateRange`.
    fn previous_date_range(&self, date: &ZonedDateTime) -> ScheduleNavigationRange;
    /// Upstream `getNextDateRange`.
    fn next_date_range(&self, date: &ZonedDateTime) -> ScheduleNavigationRange;
    /// Upstream `component`: the view for the Schedule's context.
    fn render(
        &self,
        context: &Rc<ScheduleContextValue>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement;
}

/// A view object (upstream `ScheduleView`), from a `create_schedule_*_view` factory or
/// [`ScheduleView::new`]. Cheap to clone; two clones are the same view.
#[derive(Clone)]
pub struct ScheduleView(pub(crate) Rc<dyn ScheduleViewImpl>);

impl ScheduleView {
    /// A view from its implementation.
    pub fn new(view: impl ScheduleViewImpl) -> Self {
        ScheduleView(Rc::new(view))
    }

    /// Upstream `getDateRange`.
    pub fn date_range(&self, date: &ZonedDateTime) -> ZonedDateTimeRange {
        self.0.date_range(date)
    }

    /// Upstream `getPreviousDateRange`.
    pub fn previous_date_range(&self, date: &ZonedDateTime) -> ScheduleNavigationRange {
        self.0.previous_date_range(date)
    }

    /// Upstream `getNextDateRange`.
    pub fn next_date_range(&self, date: &ZonedDateTime) -> ScheduleNavigationRange {
        self.0.next_date_range(date)
    }

    /// Whether two handles are the same view (upstream's `===`).
    pub fn is_same(&self, other: &ScheduleView) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// The header's three slots (upstream `ScheduleHeaderContent`).
#[derive(Default)]
pub struct ScheduleHeaderContent {
    /// Upstream `startContent`: the leading slot (the pagination controls by default).
    pub start_content: Vec<AnyElement>,
    /// Upstream `centerContent`: the range title and its loading spinner.
    pub center_content: Vec<AnyElement>,
    /// Upstream `endContent`: the trailing slot (empty by default).
    pub end_content: Vec<AnyElement>,
}

/// Where a plugin puts its control (upstream `SchedulePluginPosition`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SchedulePluginPosition {
    #[default]
    Start,
    End,
}

/// A header plugin (upstream `SchedulePlugin`): it may wrap or replace the header's slots.
pub trait SchedulePlugin: 'static {
    /// Upstream `renderHeader`: the slots after this plugin; the slots as given by default.
    fn render_header(&self, content: ScheduleHeaderContent) -> ScheduleHeaderContent {
        content
    }
}

/// A loader's answer: the events of the range, or why there are none.
pub type ScheduleLoad = Task<Result<Vec<CalendarEvent>, SharedString>>;

/// An async loader (upstream's `(start, end) => Promise<CalendarEvent[]>`): called with the
/// rendered range's start and end.
pub type ScheduleLoader = Rc<dyn Fn(Instant, Instant, &mut App) -> ScheduleLoad>;

/// Where events come from (upstream `ScheduleEventSource`).
#[derive(Clone)]
pub enum ScheduleEventSource {
    /// Static events, filtered to the rendered range.
    Events(Rc<Vec<CalendarEvent>>),
    /// A loader called with the rendered range's start and end; results are cached per
    /// loader identity and range, so keep the loader's `Rc` stable.
    Loader(ScheduleLoader),
}

impl From<Vec<CalendarEvent>> for ScheduleEventSource {
    fn from(events: Vec<CalendarEvent>) -> Self {
        ScheduleEventSource::Events(Rc::new(events))
    }
}

impl From<Rc<Vec<CalendarEvent>>> for ScheduleEventSource {
    fn from(events: Rc<Vec<CalendarEvent>>) -> Self {
        ScheduleEventSource::Events(events)
    }
}

/// Options the week and day views share (upstream `ScheduleTimeGridViewOptions`).
#[derive(Clone)]
pub struct ScheduleTimeGridViewOptions {
    /// Upstream `minHour`: the first hour drawn, 0 to 23 (0).
    pub min_hour: f64,
    /// Upstream `maxHour`: the hour the grid ends at, 1 to 24, kept above `min_hour` (24).
    pub max_hour: f64,
    /// Upstream `hourHeight`: one hour's height, px (100).
    pub hour_height: f64,
    /// Upstream `renderPopover`: absent, the whole grid is read-only.
    pub render_popover: Option<RenderPopover>,
}

impl Default for ScheduleTimeGridViewOptions {
    fn default() -> Self {
        ScheduleTimeGridViewOptions {
            min_hour: 0.,
            max_hour: 24.,
            hour_height: 100.,
            render_popover: None,
        }
    }
}
