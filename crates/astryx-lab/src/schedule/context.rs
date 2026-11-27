//! The context a view renders with (upstream `context.tsx`): the resolved events, the
//! range, the zone, the labels' locale and calendar, and the paging callbacks.

use std::rc::Rc;

use astryx_primitives::use_context;
use gpui::{App, SharedString, Window};

use super::format::ScheduleFormat;
use super::scroll_memory::TimeGridScrollMemory;
use super::types::{
    CalendarEvent, Instant, ScheduleCategory, SchedulePlugin, ScheduleRange, ScheduleView,
};
use super::zoned::ZonedDateTime;

type Callback = Rc<dyn Fn(&mut Window, &mut App)>;

/// What a Schedule gives its view and plugins (upstream `ScheduleContextValue`).
pub struct ScheduleContextValue {
    /// The rendered range's events, sorted by start.
    pub events: Vec<CalendarEvent>,
    /// Upstream `categories`: what events are colored and named by.
    pub categories: Vec<ScheduleCategory>,
    /// The rendered date.
    pub date: ZonedDateTime,
    /// The day treated as today.
    pub focus_date: ZonedDateTime,
    /// Upstream `timezoneID`: the IANA zone of every date calculation and label.
    pub timezone_id: SharedString,
    /// The labels' locale and calendar (upstream `locale`, and the provider's calendar).
    pub format: ScheduleFormat,
    /// Upstream `range`: the rendered range.
    pub range: ScheduleRange,
    /// An async loader is still pending.
    pub is_loading: bool,
    /// Upstream `onPreviousDate`: pages to the previous range.
    pub on_previous_date: Callback,
    /// Upstream `previousDateLabel`: the previous control's name ("Previous week").
    pub previous_date_label: SharedString,
    /// Upstream `onToday`: pages to the current time.
    pub on_today: Callback,
    /// Upstream `onNextDate`: pages to the next range.
    pub on_next_date: Callback,
    /// Upstream `nextDateLabel`: the next control's name ("Next week").
    pub next_date_label: SharedString,
    /// Upstream `view`: the view rendering.
    pub view: ScheduleView,
    /// Upstream `plugins`: the header plugins, in order.
    pub plugins: Vec<Rc<dyn SchedulePlugin>>,
    /// The level of the sub-headings within the schedule (day labels, weekday headers).
    pub heading_level: u8,
    /// The Schedule's element id, which its parts derive theirs from.
    pub id: SharedString,
    /// The time grid's scroll record, shared by the loading and the loaded view.
    pub(crate) scroll_memory: Rc<std::cell::RefCell<TimeGridScrollMemory>>,
    /// The clock the current-time affordances read (upstream's views read `useCurrentTime`).
    pub(crate) current_time: Instant,
}

/// The context's mark around a Schedule's content.
#[derive(Clone)]
pub struct ScheduleContext(pub Rc<ScheduleContextValue>);

/// The enclosing Schedule's context (upstream `useScheduleContext`); upstream throws
/// outside a Schedule, here `None`.
pub fn use_schedule_context(cx: &App) -> Option<Rc<ScheduleContextValue>> {
    use_context::<ScheduleContext>(cx).map(|c| c.0)
}
