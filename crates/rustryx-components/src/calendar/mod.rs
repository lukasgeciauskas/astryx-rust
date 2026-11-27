//! Calendar and its hooks. Upstream: `Calendar/` (component spec
//! `Calendar.spec.md`, system spec AST-111).
//!
//! - [`Calendar`]: one or two month grids for a date or a range.
//! - [`use_calendar_days`], [`use_calendar_navigation`] and
//!   [`use_calendar_constraints`]: the public hooks, for a custom month view.

#[allow(clippy::module_inception)]
mod calendar;
pub mod examples;
mod hooks;

#[cfg(test)]
mod tests;

pub use calendar::{Calendar, CalendarHandle, CalendarMode, CalendarState, DateFn, RangeFn};
pub use hooks::{
    CalendarConstraints, CalendarConstraintsOptions, CalendarDay, CalendarDays,
    CalendarDaysOptions, CalendarNavigation, CalendarNavigationOptions, DateConstraint,
    FocusDateFn, NavigationStateCell, calendar_days, use_calendar_constraints, use_calendar_days,
    use_calendar_navigation, weekday_headers,
};

#[allow(unused_imports)]
pub(crate) use calendar::day_selector;
#[allow(unused_imports)]
pub(crate) use hooks::{InitialFocus, initial_focus_date};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const CALENDAR_DOC: ComponentDoc = ComponentDoc {
    name: "Calendar",
    rust_type: "rustryx::components::calendar::Calendar",
    props: &[
        p("mode", RustApi::Method("mode")),
        p("value", RustApi::Method("value")),
        p("defaultValue", RustApi::Method("default_value")),
        p("onChange", RustApi::Method("on_change")),
        p("numberOfMonths", RustApi::Method("number_of_months")),
        p("min", RustApi::Method("min")),
        p("max", RustApi::Method("max")),
        p("dateConstraints", RustApi::Method("date_constraints")),
        p("maxRangeSpan", RustApi::Method("max_range_span")),
        p("minRangeSpan", RustApi::Method("min_range_span")),
        p("focusDate", RustApi::Method("focus_date")),
        p("onFocusDateChange", RustApi::Method("on_focus_date_change")),
        p("handleRef", RustApi::Method("handle")),
        p("hasOutsideDays", RustApi::Method("has_outside_days")),
        p("hasWeekNumbers", RustApi::Method("has_week_numbers")),
        p(
            "hasVariableRowCount",
            RustApi::Method("has_variable_row_count"),
        ),
        p("weekStartsOn", RustApi::Method("week_starts_on")),
        p("calendar", RustApi::Method("calendar")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_CALENDAR_DAYS_DOC: ComponentDoc = ComponentDoc {
    name: "useCalendarDays",
    rust_type: "rustryx::components::calendar::use_calendar_days",
    props: &[
        p("options", RustApi::Argument("options: CalendarDaysOptions")),
        p("options.year", RustApi::Argument("year")),
        p("options.month", RustApi::Argument("month")),
        p("options.weekStartsOn", RustApi::Argument("week_starts_on")),
        p(
            "options.hasVariableRowCount",
            RustApi::Argument("has_variable_row_count"),
        ),
        p("options.calendar", RustApi::Argument("calendar")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_CALENDAR_NAVIGATION_DOC: ComponentDoc = ComponentDoc {
    name: "useCalendarNavigation",
    rust_type: "rustryx::components::calendar::use_calendar_navigation",
    props: &[
        p(
            "options",
            RustApi::Argument("options: CalendarNavigationOptions"),
        ),
        p("options.initialValue", RustApi::Argument("initial_value")),
        p("options.focusDate", RustApi::Argument("focus_date")),
        p(
            "options.onFocusDateChange",
            RustApi::Argument("on_focus_date_change"),
        ),
        p(
            "options.numberOfMonths",
            RustApi::Argument("number_of_months"),
        ),
        p("options.calendar", RustApi::Argument("calendar")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_CALENDAR_CONSTRAINTS_DOC: ComponentDoc = ComponentDoc {
    name: "useCalendarConstraints",
    rust_type: "rustryx::components::calendar::use_calendar_constraints",
    props: &[
        p(
            "options",
            RustApi::Argument("options: CalendarConstraintsOptions"),
        ),
        p("options.min", RustApi::Argument("min")),
        p("options.max", RustApi::Argument("max")),
        p(
            "options.dateConstraints",
            RustApi::Argument("date_constraints"),
        ),
        p("options.maxRangeSpan", RustApi::Argument("max_range_span")),
        p("options.minRangeSpan", RustApi::Argument("min_range_span")),
        p("options.rangeAnchor", RustApi::Argument("range_anchor")),
    ],
    example: include_str!("examples.rs"),
};
