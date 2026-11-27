//! Upstream lab `Schedule`: a read-only calendar surface that renders events as a month grid,
//! a day or week time grid, or a list grouped by day, through a view object; with
//! timezone-aware date math, paging, async loading and header plugins.

mod context;
mod current_time;
mod date_math;
mod format;
mod month_layout;
mod monthly_view;
mod plugins;
mod scroll_memory;
mod shared;
mod shell;
mod time_grid_layout;
mod time_grid_view;
mod types;
mod week_views;
mod zoned;

#[cfg(test)]
mod tests;

/// The gallery's Schedule examples (upstream's stories).
pub mod examples;

pub use astryx_i18n::date::PlainDate;
pub use context::{ScheduleContext, ScheduleContextValue, use_schedule_context};
pub use format::ScheduleFormat;
pub use monthly_view::{ScheduleMonthlyViewOptions, create_schedule_monthly_view};
pub use plugins::{
    SchedulePaginationPluginOptions, ScheduleViewSelectorOption, ScheduleViewSelectorPluginOptions,
    default_schedule_plugins, schedule_pagination_plugin, schedule_view_selector_plugin,
};
pub use shell::Schedule;
pub use types::{
    CalendarEvent, EventTimes, Instant, RenderPopover, ScheduleCategory, ScheduleDate,
    ScheduleEventColor, ScheduleEventSource, ScheduleHeaderContent, ScheduleLoad, ScheduleLoader,
    ScheduleNavigationRange, SchedulePlugin, SchedulePluginPosition, ScheduleRange,
    ScheduleTimeGridViewOptions, ScheduleView, ScheduleViewImpl, create_event_from_iso,
};
pub use week_views::{
    ScheduleDayViewOptions, ScheduleListViewOptions, ScheduleWeeklyViewOptions,
    create_schedule_day_view, create_schedule_list_view, create_schedule_weekly_view,
};
pub use zoned::{ZonedDateTime, ZonedDateTimeRange};

use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `Schedule`.
pub const SCHEDULE_DOC: ComponentDoc = ComponentDoc {
    name: "Schedule",
    rust_type: "astryx::lab::schedule::Schedule",
    props: &[
        p(
            "view",
            RustApi::Argument("view (a create_schedule_*_view factory's ScheduleView)"),
        ),
        p(
            "events",
            RustApi::Argument("events (ScheduleEventSource: events or a loader)"),
        ),
        p("categories", RustApi::Method("categories")),
        p("date", RustApi::Argument("date")),
        p("focusDate", RustApi::Method("focus_date")),
        p("onChangeDate", RustApi::Method("on_change_date")),
        p("timezoneID", RustApi::Method("timezone_id")),
        p("plugins", RustApi::Method("plugins")),
        p("headingLevel", RustApi::Method("heading_level")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
