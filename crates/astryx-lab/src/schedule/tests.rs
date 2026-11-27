//! Schedule's tests: upstream `monthLayout.test.ts`, `timeGridLayout.test.ts`,
//! `TimeGridView.test.tsx`, `useCurrentTime.test.ts`, `Schedule.test.tsx` and
//! `Schedule.calendarSystems.test.tsx`. The views record what they draw (the accessible names,
//! the hidden grid, the chips) for these tests, where upstream queries the DOM.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::Duration;

use astryx_i18n::date::{DateStyle, PlainDate};
use astryx_i18n::{CalendarSystem, I18n};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Entity, Modifiers, Render, TestAppContext, VisualTestContext, Window,
    div, px,
};

use super::context::ScheduleContextValue;
use super::current_time::set_test_now;
use super::date_math::{enumerate_dates, sort_events};
use super::month_layout::{MonthLayout, layout_month_events};
use super::monthly_view::{MONTHS, MonthDrawn};
use super::scroll_memory::get_initial_time_grid_offset;
use super::time_grid_layout::{TimedEventInterval, TimedEventPlacement, layout_timed_events};
use super::time_grid_view::{GRIDS, GridDrawn};
use super::week_views::{LISTS, ListDrawn};
use super::*;

// ---- fixtures -------------------------------------------------------------------

fn d(year: i32, month: u8, day: u8) -> PlainDate {
    PlainDate { year, month, day }
}

/// `Date.UTC(year, month - 1, day, hour, minute)`.
fn utc(year: i32, month: u8, day: u8, hour: u32, minute: u32) -> Instant {
    d(year, month, day).to_epoch_days() as f64 * 86_400_000.
        + f64::from(hour) * 3_600_000.
        + f64::from(minute) * 60_000.
}

fn iso(id: &str, title: &str, category: Option<&str>, start: &str, end: &str) -> CalendarEvent {
    create_event_from_iso(
        id.to_string(),
        title.to_string(),
        category.map(|c| c.to_string().into()),
        start,
        end,
    )
    .unwrap()
}

fn en() -> ScheduleFormat {
    ScheduleFormat::new(I18n::new("en-US"))
}

fn fmt(locale: &str, calendar: &str) -> ScheduleFormat {
    let calendar = CalendarSystem::from_identifier(calendar).unwrap_or_default();
    ScheduleFormat::new(I18n::new(locale.to_string()).calendar(calendar))
}

// ---- createEventFromISO, sortEvents, formatting ------------------------------------

#[test]
fn creates_all_day_plain_date_events_from_date_only_iso_strings() {
    let event = iso(
        "planning",
        "Planning offsite",
        Some("Planning"),
        "2026-05-13",
        "2026-05-14",
    );
    assert_eq!(
        event,
        CalendarEvent::day(
            "planning",
            "Planning offsite",
            d(2026, 5, 13),
            d(2026, 5, 14)
        )
        .category("Planning")
    );
}

#[test]
fn creates_instant_events_from_date_time_iso_strings() {
    let event = iso(
        "standup",
        "Standup",
        None,
        "2026-05-13T16:00:00.000Z",
        "2026-05-13T16:30:00.000Z",
    );
    assert_eq!(
        event.times,
        EventTimes::Instant {
            start: utc(2026, 5, 13, 16, 0),
            end: utc(2026, 5, 13, 16, 30)
        }
    );
    // An unparsable instant is NaN, as `Date.parse` makes it; a date that names no day fails.
    let bad = iso("x", "X", None, "tomorrow", "2026-05-13T16:00:00Z");
    assert!(matches!(bad.times, EventTimes::Instant { start, .. } if start.is_nan()));
    assert!(create_event_from_iso("x", "X", None, "2026-02-30", "2026-03-01").is_err());
}

#[test]
fn keeps_gregorian_fields_when_the_calendar_is_gregorian() {
    assert_eq!(
        fmt("en", "gregory").with_plain_date(d(2026, 8, 22), DateStyle::YEAR),
        "2026"
    );
}

#[test]
fn formats_gregorian_dates_with_the_requested_locale() {
    // `new Intl.DateTimeFormat('th-TH', {month: 'long', year: 'numeric', calendar: 'gregory'})`
    // writes "สิงหาคม 2026"; ICU4X's data adds the era to a Gregorian year in Thai, whose
    // default calendar is Buddhist ("สิงหาคม ค.ศ. 2026").
    let title = fmt("th-TH", "gregory").month_title(d(2026, 8, 22));
    assert!(
        title.starts_with("สิงหาคม") && title.ends_with("2026"),
        "{title}"
    );
    assert_eq!(
        fmt("fr-FR", "gregory").month_title(d(2026, 8, 22)),
        "août 2026"
    );
}

#[test]
fn sorts_mixed_all_day_and_instant_events_by_start_time() {
    let sorted = sort_events(
        &[
            iso(
                "instant-later",
                "A later timed event",
                None,
                "2026-05-14T16:00:00.000Z",
                "2026-05-14T17:00:00.000Z",
            ),
            iso(
                "all-day-earlier",
                "Z earlier all-day event",
                None,
                "2026-05-13",
                "2026-05-13",
            ),
            iso(
                "instant-earliest",
                "Middle timed event",
                None,
                "2026-05-12T16:00:00.000Z",
                "2026-05-12T17:00:00.000Z",
            ),
        ],
        "UTC",
        &|a, b| en().compare(a, b),
    );
    let ids: Vec<&str> = sorted.iter().map(|e| e.id.as_ref()).collect();
    assert_eq!(
        ids,
        ["instant-earliest", "all-day-earlier", "instant-later"]
    );
}

#[test]
fn compares_titles_as_local_compare_does_digit_by_digit() {
    // `localeCompare` has no numeric collation: "10 run" sorts before "9 run".
    assert_eq!(en().compare("10 run", "9 run"), std::cmp::Ordering::Less);
    assert_eq!(en().compare("alpha", "Beta"), std::cmp::Ordering::Less);
}

// ---- layoutMonthEvents ----------------------------------------------------------------

fn week() -> Vec<PlainDate> {
    enumerate_dates(d(2026, 5, 10), d(2026, 5, 16))
}

fn two_weeks() -> Vec<PlainDate> {
    enumerate_dates(d(2026, 5, 10), d(2026, 5, 23))
}

fn all_day(id: &str, start: &str, end: &str) -> CalendarEvent {
    iso(id, id, None, start, end)
}

fn timed(id: &str, day: &str, hour: u32, title: &str) -> CalendarEvent {
    iso(
        id,
        title,
        None,
        &format!("{day}T{hour:02}:00:00.000Z"),
        &format!("{day}T{:02}:00:00.000Z", hour + 1),
    )
}

fn month_layout(events: &[CalendarEvent], days: &[PlainDate]) -> MonthLayout {
    let format = en();
    layout_month_events(events, days, "UTC", &|a, b| format.compare(a, b))
}

fn chip_keys(layout: &MonthLayout) -> Vec<String> {
    let mut keys: Vec<String> = layout
        .chips
        .iter()
        .map(|c| {
            format!(
                "{}@{}:{}-{}/{}",
                c.event.id, c.week, c.column_start, c.column_end, c.level
            )
        })
        .collect();
    keys.sort();
    keys
}

fn overflow_keys(layout: &MonthLayout) -> Vec<String> {
    layout
        .overflow
        .iter()
        .map(|o| format!("{}+{}", o.day_index, o.count))
        .collect()
}

fn covers(layout: &MonthLayout, day_index: usize) -> Vec<String> {
    let (week, column) = (day_index / 7, day_index % 7);
    layout
        .chips
        .iter()
        .filter(|c| c.week == week && c.column_start <= column && column <= c.column_end)
        .map(|c| c.event.id.to_string())
        .collect()
}

/// Every event of a day is painted on that day or counted there (FR16).
fn expect_every_event_painted_or_counted(
    layout: &MonthLayout,
    events: &[CalendarEvent],
    days: &[PlainDate],
) {
    for day_index in 0..days.len() {
        let on_day = events
            .iter()
            .filter(|e| !covers(&month_layout(&[(*e).clone()], days), day_index).is_empty())
            .count();
        let counted = layout
            .overflow
            .iter()
            .find(|o| o.day_index == day_index)
            .map_or(0, |o| o.count);
        assert_eq!(
            covers(layout, day_index).len() + counted,
            on_day,
            "day {day_index}"
        );
    }
}

fn busy_wednesday() -> Vec<CalendarEvent> {
    vec![
        all_day("conference", "2026-05-10", "2026-05-13"),
        all_day("hack-week", "2026-05-10", "2026-05-12"),
        all_day("offsite", "2026-05-11", "2026-05-13"),
        timed("alpha", "2026-05-13", 9, "alpha"),
        timed("bravo", "2026-05-13", 11, "bravo"),
        timed("charlie", "2026-05-13", 14, "charlie"),
    ]
}

#[test]
fn month_paints_two_chips_and_counts_three_on_a_day_that_needs_five_levels() {
    let events = busy_wednesday();
    let layout = month_layout(&events, &week());
    assert_eq!(
        chip_keys(&layout),
        [
            "alpha@0:3-3/1",
            "conference@0:0-3/0",
            "hack-week@0:0-2/1",
            "offsite@0:1-2/2"
        ]
    );
    assert_eq!(overflow_keys(&layout), ["3+3"]);
    assert!(layout.chips.iter().all(|c| c.level < 3));
    expect_every_event_painted_or_counted(&layout, &events, &week());
}

