//! The time grid the week and day views share (upstream `TimeGridView.tsx`): one scroll
//! viewport whose sticky day header, all-day row and hour gutter keep the day columns'
//! tracks; timed blocks side by side within their overlap clusters; spans of a day or
//! more in the all-day row; the current-time line; and, with `render_popover`, event
//! buttons that open the view's popover.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, AriaCurrent, ElementId, IntoElement, Pixels, Role, SharedString, Window,
    canvas, div, point, px, relative,
};
use rustryx_components::focus::focus_ring;
use rustryx_components::overlay::{Popover, disclosure};
use rustryx_i18n::date::{PlainDate, Width};
use rustryx_primitives::scroll_area::{
    ScrollAxis, ScrollKeyboardAccess, ScrollRole, UseScrollableAreaOptions, use_scrollable_area,
};
use rustryx_primitives::{LayerAlign, Placement, control_focus, is_focus_visible};
use rustryx_theme::{ThemeStyled, current};

use super::context::ScheduleContextValue;
use super::date_math::{
    event_occurs_on_date, is_all_day_span, plain_date_from_instant, plain_date_to_instant,
};
use super::format::{clamp, get_event_category, is_event_in_past};
use super::scroll_memory::{TimeGridRangeKey, get_initial_time_grid_offset, is_same_range_key};
use super::shared::{
    event_pill, heading_text, popover_body, sr_only, supporting_leading, surface_colors,
    title_first_pill,
};
use super::time_grid_layout::{TimedEventInterval, TimedEventPlacement, layout_timed_events};
use super::types::{CalendarEvent, Instant, RenderPopover};

/// What a time grid takes (upstream `TimeGridView`'s props).
pub(crate) struct TimeGridProps {
    pub days: Vec<PlainDate>,
    pub focus_date: PlainDate,
    pub min_hour: f64,
    pub max_hour: f64,
    pub hour_height: f64,
    /// The rendered range's accessible name; it names the scroll viewport.
    pub label: String,
    /// Absent, the grid is read-only.
    pub render_popover: Option<RenderPopover>,
}

/// One painted block, by event id and day, so a multi-day event's blocks open the
/// popover separately (upstream `blockKeyOf`).
fn block_key_of(event_id: &str, day_iso: &str) -> String {
    format!("{day_iso}/{event_id}")
}

/// An all-day span on its level (upstream `AllDayEventSegment`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AllDayEventSegment {
    pub event: CalendarEvent,
    pub column_start: usize,
    pub column_end: usize,
    pub level: usize,
}

/// The all-day row's spans (upstream `getAllDayEventSegments`): the days each event
/// touches in the zone, a timed span ending at midnight not touching the next day, each
/// on the lowest free level.
pub(crate) fn get_all_day_event_segments(
    events: &[CalendarEvent],
    days: &[PlainDate],
    timezone_id: &str,
    compare: &dyn Fn(&str, &str) -> std::cmp::Ordering,
) -> Vec<AllDayEventSegment> {
    let mut spans: Vec<(CalendarEvent, usize, usize)> = events
        .iter()
        .filter_map(|event| {
            let start_index = days
                .iter()
                .position(|day| event_occurs_on_date(event, *day, timezone_id))?;
            let mut end_index = start_index;
            while end_index + 1 < days.len()
                && event_occurs_on_date(event, days[end_index + 1], timezone_id)
            {
                end_index += 1;
            }
            Some((event.clone(), start_index, end_index))
        })
        .collect();
    spans.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| (b.2 - b.1).cmp(&(a.2 - a.1)))
            .then_with(|| compare(&a.0.title, &b.0.title))
    });
    let mut levels: Vec<usize> = Vec::new();
    spans
        .into_iter()
        .map(|(event, column_start, column_end)| {
            let level = levels
                .iter()
                .position(|end| column_start > *end)
                .unwrap_or(levels.len());
            if level == levels.len() {
                levels.push(column_end);
            } else {
                levels[level] = column_end;
            }
            AllDayEventSegment {
                event,
                column_start,
                column_end,
                level,
            }
        })
        .collect()
}

