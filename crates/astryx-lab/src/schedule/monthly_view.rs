//! The month view (upstream `MonthlyView.tsx`): a table of week rows with chips on at most
//! three levels, each chip in the cell where it starts; a "+N more" button on a busy day
//! that opens the view's popover listing that day's events; and, with `render_popover`,
//! chips and list rows that open an event's content in that same popover.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use astryx_components::focus::{focus_ring, focus_ring_inset};
use astryx_components::overlay::{Popover, disclosure};
use astryx_components::text::{Heading, TextColor};
use astryx_i18n::date::{PlainDate, Weekday, Width};
use astryx_primitives::{LayerAlign, Placement, control_focus, is_focus_visible};
use astryx_theme::{ThemeStyled, current};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, AriaCurrent, ElementId, FocusHandle, FontFeatures, IntoElement, KeyDownEvent,
    Role, ScrollHandle, SharedString, Window, div, point, px, relative,
};

use super::context::ScheduleContextValue;
use super::date_math::{enumerate_dates, get_schedule_range_from_dates};
use super::format::{get_event_category, is_event_in_past};
use super::month_layout::{
    MONTH_VISIBLE_LEVELS, MonthChipPlacement, get_event_date_span, layout_month_events,
};
use super::shared::{
    heading_level, list_event_row, month_event_pill, popover_body, range_month_title,
    schedule_frame, sr_only, supporting_leading,
};
use super::types::{
    CalendarEvent, RenderPopover, ScheduleNavigationRange, ScheduleView, ScheduleViewImpl,
};
use super::zoned::{ZonedDateTime, ZonedDateTimeRange, schedule_range_to_zoned_date_time_range};

/// The month view's options (upstream `ScheduleMonthlyViewOptions`).
#[derive(Clone, Default)]
pub struct ScheduleMonthlyViewOptions {
    /// Upstream `weekStartsOn`: 0 for Sunday (0).
    pub week_starts_on: u8,
    /// Upstream `renderPopover`: a chip whose content is not `None` is a button that opens
    /// the view's popover; absent, the month is read-only (component:Schedule FR19 to FR20).
    pub render_popover: Option<RenderPopover>,
}

// A chip level starts below the day number and the next one a chip's height plus a gap
// further down, so three levels fit in a 128 px week row in every shipped theme.
const MONTH_CHIP_TOP: f32 = 30.;
const MONTH_LEVEL_PITCH: f32 = 29.;
const MONTH_ROW_HEIGHT: f32 = 128.;

/// What the view's popover shows (upstream `MonthPopoverKey`): a busy day's list, a chip's
/// event, or an event handed over from a day's list. Event ids may contain any character,
/// so the id is always the key's last part.
#[derive(Clone, Debug, PartialEq, Eq)]
enum MonthPopoverKey {
    Day { day_iso: String },
    Chip { segment: String, event_id: String },
    Row { day_iso: String, event_id: String },
}

fn day_key(day_iso: &str) -> String {
    format!("day/{day_iso}")
}

/// One painted chip: a span cut around a busy day paints as two chips (upstream
/// `chipKey`).
fn chip_key(chip: &MonthChipPlacement) -> String {
    format!("chip/{}:{}/{}", chip.week, chip.column_start, chip.event.id)
}

fn row_key(day_iso: &str, event_id: &str) -> String {
    format!("row/{day_iso}/{event_id}")
}

/// Upstream `parseKey`.
fn parse_key(key: Option<&str>) -> Option<MonthPopoverKey> {
    let key = key?;
    let slash = key.find('/')?;
    let (kind, rest) = (&key[..slash], &key[slash + 1..]);
    if kind == "day" {
        return Some(MonthPopoverKey::Day {
            day_iso: rest.to_string(),
        });
    }
    let second = rest.find('/')?;
    let (head, event_id) = (rest[..second].to_string(), rest[second + 1..].to_string());
    match kind {
        "chip" => Some(MonthPopoverKey::Chip {
            segment: head,
            event_id,
        }),
        "row" => Some(MonthPopoverKey::Row {
            day_iso: head,
            event_id,
        }),
        _ => None,
    }
}

