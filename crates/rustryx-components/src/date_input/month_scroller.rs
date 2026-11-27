//! The touch calendar's month pager, upstream `DateInput/MonthScroller.tsx`:
//! a horizontally paged run of six-row month grids (an APG grid each),
//! drawn around the shown month. A swipe, a trackpad or a shift-wheel
//! scrolls it and it settles onto a month; arrows, the wheels and the
//! keyboard steer it; the days move focus by date across months.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, ElementId, Entity, FocusHandle, IntoElement, KeyDownEvent, RenderOnce, Role, ScrollEvent,
    ScrollSource, Task, TouchPhase, Window, div, prelude::*, px,
};
use rustryx_i18n::date::{
    DateStyle, PlainDate, Weekday, calendar_year_month_of, format_calendar_date,
    format_calendar_month_label, format_day_number,
};
use rustryx_i18n::{current as i18n, use_direction};
use rustryx_primitives::motion::uikit::picker;
use rustryx_primitives::{is_focus_visible, prefers_reduced_motion};
use rustryx_theme::{CubicBezier, ThemeStyled, current};
use web_time::Instant;

use super::month_model::MonthModel;
use crate::calendar::{CalendarDaysOptions, calendar_days};
use crate::focus::focus_ring;
use crate::wheel::options::{DAY_SIZE, PANE_HEIGHT};

/// Panes drawn on each side of the shown one (`OVERSCAN`).
const OVERSCAN: i64 = 3;
/// A scroller quiet this long is at rest (`SCROLL_QUIET_MS`).
const QUIET_MS: u64 = 220;
/// A browser's notch on the inline axis, px.
const LINE_PX: f32 = 100. / 3.;
const EASE: CubicBezier = CubicBezier(0.42, 0., 0.58, 1.);

pub(crate) type MonthFn = Rc<dyn Fn(i64, &mut Window, &mut App)>;
pub(crate) type DateFn = Rc<dyn Fn(PlainDate, &mut Window, &mut App)>;
pub(crate) type IsDisabled = Rc<dyn Fn(PlainDate) -> bool>;

/// Steers the scroller from outside (`MonthScrollerHandle`).
#[derive(Clone, Default)]
pub(crate) struct MonthScrollerHandle(Rc<RefCell<Option<Entity<ScrollerState>>>>);

impl MonthScrollerHandle {
    /// Brings `month_index` to rest, smoothly or at once.
    pub fn scroll_to_month(
        &self,
        month_index: i64,
        smooth: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let state = self.0.borrow().clone();
        if let Some(state) = state {
            scroll_to_month(&state, month_index, smooth, window, cx);
        }
    }
}

/// The month pager.
#[derive(IntoElement)]
pub(crate) struct MonthScroller {
    pub id: ElementId,
    pub handle: MonthScrollerHandle,
    pub model: MonthModel,
    pub min_index: i64,
    pub max_index: i64,
    pub initial_index: i64,
    pub on_visible_month_change: MonthFn,
    pub selected: Option<PlainDate>,
    pub today: PlainDate,
    pub is_disabled: IsDisabled,
    pub week_starts_on: Weekday,
    pub on_select: DateFn,
    /// False while the wheels cover it: it keeps its place but takes no input.
    pub active: bool,
}

struct Glide {
    from: f64,
    to: f64,
    start: Instant,
    duration: f64,
    _timer: Task<()>,
}

#[derive(Clone)]
struct Props {
    min_index: i64,
    rows: i64,
    rtl: bool,
    on_visible_month_change: MonthFn,
    model: MonthModel,
}

pub(crate) struct ScrollerState {
    /// The distance scrolled along the inline axis: a row is a pane.
    offset: f64,
    pane: f64,
    positioned: bool,
    glide: Option<Glide>,
    /// The row a programmatic scroll heads for: its passage is not news.
    steer: Option<i64>,
    reported: i64,
    quiet: Option<Task<()>>,
    /// The day the keyboard moved to, to focus once its pane exists.
    focused: Option<PlainDate>,
    restore_focus: bool,
    days: HashMap<PlainDate, FocusHandle>,
    props: Option<Props>,
}