/// One day column's timed blocks (upstream `getTimedEventLayouts`).
pub(crate) fn get_timed_event_layouts(
    events: &[CalendarEvent],
    day: PlainDate,
    timezone_id: &str,
    min_hour: f64,
    max_hour: f64,
) -> Vec<TimedEventPlacement<CalendarEvent>> {
    let min_minute = min_hour * 60.;
    let max_minute = max_hour * 60.;
    let intervals: Vec<TimedEventInterval<CalendarEvent>> = events
        .iter()
        .filter_map(|event| {
            let (start, end) = event.instants()?;
            let start_date = plain_date_from_instant(start, timezone_id);
            let raw_start = if start_date < day {
                0.
            } else {
                super::date_math::get_minutes_since_start_of_day(start, timezone_id)
            };
            // An event that runs to midnight or past it fills the rest of the day; ending
            // exactly at midnight is the bottom of the column, not minute 0.
            let raw_end = if plain_date_from_instant(end, timezone_id) > day {
                24. * 60.
            } else {
                super::date_math::get_minutes_since_start_of_day(end, timezone_id)
            };
            if raw_end <= min_minute || raw_start >= max_minute {
                return None;
            }
            let visible_start = raw_start.max(min_minute);
            // A block is at least fifteen minutes tall so a short event stays legible.
            let visible_end = max_minute.min((visible_start + 15.).max(raw_end));
            (visible_end > visible_start).then(|| TimedEventInterval {
                event: event.clone(),
                id: event.id.clone(),
                visible_start,
                visible_end,
            })
        })
        .collect();
    layout_timed_events(&intervals, min_minute, max_minute)
}

/// The current-time line's offset in a day column, percent, or none off the day or the
/// hour window (upstream `getCurrentTimeTop`).
pub(crate) fn get_current_time_top(
    current_time: Instant,
    day: PlainDate,
    timezone_id: &str,
    min_hour: f64,
    max_hour: f64,
) -> Option<f64> {
    if day != plain_date_from_instant(current_time, timezone_id) {
        return None;
    }
    let current_minute =
        super::date_math::get_minutes_since_start_of_day(current_time, timezone_id);
    let (min_minute, max_minute) = (min_hour * 60., max_hour * 60.);
    if current_minute < min_minute || current_minute > max_minute {
        return None;
    }
    Some(((current_minute - min_minute) / (max_minute - min_minute)) * 100.)
}

/// Whether a timed event overlaps an hour of a day (upstream `eventOverlapsHour`).
pub(crate) fn event_overlaps_hour(
    event: &CalendarEvent,
    day: PlainDate,
    hour: f64,
    timezone_id: &str,
) -> bool {
    let Some((start, end)) = event.instants() else {
        return false;
    };
    if !event_occurs_on_date(event, day, timezone_id) {
        return false;
    }
    let hour_start = plain_date_to_instant(day, timezone_id, hour, 0.);
    let hour_end = if hour == 23. {
        plain_date_to_instant(day.add_days(1), timezone_id, 0., 0.)
    } else {
        plain_date_to_instant(day, timezone_id, hour + 1., 0.)
    };
    start < hour_end && end > hour_start
}

/// What a time grid drew, for tests.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
#[allow(clippy::type_complexity)]
pub(crate) struct GridDrawn {
    pub viewport_label: String,
    pub hidden_grid: Vec<Vec<String>>,
    pub blocks: Vec<(
        String,
        String,
        f64,
        f64,
        usize,
        usize,
        usize,
        Option<String>,
        bool,
    )>,
    pub spans: Vec<(String, usize, usize, usize, Option<String>, bool)>,
    pub header: Vec<(String, String, bool)>,
    pub gutter: Vec<String>,
    pub now_line: Option<(usize, f64)>,
    pub timezone_label: String,
    pub open: Option<String>,
    /// The viewport's offset from its top as this frame began.
    pub scroll_top: f64,
    pub scroll_handle: Option<gpui::ScrollHandle>,
}

#[cfg(test)]
thread_local! {
    pub(crate) static GRIDS: std::cell::RefCell<std::collections::HashMap<String, GridDrawn>> = Default::default();
}

/// The grid's open popover and what it last positioned for.
#[derive(Default)]
struct GridState {
    open_block: Option<(SharedString, String)>,
    positioned: Option<(TimeGridRangeKey, bool, bool)>,
    column_width: Option<Pixels>,
}

/// An event block as the popover's trigger (upstream's `<button>` with the popover's
/// trigger props): it reads the popover's disclosure as it renders.
#[derive(IntoElement)]
struct EventButton {
    el: gpui::Stateful<gpui::Div>,
}

