//! The month's chip layout (upstream `monthLayout.ts`, component:Schedule FR15 to FR16, ORD4,
//! PR4): chip placements by week and level, cut around busy days, and each busy day's
//! "+N more" count. Pure; runs during render.

use std::cmp::Ordering;

use rustryx_i18n::date::PlainDate;

use super::date_math::{js_max, plain_date_from_instant, plain_date_to_instant};
use super::types::{CalendarEvent, EventTimes};

/// A week row paints at most this many levels of chips (upstream
/// `MONTH_VISIBLE_LEVELS`).
pub const MONTH_VISIBLE_LEVELS: usize = 3;

const DAYS_PER_WEEK: usize = 7;

/// One painted chip: an event's run of days on one level of one week row (upstream
/// `MonthChipPlacement`).
#[derive(Clone, Debug, PartialEq)]
pub struct MonthChipPlacement {
    pub event: CalendarEvent,
    pub week: usize,
    pub column_start: usize,
    pub column_end: usize,
    pub level: usize,
}

/// A day whose events need more levels than its week row paints (upstream
/// `MonthDayOverflow`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonthDayOverflow {
    pub day_index: usize,
    pub week: usize,
    pub column: usize,
    /// That day's events without a painted chip on that day.
    pub count: usize,
}

/// The month's layout (upstream `MonthLayout`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MonthLayout {
    pub chips: Vec<MonthChipPlacement>,
    pub overflow: Vec<MonthDayOverflow>,
}

struct RangedEvent {
    event: CalendarEvent,
    start_index: usize,
    end_index: usize,
    is_priority: bool,
    /// The event's start instant; a date-only event starts at its first midnight.
    start: f64,
}