/// The view's one popover (upstream `useScheduleViewPopover`): the trigger that opened it,
/// and the content it shows (switched to from inside, it keeps its trigger).
#[derive(Default)]
struct MonthPopover {
    trigger: Option<String>,
    open: Option<String>,
    /// Focus moves into switched content once it renders.
    focus_switched: bool,
    /// Where each chip painted so far starts.
    chip_start_day: HashMap<String, String>,
    scroll: ScrollHandle,
}

/// A month event button's name: the title, the time range or "all day", the category,
/// and the day (upstream `formatMonthEventName`, AR8).
fn format_month_event_name(
    context: &ScheduleContextValue,
    event: &CalendarEvent,
    day_label: &str,
) -> String {
    let time = match event.instants() {
        None => "all day".to_string(),
        Some((start, end)) => context
            .format
            .event_time_range(start, end, &context.timezone_id),
    };
    format!(
        "{}, {time}, {}, {day_label}",
        event.title,
        get_event_category(event, &context.categories).label
    )
}

/// Every event of each day (upstream `getMonthEventsByDay`).
fn get_month_events_by_day(
    events: &[CalendarEvent],
    days: &[PlainDate],
    timezone_id: &str,
) -> HashMap<String, Vec<CalendarEvent>> {
    let mut by_day: HashMap<String, Vec<CalendarEvent>> =
        days.iter().map(|d| (d.to_iso(), Vec::new())).collect();
    let (Some(first), Some(last)) = (days.first().copied(), days.last().copied()) else {
        return by_day;
    };
    for event in events {
        let (start, end) = get_event_date_span(event, timezone_id);
        if end < first || start > last {
            continue;
        }
        let mut current = if start < first { first } else { start };
        let visible_end = if end > last { last } else { end };
        while current <= visible_end {
            if let Some(list) = by_day.get_mut(&current.to_iso()) {
                list.push(event.clone());
            }
            current = current.add_days(1);
        }
    }
    by_day
}

/// The popover's content, built once it renders.
type OpenContent = Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>;

/// What a month drew, for tests.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
#[allow(clippy::type_complexity)]
pub(crate) struct MonthDrawn {
    pub table_label: String,
    pub weekday_headers: Vec<(String, String, u8)>,
    pub row_headers: Vec<String>,
    pub cells: Vec<(String, String, bool, bool)>,
    /// (key, event id, start day, span, level, name, is button, title-first pill time).
    pub chips: Vec<(
        String,
        String,
        String,
        usize,
        usize,
        Option<String>,
        bool,
        Option<String>,
    )>,
    pub hidden_names: Vec<(String, String)>,
    pub hidden_lists: Vec<(String, Vec<String>)>,
    pub more: Vec<(String, String, String, usize)>,
    pub popover: Option<(String, String)>,
    pub popover_rows: Vec<(String, Option<String>)>,
}

#[cfg(test)]
thread_local! {
    pub(crate) static MONTHS: std::cell::RefCell<HashMap<String, MonthDrawn>> = Default::default();
}

/// A trigger of the month popover (upstream's `<button>` with `getTriggerProps`): it reads the
/// popover's disclosure; a press while it shows content switched to from inside shows its
/// own content again.
#[derive(IntoElement)]
struct MonthTrigger {
    el: gpui::Stateful<gpui::Div>,
    key: String,
    expanded: bool,
    state: gpui::Entity<MonthPopover>,
}

impl RenderOnce for MonthTrigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(d) = disclosure(cx) else {
            return self.el.into_any_element();
        };
        let (state, key) = (self.state.clone(), self.key.clone());
        let switched = {
            let s = state.read(cx);
            s.trigger.as_deref() == Some(key.as_str()) && s.open.as_deref() != Some(key.as_str())
        };
        let toggle = d.clone();
        self.el
            .aria_expanded(self.expanded)
            .aria_has_popup(d.has_popup())
            .on_click(move |_, window, cx| {
                if switched {
                    // Its own content again, in the same open popover.
                    state.update(cx, |s, cx| {
                        s.open = Some(key.clone());
                        cx.notify();
                    });
                } else {
                    toggle.toggle(window, cx);
                }
            })
            .into_any_element()
    }
}

struct MonthlyView(ScheduleMonthlyViewOptions);

fn month_date_range(
    date: PlainDate,
    timezone_id: &str,
    week_starts_on: u8,
) -> super::types::ScheduleRange {
    let first = date.first_of_month();
    let next = first.add_months(1);
    let weekday = Weekday::from_index(week_starts_on);
    get_schedule_range_from_dates(
        first.start_of_week(weekday),
        next.add_days(-1).end_of_week_exclusive(weekday),
        timezone_id,
    )
}

