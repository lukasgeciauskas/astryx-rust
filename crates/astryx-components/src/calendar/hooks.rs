//! Calendar's public hooks and pure helpers: upstream `useCalendarDays`,
//! `useCalendarNavigation`, `useCalendarConstraints`, `getInitialFocusDate`
//! and `dayCellUtils`.
//!
//! The hooks take an explicit calendar that defaults to Gregorian and never
//! read the provider's calendar (AST-111 FR20, Calendar FR11).

use std::rc::Rc;

use astryx_i18n::CalendarSystem;
use astryx_i18n::date::{
    CalendarMonth, CalendarYearMonth, PlainDate, Weekday, calendar_add_months,
    calendar_day_of_month, calendar_month_from_year_month, calendar_month_of,
    calendar_year_month_of, format_calendar_month_label, weekday_header_names,
};
use gpui::{App, ElementId, Window};

/// One day of a month grid, upstream `CalendarDay`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalendarDay {
    /// The Gregorian date.
    pub date: PlainDate,
    /// Whether the day belongs to an adjacent month.
    pub is_outside: bool,
    /// The day of its calendar month.
    pub day_number: u8,
}

/// What [`use_calendar_days`] lays out, upstream `UseCalendarDaysOptions`.
#[derive(Clone, Copy, Debug)]
pub struct CalendarDaysOptions {
    /// The year, in `calendar`'s numbering.
    pub year: i32,
    /// The month's position in its year: 1 through 12 in the Gregorian
    /// calendar, 1 through 13 in a Hebrew leap year.
    pub month: u8,
    /// The first column of the grid and of `day_names`.
    pub week_starts_on: Weekday,
    /// Only the rows the month needs instead of a fixed six.
    pub has_variable_row_count: bool,
    /// The calendar that numbers `year`, `month` and each day number.
    pub calendar: CalendarSystem,
}

impl Default for CalendarDaysOptions {
    fn default() -> Self {
        Self {
            year: 1970,
            month: 1,
            week_starts_on: Weekday::Sunday,
            has_variable_row_count: false,
            calendar: CalendarSystem::Gregory,
        }
    }
}

/// One month's day grid, upstream `UseCalendarDaysReturn`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarDays {
    /// Every cell, outside days included.
    pub days: Vec<CalendarDay>,
    /// The days in rows of seven.
    pub weeks: Vec<Vec<CalendarDay>>,
    /// The weekday header names, in column order, in the provider's locale.
    pub day_names: Vec<String>,
    /// The number of cells.
    pub total_cells: usize,
}

/// One month's day grid (upstream `useCalendarDays`): every date, the
/// adjacent months' filler days included, in weeks, with the weekday names.
pub fn use_calendar_days(options: CalendarDaysOptions, cx: &App) -> CalendarDays {
    let locale = astryx_i18n::use_locale(cx);
    calendar_days(options, &locale)
}

/// [`use_calendar_days`] for an explicit locale.
pub fn calendar_days(options: CalendarDaysOptions, locale: &str) -> CalendarDays {
    let calendar = options.calendar;
    let month = calendar_month_from_year_month(
        calendar,
        CalendarYearMonth {
            year: options.year,
            month: options.month,
        },
    );
    let days = lay_out_month(
        &month,
        options.week_starts_on,
        options.has_variable_row_count,
        calendar,
    );
    let weeks = days.chunks(7).map(<[CalendarDay]>::to_vec).collect();
    CalendarDays {
        total_cells: days.len(),
        day_names: weekday_headers(locale, options.week_starts_on)
            .into_iter()
            .map(|(_, name)| name)
            .collect(),
        days,
        weeks,
    }
}

/// The cells of a calendar month's grid.
pub(crate) fn lay_out_month(
    month: &CalendarMonth,
    week_starts_on: Weekday,
    variable_rows: bool,
    calendar: CalendarSystem,
) -> Vec<CalendarDay> {
    let lead = (month.start.weekday().index() as i64 - week_starts_on.index() as i64).rem_euclid(7);
    let rows = if variable_rows {
        (month.length as i64 + lead + 6) / 7
    } else {
        6
    };
    (0..rows * 7)
        .map(|i| {
            let offset = i - lead;
            let date = month.start.add_days(offset);
            let inside = offset >= 0 && offset < month.length as i64;
            CalendarDay {
                date,
                is_outside: !inside,
                day_number: if inside {
                    offset as u8 + 1
                } else {
                    calendar_day_of_month(calendar, date)
                },
            }
        })
        .collect()
}

