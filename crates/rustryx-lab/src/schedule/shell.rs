//! The Schedule shell (upstream `Schedule.tsx`): it resolves the view's range and its
//! events, owns paging, and renders whatever the view draws inside the Schedule context.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use gpui::prelude::*;
use gpui::{App, ElementId, IntoElement, SharedString, StyleRefinement, Window, div};
use rustryx_primitives::provide;
use rustryx_theme::current;

use super::context::{ScheduleContext, ScheduleContextValue};
use super::current_time::{now, use_current_time};
use super::date_math::{
    event_overlaps_range, get_browser_timezone_id, plain_date_from_instant, sort_events,
};
use super::format::ScheduleFormat;
use super::plugins::default_schedule_plugins;
use super::scroll_memory::TimeGridScrollMemory;
use super::types::{
    CalendarEvent, Instant, ScheduleCategory, ScheduleEventSource, SchedulePlugin, ScheduleRange,
    ScheduleView,
};
use super::zoned::{ZonedDateTime, ZonedDateTimeRange, create_zoned_date_time};

type LoaderFn = dyn Fn(Instant, Instant, &mut App) -> super::types::ScheduleLoad;
type Loader = Rc<LoaderFn>;
type ChangeDate = Rc<dyn Fn(Instant, &mut Window, &mut App)>;

/// A read-only calendar surface that renders a view's range of events (upstream lab
/// `Schedule`).
#[derive(IntoElement)]
pub struct Schedule {
    id: ElementId,
    view: ScheduleView,
    events: ScheduleEventSource,
    categories: Vec<ScheduleCategory>,
    date: Instant,
    focus_date: Option<Instant>,
    on_change_date: Option<ChangeDate>,
    timezone_id: Option<SharedString>,
    plugins: Option<Vec<Rc<dyn SchedulePlugin>>>,
    heading_level: u8,
    style: StyleRefinement,
}

impl Schedule {
    /// Upstream `view` (from a `create_schedule_*_view` factory), `events` (static
    /// events or a loader) and `date` (epoch milliseconds inside the range to render; fully
    /// controlled, so pair it with [`Schedule::on_change_date`]).
    pub fn new(
        id: impl Into<ElementId>,
        view: ScheduleView,
        events: impl Into<ScheduleEventSource>,
        date: Instant,
    ) -> Self {
        Schedule {
            id: id.into(),
            view,
            events: events.into(),
            categories: Vec::new(),
            date,
            focus_date: None,
            on_change_date: None,
            timezone_id: None,
            plugins: None,
            heading_level: 3,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `categories`: matched to each event by `label == event.category` for its
    /// color and the category name in its accessible label.
    pub fn categories(mut self, categories: impl IntoIterator<Item = ScheduleCategory>) -> Self {
        self.categories = categories.into_iter().collect();
        self
    }

    /// Upstream `focusDate`: the instant whose day is treated as today. Without it, the
    /// time the Schedule mounted, never advanced.
    pub fn focus_date(mut self, focus_date: Instant) -> Self {
        self.focus_date = Some(focus_date);
        self
    }

    /// Upstream `onChangeDate`: called with the instant to render next when a plugin pages
    /// or Today is pressed.
    pub fn on_change_date(mut self, f: impl Fn(Instant, &mut Window, &mut App) + 'static) -> Self {
        self.on_change_date = Some(Rc::new(f));
        self
    }

    /// Upstream `timezoneID`: the IANA zone every date calculation and label uses (the
    /// system's by default).
    pub fn timezone_id(mut self, timezone_id: impl Into<SharedString>) -> Self {
        self.timezone_id = Some(timezone_id.into());
        self
    }

    /// Upstream `plugins`: header plugins applied in order; a list replaces
    /// [`default_schedule_plugins`](super::default_schedule_plugins).
    pub fn plugins(mut self, plugins: impl IntoIterator<Item = Rc<dyn SchedulePlugin>>) -> Self {
        self.plugins = Some(plugins.into_iter().collect());
        self
    }

    /// Upstream `headingLevel`: the level of the sub-headings inside (3), kept within 2 to 6.
    pub fn heading_level(mut self, level: u8) -> Self {
        self.heading_level = level.clamp(2, 6);
        self
    }
}

impl Styled for Schedule {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// One loader call's state (upstream `EventRecord`).
#[derive(Clone)]
enum EventRecord {
    Pending,
    Fulfilled(Rc<Vec<CalendarEvent>>),
    /// The loader failed (upstream throws its error to the nearest error boundary).
    Rejected,
}

struct LoaderCache {
    loader: Weak<LoaderFn>,
    records: HashMap<String, EventRecord>,
}

thread_local! {
    /// Upstream `eventLoaderCache`: per loader identity (held weakly, as a `WeakMap`),
    /// then per range.
    static EVENT_LOADER_CACHE: RefCell<Vec<LoaderCache>> = const { RefCell::new(Vec::new()) };
}

fn record_of(loader: &Loader, key: &str) -> Option<EventRecord> {
    EVENT_LOADER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.retain(|entry| entry.loader.strong_count() > 0);
        cache
            .iter()
            .find(|entry| {
                entry
                    .loader
                    .upgrade()
                    .is_some_and(|l| Rc::ptr_eq(&l, loader))
            })
            .and_then(|entry| entry.records.get(key).cloned())
    })
}

fn set_record(loader: &Loader, key: String, record: EventRecord) {
    EVENT_LOADER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let index = match cache.iter().position(|entry| {
            entry
                .loader
                .upgrade()
                .is_some_and(|l| Rc::ptr_eq(&l, loader))
        }) {
            Some(index) => index,
            None => {
                cache.push(LoaderCache {
                    loader: Rc::downgrade(loader),
                    records: HashMap::new(),
                });
                cache.len() - 1
            }
        };
        cache[index].records.insert(key, record);
    });
}