fn zoned(range: super::types::ScheduleRange, date: &ZonedDateTime) -> ZonedDateTimeRange {
    schedule_range_to_zoned_date_time_range(&range, &date.timezone_id)
}

impl ScheduleViewImpl for MonthlyView {
    fn date_range(&self, date: &ZonedDateTime) -> ZonedDateTimeRange {
        zoned(
            month_date_range(
                date.to_plain_date(),
                &date.timezone_id,
                self.0.week_starts_on,
            ),
            date,
        )
    }

    fn previous_date_range(&self, date: &ZonedDateTime) -> ScheduleNavigationRange {
        ScheduleNavigationRange {
            label: "Previous month".into(),
            range: zoned(
                month_date_range(
                    date.to_plain_date().add_months(-1),
                    &date.timezone_id,
                    self.0.week_starts_on,
                ),
                date,
            ),
        }
    }

    fn next_date_range(&self, date: &ZonedDateTime) -> ScheduleNavigationRange {
        ScheduleNavigationRange {
            label: "Next month".into(),
            range: zoned(
                month_date_range(
                    date.to_plain_date().add_months(1),
                    &date.timezone_id,
                    self.0.week_starts_on,
                ),
                date,
            ),
        }
    }

    fn render(
        &self,
        context: &Rc<ScheduleContextValue>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        monthly_view(context, self.0.render_popover.clone(), window, cx)
    }
}

/// The month view (upstream `createScheduleMonthlyView`).
pub fn create_schedule_monthly_view(options: ScheduleMonthlyViewOptions) -> ScheduleView {
    ScheduleView::new(MonthlyView(options))
}