/// The seven weekday headers in column order (upstream `getWeekdayHeaders`):
/// each day of the week and its name (Calendar DD1).
pub fn weekday_headers(locale: &str, week_starts_on: Weekday) -> Vec<(Weekday, String)> {
    let names = weekday_header_names(locale);
    (0..7)
        .map(|offset| {
            let day = Weekday::from_index(week_starts_on.index() + offset);
            (day, names[day.index() as usize].clone())
        })
        .collect()
}

/// A focus-date callback.
pub type FocusDateFn = Rc<dyn Fn(PlainDate, &mut Window, &mut App)>;

/// What [`use_calendar_navigation`] steps, upstream
/// `UseCalendarNavigationOptions`.
#[derive(Clone, Default)]
pub struct CalendarNavigationOptions {
    /// The date whose month opens when uncontrolled.
    pub initial_value: Option<PlainDate>,
    /// The visible month, controlled with `on_focus_date_change`.
    pub focus_date: Option<PlainDate>,
    /// Called when navigation changes the visible month.
    pub on_focus_date_change: Option<FocusDateFn>,
    /// One or two months (anything else is one).
    pub number_of_months: u8,
    /// The calendar whose months the hook steps and labels.
    pub calendar: CalendarSystem,
}

#[derive(Clone, Copy, Default)]
struct NavigationState {
    focus_date: Option<PlainDate>,
    pending_focus: Option<PlainDate>,
}

/// Month navigation, upstream `UseCalendarNavigationReturn`.
#[derive(Clone)]
pub struct CalendarNavigation {
    /// The Gregorian first day of the focus month.
    pub base_month: PlainDate,
    /// The Gregorian first day of each visible month.
    pub visible_months: Vec<PlainDate>,
    /// Each visible month in `calendar`'s numbers, as [`use_calendar_days`]
    /// reads them.
    pub visible_calendar_months: Vec<CalendarYearMonth>,
    /// The month header label, in `calendar`.
    pub month_year_label: String,
    /// The day keyboard focus moves to after a navigation.
    pub pending_focus: Option<PlainDate>,
    state: gpui::Entity<NavigationStateCell>,
    shown: Vec<CalendarMonth>,
    calendar: CalendarSystem,
    controlled: bool,
    on_focus_date_change: Option<FocusDateFn>,
}

/// The navigation's stored state.
pub struct NavigationStateCell(NavigationState);

impl CalendarNavigation {
    /// Steps the visible months by `delta` (upstream `navigateMonth`). With a
    /// focused day, the day `delta * offset` days away (1 horizontally, 7
    /// vertically) becomes the pending focus.
    pub fn navigate_month(
        &self,
        delta: i32,
        focused_date: Option<PlainDate>,
        offset: Option<i64>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(first) = self.shown.first() else {
            return;
        };
        let new_month = calendar_add_months(self.calendar, first, delta).start;
        let pending = focused_date.map(|date| date.add_days(delta as i64 * offset.unwrap_or(7)));
        let controlled = self.controlled;
        self.state.update(cx, |cell, _| {
            if let Some(pending) = pending {
                cell.0.pending_focus = Some(pending);
            }
            if !controlled {
                cell.0.focus_date = Some(new_month);
            }
        });
        if let Some(f) = &self.on_focus_date_change {
            f(new_month, window, cx);
        }
        window.refresh();
    }

    /// Clears the pending focus (upstream `clearPendingFocus`).
    pub fn clear_pending_focus(&self, cx: &mut App) {
        self.state.update(cx, |cell, _| cell.0.pending_focus = None);
    }
}