/// Upstream `readAsyncEvents`: the cached result of `loader(start, end)`, calling it the first
/// time; the windows redraw once it settles.
fn read_async_events(loader: &Loader, start: Instant, end: Instant, cx: &mut App) -> EventRecord {
    let key = format!("{start}:{end}");
    if let Some(record) = record_of(loader, &key) {
        return record;
    }
    set_record(loader, key.clone(), EventRecord::Pending);
    let load = loader(start, end, cx);
    let weak = Rc::downgrade(loader);
    cx.spawn(async move |cx| {
        let result = load.await;
        if let Some(loader) = weak.upgrade() {
            let record = match result {
                Ok(events) => EventRecord::Fulfilled(Rc::new(events)),
                Err(_) => EventRecord::Rejected,
            };
            set_record(&loader, key, record);
        }
        cx.refresh();
    })
    .detach();
    EventRecord::Pending
}

/// Upstream `getRange`.
fn get_range(view: &ScheduleView, date: &ZonedDateTime) -> ScheduleRange {
    let (start, end) = view.date_range(date);
    ScheduleRange {
        start: start.instant,
        end: end.instant,
        start_date: plain_date_from_instant(start.instant, &start.timezone_id),
        end_date: plain_date_from_instant(end.instant, &end.timezone_id),
    }
}

struct ScheduleState {
    /// Upstream `internalFocusDate`: the mount time.
    focus_date: Instant,
    /// Upstream `timeGridScrollMemory`: above the loading boundary, so the loading and the
    /// loaded view share one scroll record per range.
    scroll_memory: Rc<RefCell<TimeGridScrollMemory>>,
}