#[allow(clippy::too_many_lines)]
fn monthly_view(
    context: &Rc<ScheduleContextValue>,
    render_popover: Option<RenderPopover>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    let rem = window.rem_size();
    let format = &context.format;
    let tz: &str = &context.timezone_id;
    let id = context.id.clone();
    let has_popover = render_popover.is_some();
    let range_date = context.date.to_plain_date();
    let highlighted = context.focus_date.to_plain_date();
    let current_time = context.current_time;
    let days = enumerate_dates(context.range.start_date, context.range.end_date);
    let weeks: Vec<&[PlainDate]> = days.chunks(7).collect();
    let compare = |a: &str, b: &str| format.compare(a, b);
    let layout = layout_month_events(&context.events, &days, tz, &compare);
    let overflow_by_day: HashMap<usize, usize> = layout
        .overflow
        .iter()
        .map(|o| (o.day_index, o.count))
        .collect();
    let events_by_day = get_month_events_by_day(&context.events, &days, tz);
    // Every chip that covers each day, in level order: a chip renders in the cell where it
    // starts, and with render_popover each later day it covers names it as static text
    // (component:Schedule FR19, AR8).
    let mut chips_by_day: HashMap<usize, Vec<&MonthChipPlacement>> = HashMap::new();
    for chip in &layout.chips {
        for column in chip.column_start..=chip.column_end {
            chips_by_day
                .entry(chip.week * 7 + column)
                .or_default()
                .push(chip);
        }
    }
    for chips in chips_by_day.values_mut() {
        chips.sort_by_key(|c| c.level);
    }
    let event_by_id: HashMap<&str, &CalendarEvent> =
        context.events.iter().map(|e| (e.id.as_ref(), e)).collect();
    let day_by_iso: HashMap<String, PlainDate> = days.iter().map(|d| (d.to_iso(), *d)).collect();
    // The grid shows one Gregorian month, which can span two months of the calendar, so
    // its title names both.
    let month_start = range_date.first_of_month();
    let month_end = month_start.add_months(1).add_days(-1);
    let month_title = format.week_title(month_start, month_end);

    let state = window.use_keyed_state(
        ElementId::Name(format!("{id}-month-popover").into()),
        cx,
        |_, _| MonthPopover::default(),
    );
    let (trigger_key, open_key) = {
        let s = state.read(cx);
        (s.trigger.clone(), s.open.clone())
    };
    let open_parsed = parse_key(open_key.as_deref());
    let dialog_label = match &open_parsed {
        None => month_title.clone(),
        Some(MonthPopoverKey::Day { day_iso }) => day_by_iso
            .get(day_iso)
            .map_or(month_title.clone(), |d| format.full_date(*d)),
        Some(MonthPopoverKey::Chip { event_id, .. } | MonthPopoverKey::Row { event_id, .. }) => {
            event_by_id
                .get(event_id.as_str())
                .map_or(month_title.clone(), |e| e.title.to_string())
        }
    };
    let mut painted: HashSet<String> = HashSet::new();
    #[cfg(test)]
    let mut drawn = MonthDrawn {
        table_label: month_title.clone(),
        ..Default::default()
    };

    // The popover's content: a day's list, or one event's content.
    let open_content: Option<OpenContent> = match &open_parsed {
        Some(MonthPopoverKey::Day { day_iso }) => day_by_iso.get(day_iso).copied().map(|day| {
            let events = events_by_day.get(day_iso).cloned().unwrap_or_default();
            let (context, render_popover, state) =
                (context.clone(), render_popover.clone(), state.clone());
            let label = format!("{} events", format.full_date(day));
            #[cfg(test)]
            {
                drawn.popover_rows = events
                    .iter()
                    .map(|e| {
                        let has_content = render_popover.as_ref().is_some();
                        (
                            e.id.to_string(),
                            has_content.then(|| {
                                format_month_event_name(context.as_ref(), e, &format.full_date(day))
                            }),
                        )
                    })
                    .collect();
            }
            let body_id = format!("{id}-month-day-body");
            Box::new(move |window: &mut Window, cx: &mut App| {
                let list = month_day_events(
                    &context,
                    day,
                    &events,
                    render_popover.as_ref(),
                    &state,
                    window,
                    cx,
                );
                popover_body(body_id, label, list, window, cx)
            }) as Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>
        }),
        Some(MonthPopoverKey::Chip { event_id, .. } | MonthPopoverKey::Row { event_id, .. }) => {
            event_by_id.get(event_id.as_str()).and_then(|event| {
                let content = render_popover.as_ref().and_then(|f| f(event, window, cx))?;
                let label = format!("{} details", event.title);
                let body_id = format!("{id}-month-event-body");
                Some(Box::new(move |window: &mut Window, cx: &mut App| {
                    popover_body(body_id, label, content, window, cx)
                })
                    as Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>)
            })
        }
        None => None,
    };
    #[cfg(test)]
    {
        drawn.popover = open_key.clone().map(|k| (k, dialog_label.clone()));
    }
    let mut open_content = open_content;

    // Wraps a trigger in the view's popover: open while it is the trigger and something is
    // open; it shows the open content.
    let mut wrap = |el: gpui::Stateful<gpui::Div>,
                    key: String,
                    owns_open: bool,
                    window: &mut Window,
                    cx: &mut App|
     -> AnyElement {
        painted.insert(key.clone());
        let is_trigger = trigger_key.as_deref() == Some(key.as_str());
        if owns_open && let Some(open) = &open_key {
            painted.insert(open.clone());
        }
        let is_open = is_trigger && open_key.is_some();
        let expanded = is_open && (open_key.as_deref() == Some(key.as_str()) || owns_open);
        let state_for_change = state.clone();
        let change_key = key.clone();
        let mut popover = Popover::new(
            ElementId::Name(format!("{id}-month-popover-{key}").into()),
            dialog_label.clone(),
        )
        .placement(Placement::Below)
        .alignment(LayerAlign::Start)
        .open(is_open)
        .on_open_change(move |open, _, cx| {
            let key = change_key.clone();
            state_for_change.update(cx, |s, cx| {
                if open {
                    s.trigger = Some(key.clone());
                    s.open = Some(key);
                } else {
                    s.trigger = None;
                    s.open = None;
                }
                cx.notify();
            })
        });
        if is_open && let Some(content) = open_content.take() {
            popover = popover.content_fn(content);
        }
        let _ = window;
        let _ = &cx;
        popover
            .child(MonthTrigger {
                el,
                key,
                expanded,
                state: state.clone(),
            })
            .into_any_element()
    };

    // The table: weekday column headers, week row headers, cells named by their full date,
    // and no arrow-key promise, so a busy day's "+N more" is an ordinary Tab stop. It
    // scrolls horizontally at narrow widths, and is itself focusable for keyboard scrolling.
    let mut header_row = div()
        .id(ElementId::Name(format!("{id}-month-header").into()))
        .role(Role::Row)
        .flex()
        .border_b_1()
        .border_color(t.color.border)
        .child(sr_only(
            format!("{id}-month-week-header"),
            Role::ColumnHeader,
            "Week",
        ));
    for (index, day) in days.iter().take(7).enumerate() {
        let long = format.weekday(*day, Width::Long);
        let short = format.weekday(*day, Width::Short);
        #[cfg(test)]
        drawn
            .weekday_headers
            .push((long.clone(), short.clone(), context.heading_level));
        header_row = header_row.child(
            div()
                .id(ElementId::Name(
                    format!("{id}-month-weekday-{index}").into(),
                ))
                .flex_1()
                .flex_basis(px(0.))
                .min_w_0()
                .py(t.spacing.s2)
                .px(t.spacing.s3)
                .flex()
                .justify_center()
                .role(Role::ColumnHeader)
                .aria_column_index(index + 2)
                .aria_label(SharedString::from(long))
                .child(
                    Heading::new(heading_level(context.heading_level), short)
                        .id(ElementId::Name(
                            format!("{id}-month-weekday-heading-{index}").into(),
                        ))
                        .color(TextColor::Secondary),
                ),
        );
    }

    let border = px(1.);
    let inset = t.spacing.s0_5.to_pixels(rem);
    let tabular = FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]));
    let mut focus_fallback: HashMap<String, FocusHandle> = HashMap::new();
    let mut rows: Vec<AnyElement> = Vec::new();
    let week_count = weeks.len();
    for (week_index, week) in weeks.iter().enumerate() {
        let row_header = format.week_range(week[0], week[week.len() - 1]);
        #[cfg(test)]
        drawn.row_headers.push(row_header.clone());
        // The cells' paint: backgrounds and borders under the cells' content, so a chip
        // crossing later days stays above them.
        let mut backgrounds = div().absolute().inset_0().flex();
        for (day_index, day) in week.iter().enumerate() {
            let outside = day.month != range_date.month;
            backgrounds = backgrounds.child(
                div()
                    .flex_1()
                    .flex_basis(px(0.))
                    .min_w_0()
                    .bg(if outside {
                        t.color.background_muted
                    } else {
                        t.color.background_card
                    })
                    .when(day_index < 6, |d| d.border_r_1())
                    .when(week_index + 1 < week_count, |d| d.border_b_1())
                    .border_color(t.color.border),
            );
        }
        let mut row = div()
            .id(ElementId::Name(
                format!("{id}-month-week-{week_index}").into(),
            ))
            .role(Role::Row)
            .relative()
            .flex()
            .h(px(MONTH_ROW_HEIGHT))
            .child(backgrounds)
            .child(sr_only(
                format!("{id}-month-row-header-{week_index}"),
                Role::RowHeader,
                row_header,
            ));
        for (day_index, day) in week.iter().enumerate() {
            let index = week_index * 7 + day_index;
            let day_iso = day.to_iso();
            let full_date = format.full_date(*day);
            let is_current = *day == highlighted;
            let day_events = events_by_day.get(&day_iso).cloned().unwrap_or_default();
            let hidden_count = overflow_by_day.get(&index).copied();
            let cell_id = ElementId::Name(format!("{id}-month-cell-{day_iso}").into());
            // Focus lands here when the day's "+N more" goes away.
            let cell_focus = control_focus(&cell_id, window, cx);
            focus_fallback.insert(day_iso.clone(), cell_focus.clone());
            #[cfg(test)]
            drawn.cells.push((
                day_iso.clone(),
                full_date.clone(),
                is_current,
                day.month != range_date.month,
            ));
            let mut number = div()
                .flex()
                .items_center()
                .justify_start()
                .w(px(24.))
                .h(px(24.))
                .m(t.spacing.s0_5)
                .pl(t.spacing.s1)
                .rounded_full()
                .text_color(t.color.text_secondary)
                .font_stack(&t.font.family.body)
                .text_size(t.text.supporting.size)
                .line_height(supporting_leading(&t, rem))
                .font_weight(t.font.weight.medium)
                .font_features(tabular.clone())
                .child(format.day_number(*day));
            if is_current {
                number = number.text_color(t.color.on_accent).bg(t.color.accent);
            }
            let mut cell = div()
                .id(cell_id)
                .track_focus(&cell_focus.clone().tab_stop(false))
                .relative()
                .flex_1()
                .flex_basis(px(0.))
                .min_w_0()
                .flex()
                .flex_col()
                .gap(t.spacing.s1)
                .role(Role::Cell)
                .aria_column_index(day_index + 2)
                .aria_label(SharedString::from(full_date.clone()))
                .child(number);
            if is_current {
                cell = cell.aria_current(AriaCurrent::Date);
            }
            if is_focus_visible(&cell_focus, window) {
                cell = cell.child(focus_ring_inset(&t, px(0.), rem));
            }
            for chip in chips_by_day.get(&index).cloned().unwrap_or_default() {
                let key = chip_key(chip);
                if chip.column_start != day_index {
                    let start_day = days.get(chip.week * 7 + chip.column_start).copied();
                    if has_popover && let Some(start_day) = start_day {
                        let name = format_month_event_name(
                            context,
                            &chip.event,
                            &format!("since {}", format.full_date(start_day)),
                        );
                        #[cfg(test)]
                        drawn.hidden_names.push((day_iso.clone(), name.clone()));
                        cell = cell.child(sr_only(
                            format!("{id}-month-since-{day_iso}-{key}"),
                            Role::Label,
                            name,
                        ));
                    }
                    continue;
                }
                state.update(cx, |s, _| {
                    s.chip_start_day.insert(key.clone(), day_iso.clone());
                });
                let span = (chip.column_end - chip.column_start + 1) as f32;
                let is_past = is_event_in_past(&chip.event, current_time, tz);
                let pill = month_event_pill(context, &chip.event, is_past, window, cx);
                let wrapper = div()
                    .absolute()
                    .left_0()
                    .top(px(MONTH_CHIP_TOP + chip.level as f32 * MONTH_LEVEL_PITCH))
                    .w(relative(span));
                // A chip starts a chip inset into the cell and ends a chip inset plus one
                // border short of its last day's edge, clearing that day's end border.
                let inner = div().ml(inset).mr(inset + border).min_w_0();
                #[cfg(test)]
                let pill_time = chip
                    .event
                    .instants()
                    .map(|(start, _)| format.time(start, tz));
                let element: AnyElement = match &render_popover {
                    None => {
                        #[cfg(test)]
                        drawn.chips.push((
                            key.clone(),
                            chip.event.id.to_string(),
                            day_iso.clone(),
                            span as usize,
                            chip.level,
                            None,
                            false,
                            pill_time.clone(),
                        ));
                        wrapper.child(inner.child(pill)).into_any_element()
                    }
                    Some(render) => {
                        let name = format_month_event_name(
                            context,
                            &chip.event,
                            &format.event_days(&chip.event, tz),
                        );
                        let has_content = render(&chip.event, window, cx).is_some();
                        #[cfg(test)]
                        drawn.chips.push((
                            key.clone(),
                            chip.event.id.to_string(),
                            day_iso.clone(),
                            span as usize,
                            chip.level,
                            Some(name.clone()),
                            has_content,
                            pill_time.clone(),
                        ));
                        if !has_content {
                            wrapper
                                .child(
                                    inner
                                        .id(ElementId::Name(
                                            format!("{id}-month-chip-{key}").into(),
                                        ))
                                        .role(Role::Label)
                                        .aria_label(SharedString::from(name))
                                        .child(pill),
                                )
                                .into_any_element()
                        } else {
                            let button_id =
                                ElementId::Name(format!("{id}-month-chip-button-{key}").into());
                            let focus = control_focus(&button_id, window, cx);
                            let selector = format!("{id}-chip-{}", chip.event.id);
                            let mut el = inner
                                .id(button_id)
                                .debug_selector(move || selector)
                                .track_focus(&focus)
                                .relative()
                                .cursor_pointer()
                                .role(Role::Button)
                                .aria_label(SharedString::from(name))
                                .child(pill);
                            if is_focus_visible(&focus, window) {
                                el = el.child(focus_ring(&t, t.radius.inner.to_pixels(rem), rem));
                            }
                            wrapper
                                .child(wrap(el, key.clone(), false, window, cx))
                                .into_any_element()
                        }
                    }
                };
                cell = cell.child(element);
            }
            // With render_popover the chips are the cell's accessible events, so the hidden
            // list would repeat them (component:Schedule FR19).
            if !has_popover && !day_events.is_empty() {
                let labels: Vec<String> = day_events
                    .iter()
                    .map(|e| format.event_accessibility_label(e, tz, &context.categories))
                    .collect();
                #[cfg(test)]
                drawn.hidden_lists.push((day_iso.clone(), labels.clone()));
                let mut list = div()
                    .id(ElementId::Name(format!("{id}-month-list-{day_iso}").into()))
                    .absolute()
                    .size_0()
                    .overflow_hidden()
                    .role(Role::List);
                for (i, label) in labels.into_iter().enumerate() {
                    list = list.child(sr_only(
                        format!("{id}-month-list-{day_iso}-{i}"),
                        Role::ListItem,
                        label,
                    ));
                }
                cell = cell.child(list);
            }
            if let Some(count) = hidden_count {
                let label = format!(
                    "{count} more {}, {full_date}",
                    if count == 1 { "event" } else { "events" }
                );
                #[cfg(test)]
                drawn.more.push((
                    day_iso.clone(),
                    label.clone(),
                    format!("+{count} more"),
                    MONTH_VISIBLE_LEVELS - 1,
                ));
                let button_id = ElementId::Name(format!("{id}-month-more-{day_iso}").into());
                let focus = control_focus(&button_id, window, cx);
                let selector = format!("{id}-more-{day_iso}");
                let mut el = div()
                    .id(button_id)
                    .debug_selector(move || selector)
                    .track_focus(&focus)
                    .absolute()
                    .left(inset)
                    .right(inset + border)
                    .top(px(
                        MONTH_CHIP_TOP + (MONTH_VISIBLE_LEVELS - 1) as f32 * MONTH_LEVEL_PITCH
                    ))
                    .flex()
                    .items_center()
                    .min_w_0()
                    .overflow_hidden()
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .rounded(t.radius.inner)
                    .py(t.spacing.s0_5)
                    .px(t.spacing.s1_5)
                    .font_stack(&t.font.family.body)
                    .text_size(t.text.supporting.size)
                    .line_height(supporting_leading(&t, rem))
                    .font_weight(t.font.weight.medium)
                    .text_color(t.color.text_secondary)
                    .cursor_pointer()
                    .hover(|s| s.bg(t.color.overlay_hover))
                    .role(Role::Button)
                    .aria_label(SharedString::from(label))
                    .child(div().truncate().child(format!("+{count} more")));
                if is_focus_visible(&focus, window) {
                    el = el.child(focus_ring(&t, t.radius.inner.to_pixels(rem), rem));
                }
                // An event handed over from this day's list keeps "+N more" as its trigger
                // while the event is still on this day.
                let owns = matches!(&open_parsed, Some(MonthPopoverKey::Row { day_iso: d, event_id }) if *d == day_iso && day_events.iter().any(|e| e.id.as_ref() == event_id));
                cell = cell.child(wrap(el, day_key(&day_iso), owns, window, cx));
            }
            row = row.child(cell);
        }
        rows.push(row.into_any_element());
    }

    // The open trigger is gone: close, and focus lands on the day it belonged to, or on
    // the day's "+N more" when an event leaves a day that is still busy.
    if let Some(trigger) = &trigger_key
        && open_key.is_some()
        && !painted.contains(trigger)
    {
        let fallback = match parse_key(Some(trigger)) {
            Some(MonthPopoverKey::Chip { .. }) => {
                state.read(cx).chip_start_day.get(trigger).cloned()
            }
            Some(MonthPopoverKey::Day { day_iso } | MonthPopoverKey::Row { day_iso, .. }) => {
                Some(day_iso)
            }
            None => None,
        };
        state.update(cx, |s, _| {
            s.trigger = None;
            s.open = None;
        });
        if let Some(handle) = fallback.and_then(|d| focus_fallback.get(&d).cloned()) {
            window.focus(&handle, cx);
        }
        window.refresh();
    }

    #[cfg(test)]
    MONTHS.with(|m| m.borrow_mut().insert(id.to_string(), drawn));

    let scroll = state.read(cx).scroll.clone();
    let table_focus = control_focus(
        &ElementId::Name(format!("{id}-month-table").into()),
        window,
        cx,
    );
    let keys = {
        let scroll = scroll.clone();
        move |event: &KeyDownEvent, window: &mut Window, _: &mut App| {
            let offset = scroll.offset();
            let max = scroll.max_offset().x;
            let x = match event.keystroke.key.as_str() {
                "left" => offset.x + px(40.),
                "right" => offset.x - px(40.),
                "home" => px(0.),
                "end" => -max,
                _ => return,
            };
            scroll.set_offset(point(x.min(px(0.)).max(-max), offset.y));
            window.refresh();
        }
    };
    let selector = format!("{id}-month-table");
    let mut table = div()
        .id(ElementId::Name(format!("{id}-month-table").into()))
        .debug_selector(move || selector)
        .track_focus(&table_focus.clone().tab_stop(true))
        .on_key_down(keys)
        .relative()
        .overflow_x_scroll()
        .track_scroll(&scroll)
        .role(Role::Table)
        .aria_label(SharedString::from(month_title.clone()))
        .child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w(px(784.))
                .child(header_row)
                .child(div().relative().w_full().flex().flex_col().children(rows)),
        );
    if is_focus_visible(&table_focus, window) {
        table = table.child(focus_ring_inset(&t, px(0.), rem));
    }
    let title = range_month_title(format, month_start, month_end);
    schedule_frame(
        context,
        title,
        month_title,
        context.is_loading,
        vec![table.into_any_element()],
        window,
        cx,
    )
}