/// Month navigation (upstream `useCalendarNavigation`): the visible months,
/// their label, stepping, and the pending focus across months. `id` keys its
/// state.
pub fn use_calendar_navigation(
    id: impl Into<ElementId>,
    options: CalendarNavigationOptions,
    window: &mut Window,
    cx: &mut App,
) -> CalendarNavigation {
    let initial = options.focus_date.or(options.initial_value);
    let state = window.use_keyed_state(id.into(), cx, move |_, _| {
        NavigationStateCell(NavigationState {
            focus_date: Some(initial.unwrap_or_else(PlainDate::today)),
            pending_focus: None,
        })
    });
    let controlled = options.focus_date.is_some() && options.on_focus_date_change.is_some();
    let stored = state.read(cx).0;
    let focus = if controlled {
        options.focus_date.unwrap_or_else(PlainDate::today)
    } else {
        stored.focus_date.unwrap_or_else(PlainDate::today)
    };
    let calendar = options.calendar;
    let count = if options.number_of_months == 2 { 2 } else { 1 };
    let first = calendar_month_of(calendar, focus);
    let shown: Vec<CalendarMonth> = (0..count)
        .map(|i| calendar_add_months(calendar, &first, i))
        .collect();
    let locale = astryx_i18n::use_locale(cx);
    CalendarNavigation {
        base_month: shown[0].start,
        visible_months: shown.iter().map(|m| m.start).collect(),
        visible_calendar_months: shown
            .iter()
            .map(|m| calendar_year_month_of(calendar, m))
            .collect(),
        month_year_label: shown
            .iter()
            .map(|m| format_calendar_month_label(calendar, m, &locale, None))
            .collect::<Vec<_>>()
            .join(" – "),
        pending_focus: stored.pending_focus,
        state,
        shown,
        calendar,
        controlled,
        on_focus_date_change: options.on_focus_date_change,
    }
}

/// A date constraint: `true` when the date may be chosen.
pub type DateConstraint = Rc<dyn Fn(PlainDate) -> bool>;

/// Bounds and rules for [`use_calendar_constraints`], upstream
/// `UseCalendarConstraintsOptions`.
#[derive(Clone, Default)]
pub struct CalendarConstraintsOptions {
    pub min: Option<PlainDate>,
    pub max: Option<PlainDate>,
    /// A date is disabled when any rule returns `false`.
    pub date_constraints: Vec<DateConstraint>,
    /// The most days a range may span, both ends counted, from the anchor.
    pub max_range_span: Option<u32>,
    /// The fewest days a range must span, both ends counted, from the anchor.
    pub min_range_span: Option<u32>,
    /// The in-progress range start the spans count from.
    pub range_anchor: Option<PlainDate>,
}

/// Date validation, upstream `UseCalendarConstraintsReturn`.
#[derive(Clone)]
pub struct CalendarConstraints {
    options: Rc<CalendarConstraintsOptions>,
    /// The minimum, if any.
    pub min_date: Option<PlainDate>,
    /// The maximum, if any.
    pub max_date: Option<PlainDate>,
}

impl CalendarConstraints {
    /// Whether a date may not be chosen (upstream `isDateDisabled`): outside
    /// the bounds, beyond a span from the anchor, or rejected by a rule.
    pub fn is_date_disabled(&self, date: PlainDate) -> bool {
        let o = &self.options;
        if o.min.is_some_and(|min| date < min) || o.max.is_some_and(|max| date > max) {
            return true;
        }
        if let Some(anchor) = o.range_anchor {
            let distance = anchor.days_until(date).unsigned_abs();
            if o.max_range_span
                .is_some_and(|span| distance > (span as u64).saturating_sub(1))
            {
                return true;
            }
            // The anchor itself is never disabled by the minimum.
            if o.min_range_span
                .is_some_and(|span| distance > 0 && distance < (span as u64).saturating_sub(1))
            {
                return true;
            }
        }
        o.date_constraints.iter().any(|rule| !rule(date))
    }
}

/// Date validation (upstream `useCalendarConstraints`): bounds, range spans
/// from an anchor, and custom rules in one `is_date_disabled`.
pub fn use_calendar_constraints(options: CalendarConstraintsOptions) -> CalendarConstraints {
    CalendarConstraints {
        min_date: options.min,
        max_date: options.max,
        options: Rc::new(options),
    }
}

/// What [`initial_focus_date`] weighs (`InitialFocusDateOptions`).
pub(crate) struct InitialFocus {
    pub focus_date: Option<PlainDate>,
    /// The selected date, or a range's start.
    pub value: Option<PlainDate>,
    pub min: Option<PlainDate>,
    pub max: Option<PlainDate>,
    pub number_of_months: u8,
    pub today: PlainDate,
    pub calendar: CalendarSystem,
}

