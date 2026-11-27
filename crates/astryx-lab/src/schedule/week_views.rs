//! The week, day and list views (upstream `WeeklyView.tsx`, `DayView.tsx` and `ListView.tsx`):
//! their ranges, paging and bodies.

use std::rc::Rc;

use astryx_i18n::date::{PlainDate, Weekday, Width};
use astryx_theme::current;
use gpui::prelude::*;
use gpui::{AnyElement, App, AriaCurrent, ElementId, Role, SharedString, Window, div, px};

use super::context::ScheduleContextValue;
use super::date_math::{
    enumerate_dates, event_occurs_on_date, get_schedule_range_from_dates, plain_date_from_instant,
};
use super::format::is_event_in_past;
use super::shared::{heading_text, list_event_row, range_month_title, schedule_frame};
use super::time_grid_view::{TimeGridProps, time_grid_view};
use super::types::{
    CalendarEvent, ScheduleNavigationRange, ScheduleTimeGridViewOptions, ScheduleView,
    ScheduleViewImpl,
};
use super::zoned::{ZonedDateTime, ZonedDateTimeRange, schedule_range_to_zoned_date_time_range};

/// The week view's options (upstream `ScheduleWeeklyViewOptions`).
#[derive(Clone, Default)]
pub struct ScheduleWeeklyViewOptions {
    /// Upstream `weekStartsOn`: the weekday the week starts on, 0 for Sunday (0).
    pub week_starts_on: u8,
    /// Upstream `minHour`, `maxHour`, `hourHeight` and `renderPopover`.
    pub time_grid: ScheduleTimeGridViewOptions,
}

/// The day view's options (upstream `ScheduleDayViewOptions`, the time grid's).
pub type ScheduleDayViewOptions = ScheduleTimeGridViewOptions;

/// The list view's options (upstream `ScheduleListViewOptions`).
#[derive(Clone, Copy, Debug)]
pub struct ScheduleListViewOptions {
    /// Upstream `days`: how many days a page covers (7).
    pub days: i64,
}

impl Default for ScheduleListViewOptions {
    fn default() -> Self {
        ScheduleListViewOptions { days: 7 }
    }
}

fn zoned(range: super::types::ScheduleRange, date: &ZonedDateTime) -> ZonedDateTimeRange {
    schedule_range_to_zoned_date_time_range(&range, &date.timezone_id)
}

fn navigation(label: &str, range: ZonedDateTimeRange) -> ScheduleNavigationRange {
    ScheduleNavigationRange {
        label: label.into(),
        range,
    }
}

fn grid_props(
    context: &ScheduleContextValue,
    days: Vec<PlainDate>,
    label: String,
    options: &ScheduleTimeGridViewOptions,
) -> TimeGridProps {
    TimeGridProps {
        days,
        focus_date: context.focus_date.to_plain_date(),
        min_hour: options.min_hour,
        max_hour: options.max_hour,
        hour_height: options.hour_height,
        label,
        render_popover: options.render_popover.clone(),
    }
}

struct WeeklyView(ScheduleWeeklyViewOptions);

fn week_date_range(
    date: PlainDate,
    timezone_id: &str,
    week_starts_on: u8,
) -> super::types::ScheduleRange {
    let start = date.start_of_week(Weekday::from_index(week_starts_on));
    get_schedule_range_from_dates(start, start.add_days(7), timezone_id)
}

impl ScheduleViewImpl for WeeklyView {
    fn date_range(&self, date: &ZonedDateTime) -> ZonedDateTimeRange {
        zoned(
            week_date_range(
                date.to_plain_date(),
                &date.timezone_id,
                self.0.week_starts_on,
            ),
            date,
        )
    }

    fn previous_date_range(&self, date: &ZonedDateTime) -> ScheduleNavigationRange {
        navigation(
            "Previous week",
            zoned(
                week_date_range(
                    date.to_plain_date().add_days(-7),
                    &date.timezone_id,
                    self.0.week_starts_on,
                ),
                date,
            ),
        )
    }

    fn next_date_range(&self, date: &ZonedDateTime) -> ScheduleNavigationRange {
        navigation(
            "Next week",
            zoned(
                week_date_range(
                    date.to_plain_date().add_days(7),
                    &date.timezone_id,
                    self.0.week_starts_on,
                ),
                date,
            ),
        )
    }

