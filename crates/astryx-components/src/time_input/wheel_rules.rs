//! The time wheels' rules, upstream `TimeInput/timeWheelRules.ts`: each
//! column's rows, which rows the bounds disable, what a column's rest makes
//! of the time, and which other columns must turn with it (FR7).

use astryx_i18n::date::{HourFormat, PlainTime, clamp_time};

use crate::wheel::WheelOption;

const SECONDS_PER_DAY: u32 = 24 * 3600;

/// A time wheel column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TimeColumn {
    Hour,
    Minute,
    Second,
    Meridiem,
}

impl TimeColumn {
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Inclusive bounds in seconds since midnight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TimeBounds {
    pub min: Option<u32>,
    pub max: Option<u32>,
}

/// The bounds of a field's `min` and `max`.
pub(crate) fn wheel_bounds(min: Option<PlainTime>, max: Option<PlainTime>) -> TimeBounds {
    TimeBounds {
        min: min.map(PlainTime::seconds_of_day),
        max: max.map(PlainTime::seconds_of_day),
    }
}

/// 1 for the PM half of the day, 0 for AM.
pub(crate) fn meridiem_of(hour: u8) -> i64 {
    i64::from(hour >= 12)
}

/// The 12-hour clock's label for a 24-hour hour: 0 → 12, 13 → 1.
pub(crate) fn hour12_from_24(hour: u8) -> u8 {
    match hour % 12 {
        0 => 12,
        h => h,
    }
}

/// The 24-hour hour for a 12-hour hour in a half of the day.
pub(crate) fn hour24_from_12(hour: u8, meridiem: i64) -> u8 {
    hour % 12 + if meridiem == 1 { 12 } else { 0 }
}

fn time(hour: u8, minute: u8, second: u8) -> PlainTime {
    PlainTime {
        hour,
        minute,
        second,
    }
}

fn parts_of(seconds: u32) -> PlainTime {
    let s = seconds % SECONDS_PER_DAY;
    time((s / 3600) as u8, (s % 3600 / 60) as u8, (s % 60) as u8)
}

fn overlaps(start: u32, end: u32, bounds: &TimeBounds) -> bool {
    bounds.min.is_none_or(|min| end >= min) && bounds.max.is_none_or(|max| start <= max)
}

/// The time the wheels show: seconds dropped without a seconds column. A
/// time outside the bounds shows as it is, on disabled rows.
pub(crate) fn displayed_time(time: PlainTime, seconds: bool) -> PlainTime {
    PlainTime {
        second: if seconds { time.second } else { 0 },
        ..time
    }
}

/// What a column's rest commits: the nearest time inside the bounds that
/// the columns have a row for.
pub(crate) fn resting_time(time: PlainTime, seconds: bool, bounds: &TimeBounds) -> PlainTime {
    clamp_time(
        time,
        bounds.min.map(parts_of),
        bounds.max.map(parts_of),
        seconds,
    )
}

/// The hour the hour column makes resting on `row` from the hour `from`
/// (`hourAfterTurn`): in 12-hour time the half of the day changes only when
/// the move passes between 11 and 12.
pub(crate) fn hour_after_turn(from: u8, row: u8, hour_format: HourFormat) -> u8 {
    if hour_format == HourFormat::H24 {
        return row;
    }
    let shown = hour12_from_24(from);
    if row == shown {
        return from;
    }
    let half = meridiem_of(from);
    let passes = (shown == 12 && row >= 7) || (row == 12 && shown >= 7);
    hour24_from_12(row, if passes { 1 - half } else { half })
}

fn hour_rows_for(
    hour_format: HourFormat,
    hour_of: impl Fn(u8) -> u8,
    bounds: &TimeBounds,
) -> Vec<WheelOption> {
    let rows: Vec<u8> = match hour_format {
        HourFormat::H24 => (0..24).collect(),
        HourFormat::H12 => (1..=12).collect(),
    };
    rows.into_iter()
        .map(|row| {
            let hour = u32::from(hour_of(row));
            let label = match hour_format {
                HourFormat::H24 => format!("{row:02}"),
                HourFormat::H12 => row.to_string(),
            };
            WheelOption::new(i64::from(row), label)
                .disabled(!overlaps(hour * 3600, hour * 3600 + 3599, bounds))
                .order(i64::from(hour))
        })
        .collect()
}

/// The hour column under a finger: a 12-hour row stands for the hour a turn
/// onto it from `from` makes.
pub(crate) fn hour_rows(
    from: u8,
    hour_format: HourFormat,
    bounds: &TimeBounds,
) -> Vec<WheelOption> {
    hour_rows_for(
        hour_format,
        |row| hour_after_turn(from, row, hour_format),
        bounds,
    )
}

/// The hour column under a mouse, whose columns turn one at a time: a
/// 12-hour row stands for its hour in `from`'s half of the day.
pub(crate) fn hour_rows_in_half(
    from: u8,
    hour_format: HourFormat,
    bounds: &TimeBounds,
) -> Vec<WheelOption> {
    let half = meridiem_of(from);
    hour_rows_for(
        hour_format,
        |row| match hour_format {
            HourFormat::H24 => row,
            HourFormat::H12 => hour24_from_12(row, half),
        },
        bounds,
    )
}

/// The minute column for `hour`.
pub(crate) fn minute_rows(hour: u8, seconds: bool, bounds: &TimeBounds) -> Vec<WheelOption> {
    (0..60u32)
        .map(|minute| {
            let start = u32::from(hour) * 3600 + minute * 60;
            WheelOption::new(i64::from(minute), format!("{minute:02}")).disabled(!overlaps(
                start,
                start + if seconds { 59 } else { 0 },
                bounds,
            ))
        })
        .collect()
}

/// The second column for `hour`:`minute`.
pub(crate) fn second_rows(hour: u8, minute: u8, bounds: &TimeBounds) -> Vec<WheelOption> {
    (0..60u32)
        .map(|second| {
            let at = u32::from(hour) * 3600 + u32::from(minute) * 60 + second;
            WheelOption::new(i64::from(second), format!("{second:02}"))
                .disabled(!overlaps(at, at, bounds))
        })
        .collect()
}

/// The AM/PM column: a half is disabled when none of its hours is in bounds.
pub(crate) fn meridiem_rows(am: &str, pm: &str, bounds: &TimeBounds) -> Vec<WheelOption> {
    vec![
        WheelOption::new(0, am.to_string()).disabled(!overlaps(0, 12 * 3600 - 1, bounds)),
        WheelOption::new(1, pm.to_string()).disabled(!overlaps(
            12 * 3600,
            SECONDS_PER_DAY - 1,
            bounds,
        )),
    ]
}

/// The time made by turning `column` to `value` from `from`.
pub(crate) fn turn_column(
    from: PlainTime,
    column: TimeColumn,
    value: i64,
    hour_format: HourFormat,
) -> PlainTime {
    let v = value as u8;
    match column {
        TimeColumn::Hour => PlainTime {
            hour: hour_after_turn(from.hour, v, hour_format),
            ..from
        },
        TimeColumn::Minute => PlainTime { minute: v, ..from },
        TimeColumn::Second => PlainTime { second: v, ..from },
        TimeColumn::Meridiem => PlainTime {
            hour: hour24_from_12(hour12_from_24(from.hour), value),
            ..from
        },
    }
}

/// What a column shows for a time.
pub(crate) fn column_value(time: PlainTime, column: TimeColumn, hour_format: HourFormat) -> i64 {
    match column {
        TimeColumn::Hour => i64::from(match hour_format {
            HourFormat::H24 => time.hour,
            HourFormat::H12 => hour12_from_24(time.hour),
        }),
        TimeColumn::Minute => i64::from(time.minute),
        TimeColumn::Second => i64::from(time.second),
        TimeColumn::Meridiem => meridiem_of(time.hour),
    }
}

/// The columns that must turn after `column` rested on `value`: each whose
/// row for the committed time differs from the row it shows.
pub(crate) fn columns_to_turn(
    from: PlainTime,
    column: TimeColumn,
    value: i64,
    committed: PlainTime,
    hour_format: HourFormat,
    seconds: bool,
) -> Vec<(TimeColumn, i64)> {
    let mut columns = vec![TimeColumn::Hour, TimeColumn::Minute];
    if seconds {
        columns.push(TimeColumn::Second);
    }
    if hour_format == HourFormat::H12 {
        columns.push(TimeColumn::Meridiem);
    }
    columns
        .into_iter()
        .filter_map(|each| {
            let shown = if each == column {
                value
            } else {
                column_value(from, each, hour_format)
            };
            let target = column_value(committed, each, hour_format);
            (shown != target).then_some((each, target))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(h: u8, m: u8) -> PlainTime {
        time(h, m, 0)
    }

    #[test]
    fn fr7_the_hour_column_flips_the_half_only_between_11_and_12() {
        let h12 = HourFormat::H12;
        assert_eq!(hour_after_turn(11, 12, h12), 12, "11 AM → 12 PM");
        assert_eq!(hour_after_turn(12, 1, h12), 13, "12 PM → 1 PM");
        assert_eq!(hour_after_turn(13, 9, h12), 21, "1 PM → 9 PM");
        assert_eq!(hour_after_turn(12, 11, h12), 11, "12 PM → 11 AM");
        assert_eq!(hour_after_turn(23, 12, h12), 0, "11 PM → 12 AM");
        assert_eq!(hour_after_turn(13, 12, h12), 12, "1 PM → 12 PM");
        assert_eq!(hour_after_turn(5, 17, HourFormat::H24), 17);
    }

    #[test]
    fn fr7_rows_outside_the_bounds_are_disabled() {
        let bounds = wheel_bounds(Some(t(9, 0)), Some(t(17, 30)));
        let hours = hour_rows(10, HourFormat::H24, &bounds);
        assert!(
            hours[8].disabled && !hours[9].disabled && !hours[17].disabled && hours[18].disabled
        );
        let minutes = minute_rows(17, false, &bounds);
        assert!(!minutes[30].disabled && minutes[31].disabled);
        let half = meridiem_rows("AM", "PM", &wheel_bounds(Some(t(13, 0)), None));
        assert!(half[0].disabled && !half[1].disabled);
    }

    #[test]
    fn fr7_a_rest_past_a_bound_commits_the_bound() {
        let bounds = wheel_bounds(
            None,
            Some(PlainTime {
                hour: 16,
                minute: 55,
                second: 30,
            }),
        );
        assert_eq!(resting_time(t(17, 10), false, &bounds), t(16, 55));
        assert_eq!(resting_time(t(12, 0), false, &bounds), t(12, 0));
    }

    #[test]
    fn fr7_an_hour_across_11_and_12_turns_the_meridiem_column() {
        let from = t(11, 30);
        let next = turn_column(from, TimeColumn::Hour, 12, HourFormat::H12);
        assert_eq!(next, t(12, 30));
        let turns = columns_to_turn(from, TimeColumn::Hour, 12, next, HourFormat::H12, false);
        assert_eq!(turns, [(TimeColumn::Meridiem, 1)]);
    }

    #[test]
    fn a_12_hour_mouse_column_keeps_its_half() {
        let rows = hour_rows_in_half(14, HourFormat::H12, &TimeBounds::default());
        assert_eq!(rows[0].order, Some(13), "1 means 1 PM");
        assert_eq!(rows[11].order, Some(12), "12 means noon");
    }
}
