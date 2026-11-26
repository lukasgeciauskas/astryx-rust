//! Calendar, upstream `Calendar/Calendar.tsx`: month grids for picking a
//! date or a range, in any calendar system (AST-111).
//!
//! Each visible month is an APG grid (AR1) of day buttons with one roving tab
//! stop per month. The keyboard model resolves its target day by date
//! arithmetic, then moves the months so the target stays visible (FR5, FR9).
//! Values stay Gregorian [`PlainDate`]s in every calendar (FR8).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use astryx_i18n::date::{
    CalendarMonth, DateRange, DateStyle, PlainDate, Weekday, calendar_add_months,
    calendar_month_of, calendar_shift_months, calendar_shift_years, format_calendar_date,
    format_calendar_month_label, format_day_number, week_field_name,
};
use astryx_i18n::{CalendarSystem, LogicalStyled, use_direction};
use astryx_primitives::{Politeness, announce};
use astryx_theme::{ThemeStyled, current};
use gpui::{
    AnyElement, App, ElementId, FocusHandle, Hsla, IntoElement, KeyDownEvent, RenderOnce, Role,
    SharedString, StyleRefinement, Styled, WeakEntity, Window, div, prelude::*, px,
};

use super::hooks::{
    CalendarConstraints, CalendarConstraintsOptions, CalendarDay, DateConstraint, FocusDateFn,
    InitialFocus, Spans, day_cell_state, initial_focus_date, lay_out_month, neighbors, rounding,
    use_calendar_constraints, weekday_headers,
};
use crate::button::{ButtonVariant, IconButton};
use crate::icon::{Icon, IconSize};
use crate::press::{ClickFn, Press, apply_press};

/// Upstream `mode`: one date or a start and end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CalendarMode {
    #[default]
    Single,
    Range,
}

impl CalendarMode {
    fn as_str(self) -> &'static str {
        match self {
            CalendarMode::Single => "single",
            CalendarMode::Range => "range",
        }
    }
}

/// A picked date's callback.
pub type DateFn = Rc<dyn Fn(PlainDate, &mut Window, &mut App)>;
/// A picked range's callback.
pub type RangeFn = Rc<dyn Fn(DateRange, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Value {
    Single(PlainDate),
    Range(DateRange),
}

/// How a navigation moves the focused day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Days(i64),
    Months,
    Years,
}

/// A calendar's stored state.
pub struct CalendarState {
    value: Option<Value>,
    range_start: Option<PlainDate>,
    hovered: Option<PlainDate>,
    focus_date: PlainDate,
    pending_focus: Option<PlainDate>,
    last_label: Option<String>,
    /// One focus handle per shown day button, by date; an outside day's copy
    /// has its own.
    handles: HashMap<(PlainDate, bool), FocusHandle>,
    /// Each month's roving tab stop after focus has been in it.
    tab_stops: HashMap<PlainDate, PlainDate>,
}

impl CalendarState {
    fn handle(&mut self, date: PlainDate, outside: bool, cx: &mut App) -> FocusHandle {
        self.handles
            .entry((date, outside))
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }
}

struct HandleTarget {
    state: WeakEntity<CalendarState>,
    on_focus_date_change: Option<FocusDateFn>,
    controlled: bool,
}

/// Upstream `CalendarHandle` (`handleRef`): navigates a calendar from
/// outside it. Give one to [`Calendar::handle`] and keep a clone.
#[derive(Clone, Default)]
pub struct CalendarHandle(Rc<RefCell<Option<HandleTarget>>>);

impl CalendarHandle {
    pub fn new() -> Self {
        Self::default()
    }

    /// Shows the month that holds `date` (upstream `navigateTo`). A
    /// controlled calendar hears it through `on_focus_date_change`.
    pub fn navigate_to(&self, date: PlainDate, window: &mut Window, cx: &mut App) {
        let Some(target) = self.0.borrow().as_ref().map(|t| {
            (
                t.state.clone(),
                t.on_focus_date_change.clone(),
                t.controlled,
            )
        }) else {
            return;
        };
        let (state, callback, controlled) = target;
        if controlled {
            if let Some(f) = callback {
                f(date, window, cx);
            }
        } else if let Some(state) = state.upgrade() {
            state.update(cx, |s, _| s.focus_date = date);
        }
        window.refresh();
    }
}