impl RenderOnce for EventButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        match disclosure(cx) {
            Some(d) => d.trigger(self.el),
            None => self.el.into_any_element(),
        }
    }
}

/// The time grid (upstream `TimeGridView`).
pub(crate) fn time_grid_view(
    context: &Rc<ScheduleContextValue>,
    props: TimeGridProps,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    let rem = window.rem_size();
    let format = &context.format;
    let tz: &str = &context.timezone_id;
    let compare = |a: &str, b: &str| format.compare(a, b);
    let has_popover = props.render_popover.is_some();
    let min_hour = props.min_hour.floor().clamp(0., 23.);
    let max_hour = (props.max_hour.floor().min(24.)).max(min_hour + 1.);
    let hours: Vec<f64> = (min_hour as i64..max_hour as i64)
        .map(|h| h as f64)
        .collect();
    let days = &props.days;
    // Date-only events and timed events of a day or more are spans in the all-day row;
    // every other timed event is a block in its day columns (component:Schedule FR21).
    let all_day_events: Vec<CalendarEvent> = context
        .events
        .iter()
        .filter(|e| is_all_day_span(e))
        .cloned()
        .collect();
    let segments = get_all_day_event_segments(&all_day_events, days, tz, &compare);
    let level_count = segments.iter().map(|s| s.level + 1).max().unwrap_or(0);
    let instant_events: Vec<CalendarEvent> = context
        .events
        .iter()
        .filter(|e| !is_all_day_span(e))
        .cloned()
        .collect();
    let current_time = context.current_time;
    let current_date = plain_date_from_instant(current_time, tz);
    let timezone_label =
        format.timezone_abbreviation(days.first().copied().unwrap_or(props.focus_date), tz);
    let id = context.id.clone();
    let hour_height = props.hour_height;

    let viewport_label = format!("{} time grid", props.label);
    let area = use_scrollable_area(
        &ElementId::Name(format!("{id}-time-grid").into()),
        UseScrollableAreaOptions::new(
            ScrollAxis::Both,
            ScrollKeyboardAccess::Viewport {
                label: viewport_label.clone().into(),
                role: ScrollRole::Region,
            },
        ),
        window,
        cx,
    );
    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-time-grid-state").into()),
        cx,
        |_, _| GridState::default(),
    );

    // Initial position (component:Schedule FR4 to FR6): once per rendered range, the
    // viewport opens one hour above now when the range includes today, or at the top of
    // the hour window. The record is shared with the loading view, so a late loader
    // restores the person's own offset instead of positioning again; clock ticks, data,
    // theme and resizes never position again because the key does not change.
    let range_key = TimeGridRangeKey {
        start: context.range.start,
        end: context.range.end,
        timezone_id: context.timezone_id.clone(),
        min_hour,
        max_hour,
        hour_height,
    };
    let handle = area.scroll_handle().clone();
    let is_block_scrollable = area.state.block.is_scrollable;
    let has_clock = current_time != 0.;
    let deps = (range_key.clone(), has_clock, is_block_scrollable);
    let changed = state.read(cx).positioned.as_ref() != Some(&deps);
    let scroll_top = -f64::from(handle.offset().y);
    {
        let mut memory = context.scroll_memory.borrow_mut();
        let same = memory
            .key
            .as_ref()
            .is_some_and(|k| is_same_range_key(k, &range_key));
        if changed {
            state.update(cx, |s, _| s.positioned = Some(deps));
            if is_block_scrollable && has_clock {
                if same {
                    if (scroll_top - memory.offset).abs() > 1. {
                        handle.set_offset(point(handle.offset().x, px(-(memory.offset as f32))));
                    }
                } else {
                    let client = f64::from(handle.bounds().size.height);
                    let scroll_height = client + f64::from(handle.max_offset().y);
                    let offset = get_initial_time_grid_offset(
                        current_time,
                        days,
                        tz,
                        min_hour,
                        hour_height,
                        scroll_height,
                        client,
                    );
                    handle.set_offset(point(handle.offset().x, px(-(offset as f32))));
                    memory.key = Some(range_key.clone());
                    memory.offset = offset;
                }
                window.refresh();
            }
        } else if same {
            // The person's own scroll, remembered for this range.
            memory.offset = scroll_top;
        }
    }

    // Event popover (component:Schedule FR11 to FR12, DEC-1, DEC-5): the grid remembers
    // which painted block opened it. Each block's popover is the system's standard
    // Popover, named by the event's title; only the open block's shows, so the grid has
    // one popover open at a time.
    let open_block = state.read(cx).open_block.clone();
    let open_key = open_block.as_ref().map(|(e, d)| block_key_of(e, d));
    let mut painted: HashSet<String> = HashSet::new();
    let render_popover = props.render_popover.clone();
    let column_width = state.read(cx).column_width;
    #[cfg(test)]
    let mut drawn = GridDrawn {
        viewport_label: viewport_label.clone(),
        timezone_label: timezone_label.clone(),
        scroll_top,
        scroll_handle: Some(handle.clone()),
        ..Default::default()
    };

    // A block that opens the popover: the popover wraps it, open while it is the open
    // block; asking to open or close sets the open block.
    let wrap_popover = |el: gpui::Stateful<gpui::Div>,
                        event: &CalendarEvent,
                        day_iso: &str,
                        content: AnyElement|
     -> AnyElement {
        let key = block_key_of(&event.id, day_iso);
        let is_open = open_key.as_deref() == Some(key.as_str());
        let (state, event_id, day_iso) = (state.clone(), event.id.clone(), day_iso.to_string());
        let body_id = format!("{id}-popover-{key}");
        let body_label = format!("{} details", event.title);
        Popover::new(
            ElementId::Name(format!("{id}-block-popover-{key}").into()),
            event.title.clone(),
        )
        .placement(Placement::Below)
        .alignment(LayerAlign::Start)
        .open(is_open)
        .on_open_change(move |open, _, cx| {
            let (event_id, day_iso) = (event_id.clone(), day_iso.clone());
            state.update(cx, |s, cx| {
                s.open_block = open.then_some((event_id, day_iso));
                cx.notify();
            })
        })
        .content_fn(move |window, cx| popover_body(body_id, body_label, content, window, cx))
        .child(EventButton { el })
        .into_any_element()
    };

    // The header: weekday and day number of each column.
    let mut header = div()
        .flex()
        .flex_none()
        .h(px(56.))
        .sticky_top(px(0.))
        .child(
            div()
                .w(px(60.))
                .flex_none()
                .sticky_left(px(0.))
                .bg(t.color.background_card)
                .border_r_1()
                .border_b_1()
                .border_color(t.color.border),
        );
    for (index, day) in days.iter().enumerate() {
        let is_focus = *day == props.focus_date;
        let weekday = format.weekday(*day, Width::Short);
        let number = format.day_number(*day);
        #[cfg(test)]
        drawn
            .header
            .push((weekday.clone(), number.clone(), is_focus));
        let mut pill = div()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .child(number);
        if is_focus {
            pill = pill
                .min_w(px(30.))
                .h(px(30.))
                .line_height(px(30.))
                .text_color(t.color.on_accent)
                .bg(t.color.accent);
        }
        header = header.child(
            div()
                .flex_1()
                .flex_basis(px(0.))
                .min_w(px(140.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(t.spacing.s0_5)
                .bg(t.color.background_card)
                .when(index + 1 < days.len(), |d| d.border_r_1())
                .border_b_1()
                .border_color(t.color.border)
                .child(
                    // The painted header is decoration: the hidden grid, or the day groups,
                    // carry the dates.
                    heading_text(&t, context.heading_level)
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(t.spacing.s1)
                        .child(weekday)
                        .child(pill),
                ),
        );
    }

    // The all-day row: spans across the day columns' tracks.
    let row_height = if level_count > 0 {
        3. + level_count as f64 * 27.
    } else {
        26.
    };
    let mut all_day = div()
        .id(ElementId::Name(format!("{id}-all-day").into()))
        .relative()
        .flex_1()
        .flex()
        .min_h(px(row_height as f32))
        .bg(t.color.background_card)
        .border_b_1()
        .border_color(t.color.border);
    if has_popover {
        all_day = all_day.role(Role::Group).aria_label("All-day events");
    }
    for index in 0..days.len() {
        all_day = all_day.child(
            div()
                .flex_1()
                .flex_basis(px(0.))
                .min_w(px(140.))
                .when(index + 1 < days.len(), |d| d.border_r_1())
                .border_color(t.color.border),
        );
    }
    let n = days.len().max(1) as f32;
    for segment in &segments {
        let day = days
            .get(segment.column_start)
            .copied()
            .unwrap_or(props.focus_date);
        let day_iso = day.to_iso();
        let key = format!("{}:{}", segment.event.id, segment.column_start);
        let is_past = is_event_in_past(&segment.event, current_time, tz);
        // A timed span of a day or more leads with its title and shows its start and end
        // times when both fit (component:Schedule FR21).
        let pill = match segment.event.instants() {
            None => event_pill(context, &segment.event, None, is_past, window, cx),
            Some((start, end)) => title_first_pill(
                context,
                &segment.event,
                Some(format.event_time_range(start, end, tz)),
                is_past,
                window,
                cx,
            ),
        };
        let label = format_event_button_label(context, &segment.event, day);
        let content = render_popover
            .as_ref()
            .and_then(|f| f(&segment.event, window, cx));
        let block_key = block_key_of(&segment.event.id, &day_iso);
        #[cfg(test)]
        let is_button = content.is_some();
        #[cfg(test)]
        drawn.spans.push((
            segment.event.id.to_string(),
            segment.column_start,
            segment.column_end,
            segment.level,
            has_popover.then(|| label.clone()),
            is_button,
        ));
        let span = (segment.column_end - segment.column_start + 1) as f32;
        let border = px(1.);
        let place = div()
            .absolute()
            .top(px(2. + segment.level as f32 * 27.))
            .left(relative(segment.column_start as f32 / n))
            .w(relative(span / n));
        let inner = div()
            .ml(t.spacing.s0_5)
            .mr(t.spacing.s0_5.to_pixels(rem) + border)
            .min_w_0();
        let child: AnyElement = match content {
            None if has_popover => place
                .child(
                    inner
                        .id(ElementId::Name(format!("{id}-span-{key}").into()))
                        .role(Role::Label)
                        .aria_label(SharedString::from(label))
                        .child(pill),
                )
                .into_any_element(),
            None => place.child(inner.child(pill)).into_any_element(),
            Some(content) => {
                painted.insert(block_key.clone());
                let button_id = ElementId::Name(format!("{id}-span-button-{block_key}").into());
                let focus = control_focus(&button_id, window, cx);
                let selector = format!("{id}-span-{block_key}");
                let mut el = inner
                    .id(button_id)
                    .debug_selector(move || selector)
                    .track_focus(&focus)
                    .relative()
                    .cursor_pointer()
                    .role(Role::Button)
                    .aria_label(SharedString::from(label))
                    .child(pill);
                if is_focus_visible(&focus, window) {
                    el = el.child(focus_ring(&t, t.radius.inner.to_pixels(rem), rem));
                }
                place
                    .child(wrap_popover(el, &segment.event, &day_iso, content))
                    .into_any_element()
            }
        };
        all_day = all_day.child(child);
    }
    let all_day_row = div()
        .flex()
        .flex_none()
        .sticky_top(px(56.))
        .child(
            div()
                .w(px(60.))
                .flex_none()
                .sticky_left(px(0.))
                .py(t.spacing.s1)
                .px(t.spacing.s2)
                .flex()
                .items_center()
                .justify_end()
                .bg(t.color.background_card)
                .border_r_1()
                .border_b_1()
                .border_color(t.color.border)
                .font_stack(&t.font.family.body)
                .text_size(t.text.supporting.size)
                .line_height(supporting_leading(&t, rem))
                .font_weight(t.font.weight.bold)
                .text_color(t.color.text_secondary)
                .child(timezone_label.clone()),
        )
        .child(all_day);

    // The hour gutter and the day columns.
    let grid_height = px((hours.len() as f64 * hour_height) as f32);
    let leading = supporting_leading(&t, rem);
    let mut gutter = div()
        .w(px(60.))
        .flex_none()
        .h(grid_height)
        .relative()
        .sticky_left(px(0.))
        .bg(t.color.background_card)
        .border_r_1()
        .border_color(t.color.border);
    for (index, hour) in hours.iter().enumerate().skip(1) {
        let text = format.hour(*hour);
        #[cfg(test)]
        drawn.gutter.push(text.clone());
        gutter = gutter.child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px((index as f64 * hour_height - 1.) as f32) - leading / 2.)
                .px(t.spacing.s1)
                .font_stack(&t.font.family.body)
                .text_size(t.text.supporting.size)
                .line_height(leading)
                .text_color(t.color.text_secondary)
                .flex()
                .justify_end()
                .child(text),
        );
    }
    let mut body = div().flex().flex_none().child(gutter);
    for (index, day) in days.iter().enumerate() {
        let day_iso = day.to_iso();
        let mut column = div()
            .id(ElementId::Name(format!("{id}-column-{day_iso}").into()))
            .relative()
            .flex_1()
            .flex_basis(px(0.))
            .min_w(px(140.))
            .h(grid_height)
            .flex()
            .flex_col()
            .when(index + 1 < days.len(), |d| d.border_r_1())
            .border_color(t.color.border);
        if has_popover {
            column = column
                .role(Role::Group)
                .aria_label(SharedString::from(format.full_date(*day)));
        }
        if index == 0 {
            let probe = state.downgrade();
            column = column.child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = probe.update(cx, |s, _| s.column_width = Some(bounds.size.width));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );
        }
        for (hour_index, _) in hours.iter().enumerate() {
            column = column.child(
                div()
                    .h(px(hour_height as f32))
                    .flex_none()
                    .when(hour_index + 1 < hours.len(), |d| d.border_b_1())
                    .border_color(t.color.border),
            );
        }
        let day_events: Vec<CalendarEvent> = instant_events
            .iter()
            .filter(|e| event_occurs_on_date(e, *day, tz))
            .cloned()
            .collect();
        for placement in get_timed_event_layouts(&day_events, *day, tz, min_hour, max_hour) {
            let event = &placement.event;
            let (start, end) = event.instants().unwrap_or_default();
            let time_label = format.event_time_range(start, end, tz);
            let category = get_event_category(event, &context.categories);
            let (text, border, fill) = surface_colors(
                &t,
                category.color,
                is_event_in_past(event, current_time, tz),
            );
            let top = clamp(placement.top, 0., 100.);
            let height = clamp(placement.height, 4., 100.);
            let count = placement.column_count as f32;
            // A narrow block drops its time line while the title keeps its one line.
            let hide_time = column_width.is_some_and(|w| {
                w * (placement.column_span as f32 / count) - t.spacing.s0_5.to_pixels(rem) * 2.
                    <= px(72.)
            });
            let place = div()
                .absolute()
                .top(relative((top / 100.) as f32))
                .h(relative((height / 100.) as f32))
                .left(relative(placement.column_start as f32 / count))
                .w(relative(placement.column_span as f32 / count));
            let block = div()
                .absolute()
                .top(px(2.))
                .bottom(px(3.))
                .left(t.spacing.s0_5)
                .right(t.spacing.s0_5)
                .min_h(px(24.))
                .flex()
                .flex_col()
                .min_w_0()
                .overflow_hidden()
                .border_1()
                .border_color(border)
                .bg(fill)
                .text_color(text)
                .rounded(t.radius.inner)
                .pt(t.spacing.s0_5)
                .pb(t.spacing.s1)
                .px(t.spacing.s1)
                .font_stack(&t.font.family.body)
                .text_size(t.text.supporting.size)
                .line_height(leading)
                .font_weight(t.font.weight.medium)
                .child(
                    div()
                        .truncate()
                        .font_weight(t.font.weight.bold)
                        .child(event.title.clone()),
                )
                .when(!hide_time, |d| {
                    d.child(
                        div()
                            .truncate()
                            .opacity(0.8)
                            .font_weight(t.font.weight.normal)
                            .child(time_label.clone()),
                    )
                });
            let label = format_event_button_label(context, event, *day);
            let content = render_popover.as_ref().and_then(|f| f(event, window, cx));
            let block_key = block_key_of(&event.id, &day_iso);
            #[cfg(test)]
            drawn.blocks.push((
                event.id.to_string(),
                day_iso.clone(),
                placement.top,
                placement.height,
                placement.column_start,
                placement.column_span,
                placement.column_count,
                has_popover.then(|| label.clone()),
                content.is_some(),
            ));
            let child: AnyElement = match content {
                None if has_popover => place
                    .child(
                        block
                            .id(ElementId::Name(format!("{id}-block-{block_key}").into()))
                            .role(Role::Label)
                            .aria_label(SharedString::from(label)),
                    )
                    .into_any_element(),
                None => place.child(block).into_any_element(),
                Some(content) => {
                    painted.insert(block_key.clone());
                    let button_id =
                        ElementId::Name(format!("{id}-block-button-{block_key}").into());
                    let focus = control_focus(&button_id, window, cx);
                    let selector = format!("{id}-block-{block_key}");
                    let mut el = block
                        .id(button_id)
                        .debug_selector(move || selector)
                        .track_focus(&focus)
                        .cursor_pointer()
                        .role(Role::Button)
                        .aria_label(SharedString::from(label));
                    if is_focus_visible(&focus, window) {
                        el = el.child(focus_ring(&t, t.radius.inner.to_pixels(rem), rem));
                    }
                    place
                        .child(wrap_popover(el, event, &day_iso, content))
                        .into_any_element()
                }
            };
            column = column.child(child);
        }
        if *day == current_date
            && let Some(top) = get_current_time_top(current_time, *day, tz, min_hour, max_hour)
        {
            #[cfg(test)]
            {
                drawn.now_line = Some((index, top));
            }
            let orange = t.color.border_orange;
            let rtl = rustryx_i18n::use_direction(cx).is_rtl();
            column = column.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(relative((top / 100.) as f32))
                    .child(
                        div()
                            .relative()
                            .mt(px(2.))
                            .border_t_2()
                            .border_color(orange)
                            .child(
                                div()
                                    .absolute()
                                    .top(px(-6.))
                                    .when(!rtl, |d| d.left(px(-6.)))
                                    .when(rtl, |d| d.right(px(-6.)))
                                    .size(px(10.))
                                    .rounded_full()
                                    .bg(orange),
                            ),
                    ),
            );
        }
        body = body.child(column);
    }

    // The popover closes when its block is no longer painted.
    if let Some(key) = &open_key
        && !painted.contains(key)
    {
        state.update(cx, |s, _| s.open_block = None);
        window.refresh();
    }
    #[cfg(test)]
    {
        drawn.open = open_key.clone();
    }

    // Read-only: the painted grid is decoration and the hidden grid is what assistive
    // technology reads. With render_popover the painted events are the accessible
    // representation themselves, grouped by day, so the hidden grid is not rendered.
    let hidden = (!has_popover).then(|| {
        let grid = time_grid_accessibility_grid(
            context,
            days,
            &all_day_events,
            &instant_events,
            props.focus_date,
            &hours,
            &timezone_label,
        );
        #[cfg(test)]
        {
            drawn.hidden_grid = grid.1.clone();
        }
        grid.0
    });
    #[cfg(test)]
    GRIDS.with(|g| g.borrow_mut().insert(id.to_string(), drawn));

    let min_width = px(60. + days.len().max(1) as f32 * 140.);
    let focus_visible = area.is_focus_visible(window);
    let selector = format!("{id}-time-grid-viewport");
    let viewport = area
        .viewport(
            div()
                .debug_selector(move || selector)
                .flex_1()
                .flex_basis(px(0.))
                .min_h_0()
                .min_w_0(),
        )
        .child(
            area.content(div().flex().flex_col().min_w(min_width))
                .child(header)
                .child(all_day_row)
                .child(body),
        );
    let ring = div()
        .absolute()
        .inset_0()
        .inert(true)
        .when(focus_visible, |d| d.child(focus_ring(&t, px(0.), rem)));
    let frame = div()
        .relative()
        .flex()
        .flex_col()
        .flex_grow_1()
        .flex_shrink_1()
        .flex_basis(px(640.))
        .min_h_0()
        .min_w_0()
        .child(viewport)
        .child(ring);
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .children(hidden)
        .child(frame)
        .into_any_element()
}