#[test]
fn month_paints_a_day_with_three_levels_whole_and_counts_nothing() {
    let layout = month_layout(&busy_wednesday()[..3], &week());
    assert_eq!(
        chip_keys(&layout),
        ["conference@0:0-3/0", "hack-week@0:0-2/1", "offsite@0:1-3/2"]
    );
    assert!(layout.overflow.is_empty());
}

#[test]
fn month_counts_by_level_so_an_empty_third_level_still_holds_the_count() {
    let events = vec![
        all_day("e1", "2026-05-10", "2026-05-13"),
        all_day("e2", "2026-05-10", "2026-05-13"),
        all_day("e3", "2026-05-10", "2026-05-12"),
        all_day("e4", "2026-05-11", "2026-05-13"),
    ];
    let layout = month_layout(&events, &week());
    assert_eq!(
        chip_keys(&layout),
        ["e1@0:0-3/0", "e2@0:0-3/1", "e3@0:0-0/2"]
    );
    assert_eq!(overflow_keys(&layout), ["1+2", "2+2", "3+1"]);
    expect_every_event_painted_or_counted(&layout, &events, &week());
}

#[test]
fn month_gives_the_same_layout_for_any_input_order() {
    let events = busy_wednesday();
    let expected = month_layout(&events, &week());
    let mut reversed = events.clone();
    reversed.reverse();
    let shuffled: Vec<CalendarEvent> = [3, 0, 5, 2, 4, 1]
        .iter()
        .map(|&i| events[i].clone())
        .collect();
    for order in [reversed, shuffled] {
        let layout = month_layout(&order, &week());
        assert_eq!(chip_keys(&layout), chip_keys(&expected));
        assert_eq!(overflow_keys(&layout), overflow_keys(&expected));
    }
}

#[test]
fn month_paints_the_earliest_events_of_a_busy_day_whatever_their_titles() {
    let mut events = busy_wednesday()[..3].to_vec();
    events.push(timed("zulu", "2026-05-13", 8, "Zulu standup"));
    events.push(timed("alpha-late", "2026-05-13", 16, "Alpha retro"));
    events.push(timed("mike", "2026-05-13", 12, "Mike lunch"));
    let layout = month_layout(&events, &week());
    let starting: Vec<&str> = layout
        .chips
        .iter()
        .filter(|c| c.week == 0 && c.column_start == 3)
        .map(|c| c.event.id.as_ref())
        .collect();
    assert_eq!(starting, ["zulu"]);
    assert_eq!(overflow_keys(&layout), ["3+3"]);
}

#[test]
fn month_never_hides_a_timed_event_that_starts_before_a_painted_one() {
    // The seeded walk of upstream's test, in its floating-point arithmetic.
    let mut seed = 7_f64;
    let mut random = || {
        seed = (seed * 1_103_515_245. + 12_345.) % 2_147_483_648.;
        seed / 2_147_483_648.
    };
    let days = two_weeks();
    for run in 0..200 {
        let mut events = Vec::new();
        let count = 4 + (random() * 14.).floor() as usize;
        for index in 0..count {
            let start_day = 10 + (random() * 14.).floor() as u32;
            let day = format!("2026-05-{start_day:02}");
            if random() < 0.3 {
                let end_day = 23.min(start_day + (random() * 4.).floor() as u32);
                events.push(all_day(
                    &format!("span-{index}"),
                    &day,
                    &format!("2026-05-{end_day:02}"),
                ));
            } else {
                let hour = (random() * 23.).floor() as u32;
                let title = format!("{} meeting", char::from(90 - index as u8));
                events.push(timed(&format!("timed-{index}"), &day, hour, &title));
            }
        }
        let layout = month_layout(&events, &days);
        for day_index in 0..days.len() {
            let covered = events
                .iter()
                .filter(|e| covers(&month_layout(&[(*e).clone()], &days), day_index).len() == 1)
                .count();
            let painted: BTreeSet<String> = covers(&layout, day_index).into_iter().collect();
            let counted = layout
                .overflow
                .iter()
                .find(|o| o.day_index == day_index)
                .map_or(0, |o| o.count);
            assert_eq!(
                painted.len() + counted,
                covered,
                "run {run}, day {day_index}"
            );
            let timed_on_day: Vec<&CalendarEvent> = events
                .iter()
                .filter(|e| {
                    e.instants().is_some_and(|(start, _)| {
                        super::date_math::plain_date_from_instant(start, "UTC") == days[day_index]
                    })
                })
                .collect();
            let latest_painted = timed_on_day
                .iter()
                .filter(|e| painted.contains(e.id.as_ref()))
                .map(|e| e.instants().unwrap().0)
                .fold(f64::NEG_INFINITY, f64::max);
            for hidden in timed_on_day
                .iter()
                .filter(|e| !painted.contains(e.id.as_ref()))
            {
                assert!(
                    hidden.instants().unwrap().0 >= latest_painted,
                    "run {run}, day {day_index}: {} is hidden behind a later chip",
                    hidden.id
                );
            }
        }
    }
}

#[test]
fn month_orders_identical_events_by_id() {
    let events = [
        timed("pair-b", "2026-05-12", 9, "Pair review"),
        timed("pair-a", "2026-05-12", 9, "Pair review"),
    ];
    assert_eq!(
        chip_keys(&month_layout(&events, &week())),
        ["pair-a@0:2-2/0", "pair-b@0:2-2/1"]
    );
}

#[test]
fn month_paints_one_chip_per_week_a_span_covers() {
    let events = [all_day("sprint", "2026-05-14", "2026-05-19")];
    assert_eq!(
        chip_keys(&month_layout(&events, &two_weeks())),
        ["sprint@0:4-6/0", "sprint@1:0-2/0"]
    );
}

#[test]
fn month_lays_out_nothing_for_no_events_and_leaves_out_events_outside_the_range() {
    assert_eq!(month_layout(&[], &week()), MonthLayout::default());
    assert_eq!(
        month_layout(&[all_day("later", "2026-06-01", "2026-06-02")], &week()),
        MonthLayout::default()
    );
}

// ---- layoutTimedEvents ------------------------------------------------------------------

fn interval(id: &str, start: f64, end: f64) -> TimedEventInterval<String> {
    TimedEventInterval {
        event: id.to_string(),
        id: id.to_string().into(),
        visible_start: start,
        visible_end: end,
    }
}

fn placements(
    items: &[TimedEventInterval<String>],
) -> std::collections::BTreeMap<String, (f64, f64, usize, usize, usize)> {
    layout_timed_events(items, 480., 1080.)
        .into_iter()
        .map(|p| {
            (
                p.event.clone(),
                (
                    p.top,
                    p.height,
                    p.column_start,
                    p.column_span,
                    p.column_count,
                ),
            )
        })
        .collect()
}

fn cols(p: (f64, f64, usize, usize, usize)) -> (usize, usize, usize) {
    (p.2, p.3, p.4)
}

#[test]
fn grid_gives_a_lone_event_the_whole_column() {
    assert_eq!(
        placements(&[interval("a", 540., 600.)])["a"],
        (10., 10., 0, 1, 1)
    );
}

#[test]
fn grid_splits_an_exact_tie_into_two_equal_columns_ordered_by_id() {
    let p = placements(&[interval("tie-b", 540., 600.), interval("tie-a", 540., 600.)]);
    assert_eq!(cols(p["tie-a"]), (0, 1, 2));
    assert_eq!(cols(p["tie-b"]), (1, 1, 2));
}

#[test]
fn grid_places_three_and_five_simultaneous_events_side_by_side() {
    let three = placements(&[
        interval("1", 780., 840.),
        interval("2", 780., 840.),
        interval("3", 780., 840.),
    ]);
    let mut starts: Vec<usize> = three.values().map(|p| p.2).collect();
    starts.sort();
    assert_eq!(starts, [0, 1, 2]);
    assert!(three.values().all(|p| p.4 == 3 && p.3 == 1));
    let five = placements(&["1", "2", "3", "4", "5"].map(|id| interval(id, 600., 660.)));
    let mut starts: Vec<usize> = five.values().map(|p| p.2).collect();
    starts.sort();
    assert_eq!(starts, [0, 1, 2, 3, 4]);
    assert!(five.values().all(|p| p.4 == 5));
}

#[test]
fn grid_is_independent_of_input_order() {
    let items = vec![
        interval("standup", 540., 600.),
        interval("design-sync", 570., 630.),
        interval("retro", 600., 660.),
        interval("workshop", 780., 960.),
        interval("coffee", 840., 870.),
        interval("tie-a", 720., 750.),
        interval("tie-b", 720., 750.),
    ];
    let forward = placements(&items);
    let mut reversed = items.clone();
    reversed.reverse();
    let shuffled: Vec<_> = [4, 6, 0, 3, 2, 5, 1]
        .iter()
        .map(|&i| items[i].clone())
        .collect();
    assert_eq!(placements(&reversed), forward);
    assert_eq!(placements(&shuffled), forward);
}

#[test]
fn grid_keeps_a_chain_in_one_cluster_and_lets_its_ends_share_a_column() {
    let p = placements(&[
        interval("standup", 540., 600.),
        interval("design-sync", 570., 630.),
        interval("retro", 600., 660.),
    ]);
    assert_eq!(cols(p["standup"]), (0, 1, 2));
    assert_eq!(cols(p["design-sync"]), (1, 1, 2));
    assert_eq!(cols(p["retro"]), (0, 1, 2));
}