/// A month grid for picking a date or a range, upstream `Calendar`.
///
/// ```ignore
/// Calendar::new("due").value(due).on_change(|date, _, _| println!("{date}"))
/// ```
#[derive(IntoElement)]
pub struct Calendar {
    id: ElementId,
    mode: CalendarMode,
    value: Option<Option<PlainDate>>,
    range_value: Option<Option<DateRange>>,
    default_value: Option<PlainDate>,
    default_range: Option<DateRange>,
    on_change: Option<DateFn>,
    on_range_change: Option<RangeFn>,
    number_of_months: u8,
    min: Option<PlainDate>,
    max: Option<PlainDate>,
    date_constraints: Vec<DateConstraint>,
    max_range_span: Option<u32>,
    min_range_span: Option<u32>,
    focus_date: Option<PlainDate>,
    on_focus_date_change: Option<FocusDateFn>,
    handle: Option<CalendarHandle>,
    has_outside_days: bool,
    has_week_numbers: bool,
    has_variable_row_count: bool,
    week_starts_on: Weekday,
    calendar: Option<CalendarSystem>,
    style: StyleRefinement,
}

impl Calendar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            mode: CalendarMode::Single,
            value: None,
            range_value: None,
            default_value: None,
            default_range: None,
            on_change: None,
            on_range_change: None,
            number_of_months: 1,
            min: None,
            max: None,
            date_constraints: Vec::new(),
            max_range_span: None,
            min_range_span: None,
            focus_date: None,
            on_focus_date_change: None,
            handle: None,
            has_outside_days: true,
            has_week_numbers: false,
            has_variable_row_count: false,
            week_starts_on: Weekday::Sunday,
            calendar: None,
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `mode`.
    pub fn mode(mut self, mode: CalendarMode) -> Self {
        self.mode = mode;
        self
    }

    /// Upstream `value` in single mode: controlled; `None` is a controlled
    /// empty calendar.
    pub fn value(mut self, value: Option<PlainDate>) -> Self {
        self.value = Some(value);
        self
    }

    /// Upstream `value` in range mode: controlled; `None` is empty.
    pub fn range_value(mut self, value: Option<DateRange>) -> Self {
        self.mode = CalendarMode::Range;
        self.range_value = Some(value);
        self
    }

    /// Upstream `defaultValue` in single mode.
    pub fn default_value(mut self, value: PlainDate) -> Self {
        self.default_value = Some(value);
        self
    }

    /// Upstream `defaultValue` in range mode.
    pub fn default_range(mut self, range: DateRange) -> Self {
        self.mode = CalendarMode::Range;
        self.default_range = Some(range);
        self
    }

    /// Upstream `onChange` in single mode.
    pub fn on_change(mut self, f: impl Fn(PlainDate, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// Upstream `onChange` in range mode: the completed range, start first.
    pub fn on_range_change(
        mut self,
        f: impl Fn(DateRange, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_range_change = Some(Rc::new(f));
        self
    }

    /// Upstream `numberOfMonths`: 1 or 2 (anything else is 1).
    pub fn number_of_months(mut self, months: u8) -> Self {
        self.number_of_months = if months == 2 { 2 } else { 1 };
        self
    }

    astryx_primitives::setters! {
        /// Upstream `min`.
        ?min: PlainDate,
        /// Upstream `max`.
        ?max: PlainDate,
        /// Upstream `maxRangeSpan`: range mode's most days, both ends counted.
        ?max_range_span: u32,
        /// Upstream `minRangeSpan`: range mode's fewest days, both ends counted.
        ?min_range_span: u32,
        /// Upstream `focusDate`: the visible month, controlled with
        /// `on_focus_date_change`.
        ?focus_date: PlainDate,
        /// Upstream `hasOutsideDays` (default true).
        has_outside_days: bool,
        /// Upstream `hasWeekNumbers`: ISO 8601 week numbers in every calendar.
        has_week_numbers: bool,
        /// Upstream `hasVariableRowCount`: only the rows each month needs.
        has_variable_row_count: bool,
        /// Upstream `calendar`: else the provider's calendar.
        ?calendar: CalendarSystem,
    }

    /// Upstream `dateConstraints`: a date is disabled when any rule returns
    /// `false`.
    pub fn date_constraints(mut self, rules: impl IntoIterator<Item = DateConstraint>) -> Self {
        self.date_constraints.extend(rules);
        self
    }

    /// One rule of `date_constraints`.
    pub fn date_constraint(mut self, rule: impl Fn(PlainDate) -> bool + 'static) -> Self {
        self.date_constraints.push(Rc::new(rule));
        self
    }

    /// Upstream `onFocusDateChange`.
    pub fn on_focus_date_change(
        mut self,
        f: impl Fn(PlainDate, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_focus_date_change = Some(Rc::new(f));
        self
    }

    /// Upstream `handleRef`.
    pub fn handle(mut self, handle: CalendarHandle) -> Self {
        self.handle = Some(handle);
        self
    }

    /// Upstream `weekStartsOn`: a [`Weekday`], a number 0-6, or a name.
    pub fn week_starts_on(mut self, day: impl Into<Weekday>) -> Self {
        self.week_starts_on = day.into();
        self
    }
}

impl Styled for Calendar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The calendar's behavior, shared by its handlers.
struct Nav {
    state: gpui::Entity<CalendarState>,
    calendar: CalendarSystem,
    mode: CalendarMode,
    base: CalendarMonth,
    last: CalendarMonth,
    months: u8,
    controlled: bool,
    on_focus_date_change: Option<FocusDateFn>,
    on_change: Option<DateFn>,
    on_range_change: Option<RangeFn>,
    min: Option<PlainDate>,
    max: Option<PlainDate>,
    min_range_span: Option<u32>,
    constraints: CalendarConstraints,
    can_previous: bool,
    can_next: bool,
    locale: SharedString,
}

impl Nav {
    fn in_range(&self, date: PlainDate) -> bool {
        self.min.is_none_or(|min| date >= min) && self.max.is_none_or(|max| date <= max)
    }

    fn first_enabled(&self, month: &CalendarMonth) -> Option<PlainDate> {
        (0..month.length as i64)
            .map(|i| month.start.add_days(i))
            .find(|d| !self.constraints.is_date_disabled(*d))
    }

    fn set_focus_month(&self, month_start: PlainDate, window: &mut Window, cx: &mut App) {
        if let Some(f) = &self.on_focus_date_change {
            f(month_start, window, cx);
        }
        if !self.controlled {
            self.state.update(cx, |s, _| s.focus_date = month_start);
        }
        window.refresh();
    }

    /// Previous and Next: one calendar month, while a day past the shown
    /// months is in range (FR6, FR9).
    fn step_month(&self, delta: i32, window: &mut Window, cx: &mut App) {
        if (delta < 0 && !self.can_previous) || (delta > 0 && !self.can_next) {
            return;
        }
        let start = calendar_add_months(self.calendar, &self.base, delta).start;
        self.set_focus_month(start, window, cx);
    }

    /// A key's move from the focused day (upstream `navigateMonth` with a
    /// focused date): the target day first, then the months that show it.
    fn navigate(&self, delta: i32, from: PlainDate, unit: Unit, window: &mut Window, cx: &mut App) {
        let disabled = |d: PlainDate| self.constraints.is_date_disabled(d);
        let target = match unit {
            Unit::Months | Unit::Years => {
                let candidate = if unit == Unit::Years {
                    calendar_shift_years(self.calendar, from, delta)
                } else {
                    calendar_shift_months(self.calendar, from, delta)
                };
                if !self.in_range(candidate) {
                    return;
                }
                if disabled(candidate) {
                    match self.first_enabled(&calendar_month_of(self.calendar, candidate)) {
                        Some(first) => first,
                        None => return,
                    }
                } else {
                    candidate
                }
            }
            Unit::Days(offset) => {
                let step = delta as i64 * offset;
                let mut candidate = from.add_days(step);
                loop {
                    if from.days_until(candidate).abs() > 366 || !self.in_range(candidate) {
                        return;
                    }
                    if !disabled(candidate) {
                        break candidate;
                    }
                    candidate = candidate.add_days(step);
                }
            }
        };
        let before = target < self.base.start;
        let after = target > self.last.end;
        if !before && !after {
            let handle = self.state.update(cx, |s, cx| s.handle(target, false, cx));
            handle.focus(window, cx);
            window.refresh();
            return;
        }
        self.state.update(cx, |s, _| s.pending_focus = Some(target));
        let month = calendar_month_of(self.calendar, target);
        let start = if before {
            month.start
        } else {
            calendar_add_months(self.calendar, &month, -(self.months as i32 - 1)).start
        };
        self.set_focus_month(start, window, cx);
    }

    fn label(&self, date: PlainDate) -> String {
        format_calendar_date(
            self.calendar,
            date,
            DateStyle::LONG_WEEKDAY,
            &self.locale,
            false,
        )
    }

    /// A pick (ORD1): selection by mode, then the callback.
    fn pick(&self, date: PlainDate, window: &mut Window, cx: &mut App) {
        let i18n = astryx_i18n::current(cx);
        match self.mode {
            CalendarMode::Single => {
                self.state
                    .update(cx, |s, _| s.value = Some(Value::Single(date)));
                if let Some(f) = &self.on_change {
                    f(date, window, cx);
                }
            }
            CalendarMode::Range => {
                let start = self.state.read(cx).range_start;
                match start {
                    None => {
                        self.state.update(cx, |s, _| s.range_start = Some(date));
                        let text = i18n.format(
                            "@astryx.calendar.rangeStartAnnounce",
                            &[("date", self.label(date).as_str().into())],
                        );
                        announce(text, Politeness::Polite, window, cx);
                    }
                    Some(start) if start == date && self.min_range_span.unwrap_or(1) > 1 => {
                        self.state.update(cx, |s, _| s.range_start = None);
                        let text = i18n.format(
                            "@astryx.calendar.rangeClearedAnnounce",
                            &[("date", self.label(date).as_str().into())],
                        );
                        announce(text, Politeness::Polite, window, cx);
                    }
                    Some(start) => {
                        let range = if date < start {
                            DateRange::new(date, start)
                        } else {
                            DateRange::new(start, date)
                        };
                        self.state.update(cx, |s, _| {
                            s.value = Some(Value::Range(range));
                            s.range_start = None;
                            s.hovered = None;
                        });
                        if let Some(f) = &self.on_range_change {
                            f(range, window, cx);
                        }
                        let text = i18n.format(
                            "@astryx.calendar.rangeCompleteAnnounce",
                            &[
                                ("start", self.label(range.start).as_str().into()),
                                ("end", self.label(range.end).as_str().into()),
                            ],
                        );
                        announce(text, Politeness::Polite, window, cx);
                    }
                }
            }
        }
        window.refresh();
    }
}

/// The selector a test reads a day button's bounds by.
pub(crate) fn day_selector(id: &ElementId, date: PlainDate, outside: bool) -> String {
    format!("{id}-day-{date}{}", if outside { "-outside" } else { "" })
}

impl RenderOnce for Calendar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let i18n = astryx_i18n::current(cx);
        let locale = i18n.locale().clone();
        let dir = use_direction(cx);
        let rtl = dir.is_rtl();
        let calendar = self.calendar.unwrap_or_else(|| i18n.calendar_system());
        let months = self.number_of_months;
        let today = PlainDate::today();

        let controlled_value = match self.mode {
            CalendarMode::Single => self.value.map(|v| v.map(Value::Single)),
            CalendarMode::Range => self.range_value.map(|v| v.map(Value::Range)),
        };
        let default = match self.mode {
            CalendarMode::Single => self.default_value.map(Value::Single),
            CalendarMode::Range => self.default_range.map(Value::Range),
        };
        let opening_value = controlled_value.unwrap_or(default);
        let opening = InitialFocus {
            focus_date: self.focus_date,
            value: opening_value.map(|v| match v {
                Value::Single(d) => d,
                Value::Range(r) => r.start,
            }),
            min: self.min,
            max: self.max,
            number_of_months: months,
            today,
            calendar,
        };
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| CalendarState {
            value: default,
            range_start: None,
            hovered: None,
            focus_date: initial_focus_date(&opening),
            pending_focus: None,
            last_label: None,
            handles: HashMap::new(),
            tab_stops: HashMap::new(),
        });
        let controlled = self.focus_date.is_some() && self.on_focus_date_change.is_some();
        if let Some(handle) = &self.handle {
            *handle.0.borrow_mut() = Some(HandleTarget {
                state: state.downgrade(),
                on_focus_date_change: self.on_focus_date_change.clone(),
                controlled,
            });
        }
        let (stored_value, range_start, hovered, focus_date) = {
            let s = state.read(cx);
            (s.value, s.range_start, s.hovered, s.focus_date)
        };
        let value = match controlled_value {
            Some(v) => v,
            None => stored_value,
        };
        let focus = if controlled {
            self.focus_date.unwrap_or(focus_date)
        } else {
            focus_date
        };
        let base = calendar_month_of(calendar, focus);
        let shown: Vec<CalendarMonth> = (0..months as i32)
            .map(|i| calendar_add_months(calendar, &base, i))
            .collect();
        let last = *shown.last().unwrap_or(&base);

        // FR1: each month is its own stand-alone label.
        let label = shown
            .iter()
            .map(|m| format_calendar_month_label(calendar, m, &locale, None))
            .collect::<Vec<_>>()
            .join(" – ");
        // FR7: a changed month is announced; the first one shown is not.
        let previous = state.update(cx, |s, _| s.last_label.replace(label.clone()));
        if previous.is_some_and(|p| p != label) {
            let text = label.clone();
            window.defer(cx, move |window, cx| {
                announce(text, Politeness::Polite, window, cx)
            });
        }

        let range_anchor = (self.mode == CalendarMode::Range)
            .then_some(range_start)
            .flatten();
        let constraints = use_calendar_constraints(CalendarConstraintsOptions {
            min: self.min,
            max: self.max,
            date_constraints: self.date_constraints.clone(),
            max_range_span: self.max_range_span,
            min_range_span: self.min_range_span,
            range_anchor,
        });
        let nav = Rc::new(Nav {
            state: state.clone(),
            calendar,
            mode: self.mode,
            base,
            last,
            months,
            controlled,
            on_focus_date_change: self.on_focus_date_change.clone(),
            on_change: self.on_change.clone(),
            on_range_change: self.on_range_change.clone(),
            min: self.min,
            max: self.max,
            min_range_span: self.min_range_span,
            can_previous: self.min.is_none_or(|min| min < base.start),
            can_next: self.max.is_none_or(|max| max > last.end),
            constraints: constraints.clone(),
            locale: locale.clone(),
        });

        // The selection the grids paint.
        let (selected, mut range) = match value {
            Some(Value::Single(d)) if self.mode == CalendarMode::Single => (Some(d), None),
            Some(Value::Range(r)) if self.mode == CalendarMode::Range => {
                (None, Some((r.start, r.end)))
            }
            _ => (None, None),
        };
        if let Some(start) = range_anchor {
            range = Some((start, start));
        }
        let preview = match (range_anchor, hovered) {
            (Some(start), Some(hover)) if hover != start => Some(if hover < start {
                (hover, start)
            } else {
                (start, hover)
            }),
            _ => None,
        };
        let spans = Spans {
            range_mode: self.mode == CalendarMode::Range,
            selected,
            range,
            preview,
            today: Some(today),
        };

        // Focus handles for the shown days, a pending target claimed by the
        // month that shows it, and each month's roving tab stop.
        let grids: Vec<Vec<CalendarDay>> = shown
            .iter()
            .map(|m| {
                lay_out_month(
                    m,
                    self.week_starts_on,
                    self.has_variable_row_count,
                    calendar,
                )
            })
            .collect();
        let pending = state.update(cx, |s, cx| {
            let mut keep = std::collections::HashSet::new();
            for day in grids.iter().flatten() {
                if day.is_outside && !self.has_outside_days {
                    continue;
                }
                keep.insert((day.date, day.is_outside));
                s.handle(day.date, day.is_outside, cx);
            }
            s.handles.retain(|key, _| keep.contains(key));
            let pending = s.pending_focus.take();
            pending.filter(|p| shown.iter().any(|m| m.contains(*p)))
        });
        if let Some(target) = pending {
            let handle = state.update(cx, |s, cx| s.handle(target, false, cx));
            window.defer(cx, move |window, cx| handle.focus(window, cx));
        }
        let focused = state
            .read(cx)
            .handles
            .iter()
            .find(|(_, h)| h.is_focused(window))
            .map(|((date, _), _)| *date);
        if let Some(date) = focused
            && let Some(month) = shown.iter().find(|m| m.contains(date))
        {
            let start = month.start;
            state.update(cx, |s, _| {
                s.tab_stops.insert(start, date);
            });
        }

        let id = self.id.clone();
        let nav_button = |key: &'static str,
                          label: SharedString,
                          icon: &'static str,
                          enabled: bool,
                          delta: i32| {
            let nav = nav.clone();
            let mut button = IconButton::new(
                ElementId::Name(format!("{id}-{key}").into()),
                label,
                Icon::new(icon).size(IconSize::Sm).mirror_in_rtl(dir),
            )
            .variant(ButtonVariant::Ghost)
            .disabled(!enabled)
            .on_click(move |_, window, cx| nav.step_month(delta, window, cx));
            theme.refine(
                button.style(),
                "calendar-nav",
                &[
                    ("nav", key),
                    ("disabled", if enabled { "" } else { "disabled" }),
                ],
                None,
            );
            button
        };
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(t.spacing.s2)
            .mb(t.spacing.s2)
            .child(nav_button(
                "prev",
                i18n.t("@astryx.calendar.previousMonth"),
                "chevronLeft",
                nav.can_previous,
                -1,
            ))
            .child(
                div()
                    .flex_1()
                    .text_center()
                    .text_style(&t.text.label)
                    .font_weight(t.font.weight.semibold)
                    .text_color(t.color.text_primary)
                    .child(label),
            )
            .child(nav_button(
                "next",
                i18n.t("@astryx.calendar.nextMonth"),
                "chevronRight",
                nav.can_next,
                1,
            ));

        let mut grids_row = div().flex().flex_row().flex_wrap().gap(t.spacing.s4);
        for (month, days) in shown.iter().zip(grids) {
            grids_row = grids_row.child(self.month_grid(
                GridInput {
                    month: *month,
                    days,
                    spans,
                    nav: nav.clone(),
                    state: state.clone(),
                    today,
                    locale: locale.clone(),
                    rtl,
                    dir,
                },
                window,
                cx,
            ));
        }

        // Upstream `display: inline-block`: as wide as its months.
        let mut root = div()
            .id(self.id.clone())
            .flex()
            .flex_col()
            .self_start()
            .flex_none()
            .p(t.spacing.s3)
            .min_w(px(220.))
            .child(header)
            .child(grids_row);
        theme.refine(
            root.style(),
            "calendar",
            &[("mode", self.mode.as_str())],
            None,
        );
        root.style().refine(&self.style);
        // Escape clears a range in progress (FR4), and only that: stopping it
        // here keeps it from the layers' keystroke observer (GPUI runs no
        // observer for a key a key-down handler stopped), so a popover around
        // the calendar stays open.
        let escape_state = state.clone();
        let root = root.on_key_down(move |event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" && escape_state.read(cx).range_start.is_some() {
                escape_state.update(cx, |s, _| {
                    s.range_start = None;
                    s.hovered = None;
                });
                cx.stop_propagation();
                window.refresh();
            }
        });
        // Upstream's `inline-block`: in any parent, the calendar keeps its
        // content's width.
        div().flex().flex_row().items_start().child(root)
    }
}

