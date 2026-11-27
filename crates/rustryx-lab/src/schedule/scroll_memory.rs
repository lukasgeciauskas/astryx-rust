//! Where a time-grid viewport opens for a range, and the memory that keeps a person's
//! scroll offset for that range across the suspended fallback (upstream
//! `timeGridScrollMemory.ts`). Schedule owns the memory, so the loading view and the loaded
//! view share one record.

use gpui::SharedString;
use rustryx_i18n::date::PlainDate;

use super::date_math::{get_minutes_since_start_of_day, plain_date_from_instant};
use super::types::Instant;

/// One intentional open of a time grid (upstream `TimeGridRangeKey`): the range's bounds,
/// the zone, and the hour window the offset was computed for. Object identities are
/// deliberately absent, so a caller that rebuilds its view on every render cannot turn a
/// data refresh into a new open.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeGridRangeKey {
    pub start: Instant,
    pub end: Instant,
    pub timezone_id: SharedString,
    pub min_hour: f64,
    pub max_hour: f64,
    pub hour_height: f64,
}

/// The viewport's last known offset for its range (upstream `TimeGridScrollMemory`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TimeGridScrollMemory {
    pub key: Option<TimeGridRangeKey>,
    pub offset: f64,
}

/// Upstream `isSameRangeKey`.
pub fn is_same_range_key(a: &TimeGridRangeKey, b: &TimeGridRangeKey) -> bool {
    a.start == b.start
        && a.end == b.end
        && a.timezone_id == b.timezone_id
        && a.min_hour == b.min_hour
        && a.max_hour == b.max_hour
        && a.hour_height == b.hour_height
}

/// The offset a time-grid viewport opens at for a range it has not shown yet (upstream
/// `getInitialTimeGridOffset`): one hour above the current minute when the range's days
/// include today, so what just happened and what is next are both in view; otherwise the
/// top of the configured hour window. Clamped to what the viewport can scroll.
#[allow(clippy::too_many_arguments)]
pub fn get_initial_time_grid_offset(
    current_time: Instant,
    days: &[PlainDate],
    timezone_id: &str,
    min_hour: f64,
    hour_height: f64,
    scroll_height: f64,
    client_height: f64,
) -> f64 {
    let max_offset = (scroll_height - client_height).max(0.);
    let today = plain_date_from_instant(current_time, timezone_id);
    if !days.contains(&today) {
        return 0.;
    }
    let current_minute = get_minutes_since_start_of_day(current_time, timezone_id);
    // The now-line sits at `<minutes since min_hour>% + 2px` inside the column; the hour
    // above it is the lead.
    let now_offset = ((current_minute - min_hour * 60.) / 60.) * hour_height + 2.;
    max_offset.min(js_round(now_offset - hour_height).max(0.))
}

/// JavaScript's `Math.round`: the nearest integer, halves toward positive infinity.
pub(crate) fn js_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 { floor + 1. } else { floor }
}