#[test]
fn grid_lays_a_contained_event_beside_its_container() {
    let p = placements(&[
        interval("coffee", 600., 630.),
        interval("workshop", 540., 720.),
    ]);
    assert_eq!(cols(p["workshop"]), (0, 1, 2));
    assert_eq!(cols(p["coffee"]), (1, 1, 2));
}

#[test]
fn grid_widens_a_block_into_following_columns_nothing_occupies() {
    let p = placements(&[
        interval("a", 540., 600.),
        interval("b", 540., 600.),
        interval("c", 570., 660.),
        interval("d", 600., 660.),
    ]);
    assert_eq!(cols(p["a"]), (0, 1, 3));
    assert_eq!(cols(p["b"]), (1, 1, 3));
    assert_eq!(cols(p["c"]), (2, 1, 3));
    assert_eq!(cols(p["d"]), (0, 2, 3));
}

#[test]
fn grid_starts_a_new_cluster_once_every_earlier_event_has_ended() {
    let p = placements(&[
        interval("morning-a", 540., 600.),
        interval("morning-b", 540., 600.),
        interval("afternoon", 900., 960.),
    ]);
    assert_eq!(cols(p["afternoon"]), (0, 1, 1));
}

#[test]
fn grid_never_lets_two_blocks_in_a_cluster_share_a_track_at_once() {
    let items: Vec<_> = (0..12)
        .map(|i| {
            interval(
                &format!("e{i}"),
                540. + f64::from(i) * 20.,
                540. + f64::from(i) * 20. + 50.,
            )
        })
        .collect();
    let placed: Vec<TimedEventPlacement<String>> = layout_timed_events(&items, 480., 1080.);
    for (i, a) in placed.iter().enumerate() {
        for (j, b) in placed.iter().enumerate() {
            if i == j {
                continue;
            }
            let time = a.top < b.top + b.height && b.top < a.top + a.height;
            let column = a.column_start < b.column_start + b.column_span
                && b.column_start < a.column_start + a.column_span;
            assert!(!(time && column), "{} and {}", a.event, b.event);
        }
    }
}

// ---- getInitialTimeGridOffset -----------------------------------------------------------------

const SCROLL_HEIGHT: f64 = 2483.;

fn offset(current_time: Instant, min_hour: f64) -> f64 {
    let days = [
        d(2026, 5, 10),
        d(2026, 5, 11),
        d(2026, 5, 12),
        d(2026, 5, 13),
    ];
    get_initial_time_grid_offset(
        current_time,
        &days,
        "UTC",
        min_hour,
        100.,
        SCROLL_HEIGHT,
        600.,
    )
}

#[test]
fn offset_opens_one_hour_above_now_when_today_is_in_the_range() {
    assert_eq!(offset(utc(2026, 5, 13, 14, 0), 0.), 1302.);
}

#[test]
fn offset_opens_at_the_top_when_today_is_not_in_the_range() {
    assert_eq!(offset(utc(2026, 7, 1, 14, 0), 0.), 0.);
}

#[test]
fn offset_clamps_to_the_top_shortly_after_the_window_starts() {
    assert_eq!(offset(utc(2026, 5, 13, 0, 20), 0.), 0.);
}

#[test]
fn offset_clamps_to_the_end_of_the_grid_near_midnight() {
    assert_eq!(offset(utc(2026, 5, 13, 23, 50), 0.), SCROLL_HEIGHT - 600.);
}

#[test]
fn offset_measures_from_the_configured_first_hour() {
    assert_eq!(offset(utc(2026, 5, 13, 9, 0), 7.), 102.);
}

// ---- calendar systems ------------------------------------------------------------------

/// Upstream's `Intl.DateTimeFormat` oracles at Node 24 (ICU 77): calendar, locale, the year in
/// it, the full date, the month title and the day number of 2026-10-30, and the dates of a
/// long timed event from 2026-10-30 09:00 to 2026-11-02 17:00 UTC.
const CALENDAR_LOCALES: &[(&str, &str, &str, &str, &str, &str, &str)] = &[
    (
        "persian",
        "en",
        "1405",
        "Friday, Aban 8, 1405 AP",
        "Aban 1405 AP",
        "8",
        "Aban 8 at 9:00\u{202f}AM\u{2009}\u{2013}\u{2009}Aban 11 at 5:00\u{202f}PM",
    ),
    (
        "persian",
        "fa",
        "۱۴۰۵",
        "۱۴۰۵ آبان ۸, جمعه",
        "۱۴۰۵ آبان",
        "۸",
        "۸ آبان ساعت ۹:۰۰ تا ۱۱ آبان ساعت ۱۷:۰۰",
    ),
    (
        "hebrew",
        "en",
        "5787",
        "Friday, 19 Heshvan 5787",
        "Heshvan 5787",
        "19",
        "19 Heshvan at 9:00\u{202f}AM\u{2009}\u{2013}\u{2009}22 Heshvan at 5:00\u{202f}PM",
    ),
    (
        "hebrew",
        "he",
        "5787",
        "יום שישי, 19 בחשוון 5787",
        "חשוון 5787",
        "19",
        "19 בחשוון בשעה 9:00 \u{2013} 22 בחשוון בשעה 17:00",
    ),
    (
        "buddhist",
        "en",
        "2569",
        "Friday, October 30, 2569 BE",
        "October 2569 BE",
        "30",
        "October 30 at 9:00\u{202f}AM\u{2009}\u{2013}\u{2009}November 2 at 5:00\u{202f}PM",
    ),
    (
        "buddhist",
        "th",
        "2569",
        "วันศุกร์ที่ 30 ตุลาคม 2569",
        "ตุลาคม 2569",
        "30",
        "30 ตุลาคม เวลา 09:00 - 2 พฤศจิกายน เวลา 17:00",
    ),
];

#[test]
fn writes_the_full_date_month_title_and_day_number_in_the_calendar() {
    let date = d(2026, 10, 30);
    for (calendar, locale, year, full, month, day, _) in CALENDAR_LOCALES {
        let f = fmt(locale, calendar);
        let value = f.full_date(date);
        assert!(
            value.contains(year) && !value.contains("2026"),
            "{calendar} {locale}: {value}"
        );
        assert!(f.month_title(date).contains(year), "{calendar} {locale}");
        assert_eq!(f.day_number(date), *day, "{calendar} {locale}");
        // ICU4X's CLDR patterns match Intl's for English; other locales can order the
        // fields differently (Persian in Persian: "جمعه ۸ آبان ۱۴۰۵").
        if *locale == "en" {
            assert_eq!(value, *full, "{calendar} {locale}");
            assert_eq!(f.month_title(date), *month, "{calendar} {locale}");
        }
    }
}

#[test]
fn writes_a_week_range_in_the_calendar() {
    for (calendar, locale, year, ..) in CALENDAR_LOCALES {
        let value = fmt(locale, calendar).week_range(d(2026, 10, 25), d(2026, 10, 31));
        assert!(
            value.contains(year) && !value.contains("2026"),
            "{calendar} {locale}: {value}"
        );
    }
}

#[test]
fn writes_the_dates_of_a_long_timed_event_in_the_calendar_with_its_times() {
    let (start, end) = (utc(2026, 10, 30, 9, 0), utc(2026, 11, 2, 17, 0));
    for (calendar, locale, .., range) in CALENDAR_LOCALES.iter().filter(|c| c.1 == "en") {
        assert_eq!(
            fmt(locale, calendar).event_date_time_range(start, end, "UTC"),
            *range,
            "{calendar} {locale}"
        );
    }
}

#[test]
fn titles_a_month_with_an_era_change_by_its_middle() {
    let title = fmt("en", "japanese").week_title(d(1989, 1, 1), d(1989, 1, 31));
    assert_eq!(title, "January 1 Heisei");
    assert!(!title.contains("Shōwa"));
}

#[test]
fn names_both_calendar_months_when_a_gregorian_month_spans_two() {
    assert_eq!(
        fmt("en", "persian").week_title(d(2026, 10, 1), d(2026, 10, 31)),
        "Mehr - Aban 1405 AP"
    );
}

#[test]
fn keeps_one_gregorian_month_as_one_title() {
    assert_eq!(
        fmt("en", "gregory").week_title(d(2026, 10, 1), d(2026, 10, 31)),
        "October 2026"
    );
}

#[test]
fn writes_iso8601_titles_and_ranges_as_gregorian_ones() {
    let (iso_f, greg) = (fmt("en", "iso8601"), fmt("en", "gregory"));
    assert_eq!(iso_f.month_title(d(2026, 10, 30)), "October 2026");
    assert_eq!(
        iso_f.week_range(d(2026, 10, 25), d(2026, 10, 31)),
        greg.week_range(d(2026, 10, 25), d(2026, 10, 31))
    );
    let (start, end) = (utc(2026, 10, 30, 9, 0), utc(2026, 11, 2, 17, 0));
    assert_eq!(
        iso_f.event_date_time_range(start, end, "UTC"),
        greg.event_date_time_range(start, end, "UTC")
    );
}

// ---- rendering --------------------------------------------------------------------------

type Build = Rc<dyn Fn() -> AnyElement>;

struct Host(Build);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child((self.0)())
    }
}

fn host(
    cx: &mut TestAppContext,
    build: impl Fn() -> AnyElement + 'static,
) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(|cx| {
        astryx_theme::init(cx);
        astryx_primitives::focus::init(cx);
        astryx_primitives::editing::init(cx);
    });
    let (view, cx) = cx.add_window_view(move |_, _| Host(Rc::new(build)));
    cx.update(|window, _| window.activate_window());
    settle(cx);
    (view, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not drawn"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    settle(cx);
}