impl ScrollerState {
    fn shown(&self, now: Instant) -> f64 {
        match &self.glide {
            Some(g) => {
                let t = (now.saturating_duration_since(g.start).as_secs_f64() / g.duration).min(1.);
                g.from + (g.to - g.from) * f64::from(EASE.ease(t as f32))
            }
            None => self.offset,
        }
    }

    fn props(&self) -> Props {
        self.props.clone().expect("rendered before it scrolls")
    }

    fn row_at(&self, offset: f64) -> i64 {
        if self.pane <= 0. {
            return 0;
        }
        let rows = self.props.as_ref().map_or(1, |p| p.rows);
        ((offset / self.pane).round() as i64).clamp(0, rows - 1)
    }
}

fn now(cx: &App) -> Instant {
    cx.background_executor().now()
}

/// The scroll moved: report a new month unless a steer is under way.
fn passed(state: &Entity<ScrollerState>, window: &mut Window, cx: &mut App) {
    let report = state.update(cx, |s, _| {
        let row = s.row_at(s.offset);
        if row == s.reported {
            return None;
        }
        s.reported = row;
        match s.steer {
            Some(steer) => {
                if steer == row {
                    s.steer = None;
                }
                None
            }
            None => {
                let p = s.props();
                Some((p.on_visible_month_change.clone(), p.min_index + row))
            }
        }
    });
    if let Some((report, month)) = report {
        report(month, window, cx);
    }
}

fn glide_to(state: &Entity<ScrollerState>, to: f64, window: &mut Window, cx: &mut App) {
    let at = now(cx);
    let from = state.update(cx, |s, _| {
        let from = s.shown(at);
        s.offset = from;
        s.glide = None;
        from
    });
    if (from - to).abs() < 0.5 {
        state.update(cx, |s, _| s.offset = to);
        return;
    }
    let duration = picker::TAP_SPIN_MS / 1000.;
    let timer = cx
        .background_executor()
        .timer(Duration::from_secs_f64(duration));
    let entity = state.clone();
    let task = window.spawn(cx, async move |cx| {
        timer.await;
        cx.update(|window, cx| {
            entity.update(cx, |s, _| {
                if let Some(g) = s.glide.take() {
                    s.offset = g.to;
                }
            });
            passed(&entity, window, cx);
            window.refresh();
        })
        .ok();
    });
    state.update(cx, |s, _| {
        s.glide = Some(Glide {
            from,
            to,
            start: at,
            duration,
            _timer: task,
        });
    });
    window.refresh();
}

fn scroll_to_month(
    state: &Entity<ScrollerState>,
    month_index: i64,
    smooth: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let at = now(cx);
    let (pane, row, far) = state.update(cx, |s, _| {
        let p = s.props();
        let row = (month_index - p.min_index).clamp(0, p.rows - 1);
        let center = s.row_at(s.shown(at));
        (s.pane, row, (row - center).abs() > OVERSCAN)
    });
    if pane <= 0. {
        return;
    }
    let target = row as f64 * pane;
    // A jump past the drawn panes, a step under Reduce Motion, or an
    // immediate scroll lands at once.
    if far || !smooth || prefers_reduced_motion(cx) {
        state.update(cx, |s, _| {
            s.glide = None;
            s.offset = target;
            s.reported = row;
            s.steer = None;
        });
        window.refresh();
        return;
    }
    state.update(cx, |s, _| s.steer = Some(row));
    glide_to(state, target, window, cx);
}

/// At rest after the user's scroll: onto the nearest pane.
fn settle(state: &Entity<ScrollerState>, window: &mut Window, cx: &mut App) {
    let (target, needed) = state.update(cx, |s, _| {
        s.quiet = None;
        let row = s.row_at(s.offset);
        let target = row as f64 * s.pane;
        (target, s.glide.is_none() && (s.offset - target).abs() >= 1.)
    });
    if needed {
        glide_to(state, target, window, cx);
    }
}