struct GridInput {
    month: CalendarMonth,
    days: Vec<CalendarDay>,
    spans: Spans,
    nav: Rc<Nav>,
    state: gpui::Entity<CalendarState>,
    today: PlainDate,
    locale: SharedString,
    rtl: bool,
    dir: astryx_i18n::Direction,
}

impl Calendar {
    fn month_grid(&self, g: GridInput, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let i18n = astryx_i18n::current(cx);
        let calendar = g.nav.calendar;
        let month_label = format_calendar_month_label(calendar, &g.month, &g.locale, None);
        let disabled = |d: PlainDate| g.nav.constraints.is_date_disabled(d);

        // The month's tab stop (AR1): the focused or last-focused day, else
        // the selected day, else today, else the first enabled day; never a
        // disabled one (FR3).
        let usable = |d: &PlainDate| g.month.contains(*d) && !disabled(*d);
        let stored = g.state.read(cx).tab_stops.get(&g.month.start).copied();
        let tab_stop = stored
            .filter(usable)
            .or(g.spans.selected.filter(usable))
            .or(Some(g.today).filter(usable))
            .or_else(|| {
                g.days
                    .iter()
                    .find(|d| !d.is_outside && !disabled(d.date))
                    .map(|d| d.date)
            });

        let grid_id = format!("{}-grid-{}", self.id, g.month.start);
        let node = |part: String| div().id(ElementId::Name(format!("{grid_id}-{part}").into()));
        let row = |part: String, rtl: bool| {
            let el = node(part).flex();
            if rtl {
                el.flex_row_reverse()
            } else {
                el.flex_row()
            }
        };
        let rem = window.rem_size();
        let cell_min = t.size.element_md;
        let header_height = cell_min.to_pixels(rem) + t.spacing.s1.to_pixels(rem);
        let mut header = row("header".into(), g.rtl).role(Role::Row);
        if self.has_week_numbers {
            header = header.child(
                node("week-header".into())
                    .role(Role::ColumnHeader)
                    .aria_label(week_field_name(&g.locale))
                    .w(cell_min)
                    .flex_none(),
            );
        }
        for (day, name) in weekday_headers(&g.locale, self.week_starts_on) {
            header = header.child(
                node(format!("weekday-{}", day.index()))
                    .role(Role::ColumnHeader)
                    .flex_1()
                    .min_w(cell_min)
                    .h(header_height)
                    .pb(t.spacing.s1)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_style(&t.text.supporting)
                    .text_color(t.color.text_secondary)
                    .whitespace_nowrap()
                    .child(name),
            );
        }

        let mut grid = div()
            .id(ElementId::Name(grid_id.clone().into()))
            .flex()
            .flex_col()
            .role(Role::Grid)
            .aria_label(month_label)
            .child(header);
        if g.nav.mode == CalendarMode::Range {
            grid = grid.aria_multiselectable(true);
        }
        for (row_index, week) in g.days.chunks(7).enumerate() {
            let mut row_el = row(format!("row-{row_index}"), g.rtl).role(Role::Row);
            if self.has_week_numbers {
                let first = week[0].date;
                let thursday = first.add_days((4 - first.weekday().index() as i64).rem_euclid(7));
                row_el = row_el.child(
                    node(format!("week-{row_index}"))
                        .role(Role::RowHeader)
                        .w(cell_min)
                        .h(cell_min)
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_style(&t.text.supporting)
                        .text_color(t.color.text_secondary)
                        .child(format_day_number(
                            thursday.iso_week_number() as i64,
                            &g.locale,
                        )),
                );
            }
            for (index, day) in week.iter().enumerate() {
                row_el =
                    row_el.child(self.day_cell(&g, week, index, day, tab_stop, &i18n, window, cx));
            }
            grid = grid.child(row_el);
        }
        // The keyboard model (AR1, FR5, FR9).
        let nav = g.nav.clone();
        let state = g.state.clone();
        let month = g.month;
        let week_starts_on = self.week_starts_on;
        let variable = self.has_variable_row_count;
        let rtl = g.rtl;
        let grid = grid.on_key_down(move |event: &KeyDownEvent, window, cx| {
            let keys = &event.keystroke;
            let m = &keys.modifiers;
            if m.control || m.alt || m.platform {
                return;
            }
            let Some(from) = state
                .read(cx)
                .handles
                .iter()
                .find(|(_, h)| h.is_focused(window))
                .map(|((date, _), _)| *date)
            else {
                return;
            };
            let forward = if rtl { -1 } else { 1 };
            match keys.key.as_str() {
                "left" => nav.navigate(-forward, from, Unit::Days(1), window, cx),
                "right" => nav.navigate(forward, from, Unit::Days(1), window, cx),
                "up" => nav.navigate(-1, from, Unit::Days(7), window, cx),
                "down" => nav.navigate(1, from, Unit::Days(7), window, cx),
                "pageup" => nav.navigate(
                    -1,
                    from,
                    if m.shift { Unit::Years } else { Unit::Months },
                    window,
                    cx,
                ),
                "pagedown" => nav.navigate(
                    1,
                    from,
                    if m.shift { Unit::Years } else { Unit::Months },
                    window,
                    cx,
                ),
                key @ ("home" | "end") => {
                    // The first or last enabled day of the focused day's week.
                    let days = lay_out_month(&month, week_starts_on, variable, nav.calendar);
                    let Some(row) = days.chunks(7).find(|w| w.iter().any(|d| d.date == from))
                    else {
                        return;
                    };
                    let enabled = |d: &&CalendarDay| {
                        !d.is_outside && !nav.constraints.is_date_disabled(d.date)
                    };
                    let target = if key == "home" {
                        row.iter().find(enabled)
                    } else {
                        row.iter().rev().find(enabled)
                    };
                    if let Some(target) = target {
                        let handle = state.update(cx, |s, cx| s.handle(target.date, false, cx));
                        handle.focus(window, cx);
                        window.refresh();
                    }
                }
                _ => return,
            }
            cx.stop_propagation();
        });
        div().flex_1().child(grid).into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn day_cell(
        &self,
        g: &GridInput,
        week: &[CalendarDay],
        index: usize,
        day: &CalendarDay,
        tab_stop: Option<PlainDate>,
        i18n: &astryx_i18n::I18n,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let theme = current(window, cx);
        let t = theme.tokens().clone();
        let rem = window.rem_size();
        let band = px(2.);
        let cell = div()
            .id(ElementId::Name(
                format!(
                    "{}-cell-{}{}",
                    self.id,
                    day.date,
                    if day.is_outside { "-o" } else { "" }
                )
                .into(),
            ))
            .relative()
            .flex_1()
            .min_w(t.size.element_md)
            .min_h(t.size.element_md)
            .p(band)
            .flex()
            .items_center()
            .justify_center()
            .role(Role::GridCell);
        if day.is_outside && !self.has_outside_days {
            // An empty cell keeps the seven columns (upstream's placeholder).
            return cell.into_any_element();
        }
        let disabled_by_rule = g.nav.constraints.is_date_disabled(day.date);
        let state = day_cell_state(day, index, &g.spans, disabled_by_rule);
        let calendar = g.nav.calendar;
        let is_disabled = |d: PlainDate| g.nav.constraints.is_date_disabled(d);
        let [prev_range, next_range, prev_preview, next_preview] =
            neighbors(week, index, &g.spans, &is_disabled);
        let mut cell = cell.aria_selected(state.is_selected || state.is_in_range);

        // The range and preview bands behind the day (FR13).
        let band_el = |start: bool,
                       end: bool,
                       prev: bool,
                       next: bool,
                       color: Hsla,
                       target: &str,
                       start_flag: bool,
                       end_flag: bool| {
            let (round_start, round_end) = rounding(start, end, &state, prev, next);
            let inset_start = if round_start || start_flag {
                band
            } else {
                px(0.)
            };
            let inset_end = if round_end || end_flag { band } else { px(0.) };
            let (left, right) = if g.rtl {
                (inset_end, inset_start)
            } else {
                (inset_start, inset_end)
            };
            let mut el = div()
                .absolute()
                .top(band)
                .bottom(band)
                .left(left)
                .right(right)
                .bg(color);
            if round_start {
                el = el.rounded_start(g.dir, t.radius.full);
            }
            if round_end {
                el = el.rounded_end(g.dir, t.radius.full);
            }
            theme.refine(
                el.style(),
                target,
                &[
                    ("start", if start_flag { "start" } else { "" }),
                    ("end", if end_flag { "end" } else { "" }),
                ],
                None,
            );
            el
        };
        if state.is_in_range {
            cell = cell.child(band_el(
                state.is_range_start,
                state.is_range_end,
                prev_range,
                next_range,
                t.color.accent_muted,
                "calendar-range",
                state.is_range_start,
                state.is_range_end,
            ));
        }
        if state.is_in_preview {
            cell = cell.child(band_el(
                state.is_preview_start,
                state.is_preview_end,
                prev_preview,
                next_preview,
                t.color.overlay_hover,
                "calendar-range-preview",
                state.is_preview_start,
                state.is_preview_end,
            ));
        }

        // The day's name says its selection or range role (AR1).
        let date_label = format_calendar_date(
            calendar,
            day.date,
            DateStyle::LONG_WEEKDAY,
            &g.locale,
            false,
        );
        let arg = [("date", date_label.as_str().into())];
        let in_progress = g.nav.state.read(cx).range_start.is_some();
        let name: SharedString = if state.is_selected {
            i18n.format("@astryx.calendar.daySelected", &arg)
        } else if state.is_range_start && state.is_range_end {
            if in_progress {
                i18n.format("@astryx.calendar.dayRangeStart", &arg)
            } else {
                i18n.format("@astryx.calendar.dayRangeStartAndEnd", &arg)
            }
        } else if state.is_range_start {
            i18n.format("@astryx.calendar.dayRangeStart", &arg)
        } else if state.is_range_end {
            i18n.format("@astryx.calendar.dayRangeEnd", &arg)
        } else if state.is_in_range {
            i18n.format("@astryx.calendar.dayInRange", &arg)
        } else {
            date_label.clone().into()
        };

        let endpoint = state.is_endpoint();
        let today_ring = state.is_today && !state.is_selected && !state.is_in_range;
        let today_in_range_ring = state.is_today && !state.is_selected && state.is_in_range;
        let marker = if today_ring {
            "today-only"
        } else if today_in_range_ring {
            "today-in-range"
        } else {
            ""
        };
        let fill = endpoint.then_some(t.color.accent);
        let fg = if endpoint {
            t.color.on_accent
        } else if day.is_outside {
            t.color.text_secondary
        } else {
            t.color.text_primary
        };
        let handle = g
            .state
            .update(cx, |s, cx| s.handle(day.date, day.is_outside, cx))
            .tab_stop(!day.is_outside && tab_stop == Some(day.date));
        let selector = day_selector(&self.id, day.date, day.is_outside);
        let mut button = div()
            .id(ElementId::Name(selector.clone().into()))
            .debug_selector(move || selector.clone())
            .relative()
            .min_w(t.size.element_sm)
            .min_h(t.size.element_sm)
            .flex()
            .items_center()
            .justify_center()
            .rounded(t.radius.full)
            .text_style(&t.text.body)
            .text_color(fg)
            .role(Role::Button)
            .aria_label(name)
            .child(format_day_number(day.day_number as i64, &g.locale));
        if let Some(fill) = fill {
            button = button.bg(fill);
        }
        if today_ring {
            button = button.border_1().border_color(t.color.border_emphasized);
        } else if today_in_range_ring {
            button = button.border_1().border_color(t.color.text_primary);
        }
        if state.is_today {
            button = button.aria_current(gpui::AriaCurrent::Date);
        }
        if state.effectively_disabled {
            button = button.aria_disabled(true);
        }
        let props = [
            ("selected", if endpoint { "selected" } else { "" }),
            ("today", if state.is_today { "today" } else { "" }),
            (
                "disabled",
                if state.effectively_disabled {
                    "disabled"
                } else {
                    ""
                },
            ),
            ("in-range", if state.is_in_range { "in-range" } else { "" }),
            ("marker", marker),
        ];
        theme.refine(button.style(), "calendar-day", &props, None);
        let nav = g.nav.clone();
        let date = day.date;
        let pick: ClickFn = Rc::new(move |_, window, cx| nav.pick(date, window, cx));
        // A finger's press paints as UIKit tracks a control (upstream `usePressFeedback`).
        let feedback = crate::gestures::use_press_feedback(
            ElementId::Name(
                format!("{}-day-{}-{}", g.state.entity_id(), date, day.is_outside).into(),
            ),
            window,
            cx,
        )
        .disabled(state.effectively_disabled);
        let mut button = apply_press(
            button,
            &handle,
            Press {
                feedback: Some(&feedback),
                fill,
                scale: None,
                disabled: state.effectively_disabled,
                busy: false,
                ring: Some((t.radius.full.to_pixels(rem), None)),
                raise_ring: false,
                theme_target: Some(("calendar-day", &props)),
            },
            Some(pick),
            &theme,
            window,
            cx,
        );
        if state.effectively_disabled {
            // Upstream dims a disabled day further than a disabled button.
            button = button.opacity(0.3);
        } else if day.is_outside {
            button = button.opacity(0.5);
        }
        if g.nav.mode == CalendarMode::Range && !state.effectively_disabled {
            let hover_state = g.state.clone();
            button = button.on_hover(move |over, window, cx| {
                let changed = hover_state.update(cx, |s, _| {
                    let next = if *over { Some(date) } else { None };
                    let changed = s.range_start.is_some() && s.hovered != next;
                    s.hovered = next;
                    changed
                });
                if changed {
                    window.refresh();
                }
            });
        }
        cell.child(feedback.surface(button)).into_any_element()
    }
}