fn press(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = gpui::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    settle(cx);
}

fn grid() -> GridDrawn {
    GRIDS
        .with(|g| g.borrow().get("s").cloned())
        .expect("no time grid drawn")
}

fn month() -> MonthDrawn {
    MONTHS
        .with(|m| m.borrow().get("s").cloned())
        .expect("no month drawn")
}

fn list() -> ListDrawn {
    LISTS
        .with(|l| l.borrow().get("s").cloned())
        .expect("no list drawn")
}

fn categories() -> Vec<ScheduleCategory> {
    vec![
        ScheduleCategory::new("Sync", ScheduleEventColor::Blue),
        ScheduleCategory::new("Design", ScheduleEventColor::Purple),
        ScheduleCategory::new("Blocked", ScheduleEventColor::Red),
        ScheduleCategory::new("Migration", ScheduleEventColor::Pink),
    ]
}

fn events() -> Vec<CalendarEvent> {
    vec![
        iso(
            "visible",
            "Visible sync",
            Some("Sync"),
            "2026-05-13T16:00:00.000Z",
            "2026-05-13T16:30:00.000Z",
        ),
        iso(
            "all-day",
            "Design review",
            Some("Design"),
            "2026-05-13",
            "2026-05-13",
        ),
        iso(
            "outside",
            "Outside range",
            Some("Blocked"),
            "2026-08-13",
            "2026-08-13",
        ),
    ]
}

fn sized(schedule: Schedule) -> AnyElement {
    schedule.w(px(1000.)).h(px(700.)).into_any_element()
}

/// A Schedule named `s` in UTC, focused on its date, with upstream's categories.
fn schedule(view: &ScheduleView, events: Vec<CalendarEvent>, date: Instant) -> Schedule {
    Schedule::new("s", view.clone(), events, date)
        .categories(categories())
        .focus_date(date)
        .timezone_id("UTC")
}

fn grid_options(
    min_hour: f64,
    max_hour: f64,
    hour_height: f64,
    render_popover: Option<RenderPopover>,
) -> ScheduleTimeGridViewOptions {
    ScheduleTimeGridViewOptions {
        min_hour,
        max_hour,
        hour_height,
        render_popover,
    }
}

fn weekly(options: ScheduleTimeGridViewOptions) -> ScheduleView {
    create_schedule_weekly_view(ScheduleWeeklyViewOptions {
        week_starts_on: 0,
        time_grid: options,
    })
}

fn details() -> RenderPopover {
    Rc::new(|event: &CalendarEvent, _: &mut Window, _: &mut App| {
        Some(
            div()
                .child(format!("Details for {}", event.title))
                .into_any_element(),
        )
    })
}

/// A plugin that keeps the context it renders with, and the slots it was given.
#[derive(Default)]
struct Probe {
    context: Rc<RefCell<Option<Rc<ScheduleContextValue>>>>,
    slots: Rc<Cell<(usize, usize, usize)>>,
}

#[derive(IntoElement)]
struct ProbeElement(Rc<RefCell<Option<Rc<ScheduleContextValue>>>>);

impl RenderOnce for ProbeElement {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        *self.0.borrow_mut() = use_schedule_context(cx);
        div()
    }
}

impl SchedulePlugin for Probe {
    fn render_header(&self, mut content: ScheduleHeaderContent) -> ScheduleHeaderContent {
        self.slots.set((
            content.start_content.len(),
            content.center_content.len(),
            content.end_content.len(),
        ));
        content
            .end_content
            .push(ProbeElement(self.context.clone()).into_any_element());
        content
    }
}

#[gpui::test]
fn filters_array_events_to_the_active_view_range(cx: &mut TestAppContext) {
    let view = create_schedule_monthly_view(ScheduleMonthlyViewOptions::default());
    let (_, _cx) = host(cx, move || {
        sized(schedule(&view, events(), utc(2026, 5, 13, 0, 0)))
    });
    let ids: BTreeSet<String> = month().chips.iter().map(|c| c.1.clone()).collect();
    assert_eq!(ids, ["all-day".to_string(), "visible".to_string()].into());
}

#[gpui::test]
fn renders_monthly_dates_and_times_with_the_provider_locale(cx: &mut TestAppContext) {
    let view = create_schedule_monthly_view(ScheduleMonthlyViewOptions::default());
    let (_, _cx) = host(cx, move || {
        astryx_i18n::provider(
            I18n::new("fr-FR"),
            sized(schedule(&view, events(), utc(2026, 5, 13, 0, 0))),
        )
        .into_any_element()
    });
    let drawn = month();
    assert_eq!(drawn.table_label, "mai 2026");
    assert!(drawn.weekday_headers.iter().any(|h| h.0 == "mercredi"));
    let wednesday = drawn
        .hidden_lists
        .iter()
        .find(|l| l.0 == "2026-05-13")
        .unwrap();
    assert!(
        wednesday
            .1
            .contains(&"Visible sync, Sync, 16:00 - 16:30".to_string()),
        "{:?}",
        wednesday.1
    );
}

#[gpui::test]
fn leads_each_month_chip_with_the_title_and_gives_an_all_day_chip_no_time(cx: &mut TestAppContext) {
    let view = create_schedule_monthly_view(ScheduleMonthlyViewOptions::default());
    let (_, _cx) = host(cx, move || {
        sized(schedule(&view, events(), utc(2026, 5, 13, 0, 0)))
    });
    let chips = month().chips;
    assert_eq!(
        chips
            .iter()
            .find(|c| c.1 == "visible")
            .unwrap()
            .7
            .as_deref(),
        Some("4:00 PM")
    );
    assert_eq!(chips.iter().find(|c| c.1 == "all-day").unwrap().7, None);
}

#[gpui::test]
fn renders_monthly_weekday_headings_at_the_configured_heading_level(cx: &mut TestAppContext) {
    let view = create_schedule_monthly_view(ScheduleMonthlyViewOptions::default());
    let level = Rc::new(Cell::new(None::<u8>));
    let at = level.clone();
    let (host_view, cx) = host(cx, move || {
        let s = schedule(&view, events(), utc(2026, 5, 13, 0, 0));
        sized(match at.get() {
            Some(level) => s.heading_level(level),
            None => s,
        })
    });
    assert!(
        month().weekday_headers.iter().all(|h| h.2 == 3),
        "the default is 3"
    );
    level.set(Some(5));
    host_view.update(cx, |_, cx| cx.notify());
    settle(cx);
    assert!(month().weekday_headers.iter().all(|h| h.2 == 5));
}

#[gpui::test]
fn loads_async_events_with_instant_range_boundaries(cx: &mut TestAppContext) {
    let calls: Rc<RefCell<Vec<(Instant, Instant)>>> = Rc::default();
    let log = calls.clone();
    let loader: ScheduleEventSource =
        ScheduleEventSource::Loader(Rc::new(move |start, end, cx: &mut App| {
            log.borrow_mut().push((start, end));
            let timer = cx.background_executor().timer(Duration::from_secs(1));
            cx.background_spawn(async move {
                timer.await;
                Ok(vec![iso(
                    "async",
                    "Loaded event",
                    None,
                    "2026-05-13T17:00:00.000Z",
                    "2026-05-13T18:00:00.000Z",
                )])
            })
        }));
    let view = create_schedule_day_view(ScheduleDayViewOptions::default());
    let (_, cx) = host(cx, move || {
        sized(
            Schedule::new("s", view.clone(), loader.clone(), utc(2026, 5, 13, 0, 0))
                .timezone_id("UTC"),
        )
    });
    assert_eq!(
        *calls.borrow(),
        [(utc(2026, 5, 13, 0, 0), utc(2026, 5, 14, 0, 0))]
    );
    assert!(
        grid().blocks.is_empty(),
        "the view renders empty while the loader is pending"
    );
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx);
    assert_eq!(
        grid()
            .blocks
            .iter()
            .map(|b| b.0.as_str())
            .collect::<Vec<_>>(),
        ["async"]
    );
    assert_eq!(
        calls.borrow().len(),
        1,
        "the result is cached per loader and range"
    );
}

#[gpui::test]
fn renders_list_view_grouped_by_localized_day(cx: &mut TestAppContext) {
    let mut list_events = events();
    list_events.push(iso(
        "overnight",
        "Overnight migration",
        Some("Migration"),
        "2026-05-13T23:00:00.000Z",
        "2026-05-14T02:00:00.000Z",
    ));
    let view = create_schedule_list_view(ScheduleListViewOptions { days: 7 });
    let (_, _cx) = host(cx, move || {
        sized(
            Schedule::new(
                "s",
                view.clone(),
                list_events.clone(),
                utc(2026, 5, 13, 0, 0),
            )
            .categories(categories())
            .timezone_id("UTC"),
        )
    });
    let days = list();
    let wednesday = days.iter().find(|d| d.0 == "2026-05-13").unwrap();
    assert_eq!(wednesday.2, "13Wed");
    assert_eq!(
        wednesday.1, "Wednesday, May 13, 2026",
        "the heading is named by the full date"
    );
    for id in ["visible", "all-day", "overnight"] {
        assert!(wednesday.4.contains(&id.to_string()), "{id}");
    }
    let thursday = days.iter().find(|d| d.0 == "2026-05-14").unwrap();
    assert!(
        thursday.4.contains(&"overnight".to_string()),
        "an overnight event lists on both days"
    );
    assert_eq!(
        en().event_time_range(utc(2026, 5, 13, 23, 0), utc(2026, 5, 14, 2, 0), "UTC"),
        "11:00 PM - 2:00 AM"
    );
}