/// JavaScript's string `<`: UTF-16 code unit order.
pub(crate) fn code_unit_order(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Lays out the month (upstream `layoutMonthEvents`): every event takes, in each week it
/// covers, the lowest level free on all its days there. A day with an event on level
/// four or higher gives its third level to "+N more", which counts that day's events on
/// level three or higher; a third-level chip that crosses such a day is painted on its
/// other days only. Every event of a day is therefore painted on that day or counted
/// there, and no chip sits below the third level.
pub fn layout_month_events(
    events: &[CalendarEvent],
    days: &[PlainDate],
    timezone_id: &str,
    compare: &dyn Fn(&str, &str) -> Ordering,
) -> MonthLayout {
    let (Some(first_day), Some(last_day)) = (days.first().copied(), days.last().copied()) else {
        return MonthLayout::default();
    };
    let mut ranged: Vec<RangedEvent> = events
        .iter()
        .filter_map(|event| {
            let (event_start, event_end) = get_event_date_span(event, timezone_id);
            if event_end < first_day || event_start > last_day {
                return None;
            }
            let start_index = days.iter().position(|day| !(*day < event_start))?;
            let mut end_index = days.len() - 1;
            while end_index > 0 && days[end_index] > event_end {
                end_index -= 1;
            }
            if end_index < start_index {
                return None;
            }
            Some(RangedEvent {
                event: event.clone(),
                start_index,
                end_index,
                is_priority: event.is_day_event() || end_index > start_index,
                start: match event.times {
                    EventTimes::Day { start, .. } => {
                        plain_date_to_instant(start, timezone_id, 0., 0.)
                    }
                    EventTimes::Instant { start, .. } => start,
                },
            })
        })
        .collect();
    ranged.sort_by(|a, b| compare_ranged_events(a, b, compare));

    // Levels, per week, by the last column each level is occupied to. Events arrive in
    // first-day order, so each week sees its runs in start order and a level whose last
    // run ends before this run starts is free on every day of it.
    let mut levels_by_week: Vec<Vec<usize>> = Vec::new();
    let mut placements: Vec<MonthChipPlacement> = Vec::new();
    for r in &ranged {
        let first_week = r.start_index / DAYS_PER_WEEK;
        let last_week = r.end_index / DAYS_PER_WEEK;
        for week in first_week..=last_week {
            let column_start = if week == first_week {
                r.start_index % DAYS_PER_WEEK
            } else {
                0
            };
            let column_end = if week == last_week {
                r.end_index % DAYS_PER_WEEK
            } else {
                DAYS_PER_WEEK - 1
            };
            if levels_by_week.len() <= week {
                levels_by_week.resize(week + 1, Vec::new());
            }
            let week_levels = &mut levels_by_week[week];
            let level = week_levels
                .iter()
                .position(|occupied_to| *occupied_to < column_start)
                .unwrap_or(week_levels.len());
            if level == week_levels.len() {
                week_levels.push(column_end);
            } else {
                week_levels[level] = column_end;
            }
            placements.push(MonthChipPlacement {
                event: r.event.clone(),
                week,
                column_start,
                column_end,
                level,
            });
        }
    }

    let last_painted_level = MONTH_VISIBLE_LEVELS - 1;
    let mut is_busy = vec![false; days.len()];
    let mut counted_on_day = vec![0usize; days.len()];
    for placement in &placements {
        for column in placement.column_start..=placement.column_end {
            let day_index = placement.week * DAYS_PER_WEEK + column;
            if placement.level > last_painted_level {
                is_busy[day_index] = true;
            }
            if placement.level >= last_painted_level {
                counted_on_day[day_index] += 1;
            }
        }
    }

    let mut chips = Vec::new();
    for placement in &placements {
        if placement.level < last_painted_level {
            chips.push(placement.clone());
        } else if placement.level == last_painted_level {
            // The third level belongs to "+N more" on a busy day, so the chip is cut into
            // the runs of days around it.
            let mut run_start: Option<usize> = None;
            for column in placement.column_start..=placement.column_end + 1 {
                let is_free = column <= placement.column_end
                    && !is_busy[placement.week * DAYS_PER_WEEK + column];
                if is_free && run_start.is_none() {
                    run_start = Some(column);
                } else if !is_free && let Some(start) = run_start {
                    chips.push(MonthChipPlacement {
                        column_start: start,
                        column_end: column - 1,
                        ..placement.clone()
                    });
                    run_start = None;
                }
            }
        }
    }

    let overflow = is_busy
        .iter()
        .enumerate()
        .filter(|(_, busy)| **busy)
        .map(|(day_index, _)| MonthDayOverflow {
            day_index,
            week: day_index / DAYS_PER_WEEK,
            column: day_index % DAYS_PER_WEEK,
            count: counted_on_day[day_index],
        })
        .collect();

    MonthLayout { chips, overflow }
}

/// First day, then all-day and multi-day events before single-day timed ones, then
/// events covering more days, then earlier starts, then title, then id (upstream
/// `compareRangedEvents`). The layout depends on the data alone, never on input order,
/// and a busy day paints its earliest events.
fn compare_ranged_events(
    a: &RangedEvent,
    b: &RangedEvent,
    compare: &dyn Fn(&str, &str) -> Ordering,
) -> Ordering {
    if a.start_index != b.start_index {
        return a.start_index.cmp(&b.start_index);
    }
    if a.is_priority != b.is_priority {
        return if a.is_priority {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let duration_difference =
        (b.end_index - b.start_index) as i64 - (a.end_index - a.start_index) as i64;
    if duration_difference != 0 {
        return if duration_difference < 0 {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    if a.start != b.start {
        // `a.start - b.start`: NaN compares as equal.
        let d = a.start - b.start;
        if d < 0. {
            return Ordering::Less;
        }
        if d > 0. {
            return Ordering::Greater;
        }
    }
    let title_order = compare(&a.event.title, &b.event.title);
    if title_order != Ordering::Equal {
        return title_order;
    }
    code_unit_order(&a.event.id, &b.event.id)
}

/// The calendar days an event covers in the schedule's zone (upstream
/// `getEventDateSpan`).
pub fn get_event_date_span(event: &CalendarEvent, timezone_id: &str) -> (PlainDate, PlainDate) {
    match event.times {
        EventTimes::Day { start, end } => (start, end),
        EventTimes::Instant { start, end } => (
            plain_date_from_instant(start, timezone_id),
            plain_date_from_instant(js_max(end - 1., start), timezone_id),
        ),
    }
}