/// The date whose month the calendar opens on (`getInitialFocusDate`): the
/// focus date, else the value, else today clamped into the bounds; a later
/// pane never lies wholly past `max`.
pub(crate) fn initial_focus_date(o: &InitialFocus) -> PlainDate {
    if let Some(focus) = o.focus_date {
        return focus;
    }
    if let Some(value) = o.value {
        return keep_last_pane_within_max(value, o);
    }
    if let Some(min) = o.min
        && o.today < min
    {
        return min;
    }
    if let Some(max) = o.max
        && o.today > max
    {
        let offset = o.number_of_months.saturating_sub(1) as i32;
        let shifted =
            calendar_add_months(o.calendar, &calendar_month_of(o.calendar, max), -offset).start;
        if let Some(min) = o.min
            && shifted < calendar_month_of(o.calendar, min).start
        {
            return min;
        }
        return shifted;
    }
    keep_last_pane_within_max(o.today, o)
}

fn keep_last_pane_within_max(date: PlainDate, o: &InitialFocus) -> PlainDate {
    let offset = o.number_of_months.saturating_sub(1) as i32;
    let Some(max) = o.max.filter(|_| offset > 0) else {
        return date;
    };
    let month = calendar_month_of(o.calendar, date).start;
    let max_month = calendar_month_of(o.calendar, max);
    let latest_first = calendar_add_months(o.calendar, &max_month, -offset).start;
    if month <= latest_first || month > max_month.start {
        return date;
    }
    let Some(min) = o.min else {
        return latest_first;
    };
    let min_month = calendar_month_of(o.calendar, min).start;
    if latest_first >= min_month {
        return latest_first;
    }
    if month > min_month { min } else { date }
}

/// Everything a day cell shows and exposes (`DayCellState`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DayCellState {
    pub effectively_disabled: bool,
    pub is_today: bool,
    pub is_selected: bool,
    pub is_in_range: bool,
    pub is_range_start: bool,
    pub is_range_end: bool,
    pub is_in_preview: bool,
    pub is_preview_start: bool,
    pub is_preview_end: bool,
    pub is_first_column: bool,
    pub is_last_column: bool,
}

impl DayCellState {
    /// A selection endpoint: selected, or a range's start or end.
    pub fn is_endpoint(&self) -> bool {
        self.is_selected || self.is_range_start || self.is_range_end
    }
}

/// The spans a day is measured against.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Spans {
    pub range_mode: bool,
    pub selected: Option<PlainDate>,
    pub range: Option<(PlainDate, PlainDate)>,
    pub preview: Option<(PlainDate, PlainDate)>,
    pub today: Option<PlainDate>,
}

/// A day's states (`computeDayCellState`). Outside days never take a
/// selection, range or preview: the same date shows in both panes of a
/// two-month view.
pub(crate) fn day_cell_state(
    day: &CalendarDay,
    index: usize,
    spans: &Spans,
    disabled: bool,
) -> DayCellState {
    let inside = !day.is_outside;
    let date = day.date;
    let within =
        |span: Option<(PlainDate, PlainDate)>| span.is_some_and(|(a, b)| date.is_in_range(a, b));
    DayCellState {
        effectively_disabled: disabled || day.is_outside,
        is_today: inside && spans.today == Some(date),
        is_selected: inside && !spans.range_mode && spans.selected == Some(date),
        is_in_range: inside && spans.range_mode && within(spans.range),
        is_range_start: inside && spans.range_mode && spans.range.is_some_and(|(a, _)| a == date),
        is_range_end: inside && spans.range_mode && spans.range.is_some_and(|(_, b)| b == date),
        is_in_preview: inside && within(spans.preview),
        is_preview_start: inside && spans.preview.is_some_and(|(a, _)| a == date),
        is_preview_end: inside && spans.preview.is_some_and(|(_, b)| b == date),
        is_first_column: index == 0,
        is_last_column: index == 6,
    }
}

/// Where a highlight run rounds its ends (`computeHighlightRounding`).
pub(crate) fn rounding(
    is_start: bool,
    is_end: bool,
    state: &DayCellState,
    prev: bool,
    next: bool,
) -> (bool, bool) {
    (
        is_start || state.is_first_column || !prev,
        is_end || state.is_last_column || !next,
    )
}