#[gpui::test]
fn renders_weekly_view_with_the_same_month_title_as_monthly_view(cx: &mut TestAppContext) {
    let view = weekly(ScheduleTimeGridViewOptions::default());
    let (_, _cx) = host(cx, move || {
        sized(schedule(&view, events(), utc(2026, 5, 13, 0, 0)))
    });
    assert_eq!(grid().viewport_label, "May 2026 time grid");
}

#[gpui::test]
fn exposes_the_month_view_as_a_table_with_week_row_headers_and_date_named_cells(
    cx: &mut TestAppContext,
) {
    let view = create_schedule_monthly_view(ScheduleMonthlyViewOptions::default());
    let (_, _cx) = host(cx, move || {
        sized(schedule(&view, events(), utc(2026, 5, 13, 0, 0)))
    });
    let drawn = month();
    assert_eq!(drawn.table_label, "May 2026");
    let short: Vec<&str> = drawn.weekday_headers.iter().map(|h| h.1.as_str()).collect();
    assert_eq!(short, ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]);
    assert_eq!(drawn.weekday_headers[3].0, "Wednesday");
    assert_eq!(drawn.row_headers.len(), 6);
    assert_eq!(
        drawn.row_headers[2],
        "May 10\u{2009}\u{2013}\u{2009}16, 2026"
    );
    assert_eq!(drawn.cells.len(), 42);
    let wednesday = drawn.cells.iter().find(|c| c.0 == "2026-05-13").unwrap();
    assert_eq!(
        (wednesday.1.as_str(), wednesday.2),
        ("Wednesday, May 13, 2026", true)
    );
    let labels = &drawn
        .hidden_lists
        .iter()
        .find(|l| l.0 == "2026-05-13")
        .unwrap()
        .1;
    assert!(
        labels.contains(&"Visible sync, Sync, 4:00 PM - 4:30 PM".to_string()),
        "{labels:?}"
    );
    assert!(
        labels.contains(&"Design review, Design, all day".to_string()),
        "{labels:?}"
    );
}

#[gpui::test]
fn exposes_time_grid_views_as_aria_grids(cx: &mut TestAppContext) {
    let view = create_schedule_day_view(grid_options(8., 10., 100., None));
    let (_, _cx) = host(cx, move || {
        sized(schedule(&view, events(), utc(2026, 5, 13, 0, 0)))
    });
    let drawn = grid();
    assert_eq!(drawn.timezone_label, "UTC");
    let rows = drawn.hidden_grid;
    assert_eq!(rows[0], ["Time", "Wednesday, May 13, 2026"]);
    assert_eq!(rows[1][0], "UTC all day");
    assert_eq!(
        rows[1][1],
        "Wednesday, May 13, 2026 all day. Design review, Design, all day"
    );
    assert_eq!(rows[2][1], "Wednesday, May 13, 2026 8 AM");
    assert_eq!(rows.len(), 4, "a header, the all-day row and two hours");
}

#[gpui::test]
fn exposes_timed_events_in_the_accessible_time_grid_cells(cx: &mut TestAppContext) {
    let view = create_schedule_day_view(grid_options(16., 17., 100., None));
    let (_, _cx) = host(cx, move || {
        sized(schedule(&view, events(), utc(2026, 5, 13, 0, 0)))
    });
    assert_eq!(
        grid().hidden_grid[2][1],
        "Wednesday, May 13, 2026 4 PM. Visible sync, Sync, 4:00 PM - 4:30 PM"
    );
}

#[gpui::test]
fn keeps_every_painted_time_grid_part_read_only_without_the_option(cx: &mut TestAppContext) {
    let view = weekly(grid_options(8., 20., 30., None));
    let (_, _cx) = host(cx, move || {
        sized(schedule(&view, events(), utc(2026, 5, 13, 0, 0)))
    });
    let drawn = grid();
    assert!(
        !drawn.hidden_grid.is_empty(),
        "the read-only grid stays the accessible representation"
    );
    assert!(
        drawn.blocks.iter().all(|b| b.7.is_none() && !b.8),
        "no block is a button"
    );
    assert!(drawn.spans.iter().all(|s| s.4.is_none() && !s.5));
}

#[gpui::test]
fn calls_on_change_date_with_the_previous_view_date_preserving_time_of_day(
    cx: &mut TestAppContext,
) {
    let changed: Rc<RefCell<Vec<Instant>>> = Rc::default();
    let probe = Rc::new(Probe::default());
    let (log, plugin) = (changed.clone(), probe.clone());
    let view = create_schedule_day_view(ScheduleDayViewOptions::default());
    let (_, cx) = host(cx, move || {
        let log = log.clone();
        let mut plugins = default_schedule_plugins();
        plugins.push(plugin.clone() as Rc<dyn SchedulePlugin>);
        sized(
            Schedule::new("s", view.clone(), events(), utc(2026, 5, 13, 15, 6))
                .timezone_id("UTC")
                .plugins(plugins)
                .on_change_date(move |date, _, _| log.borrow_mut().push(date)),
        )
    });
    let context = probe
        .context
        .borrow()
        .clone()
        .expect("the plugin renders inside the Schedule");
    assert_eq!(context.previous_date_label, "Previous day");
    assert_eq!(context.next_date_label, "Next day");
    assert_eq!(
        probe.slots.get().0,
        1,
        "the pagination controls lead the start slot"
    );
    cx.update(|window, cx| (context.on_previous_date)(window, cx));
    cx.update(|window, cx| (context.on_next_date)(window, cx));
    assert_eq!(
        *changed.borrow(),
        [utc(2026, 5, 12, 15, 6), utc(2026, 5, 14, 15, 6)]
    );
}

#[gpui::test]
fn today_passes_the_current_time(cx: &mut TestAppContext) {
    set_test_now(Some(utc(2026, 7, 4, 10, 30)));
    let changed: Rc<RefCell<Vec<Instant>>> = Rc::default();
    let probe = Rc::new(Probe::default());
    let (log, plugin) = (changed.clone(), probe.clone());
    let view = weekly(ScheduleTimeGridViewOptions::default());
    let (_, cx) = host(cx, move || {
        let log = log.clone();
        sized(
            Schedule::new("s", view.clone(), events(), utc(2026, 5, 13, 0, 0))
                .timezone_id("UTC")
                .plugins([plugin.clone() as Rc<dyn SchedulePlugin>])
                .on_change_date(move |date, _, _| log.borrow_mut().push(date)),
        )
    });
    let context = probe.context.borrow().clone().unwrap();
    assert_eq!(
        (
            context.previous_date_label.as_ref(),
            context.next_date_label.as_ref()
        ),
        ("Previous week", "Next week")
    );
    cx.update(|window, cx| (context.on_today)(window, cx));
    assert_eq!(*changed.borrow(), [utc(2026, 7, 4, 10, 30)]);
    set_test_now(None);
}

#[gpui::test]
fn allows_plugins_to_customize_header_slots(cx: &mut TestAppContext) {
    struct Custom;
    impl SchedulePlugin for Custom {
        fn render_header(&self, content: ScheduleHeaderContent) -> ScheduleHeaderContent {
            ScheduleHeaderContent {
                start_content: vec![
                    div()
                        .debug_selector(|| "custom-start".into())
                        .w(px(40.))
                        .h(px(10.))
                        .child("Custom start")
                        .into_any_element(),
                ],
                ..content
            }
        }
    }
    let probe = Rc::new(Probe::default());
    let plugin = probe.clone();
    let view = create_schedule_day_view(ScheduleDayViewOptions::default());
    let (_, cx) = host(cx, move || {
        sized(
            Schedule::new("s", view.clone(), events(), utc(2026, 5, 13, 0, 0))
                .timezone_id("UTC")
                .plugins([plugin.clone() as Rc<dyn SchedulePlugin>, Rc::new(Custom)]),
        )
    });
    assert!(cx.debug_bounds("custom-start").is_some());
    assert_eq!(
        probe.slots.get(),
        (0, 1, 0),
        "a list replaces the default pagination controls"
    );
}

#[gpui::test]
fn renders_a_view_selector_plugin_in_the_header_end_slot(cx: &mut TestAppContext) {
    let probe = Rc::new(Probe::default());
    let plugin = probe.clone();
    let (day, month_view) = (
        create_schedule_day_view(ScheduleDayViewOptions::default()),
        create_schedule_monthly_view(ScheduleMonthlyViewOptions::default()),
    );
    let options = vec![
        ScheduleViewSelectorOption::new(month_view, "Month"),
        ScheduleViewSelectorOption::new(day.clone(), "Day"),
    ];
    let (_, _cx) = host(cx, move || {
        let selector = schedule_view_selector_plugin(
            options.clone(),
            ScheduleViewSelectorPluginOptions {
                on_change_view: Some(Rc::new(|_, _, _| {})),
                ..Default::default()
            },
        );
        sized(
            Schedule::new("s", day.clone(), events(), utc(2026, 5, 13, 0, 0))
                .timezone_id("UTC")
                .plugins([selector, plugin.clone() as Rc<dyn SchedulePlugin>]),
        )
    });
    assert_eq!(
        probe.slots.get(),
        (0, 1, 1),
        "the selector fills the end slot"
    );
}