/// A block's name when it is exposed on its own (upstream `formatEventButtonLabel`): the
/// visible title and time lead, then the category and the full date.
fn format_event_button_label(
    context: &ScheduleContextValue,
    event: &CalendarEvent,
    day: PlainDate,
) -> String {
    let category = get_event_category(event, &context.categories);
    let time = context
        .format
        .time_grid_event_time(event, &context.timezone_id);
    format!(
        "{}, {time}, {}, {}",
        event.title,
        category.label,
        context.format.full_date(day)
    )
}

/// A hidden grid cell's name (upstream `formatTimeGridAccessibilityCellLabel`).
fn cell_label(context: &ScheduleContextValue, events: &[&CalendarEvent], label: String) -> String {
    if events.is_empty() {
        return label;
    }
    let labels: Vec<String> = events
        .iter()
        .map(|event| {
            format!(
                "{}, {}, {}",
                event.title,
                get_event_category(event, &context.categories).label,
                context
                    .format
                    .time_grid_event_time(event, &context.timezone_id)
            )
        })
        .collect();
    format!("{label}. {}", labels.join(". "))
}

/// The hidden read-only grid assistive technology reads (upstream
/// `TimeGridAccessibilityGrid`): a row of day column headers, an all-day row and an hour
/// row each, every cell named by its date, its hour and its events.
fn time_grid_accessibility_grid(
    context: &ScheduleContextValue,
    days: &[PlainDate],
    all_day_events: &[CalendarEvent],
    events: &[CalendarEvent],
    focus_date: PlainDate,
    hours: &[f64],
    timezone_label: &str,
) -> (AnyElement, Vec<Vec<String>>) {
    let id = &context.id;
    let format = &context.format;
    let tz: &str = &context.timezone_id;
    let mut labels: Vec<Vec<String>> = Vec::new();
    let mut grid = div()
        .id(ElementId::Name(format!("{id}-hidden-grid").into()))
        .absolute()
        .size_0()
        .overflow_hidden()
        .role(Role::Grid)
        .aria_label("Schedule time grid")
        .aria_read_only(true);
    let mut header = div()
        .id(ElementId::Name(format!("{id}-hidden-header").into()))
        .role(Role::Row);
    let mut row_labels = vec!["Time".to_string()];
    header = header.child(sr_only(
        format!("{id}-hidden-time"),
        Role::ColumnHeader,
        "Time",
    ));
    for (index, day) in days.iter().enumerate() {
        let label = format.full_date(*day);
        row_labels.push(label.clone());
        let mut cell = div()
            .id(ElementId::Name(format!("{id}-hidden-col-{index}").into()))
            .role(Role::ColumnHeader)
            .aria_column_index(index + 2)
            .aria_label(SharedString::from(label));
        if *day == focus_date {
            cell = cell.aria_current(AriaCurrent::Date);
        }
        header = header.child(cell);
    }
    labels.push(row_labels);
    grid = grid.child(header);
    let all_day_header = format!("{timezone_label} all day");
    let mut row = div()
        .id(ElementId::Name(format!("{id}-hidden-all-day").into()))
        .role(Role::Row)
        .child(sr_only(
            format!("{id}-hidden-all-day-header"),
            Role::RowHeader,
            all_day_header.clone(),
        ));
    let mut row_labels = vec![all_day_header];
    for (index, day) in days.iter().enumerate() {
        let cell_events: Vec<&CalendarEvent> = all_day_events
            .iter()
            .filter(|e| event_occurs_on_date(e, *day, tz))
            .collect();
        let label = cell_label(
            context,
            &cell_events,
            format!("{} all day", format.full_date(*day)),
        );
        row_labels.push(label.clone());
        row = row.child(
            div()
                .id(ElementId::Name(
                    format!("{id}-hidden-all-day-{index}").into(),
                ))
                .role(Role::GridCell)
                .aria_column_index(index + 2)
                .aria_label(SharedString::from(label)),
        );
    }
    labels.push(row_labels);
    grid = grid.child(row);
    for hour in hours {
        let hour_label = format.hour(*hour);
        let mut row = div()
            .id(ElementId::Name(format!("{id}-hidden-hour-{hour}").into()))
            .role(Role::Row)
            .child(sr_only(
                format!("{id}-hidden-hour-header-{hour}"),
                Role::RowHeader,
                hour_label.clone(),
            ));
        let mut row_labels = vec![hour_label.clone()];
        for (index, day) in days.iter().enumerate() {
            let cell_events: Vec<&CalendarEvent> = events
                .iter()
                .filter(|e| event_overlaps_hour(e, *day, *hour, tz))
                .collect();
            let label = cell_label(
                context,
                &cell_events,
                format!("{} {hour_label}", format.full_date(*day)),
            );
            row_labels.push(label.clone());
            row = row.child(
                div()
                    .id(ElementId::Name(
                        format!("{id}-hidden-cell-{hour}-{index}").into(),
                    ))
                    .role(Role::GridCell)
                    .aria_column_index(index + 2)
                    .aria_label(SharedString::from(label)),
            );
        }
        labels.push(row_labels);
        grid = grid.child(row);
    }
    (grid.into_any_element(), labels)
}