/// Whether a neighbour continues the range and the preview
/// (`computeDayNeighborContinuity`): a disabled or outside day breaks a run.
pub(crate) fn neighbors(
    week: &[CalendarDay],
    index: usize,
    spans: &Spans,
    is_disabled: &dyn Fn(PlainDate) -> bool,
) -> [bool; 4] {
    let lit = |day: Option<&CalendarDay>, span: Option<(PlainDate, PlainDate)>, range: bool| {
        day.is_some_and(|d| {
            !d.is_outside
                && (!range || spans.range_mode)
                && !is_disabled(d.date)
                && span.is_some_and(|(a, b)| d.date.is_in_range(a, b))
        })
    };
    let prev = index.checked_sub(1).and_then(|i| week.get(i));
    let next = week.get(index + 1);
    [
        lit(prev, spans.range, true),
        lit(next, spans.range, true),
        lit(prev, spans.preview, false),
        lit(next, spans.preview, false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u8, day: u8) -> PlainDate {
        PlainDate::new(y, m, day).unwrap()
    }

    #[test]
    fn fr11_days_lay_out_gregorian_months_without_a_calendar() {
        let days = calendar_days(
            CalendarDaysOptions {
                year: 2026,
                month: 1,
                ..Default::default()
            },
            "en",
        );
        assert_eq!(days.total_cells, 42);
        // January 1, 2026 is a Thursday.
        assert_eq!(days.days[4].date, d(2026, 1, 1));
        assert!(days.days[3].is_outside);
        assert_eq!(days.day_names[0], "Sun");
        let variable = calendar_days(
            CalendarDaysOptions {
                year: 2026,
                month: 2,
                has_variable_row_count: true,
                ..Default::default()
            },
            "en",
        );
        // February 2026 starts on a Sunday and has 28 days: four rows.
        assert_eq!(variable.weeks.len(), 4);
    }

    #[test]
    fn fr11_days_read_calendar_numbers_with_a_calendar() {
        let aban = calendar_days(
            CalendarDaysOptions {
                year: 1405,
                month: 8,
                calendar: CalendarSystem::Persian,
                ..Default::default()
            },
            "en",
        );
        let first = aban.days.iter().find(|d| !d.is_outside).unwrap();
        assert_eq!(first.date, d(2026, 10, 23));
        assert_eq!(first.day_number, 1);
        // Month 13 exists in a Hebrew leap year.
        let adar2 = calendar_days(
            CalendarDaysOptions {
                year: 5784,
                month: 13,
                calendar: CalendarSystem::Hebrew,
                ..Default::default()
            },
            "en",
        );
        assert!(adar2.days.iter().any(|d| !d.is_outside));
    }

    #[test]
    fn constraints_combine_bounds_spans_and_rules() {
        let weekdays: DateConstraint = Rc::new(|date: PlainDate| {
            !matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday)
        });
        let c = use_calendar_constraints(CalendarConstraintsOptions {
            min: Some(d(2026, 1, 5)),
            max: Some(d(2026, 2, 27)),
            date_constraints: vec![weekdays],
            max_range_span: Some(7),
            min_range_span: Some(3),
            range_anchor: Some(d(2026, 1, 14)),
        });
        assert!(c.is_date_disabled(d(2026, 1, 4)));
        assert!(c.is_date_disabled(d(2026, 1, 17)), "a Saturday");
        assert!(!c.is_date_disabled(d(2026, 1, 14)), "the anchor stays");
        assert!(
            c.is_date_disabled(d(2026, 1, 15)),
            "inside the minimum span"
        );
        assert!(!c.is_date_disabled(d(2026, 1, 16)));
        assert!(!c.is_date_disabled(d(2026, 1, 20)));
        assert!(c.is_date_disabled(d(2026, 1, 21)), "past the maximum span");
    }

    #[test]
    fn the_opening_month_follows_upstream() {
        let base = InitialFocus {
            focus_date: None,
            value: None,
            min: None,
            max: None,
            number_of_months: 1,
            today: d(2026, 11, 25),
            calendar: CalendarSystem::Gregory,
        };
        assert_eq!(initial_focus_date(&base), d(2026, 11, 25));
        let before = InitialFocus {
            min: Some(d(2027, 3, 1)),
            ..base
        };
        assert_eq!(initial_focus_date(&before), d(2027, 3, 1));
        let after = InitialFocus {
            max: Some(d(2019, 6, 30)),
            number_of_months: 2,
            ..base
        };
        assert_eq!(initial_focus_date(&after), d(2019, 5, 1));
        let at_max = InitialFocus {
            max: Some(d(2026, 11, 25)),
            number_of_months: 2,
            ..base
        };
        assert_eq!(initial_focus_date(&at_max), d(2026, 10, 1));
        let valued = InitialFocus {
            value: Some(d(2024, 2, 10)),
            ..base
        };
        assert_eq!(initial_focus_date(&valued), d(2024, 2, 10));
    }
}