#[gpui::test]
fn keeps_seven_days_in_the_week_and_one_in_the_day_view_across_a_skipped_midnight(
    cx: &mut TestAppContext,
) {
    let cases: [(&str, Instant, Instant, [&str; 7]); 3] = [
        (
            "America/Santiago",
            utc(2026, 9, 8, 15, 0),
            utc(2026, 9, 6, 15, 0),
            [
                "Sunday, September 6, 2026",
                "Monday, September 7, 2026",
                "Tuesday, September 8, 2026",
                "Wednesday, September 9, 2026",
                "Thursday, September 10, 2026",
                "Friday, September 11, 2026",
                "Saturday, September 12, 2026",
            ],
        ),
        (
            "America/Havana",
            utc(2026, 3, 10, 15, 0),
            utc(2026, 3, 8, 15, 0),
            [
                "Sunday, March 8, 2026",
                "Monday, March 9, 2026",
                "Tuesday, March 10, 2026",
                "Wednesday, March 11, 2026",
                "Thursday, March 12, 2026",
                "Friday, March 13, 2026",
                "Saturday, March 14, 2026",
            ],
        ),
        // A week that ends at the skipped midnight: its end is the start of September 6.
        (
            "America/Santiago",
            utc(2026, 9, 5, 15, 0),
            utc(2026, 9, 5, 15, 0),
            [
                "Sunday, August 30, 2026",
                "Monday, August 31, 2026",
                "Tuesday, September 1, 2026",
                "Wednesday, September 2, 2026",
                "Thursday, September 3, 2026",
                "Friday, September 4, 2026",
                "Saturday, September 5, 2026",
            ],
        ),
    ];
    let current: Rc<RefCell<(ScheduleView, Instant, &'static str)>> = Rc::new(RefCell::new((
        weekly(ScheduleTimeGridViewOptions::default()),
        0.,
        "UTC",
    )));
    let shown = current.clone();
    let (host_view, cx) = host(cx, move || {
        let (view, date, tz) = shown.borrow().clone();
        sized(Schedule::new("s", view, Vec::<CalendarEvent>::new(), date).timezone_id(tz))
    });
    let days = |cx: &mut VisualTestContext| -> Vec<String> {
        settle(cx);
        grid().hidden_grid[0][1..].to_vec()
    };
    for (tz, date, day_date, week) in cases {
        *current.borrow_mut() = (weekly(ScheduleTimeGridViewOptions::default()), date, tz);
        host_view.update(cx, |_, cx| cx.notify());
        assert_eq!(days(cx), week, "{tz} week");
        *current.borrow_mut() = (
            create_schedule_day_view(ScheduleDayViewOptions::default()),
            day_date,
            tz,
        );
        host_view.update(cx, |_, cx| cx.notify());
        assert_eq!(
            days(cx),
            [week[if day_date == date { 6 } else { 0 }]],
            "{tz} day"
        );
    }
}

// ---- the time grid's popover -------------------------------------------------------------

fn popover_events() -> Vec<CalendarEvent> {
    vec![
        iso(
            "later",
            "Later sync",
            Some("Sync"),
            "2026-05-13T17:00:00.000Z",
            "2026-05-13T17:30:00.000Z",
        ),
        iso(
            "earlier",
            "Earlier sync",
            Some("Sync"),
            "2026-05-13T16:00:00.000Z",
            "2026-05-13T16:30:00.000Z",
        ),
        iso(
            "tuesday",
            "Tuesday sync",
            Some("Sync"),
            "2026-05-12T16:00:00.000Z",
            "2026-05-12T16:30:00.000Z",
        ),
        iso(
            "all-day",
            "Design review",
            Some("Design"),
            "2026-05-13",
            "2026-05-13",
        ),
    ]
}

fn render_week(
    cx: &mut TestAppContext,
    options: ScheduleTimeGridViewOptions,
    events: Vec<CalendarEvent>,
) -> (Entity<Host>, &mut VisualTestContext) {
    let view = weekly(options);
    host(cx, move || {
        sized(schedule(&view, events.clone(), utc(2026, 5, 13, 0, 0)))
    })
}

#[gpui::test]
fn paints_a_timed_event_that_ends_exactly_at_midnight_on_the_day_it_starts(
    cx: &mut TestAppContext,
) {
    let late = iso(
        "late",
        "Late sync",
        Some("Sync"),
        "2026-05-13T22:00:00.000Z",
        "2026-05-14T00:00:00.000Z",
    );
    let (_, _cx) = render_week(cx, grid_options(0., 24., 20., Some(details())), vec![late]);
    let blocks = grid().blocks;
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].1, "2026-05-13");
    assert_eq!(
        blocks[0].7.as_deref(),
        Some("Late sync, 10:00 PM - 12:00 AM, Sync, Wednesday, May 13, 2026")
    );
}

#[gpui::test]
fn renders_each_event_with_content_as_a_named_popup_button(cx: &mut TestAppContext) {
    let (_, _cx) = render_week(
        cx,
        grid_options(8., 20., 30., Some(details())),
        popover_events(),
    );
    let drawn = grid();
    assert!(
        drawn.hidden_grid.is_empty(),
        "the buttons replace the read-only grid"
    );
    assert_eq!(drawn.spans.len(), 1);
    assert_eq!(
        drawn.spans[0].4.as_deref(),
        Some("Design review, all day, Design, Wednesday, May 13, 2026")
    );
    let wednesday: Vec<&str> = drawn
        .blocks
        .iter()
        .filter(|b| b.1 == "2026-05-13")
        .filter_map(|b| b.7.as_deref())
        .collect();
    assert_eq!(
        wednesday,
        [
            "Earlier sync, 4:00 PM - 4:30 PM, Sync, Wednesday, May 13, 2026",
            "Later sync, 5:00 PM - 5:30 PM, Sync, Wednesday, May 13, 2026"
        ]
    );
    assert!(
        drawn.blocks.iter().all(|b| b.8),
        "every event with content is a button"
    );
}

#[gpui::test]
fn opens_one_popover_named_by_the_event_switches_and_closes_on_escape(cx: &mut TestAppContext) {
    let (_, cx) = render_week(
        cx,
        grid_options(8., 20., 30., Some(details())),
        popover_events(),
    );
    assert_eq!(grid().open, None);
    click(cx, "s-block-2026-05-13/earlier");
    assert_eq!(grid().open.as_deref(), Some("2026-05-13/earlier"));
    // Tuesday's column is clear of the open surface, which hangs below Wednesday's block.
    click(cx, "s-block-2026-05-12/tuesday");
    assert_eq!(
        grid().open.as_deref(),
        Some("2026-05-12/tuesday"),
        "one popover, switched to the other event"
    );
    press(cx, "escape");
    assert_eq!(grid().open, None);
}

#[gpui::test]
fn closes_when_the_open_block_is_pressed_again(cx: &mut TestAppContext) {
    let (_, cx) = render_week(
        cx,
        grid_options(8., 20., 30., Some(details())),
        popover_events(),
    );
    click(cx, "s-block-2026-05-13/earlier");
    assert!(grid().open.is_some());
    click(cx, "s-block-2026-05-13/earlier");
    assert_eq!(grid().open, None);
}

#[gpui::test]
fn leaves_an_event_without_content_read_only_but_still_exposed(cx: &mut TestAppContext) {
    let only_later: RenderPopover =
        Rc::new(|event: &CalendarEvent, _: &mut Window, _: &mut App| {
            (event.id.as_ref() == "later").then(|| div().child("Later details").into_any_element())
        });
    let (_, _cx) = render_week(
        cx,
        grid_options(8., 20., 30., Some(only_later)),
        popover_events(),
    );
    let blocks = grid().blocks;
    let earlier = blocks.iter().find(|b| b.0 == "earlier").unwrap();
    assert!(!earlier.8 && earlier.7.is_some(), "named, not a button");
    assert!(blocks.iter().find(|b| b.0 == "later").unwrap().8);
}

#[gpui::test]
fn closes_when_the_open_event_leaves(cx: &mut TestAppContext) {
    let shown = Rc::new(RefCell::new(popover_events()));
    let source = shown.clone();
    let view = weekly(grid_options(8., 20., 30., Some(details())));
    let (host_view, cx) = host(cx, move || {
        sized(schedule(
            &view,
            source.borrow().clone(),
            utc(2026, 5, 13, 0, 0),
        ))
    });
    click(cx, "s-block-2026-05-13/earlier");
    assert!(grid().open.is_some());
    shown.borrow_mut().retain(|e| e.id.as_ref() != "earlier");
    host_view.update(cx, |_, cx| cx.notify());
    settle(cx);
    assert_eq!(grid().open, None);
}

#[gpui::test]
fn exposes_the_same_option_on_the_day_view(cx: &mut TestAppContext) {
    let view = create_schedule_day_view(grid_options(8., 20., 30., Some(details())));
    let (_, cx) = host(cx, move || {
        sized(schedule(&view, popover_events(), utc(2026, 5, 13, 0, 0)))
    });
    click(cx, "s-block-2026-05-13/earlier");
    assert_eq!(grid().open.as_deref(), Some("2026-05-13/earlier"));
}

// ---- the month's overflow and event popover ------------------------------------------------

fn busy_month_events() -> Vec<CalendarEvent> {
    let mut events = busy_wednesday();
    for e in &mut events {
        e.category = Some("Sync".into());
    }
    events
}