fn scrolled(
    state: &Entity<ScrollerState>,
    event: &ScrollEvent,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let delta = event.delta.pixel_delta(px(LINE_PX));
    let (dx, dy) = (f64::from(delta.x), f64::from(delta.y));
    // The inline axis: a horizontal scroll, or a shift-wheel.
    let along = if dx != 0. {
        dx
    } else if event.modifiers.shift && event.source == ScrollSource::Pointer {
        dy
    } else {
        return false;
    };
    let at = now(cx);
    state.update(cx, |s, _| {
        let p = s.props();
        let from = s.shown(at);
        s.glide = None;
        // The user's input ends a steer.
        s.steer = None;
        let max = (p.rows - 1) as f64 * s.pane;
        let moved = if p.rtl { from + along } else { from - along };
        s.offset = moved.clamp(0., max.max(0.));
    });
    passed(state, window, cx);
    if event.source == ScrollSource::Touch && event.touch_phase == TouchPhase::Ended {
        // The finger and GPUI's momentum are done.
        settle(state, window, cx);
    } else {
        let timer = cx
            .background_executor()
            .timer(Duration::from_millis(QUIET_MS));
        let entity = state.clone();
        let task = window.spawn(cx, async move |cx| {
            timer.await;
            cx.update(|window, cx| settle(&entity, window, cx)).ok();
        });
        state.update(cx, |s, _| s.quiet = Some(task));
    }
    window.refresh();
    true
}

/// Keyboard focus moves by days and the scroller follows (APG grid).
fn move_focus(
    state: &Entity<ScrollerState>,
    from: PlainDate,
    days: i64,
    window: &mut Window,
    cx: &mut App,
) {
    let next = from.add_days(days);
    let (model, min, rows) = state.read_with(cx, |s, _| {
        let p = s.props();
        (p.model, p.min_index, p.rows)
    });
    let index = model.index_of(next);
    if index < min || index > min + rows - 1 {
        return;
    }
    state.update(cx, |s, _| {
        s.focused = Some(next);
        s.restore_focus = true;
    });
    if index != model.index_of(from) {
        scroll_to_month(state, index, true, window, cx);
    }
    window.refresh();
}

pub(crate) fn day_selector(id: &ElementId, date: PlainDate) -> String {
    format!("{id}-day-{date}")
}