    fn render(
        &self,
        context: &Rc<ScheduleContextValue>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let days = enumerate_dates(context.range.start_date, context.range.end_date);
        let (first, last) = (
            days.first().copied().unwrap_or(context.range.start_date),
            days.last().copied().unwrap_or(context.range.start_date),
        );
        let title_label = context.format.week_title(first, last);
        let grid = time_grid_view(
            context,
            grid_props(context, days, title_label.clone(), &self.0.time_grid),
            window,
            cx,
        );
        schedule_frame(
            context,
            range_month_title(&context.format, first, last),
            title_label,
            context.is_loading,
            vec![grid],
            window,
            cx,
        )
    }
}

/// The week view (upstream `createScheduleWeeklyView`).
pub fn create_schedule_weekly_view(options: ScheduleWeeklyViewOptions) -> ScheduleView {
    ScheduleView::new(WeeklyView(options))
}

struct DayView(ScheduleDayViewOptions);

fn day_date_range(date: PlainDate, timezone_id: &str) -> super::types::ScheduleRange {
    get_schedule_range_from_dates(date, date.add_days(1), timezone_id)
}

impl ScheduleViewImpl for DayView {
    fn date_range(&self, date: &ZonedDateTime) -> ZonedDateTimeRange {
        zoned(
            day_date_range(date.to_plain_date(), &date.timezone_id),
            date,
        )
    }

    fn previous_date_range(&self, date: &ZonedDateTime) -> ScheduleNavigationRange {
        navigation(
            "Previous day",
            zoned(
                day_date_range(date.to_plain_date().add_days(-1), &date.timezone_id),
                date,
            ),
        )
    }

    fn next_date_range(&self, date: &ZonedDateTime) -> ScheduleNavigationRange {
        navigation(
            "Next day",
            zoned(
                day_date_range(date.to_plain_date().add_days(1), &date.timezone_id),
                date,
            ),
        )
    }

    fn render(
        &self,
        context: &Rc<ScheduleContextValue>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let range_date = context.date.to_plain_date();
        let days = enumerate_dates(context.range.start_date, context.range.end_date);
        let title_label = context.format.full_date(range_date);
        let grid = time_grid_view(
            context,
            grid_props(context, days, title_label.clone(), &self.0),
            window,
            cx,
        );
        schedule_frame(
            context,
            range_month_title(&context.format, range_date, range_date),
            title_label,
            context.is_loading,
            vec![grid],
            window,
            cx,
        )
    }
}

/// The day view (upstream `createScheduleDayView`).
pub fn create_schedule_day_view(options: ScheduleDayViewOptions) -> ScheduleView {
    ScheduleView::new(DayView(options))
}

/// What a list drew, for tests.
#[cfg(test)]
pub(crate) type ListDrawn = Vec<(String, String, String, bool, Vec<String>)>;

// What a list drew, for tests: each day shown, its heading's name, its painted number and
// weekday, whether it is today, and its rows (a now marker as `"now"`).
#[cfg(test)]
thread_local! {
    pub(crate) static LISTS: std::cell::RefCell<std::collections::HashMap<String, ListDrawn>> = Default::default();
}

struct ListView(ScheduleListViewOptions);

fn list_date_range(date: PlainDate, timezone_id: &str, days: i64) -> super::types::ScheduleRange {
    get_schedule_range_from_dates(date, date.add_days(days), timezone_id)
}

impl ScheduleViewImpl for ListView {
    fn date_range(&self, date: &ZonedDateTime) -> ZonedDateTimeRange {
        zoned(
            list_date_range(date.to_plain_date(), &date.timezone_id, self.0.days),
            date,
        )
    }

    fn previous_date_range(&self, date: &ZonedDateTime) -> ScheduleNavigationRange {
        navigation(
            "Previous list range",
            zoned(
                list_date_range(
                    date.to_plain_date().add_days(-self.0.days),
                    &date.timezone_id,
                    self.0.days,
                ),
                date,
            ),
        )
    }

    fn next_date_range(&self, date: &ZonedDateTime) -> ScheduleNavigationRange {
        navigation(
            "Next list range",
            zoned(
                list_date_range(
                    date.to_plain_date().add_days(self.0.days),
                    &date.timezone_id,
                    self.0.days,
                ),
                date,
            ),
        )
    }