#[gpui::test]
fn gives_a_busy_day_a_named_more_button_in_its_cell(cx: &mut TestAppContext) {
    let view = create_schedule_monthly_view(ScheduleMonthlyViewOptions::default());
    let (_, _cx) = host(cx, move || {
        sized(schedule(&view, busy_month_events(), utc(2026, 5, 13, 0, 0)))
    });
    let more = month().more;
    assert_eq!(more.len(), 1);
    assert_eq!(more[0].0, "2026-05-13");
    assert_eq!(more[0].2, "+3 more");
}

#[gpui::test]
fn opens_one_popover_named_by_the_day_and_closes_on_escape(cx: &mut TestAppContext) {
    let view = create_schedule_monthly_view(ScheduleMonthlyViewOptions::default());
    let (_, cx) = host(cx, move || {
        sized(schedule(&view, busy_month_events(), utc(2026, 5, 13, 0, 0)))
    });
    click(cx, "s-more-2026-05-13");
    let drawn = month();
    assert_eq!(
        drawn.popover.as_ref().map(|p| p.0.as_str()),
        Some("day/2026-05-13")
    );
    // Conference, offsite and Wednesday's three timed events (hack week ended Tuesday).
    assert_eq!(
        drawn.popover_rows.len(),
        5,
        "the popover lists every event of the day"
    );
    press(cx, "escape");
    assert_eq!(month().popover, None);
}

#[gpui::test]
fn adds_no_more_button_to_a_month_whose_days_fit(cx: &mut TestAppContext) {
    let view = create_schedule_monthly_view(ScheduleMonthlyViewOptions::default());
    let (_, _cx) = host(cx, move || {
        sized(schedule(
            &view,
            busy_month_events()[..3].to_vec(),
            utc(2026, 5, 13, 0, 0),
        ))
    });
    assert!(month().more.is_empty());
}

#[gpui::test]
fn keeps_chips_painted_decoration_without_the_option(cx: &mut TestAppContext) {
    let view = create_schedule_monthly_view(ScheduleMonthlyViewOptions::default());
    let (_, _cx) = host(cx, move || {
        sized(schedule(&view, events(), utc(2026, 5, 13, 0, 0)))
    });
    assert!(month().chips.iter().all(|c| c.5.is_none() && !c.6));
}

#[gpui::test]
fn renders_each_month_chip_with_content_as_a_named_button(cx: &mut TestAppContext) {
    let view = create_schedule_monthly_view(ScheduleMonthlyViewOptions {
        week_starts_on: 0,
        render_popover: Some(details()),
    });
    let (_, cx) = host(cx, move || {
        sized(schedule(&view, events(), utc(2026, 5, 13, 0, 0)))
    });
    let chips = month().chips;
    let visible = chips.iter().find(|c| c.1 == "visible").unwrap();
    assert!(visible.6);
    assert!(
        visible
            .5
            .as_deref()
            .is_some_and(|n| n.starts_with("Visible sync")),
        "{:?}",
        visible.5
    );
    click(cx, "s-chip-visible");
    assert!(
        month()
            .popover
            .as_ref()
            .is_some_and(|p| p.0.ends_with("/visible")),
        "{:?}",
        month().popover
    );
    press(cx, "escape");
    assert_eq!(month().popover, None);
}

#[gpui::test]
fn names_a_chip_as_static_text_on_each_later_day_it_covers(cx: &mut TestAppContext) {
    let view = create_schedule_monthly_view(ScheduleMonthlyViewOptions {
        week_starts_on: 0,
        render_popover: Some(details()),
    });
    let span = vec![iso(
        "sprint",
        "Sprint",
        Some("Sync"),
        "2026-05-12",
        "2026-05-14",
    )];
    let (_, _cx) = host(cx, move || {
        sized(schedule(&view, span.clone(), utc(2026, 5, 13, 0, 0)))
    });
    let names: Vec<String> = month().hidden_names.iter().map(|n| n.0.clone()).collect();
    assert_eq!(names, ["2026-05-13", "2026-05-14"]);
}

// ---- the time grid's initial position ----------------------------------------------------

fn harness(
    cx: &mut TestAppContext,
    view: ScheduleView,
    initial: Instant,
    source: Rc<RefCell<ScheduleEventSource>>,
) -> (Entity<Host>, &mut VisualTestContext) {
    let date = Rc::new(Cell::new(initial));
    host(cx, move || {
        let date_cell = date.clone();
        Schedule::new("s", view.clone(), source.borrow().clone(), date.get())
            .timezone_id("UTC")
            .on_change_date(move |next, window, _| {
                date_cell.set(next);
                window.refresh();
            })
            .w(px(1000.))
            .h(px(700.))
            .into_any_element()
    })
}

fn scroll_top() -> f64 {
    grid().scroll_top
}

fn static_events() -> Rc<RefCell<ScheduleEventSource>> {
    Rc::new(RefCell::new(
        vec![iso(
            "visible",
            "Visible sync",
            None,
            "2026-05-13T16:00:00.000Z",
            "2026-05-13T16:30:00.000Z",
        )]
        .into(),
    ))
}

#[gpui::test]
fn opens_a_week_containing_today_one_hour_before_now(cx: &mut TestAppContext) {
    set_test_now(Some(utc(2026, 5, 13, 14, 0)));
    let (_, _cx) = harness(
        cx,
        weekly(ScheduleTimeGridViewOptions::default()),
        utc(2026, 5, 13, 14, 0),
        static_events(),
    );
    assert_eq!(scroll_top(), 1302.);
    set_test_now(None);
}

#[gpui::test]
fn opens_a_day_containing_today_one_hour_before_now(cx: &mut TestAppContext) {
    set_test_now(Some(utc(2026, 5, 13, 9, 30)));
    let (_, _cx) = harness(
        cx,
        create_schedule_day_view(ScheduleDayViewOptions::default()),
        utc(2026, 5, 13, 9, 30),
        static_events(),
    );
    assert_eq!(scroll_top(), 852.);
    set_test_now(None);
}

#[gpui::test]
fn opens_a_range_without_today_at_its_configured_start(cx: &mut TestAppContext) {
    set_test_now(Some(utc(2026, 7, 1, 14, 0)));
    let (_, _cx) = harness(
        cx,
        weekly(ScheduleTimeGridViewOptions::default()),
        utc(2026, 5, 13, 14, 0),
        static_events(),
    );
    assert_eq!(scroll_top(), 0.);
    set_test_now(None);
}

#[gpui::test]
fn leaves_the_persons_own_scroll_alone_across_a_clock_tick_and_a_data_refresh(
    cx: &mut TestAppContext,
) {
    set_test_now(Some(utc(2026, 5, 13, 14, 0)));
    let source = static_events();
    let (host_view, cx) = harness(
        cx,
        weekly(ScheduleTimeGridViewOptions::default()),
        utc(2026, 5, 13, 14, 0),
        source.clone(),
    );
    assert_eq!(scroll_top(), 1302.);
    let handle = grid().scroll_handle.unwrap();
    handle.set_offset(gpui::point(px(0.), px(-500.)));
    settle(cx);
    assert_eq!(scroll_top(), 500.);
    set_test_now(Some(utc(2026, 5, 13, 14, 1)));
    cx.executor().advance_clock(Duration::from_secs(60));
    settle(cx);
    assert_eq!(scroll_top(), 500., "a clock tick leaves it");
    let refreshed: ScheduleEventSource = match &*source.borrow() {
        ScheduleEventSource::Events(events) => events.as_ref().clone().into(),
        other => other.clone(),
    };
    *source.borrow_mut() = refreshed;
    host_view.update(cx, |_, cx| cx.notify());
    settle(cx);
    assert_eq!(scroll_top(), 500., "new data leaves it");
    set_test_now(None);
}

#[gpui::test]
fn positions_again_when_the_person_opens_another_range(cx: &mut TestAppContext) {
    set_test_now(Some(utc(2026, 5, 13, 14, 0)));
    let probe = Rc::new(Probe::default());
    let plugin = probe.clone();
    let date = Rc::new(Cell::new(utc(2026, 5, 13, 14, 0)));
    let view = weekly(ScheduleTimeGridViewOptions::default());
    let (_, cx) = host(cx, move || {
        let date_cell = date.clone();
        sized(
            Schedule::new("s", view.clone(), events(), date.get())
                .timezone_id("UTC")
                .plugins([plugin.clone() as Rc<dyn SchedulePlugin>])
                .on_change_date(move |next, window, _| {
                    date_cell.set(next);
                    window.refresh();
                }),
        )
    });
    assert_eq!(scroll_top(), 1302.);
    let next = probe.context.borrow().clone().unwrap().on_next_date.clone();
    cx.update(|window, cx| next(window, cx));
    settle(cx);
    assert_eq!(scroll_top(), 0.);
    let previous = probe
        .context
        .borrow()
        .clone()
        .unwrap()
        .on_previous_date
        .clone();
    cx.update(|window, cx| previous(window, cx));
    settle(cx);
    assert_eq!(scroll_top(), 1302.);
    set_test_now(None);
}