/// A busy day's list: every event of the day, in start order (upstream
/// `MonthDayEvents`). With render_popover, a row whose event has content is a button that
/// hands the event to the view's popover (component:Schedule FR20).
fn month_day_events(
    context: &Rc<ScheduleContextValue>,
    day: PlainDate,
    events: &[CalendarEvent],
    render_popover: Option<&RenderPopover>,
    state: &gpui::Entity<MonthPopover>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    let rem = window.rem_size();
    let tz: &str = &context.timezone_id;
    let full_date = context.format.full_date(day);
    let day_iso = day.to_iso();
    let mut list = div()
        .id(ElementId::Name(
            format!("{}-month-day-list", context.id).into(),
        ))
        .flex()
        .flex_col()
        .gap(t.spacing.s2)
        .role(Role::List);
    for event in events {
        let row = list_event_row(
            context,
            event,
            is_event_in_past(event, context.current_time, tz),
            window,
            cx,
        );
        let has_content = render_popover.is_some_and(|f| f(event, window, cx).is_some());
        let item = if has_content {
            let button_id =
                ElementId::Name(format!("{}-month-row-{day_iso}-{}", context.id, event.id).into());
            let focus = control_focus(&button_id, window, cx);
            let (state, key) = (state.clone(), row_key(&day_iso, &event.id));
            let selector = format!("{}-row-{}", context.id, event.id);
            let mut button = div()
                .id(button_id)
                .debug_selector(move || selector)
                .track_focus(&focus)
                .relative()
                .w_full()
                .rounded(t.radius.inner)
                .cursor_pointer()
                .hover(|s| s.bg(t.color.overlay_hover))
                .role(Role::Button)
                .aria_label(SharedString::from(format_month_event_name(
                    context, event, &full_date,
                )))
                .on_click(move |_, _, cx| {
                    // Shows the event in the open popover, which keeps its trigger.
                    state.update(cx, |s, cx| {
                        if s.open.is_some() {
                            s.open = Some(key.clone());
                            s.focus_switched = true;
                            cx.notify();
                        }
                    })
                })
                .child(row);
            if is_focus_visible(&focus, window) {
                button = button.child(focus_ring(&t, t.radius.inner.to_pixels(rem), rem));
            }
            button.into_any_element()
        } else {
            row
        };
        list = list.child(
            div()
                .id(ElementId::Name(
                    format!("{}-month-item-{day_iso}-{}", context.id, event.id).into(),
                ))
                .role(Role::ListItem)
                .child(item),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.s2)
        .max_w(px(360.))
        .child(
            div()
                .font_stack(&t.font.family.body)
                .text_size(t.text.supporting.size)
                .line_height(supporting_leading(&t, rem))
                .font_weight(t.font.weight.bold)
                .text_color(t.color.text_secondary)
                .child(full_date),
        )
        .child(list)
        .into_any_element()
}