impl RenderOnce for Schedule {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id: SharedString = self.id.to_string().into();
        let theme = current(window, cx);
        let format = ScheduleFormat::new(rustryx_i18n::current(cx));
        let timezone_id: SharedString = self
            .timezone_id
            .clone()
            .unwrap_or_else(|| get_browser_timezone_id().into());
        let state = window.use_keyed_state(
            ElementId::Name(format!("{id}-schedule").into()),
            cx,
            |_, _| ScheduleState {
                focus_date: now(),
                scroll_memory: Rc::new(RefCell::new(TimeGridScrollMemory::default())),
            },
        );
        let (internal_focus_date, scroll_memory) = {
            let state = state.read(cx);
            (state.focus_date, state.scroll_memory.clone())
        };
        let focus_date = self.focus_date.unwrap_or(internal_focus_date);
        let zoned = create_zoned_date_time(self.date, timezone_id.clone());
        let focus_zoned = create_zoned_date_time(focus_date, timezone_id.clone());
        let view = self.view.clone();
        let previous = view.previous_date_range(&zoned);
        let next = view.next_date_range(&zoned);
        let update_date = self.on_change_date.clone();
        let shift_to_range = {
            let (view, zoned, update_date) = (view.clone(), zoned.clone(), update_date.clone());
            Rc::new(
                move |next_range: &ZonedDateTimeRange, window: &mut Window, cx: &mut App| {
                    let current_range = view.date_range(&zoned);
                    if let Some(update) = &update_date {
                        update(
                            zoned.instant + next_range.0.instant - current_range.0.instant,
                            window,
                            cx,
                        );
                    }
                },
            )
        };
        let on_previous_date = {
            let (shift, range) = (shift_to_range.clone(), previous.range.clone());
            Rc::new(move |window: &mut Window, cx: &mut App| shift(&range, window, cx))
        };
        let on_next_date = {
            let (shift, range) = (shift_to_range, next.range.clone());
            Rc::new(move |window: &mut Window, cx: &mut App| shift(&range, window, cx))
        };
        let on_today = {
            let update_date = update_date.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| {
                if let Some(update) = &update_date {
                    update(now(), window, cx);
                }
            })
        };
        let range = get_range(&view, &zoned);
        // Upstream suspends on a pending loader and renders the same view as the fallback,
        // loading and with no events. A rejected loader throws to an error boundary
        // upstream; here the view renders empty.
        let (events, is_loading): (Rc<Vec<CalendarEvent>>, bool) = match &self.events {
            ScheduleEventSource::Events(events) => (events.clone(), false),
            ScheduleEventSource::Loader(loader) => {
                match read_async_events(loader, range.start, range.end, cx) {
                    EventRecord::Pending => (Rc::new(Vec::new()), true),
                    EventRecord::Fulfilled(events) => (events, false),
                    EventRecord::Rejected => (Rc::new(Vec::new()), false),
                }
            }
        };
        let in_range: Vec<CalendarEvent> = events
            .iter()
            .filter(|event| event_overlaps_range(event, &range, &timezone_id))
            .cloned()
            .collect();
        let events = sort_events(&in_range, &timezone_id, &|a, b| format.compare(a, b));
        let current_time = use_current_time(&id, window, cx);
        let context = Rc::new(ScheduleContextValue {
            events,
            categories: self.categories.clone(),
            date: zoned,
            focus_date: focus_zoned,
            timezone_id,
            format,
            range,
            is_loading,
            on_previous_date,
            previous_date_label: previous.label,
            on_today,
            on_next_date,
            next_date_label: next.label,
            view: view.clone(),
            plugins: self
                .plugins
                .clone()
                .unwrap_or_else(default_schedule_plugins),
            heading_level: self.heading_level,
            id: id.clone(),
            scroll_memory,
            current_time,
        });
        let body = rustryx_primitives::with_context(&ScheduleContext(context.clone()), cx, |cx| {
            view.0.render(&context, window, cx)
        });
        let mut root = div()
            .id(ElementId::Name(id))
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0();
        theme.refine(root.style(), "schedule", &[], None);
        root.style().refine(&self.style);
        root.child(provide(ScheduleContext(context), body))
    }
}