#[gpui::test]
fn restores_the_offset_when_loaded_content_replaces_the_loading_view(cx: &mut TestAppContext) {
    set_test_now(Some(utc(2026, 5, 13, 14, 0)));
    let loader: ScheduleEventSource = ScheduleEventSource::Loader(Rc::new(|_, _, cx: &mut App| {
        let timer = cx.background_executor().timer(Duration::from_secs(1));
        cx.background_spawn(async move {
            timer.await;
            Ok(vec![iso(
                "visible",
                "Visible sync",
                None,
                "2026-05-13T16:00:00.000Z",
                "2026-05-13T16:30:00.000Z",
            )])
        })
    }));
    let (_, cx) = harness(
        cx,
        weekly(ScheduleTimeGridViewOptions::default()),
        utc(2026, 5, 13, 14, 0),
        Rc::new(RefCell::new(loader)),
    );
    assert_eq!(scroll_top(), 1302.);
    grid()
        .scroll_handle
        .unwrap()
        .set_offset(gpui::point(px(0.), px(-700.)));
    settle(cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx);
    assert_eq!(grid().blocks.len(), 1, "the loaded events");
    assert_eq!(scroll_top(), 700.);
    set_test_now(None);
}

// ---- the time grid's long spans --------------------------------------------------------------

fn span_events() -> Vec<CalendarEvent> {
    vec![
        iso(
            "offsite",
            "Offsite",
            None,
            "2026-05-11T09:00:00.000Z",
            "2026-05-13T09:00:00.000Z",
        ),
        iso(
            "handoff",
            "On-call handoff",
            None,
            "2026-05-14T08:00:00.000Z",
            "2026-05-15T08:00:00.000Z",
        ),
        iso(
            "shift",
            "Long shift",
            None,
            "2026-05-15T00:00:00.000Z",
            "2026-05-15T23:59:00.000Z",
        ),
        iso(
            "deploy",
            "Late deploy",
            None,
            "2026-05-12T22:00:00.000Z",
            "2026-05-13T02:00:00.000Z",
        ),
    ]
}

fn span_week<'a>(
    cx: &'a mut TestAppContext,
    render_popover: Option<RenderPopover>,
    events: Vec<CalendarEvent>,
    date: Instant,
    tz: &'static str,
) -> (Entity<Host>, &'a mut VisualTestContext) {
    let view = weekly(grid_options(0., 24., 20., render_popover));
    host(cx, move || {
        sized(Schedule::new("s", view.clone(), events.clone(), date).timezone_id(tz))
    })
}

#[gpui::test]
fn paints_a_timed_event_of_24_hours_or_more_as_one_all_day_span(cx: &mut TestAppContext) {
    let (_, _cx) = span_week(
        cx,
        Some(details()),
        span_events(),
        utc(2026, 5, 13, 12, 0),
        "UTC",
    );
    let drawn = grid();
    let offsite: Vec<_> = drawn.spans.iter().filter(|s| s.0 == "offsite").collect();
    assert_eq!(offsite.len(), 1);
    assert_eq!(
        offsite[0].4.as_deref(),
        Some(
            "Offsite, May 11 at 9:00\u{202f}AM\u{2009}\u{2013}\u{2009}May 13 at 9:00\u{202f}AM, Event, Monday, May 11, 2026"
        )
    );
    assert!(
        drawn.blocks.iter().all(|b| b.0 != "offsite"),
        "no day column paints it"
    );
    assert_eq!(drawn.spans.iter().filter(|s| s.0 == "handoff").count(), 1);
}

#[gpui::test]
fn keeps_a_timed_event_shorter_than_24_hours_in_the_day_columns(cx: &mut TestAppContext) {
    let (_, _cx) = span_week(
        cx,
        Some(details()),
        span_events(),
        utc(2026, 5, 13, 12, 0),
        "UTC",
    );
    let drawn = grid();
    assert!(
        drawn
            .spans
            .iter()
            .all(|s| s.0 != "shift" && s.0 != "deploy")
    );
    let days_of = |id: &str| {
        drawn
            .blocks
            .iter()
            .filter(|b| b.0 == id)
            .map(|b| b.1.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(days_of("shift"), ["2026-05-15"]);
    assert_eq!(days_of("deploy"), ["2026-05-12", "2026-05-13"]);
}

#[gpui::test]
fn measures_the_threshold_on_the_events_own_instants_across_daylight_saving_days(
    cx: &mut TestAppContext,
) {
    let dst = vec![
        iso(
            "spring",
            "Spring day",
            None,
            "2026-03-08T08:00:00.000Z",
            "2026-03-09T07:00:00.000Z",
        ),
        iso(
            "fall",
            "Fall day",
            None,
            "2026-11-01T07:00:00.000Z",
            "2026-11-02T08:00:00.000Z",
        ),
    ];
    let shown = Rc::new(Cell::new(utc(2026, 3, 10, 20, 0)));
    let date = shown.clone();
    let view = weekly(grid_options(0., 24., 20., Some(details())));
    let (host_view, cx) = host(cx, move || {
        sized(
            Schedule::new("s", view.clone(), dst.clone(), date.get())
                .timezone_id("America/Los_Angeles"),
        )
    });
    let drawn = grid();
    assert!(drawn.spans.iter().all(|s| s.0 != "spring"));
    assert_eq!(
        drawn
            .blocks
            .iter()
            .filter(|b| b.0 == "spring")
            .map(|b| b.1.as_str())
            .collect::<Vec<_>>(),
        ["2026-03-08"]
    );
    shown.set(utc(2026, 11, 3, 20, 0));
    host_view.update(cx, |_, cx| cx.notify());
    settle(cx);
    let drawn = grid();
    assert_eq!(drawn.spans.iter().filter(|s| s.0 == "fall").count(), 1);
    assert!(drawn.blocks.iter().all(|b| b.0 != "fall"));
}

#[gpui::test]
fn lists_a_long_timed_event_in_the_hidden_grids_all_day_cells_only(cx: &mut TestAppContext) {
    let (_, _cx) = span_week(cx, None, span_events(), utc(2026, 5, 13, 12, 0), "UTC");
    let offsite: Vec<String> = grid()
        .hidden_grid
        .iter()
        .flatten()
        .filter(|l| l.contains("Offsite"))
        .cloned()
        .collect();
    let days: Vec<&str> = offsite
        .iter()
        .map(|l| l.split('.').next().unwrap())
        .collect();
    assert_eq!(
        days,
        [
            "Monday, May 11, 2026 all day",
            "Tuesday, May 12, 2026 all day",
            "Wednesday, May 13, 2026 all day"
        ]
    );
    for label in &offsite {
        assert_eq!(
            label.split(". ").nth(1),
            Some(
                "Offsite, Event, May 11 at 9:00\u{202f}AM\u{2009}\u{2013}\u{2009}May 13 at 9:00\u{202f}AM"
            )
        );
    }
}

// ---- useCurrentTime -------------------------------------------------------------------------

#[gpui::test]
fn reads_the_clock_when_the_first_subscriber_mounts(cx: &mut TestAppContext) {
    set_test_now(Some(utc(2026, 1, 1, 9, 0)));
    let probe = Rc::new(Probe::default());
    let plugin = probe.clone();
    let mounted = Rc::new(Cell::new(false));
    let shown = mounted.clone();
    let view = weekly(ScheduleTimeGridViewOptions::default());
    let (host_view, cx) = host(cx, move || {
        if !shown.get() {
            return div().into_any_element();
        }
        sized(
            Schedule::new("s", view.clone(), events(), utc(2026, 1, 2, 0, 0))
                .timezone_id("UTC")
                .plugins([plugin.clone() as Rc<dyn SchedulePlugin>]),
        )
    });
    set_test_now(Some(utc(2026, 1, 2, 9, 0)));
    mounted.set(true);
    host_view.update(cx, |_, cx| cx.notify());
    settle(cx);
    assert_eq!(
        probe.context.borrow().as_ref().unwrap().current_time,
        utc(2026, 1, 2, 9, 0)
    );
    // Later reads keep the mounted reading until the next tick.
    set_test_now(Some(utc(2026, 1, 2, 9, 0) + 30_000.));
    host_view.update(cx, |_, cx| cx.notify());
    settle(cx);
    assert_eq!(
        probe.context.borrow().as_ref().unwrap().current_time,
        utc(2026, 1, 2, 9, 0)
    );
    cx.executor().advance_clock(Duration::from_secs(60));
    settle(cx);
    assert_eq!(
        probe.context.borrow().as_ref().unwrap().current_time,
        utc(2026, 1, 2, 9, 0) + 30_000.
    );
    set_test_now(None);
}

#[gpui::test]
fn re_reads_the_clock_after_the_last_subscriber_unmounts(cx: &mut TestAppContext) {
    set_test_now(Some(utc(2026, 1, 1, 9, 0)));
    let probe = Rc::new(Probe::default());
    let plugin = probe.clone();
    let mounted = Rc::new(Cell::new(true));
    let shown = mounted.clone();
    let view = weekly(ScheduleTimeGridViewOptions::default());
    let (host_view, cx) = host(cx, move || {
        if !shown.get() {
            return div().into_any_element();
        }
        sized(
            Schedule::new("s", view.clone(), events(), utc(2026, 1, 2, 0, 0))
                .timezone_id("UTC")
                .plugins([plugin.clone() as Rc<dyn SchedulePlugin>]),
        )
    });
    assert_eq!(
        probe.context.borrow().as_ref().unwrap().current_time,
        utc(2026, 1, 1, 9, 0)
    );
    mounted.set(false);
    host_view.update(cx, |_, cx| cx.notify());
    settle(cx);
    set_test_now(Some(utc(2026, 1, 1, 17, 0)));
    mounted.set(true);
    host_view.update(cx, |_, cx| cx.notify());
    settle(cx);
    assert_eq!(
        probe.context.borrow().as_ref().unwrap().current_time,
        utc(2026, 1, 1, 17, 0)
    );
    set_test_now(None);
}