    fn render(
        &self,
        context: &Rc<ScheduleContextValue>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let format = &context.format;
        let tz: &str = &context.timezone_id;
        let range = context.range;
        let days = enumerate_dates(range.start_date, range.end_date);
        let current_time = context.current_time;
        let current_date = plain_date_from_instant(current_time, tz);
        let end_date = range.end_date.add_days(-1);
        let title_label = format.week_title(range.start_date, end_date);
        // A day shows when it has events, is today, or is the range's first day.
        let visible: Vec<(PlainDate, Vec<CalendarEvent>, bool)> = days
            .iter()
            .filter_map(|day| {
                let is_current = *day == current_date;
                let events: Vec<CalendarEvent> = context
                    .events
                    .iter()
                    .filter(|e| event_occurs_on_date(e, *day, tz))
                    .cloned()
                    .collect();
                (!events.is_empty() || is_current || *day == range.start_date)
                    .then_some((*day, events, is_current))
            })
            .collect();
        #[cfg(test)]
        let mut drawn = Vec::new();
        let mut list = div().flex().flex_col();
        let count = visible.len();
        for (index, (day, events, is_current)) in visible.into_iter().enumerate() {
            let full_date = format.full_date(day);
            let number = format.day_number(day);
            let weekday = format.weekday(day, Width::Short);
            let rows = list_rows(
                context,
                &events,
                is_current && !context.is_loading,
                current_time,
                window,
                cx,
            );
            #[cfg(test)]
            drawn.push((
                day.to_iso(),
                full_date.clone(),
                format!("{number}{weekday}"),
                is_current,
                rows.1.clone(),
            ));
            let mut number_pill = div()
                .flex()
                .items_center()
                .justify_end()
                .w(px(30.))
                .h(px(30.))
                .rounded_full()
                .child(div().font_weight(t.font.weight.bold).child(number));
            if is_current {
                number_pill = number_pill
                    .justify_center()
                    .text_color(t.color.on_accent)
                    .bg(t.color.accent);
            }
            let mut heading = heading_text(&t, context.heading_level)
                .id(ElementId::Name(
                    format!("{}-list-day-{}", context.id, day.to_iso()).into(),
                ))
                .w(px(88.))
                .flex_none()
                .flex()
                .items_center()
                .justify_start()
                .gap_x(t.spacing.s2)
                .mt(-t.spacing.s1)
                .ml(-t.spacing.s1)
                .role(Role::Heading)
                .aria_level(context.heading_level as usize)
                .aria_label(SharedString::from(full_date))
                .child(number_pill)
                .child(weekday);
            if is_current {
                heading = heading.aria_current(AriaCurrent::Date);
            }
            list = list.child(
                div()
                    .flex()
                    .items_start()
                    .gap_x(t.spacing.s3)
                    .py(t.spacing.s3)
                    .px(t.spacing.s3)
                    .when(index + 1 < count, |d| d.border_b_1())
                    .border_color(t.color.border)
                    .child(heading)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(t.spacing.s2)
                            .children(rows.0),
                    ),
            );
        }
        #[cfg(test)]
        LISTS.with(|l| l.borrow_mut().insert(context.id.to_string(), drawn));
        let title = range_month_title(format, range.start_date, end_date);
        schedule_frame(
            context,
            title,
            title_label,
            context.is_loading,
            vec![list.into_any_element()],
            window,
            cx,
        )
    }
}

/// A day's rows, with the now marker before the first timed event still to start, or
/// last (upstream `renderListRows`).
fn list_rows(
    context: &ScheduleContextValue,
    events: &[CalendarEvent],
    show_current_time: bool,
    current_time: f64,
    window: &mut Window,
    cx: &mut App,
) -> (Vec<AnyElement>, Vec<String>) {
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    let tz: &str = &context.timezone_id;
    let mut rows: Vec<AnyElement> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for event in events {
        rows.push(list_event_row(
            context,
            event,
            is_event_in_past(event, current_time, tz),
            window,
            cx,
        ));
        names.push(event.id.to_string());
    }
    if !show_current_time {
        return (rows, names);
    }
    let orange = t.color.border_orange;
    let marker = div()
        .relative()
        .h(px(10.))
        .my(t.spacing.s0_5)
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(4.))
                .border_t_2()
                .border_color(orange),
        )
        .child(
            div()
                .absolute()
                .left(px(-1.))
                .top_0()
                .size(px(10.))
                .rounded_full()
                .bg(orange),
        )
        .into_any_element();
    let insert = events
        .iter()
        .position(|e| e.instants().is_some_and(|(start, _)| start > current_time));
    match insert {
        None => {
            rows.push(marker);
            names.push("now".into());
        }
        Some(i) => {
            rows.insert(i, marker);
            names.insert(i, "now".into());
        }
    }
    (rows, names)
}

/// The list view (upstream `createScheduleListView`).
pub fn create_schedule_list_view(options: ScheduleListViewOptions) -> ScheduleView {
    ScheduleView::new(ListView(options))
}