impl RenderOnce for MonthScroller {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let locale = i18n(cx).locale().clone();
        let rtl = use_direction(cx).is_rtl();
        let coarse = rustryx_primitives::coarse_primary_pointer(window, cx);
        let rows = (self.max_index - self.min_index + 1).max(1);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ScrollerState {
            offset: 0.,
            pane: 0.,
            positioned: false,
            glide: None,
            steer: None,
            reported: self.initial_index - self.min_index,
            quiet: None,
            focused: None,
            restore_focus: false,
            days: HashMap::new(),
            props: None,
        });
        *self.handle.0.borrow_mut() = Some(state.clone());
        let initial_row = (self.initial_index - self.min_index).clamp(0, rows - 1);
        state.update(cx, |s, _| {
            s.props = Some(Props {
                min_index: self.min_index,
                rows,
                rtl,
                on_visible_month_change: self.on_visible_month_change.clone(),
                model: self.model.clone(),
            });
            // Land on the opening month before the first paint that knows
            // the width.
            if !s.positioned && s.pane > 0. {
                s.positioned = true;
                s.offset = initial_row as f64 * s.pane;
                s.reported = initial_row;
            }
        });
        let at = now(cx);
        let (offset, pane, gliding, focused, restore) = state.read_with(cx, |s, _| {
            (
                s.shown(at),
                s.pane,
                s.glide.is_some(),
                s.focused,
                s.restore_focus,
            )
        });
        if gliding {
            window.request_animation_frame();
        }
        let center = if pane > 0. {
            ((offset / pane).round() as i64).clamp(0, rows - 1)
        } else {
            initial_row
        };

        let mut scroller = div()
            .id(self.id.clone())
            .debug_selector({
                let selector = format!("{}", self.id);
                move || selector.clone()
            })
            .relative()
            .w_full()
            .h(px(PANE_HEIGHT as f32))
            .overflow_hidden();
        if pane > 0. {
            let mut live: Vec<PlainDate> = Vec::new();
            for row in (center - OVERSCAN).max(0)..=(center + OVERSCAN).min(rows - 1) {
                let index = self.min_index + row;
                let month = self.model.month_at(index);
                let ym = calendar_year_month_of(self.model.calendar, &month);
                let days = calendar_days(
                    CalendarDaysOptions {
                        year: ym.year,
                        month: ym.month,
                        week_starts_on: self.week_starts_on,
                        has_variable_row_count: false,
                        calendar: self.model.calendar,
                    },
                    &locale,
                );
                let x = if rtl {
                    offset - row as f64 * pane
                } else {
                    row as f64 * pane - offset
                };
                // One day per pane is a tab stop, and only in the shown pane:
                // the keyboard's day, else the selection, else the first.
                let tabbable = [focused, self.selected]
                    .into_iter()
                    .flatten()
                    .find(|d| month.contains(*d))
                    .unwrap_or(month.start);
                let label = format_calendar_month_label(self.model.calendar, &month, &locale, None);
                let mut grid = div()
                    .id(ElementId::Name(format!("{}-pane-{index}", self.id).into()))
                    .absolute()
                    .top_0()
                    .left(px(x as f32))
                    .w(px(pane as f32))
                    .h(px(PANE_HEIGHT as f32))
                    .flex()
                    .flex_col()
                    .role(Role::Grid)
                    .aria_label(label);
                for week in &days.weeks {
                    let mut line = div()
                        .id(ElementId::Name(
                            format!("{}-week-{}", self.id, week[0].date).into(),
                        ))
                        .flex()
                        .flex_1()
                        .role(Role::Row);
                    if rtl {
                        line = line.flex_row_reverse();
                    }
                    for day in week {
                        let disabled = day.is_outside || (self.is_disabled)(day.date);
                        let selected = !day.is_outside && self.selected == Some(day.date);
                        let today = !day.is_outside && self.today == day.date;
                        let mut puck = div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px((DAY_SIZE - 8.) as f32))
                            .rounded_full()
                            .border(t.border.width)
                            .border_color(gpui::transparent_black())
                            .text_style(&t.text.body)
                            .child(format_day_number(i64::from(day.day_number), &locale));
                        if today {
                            puck = puck
                                .border_color(t.color.border_emphasized)
                                .font_weight(t.font.weight.semibold);
                        }
                        if selected {
                            puck = puck
                                .bg(t.color.accent)
                                .border_color(t.color.accent)
                                .text_color(t.color.on_accent)
                                .font_weight(t.font.weight.semibold);
                        } else if !disabled && !coarse {
                            let hover = t.color.overlay_hover;
                            puck = puck.hover(move |s| s.bg(hover));
                        }
                        let mut button = div()
                            .id(ElementId::Name(
                                format!("{}-{}-{}", self.id, day.date, u8::from(day.is_outside))
                                    .into(),
                            ))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size_full()
                            .min_w(px(DAY_SIZE as f32))
                            .relative()
                            .text_color(if day.is_outside {
                                t.color.text_secondary
                            } else {
                                t.color.text_primary
                            })
                            .role(Role::Button)
                            .aria_label(format_calendar_date(
                                self.model.calendar,
                                day.date,
                                DateStyle::LONG_WEEKDAY,
                                &locale,
                                false,
                            ))
                            .aria_disabled(disabled)
                            .child(puck);
                        if today {
                            button = button.aria_current(gpui::AriaCurrent::Date);
                        }
                        if disabled {
                            button = button.opacity(0.3);
                        } else if day.is_outside {
                            button = button.opacity(0.5);
                        } else {
                            button = button.cursor_pointer();
                        }
                        if !day.is_outside {
                            let selector = day_selector(&self.id, day.date);
                            button = button.debug_selector(move || selector.clone());
                            live.push(day.date);
                            let handle = state.update(cx, |s, cx| {
                                s.days
                                    .entry(day.date)
                                    .or_insert_with(|| cx.focus_handle())
                                    .clone()
                            });
                            let stop = self.active && row == center && day.date == tabbable;
                            button = button.track_focus(&handle.clone().tab_stop(stop));
                            if is_focus_visible(&handle, window) {
                                button = button.child(focus_ring(
                                    &t,
                                    px((DAY_SIZE - 8.) as f32 / 2.),
                                    rem,
                                ));
                            }
                            if self.active {
                                let date = day.date;
                                let (keys, focus_state) = (state.clone(), state.clone());
                                let week_start = self.week_starts_on;
                                button = button
                                    .on_key_down(move |event: &KeyDownEvent, window, cx| {
                                        let column = (i64::from(date.weekday().index())
                                            - i64::from(week_start.index())
                                            + 7)
                                            % 7;
                                        let (forward, backward) = if rtl {
                                            ("left", "right")
                                        } else {
                                            ("right", "left")
                                        };
                                        let delta = match event.keystroke.key.as_str() {
                                            k if k == backward => -1,
                                            k if k == forward => 1,
                                            "up" => -7,
                                            "down" => 7,
                                            "home" => -column,
                                            "end" => 6 - column,
                                            _ => return,
                                        };
                                        cx.stop_propagation();
                                        move_focus(&keys, date, delta, window, cx);
                                    })
                                    .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                        focus_state.update(cx, |s, _| s.focused = Some(date));
                                    });
                                if !disabled {
                                    let select = self.on_select.clone();
                                    button = button
                                        .on_click(move |_, window, cx| select(date, window, cx));
                                }
                            }
                        }
                        let cell = div()
                            .id(ElementId::Name(
                                format!(
                                    "{}-cell-{}-{}",
                                    self.id,
                                    day.date,
                                    u8::from(day.is_outside)
                                )
                                .into(),
                            ))
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .role(Role::GridCell)
                            .aria_selected(selected)
                            .child(button);
                        line = line.child(cell);
                    }
                    grid = grid.child(line);
                }
                scroller = scroller.child(grid);
            }
            // The keyboard's day takes focus once its pane exists.
            if restore
                && let Some(date) = focused
                && live.contains(&date)
            {
                let handle = state.update(cx, |s, _| {
                    s.restore_focus = false;
                    s.days.get(&date).cloned()
                });
                if let Some(handle) = handle {
                    window.defer(cx, move |window, cx| window.focus(&handle, cx));
                }
            }
            state.update(cx, |s, _| s.days.retain(|d, _| live.contains(d)));
        }
        if self.active {
            let scroll = state.clone();
            scroller = scroller.on_scroll(move |event: &ScrollEvent, window, cx| {
                if scrolled(&scroll, event, window, cx) {
                    cx.stop_propagation();
                }
            });
        }
        // The probe: the scrollport's width is the pane's.
        let probe = state.clone();
        scroller.child(
            gpui::canvas(
                move |bounds, window, cx| {
                    let width = f64::from(bounds.size.width);
                    let changed = probe.update(cx, |s, _| {
                        if width > 0. && (s.pane - width).abs() > 0.5 {
                            let row = s.row_at(s.offset);
                            s.pane = width;
                            if s.positioned {
                                s.offset = row as f64 * width;
                            }
                            true
                        } else {
                            false
                        }
                    });
                    if changed {
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
    }
}
