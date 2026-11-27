//! Schedule's gallery (upstream's `Schedule` stories): the month, week, day and list views of a
//! fixed week, a right-to-left locale, an async loader, overlapping events, the event and
//! month popovers, days whose midnight daylight saving skips, events ending at midnight, a
//! busy month, the view selector and timed events of a day or more.

use std::rc::Rc;
use std::time::Duration;

use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Text, TextColor, TextType, TextWeight};
use astryx_i18n::date::{PlainDate, PlainDateTime, PlainTime};
use astryx_i18n::datetime::{day_of_instant, instant_in_zone};
use astryx_i18n::{I18n, TimeZoneChoice};
use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, Entity, Window, div, px};

use super::{
    CalendarEvent, Instant, RenderPopover, Schedule, ScheduleCategory, ScheduleDayViewOptions,
    ScheduleEventColor, ScheduleEventSource, ScheduleListViewOptions, ScheduleMonthlyViewOptions,
    ScheduleTimeGridViewOptions, ScheduleView, ScheduleViewSelectorOption,
    ScheduleViewSelectorPluginOptions, ScheduleWeeklyViewOptions, create_event_from_iso,
    create_schedule_day_view, create_schedule_list_view, create_schedule_monthly_view,
    create_schedule_weekly_view, schedule_pagination_plugin, schedule_view_selector_plugin,
};

// The demo week is fixed and its events are built in the Schedule's own zone, so every
// viewer sees the same week on the 7 to 19 grid, whatever their zone or today's date.
const STORY_TIMEZONE: &str = "America/Los_Angeles";
// A Wednesday.
const STORY_DATE: PlainDate = PlainDate {
    year: 2026,
    month: 5,
    day: 13,
};
// A fixed week in UTC, so the overlap layout is identical on every machine.
const FIXTURE_TIMEZONE: &str = "UTC";

fn zone(id: &str) -> TimeZoneChoice {
    TimeZoneChoice::Iana(id.to_string())
}

/// A wall-clock time on a day in a zone, epoch milliseconds.
fn at(date: PlainDate, timezone_id: &str, hour: u8, minute: u8) -> Instant {
    let time = PlainDateTime::new(
        date,
        PlainTime {
            hour,
            minute,
            second: 0,
        },
    );
    instant_in_zone(time, &zone(timezone_id)) as f64 * 1000.
}

fn focus_date() -> Instant {
    at(STORY_DATE, STORY_TIMEZONE, 12, 0)
}

fn fixture_date() -> Instant {
    at(
        PlainDate {
            year: 2026,
            month: 5,
            day: 13,
        },
        FIXTURE_TIMEZONE,
        12,
        0,
    )
}

fn categories() -> Vec<ScheduleCategory> {
    use ScheduleEventColor::*;
    [
        ("Company", Blue),
        ("Design", Purple),
        ("Launch", Green),
        ("Focus", Teal),
        ("Holiday", Yellow),
        ("Retro", Orange),
        ("Incident", Red),
        ("Migration", Pink),
    ]
    .into_iter()
    .map(|(label, color)| ScheduleCategory::new(label, color))
    .collect()
}

fn timed(
    id: &str,
    title: &str,
    category: &str,
    day: PlainDate,
    start: (u8, u8),
    end_day: PlainDate,
    end: (u8, u8),
) -> CalendarEvent {
    CalendarEvent::instant(
        id.to_string(),
        title.to_string(),
        at(day, STORY_TIMEZONE, start.0, start.1),
        at(end_day, STORY_TIMEZONE, end.0, end.1),
    )
    .category(category.to_string())
}

fn all_day(
    id: &str,
    title: &str,
    category: &str,
    start: PlainDate,
    end: PlainDate,
) -> CalendarEvent {
    CalendarEvent::day(id.to_string(), title.to_string(), start, end).category(category.to_string())
}

fn iso(id: &str, title: &str, category: &str, start: &str, end: &str) -> CalendarEvent {
    create_event_from_iso(
        id.to_string(),
        title.to_string(),
        Some(category.to_string().into()),
        start,
        end,
    )
    .expect("a valid date")
}

fn events() -> Vec<CalendarEvent> {
    let now = STORY_DATE;
    let week_start = now.add_days(-3);
    vec![
        timed(
            "all-hands",
            "Company all hands",
            "Company",
            now,
            (9, 0),
            now,
            (10, 0),
        ),
        timed(
            "all-hands-previous-day",
            "Company all hands",
            "Company",
            now.add_days(-1),
            (9, 0),
            now.add_days(-1),
            (10, 0),
        ),
        timed(
            "all-hands-next-day",
            "Company all hands",
            "Company",
            now.add_days(1),
            (9, 0),
            now.add_days(1),
            (10, 0),
        ),
        timed(
            "design-review",
            "Design review",
            "Design",
            now,
            (11, 0),
            now,
            (12, 30),
        ),
        all_day(
            "launch",
            "Launch window",
            "Launch",
            week_start.add_days(3),
            week_start.add_days(4),
        ),
        all_day("focus-day", "Focus day", "Focus", now, now),
        all_day(
            "company-holiday",
            "Company holiday",
            "Holiday",
            week_start.add_days(6),
            week_start.add_days(6),
        ),
        timed(
            "retro",
            "Weekly retro",
            "Retro",
            week_start.add_days(5),
            (14, 0),
            week_start.add_days(5),
            (15, 0),
        ),
        timed(
            "incident",
            "Incident review",
            "Incident",
            week_start.add_days(1),
            (10, 30),
            week_start.add_days(1),
            (11, 30),
        ),
        timed(
            "overnight-migration",
            "Overnight migration",
            "Migration",
            week_start.add_days(5),
            (23, 30),
            week_start.add_days(6),
            (2, 0),
        ),
    ]
}

/// Upstream's seeded events for a loaded range: one to three timed events a day, and
/// sometimes a focus day.
fn create_async_events(start: Instant, end: Instant) -> Vec<CalendarEvent> {
    let mut value = (start / 86_400_000.).floor();
    let mut random = move || {
        value = (value * 1_664_525. + 1_013_904_223.) % 4_294_967_296.;
        value / 4_294_967_296.
    };
    let categories = categories();
    let titles = [
        "Planning sync",
        "Design critique",
        "Customer review",
        "Metrics readout",
        "Launch check",
        "Office hours",
    ];
    let mut generated = Vec::new();
    let mut day_start = start;
    while day_start < end {
        let count = 1 + (random() * 3.).floor() as usize;
        // Midday, so a 23- or 25-hour daylight-saving day still lands on its date.
        let day = day_of_instant(
            ((day_start + 43_200_000.) / 1000.) as i64,
            &zone(STORY_TIMEZONE),
        );
        for index in 0..count {
            let hour = 8 + (random() * 8.).floor() as u8;
            let minute = if random() > 0.65 { 30 } else { 0 };
            let duration = 1 + (random() * 2.).floor() as u8;
            let title = titles[(random() * titles.len() as f64).floor() as usize];
            let category = &categories[(random() * 6.).floor() as usize];
            generated.push(timed(
                &format!("{day_start}:{index}"),
                title,
                &category.label,
                day,
                (hour, minute),
                day,
                (hour + duration, minute),
            ));
        }
        if random() > 0.68 {
            let category = &categories[(random() * 6.).floor() as usize];
            generated.push(all_day(
                &format!("{day_start}:all-day"),
                "Focus day",
                &category.label,
                day,
                day,
            ));
        }
        day_start += 86_400_000.;
    }
    generated
}

fn fixture(
    id: &str,
    title: &str,
    category: &str,
    day: u8,
    start: (u8, u8),
    end: (u8, u8),
) -> CalendarEvent {
    let stamp =
        |(hour, minute): (u8, u8)| format!("2026-05-{day:02}T{hour:02}:{minute:02}:00.000Z");
    iso(id, title, category, &stamp(start), &stamp(end))
}

fn overlapping_events() -> Vec<CalendarEvent> {
    vec![
        // Monday: an exact tie, the same title and minutes with different ids.
        fixture("tie-a", "Pair review", "Design", 11, (9, 0), (10, 0)),
        fixture("tie-b", "Pair review", "Design", 11, (9, 0), (10, 0)),
        // Tuesday: three simultaneous events.
        fixture(
            "trio-1",
            "Interview loop 1",
            "Company",
            12,
            (13, 0),
            (14, 0),
        ),
        fixture("trio-2", "Interview loop 2", "Launch", 12, (13, 0), (14, 0)),
        fixture("trio-3", "Interview loop 3", "Focus", 12, (13, 0), (14, 0)),
        // Wednesday: five simultaneous events.
        fixture("quint-1", "Office hours A", "Company", 13, (10, 0), (11, 0)),
        fixture("quint-2", "Office hours B", "Design", 13, (10, 0), (11, 0)),
        fixture("quint-3", "Office hours C", "Launch", 13, (10, 0), (11, 0)),
        fixture("quint-4", "Office hours D", "Focus", 13, (10, 0), (11, 0)),
        fixture("quint-5", "Office hours E", "Retro", 13, (10, 0), (11, 0)),
        // Thursday: a chain whose first and last do not overlap each other.
        fixture("chain-1", "Standup", "Company", 14, (9, 0), (10, 0)),
        fixture("chain-2", "Design sync", "Design", 14, (9, 30), (10, 30)),
        fixture("chain-3", "Retro", "Retro", 14, (10, 0), (11, 0)),
        // Friday: containment, and the shortest block the grid draws.
        fixture("contain-long", "Workshop", "Launch", 15, (9, 0), (12, 0)),
        fixture(
            "contain-short",
            "Coffee chat",
            "Focus",
            15,
            (10, 0),
            (10, 30),
        ),
        fixture(
            "quarter",
            "Quick check-in",
            "Company",
            15,
            (14, 0),
            (14, 15),
        ),
        // All-day spans that overlap each other.
        iso("offsite", "Offsite", "Company", "2026-05-11", "2026-05-12"),
        iso(
            "hack-week",
            "Hack week",
            "Launch",
            "2026-05-12",
            "2026-05-13",
        ),
    ]
}

fn midnight_events() -> Vec<CalendarEvent> {
    vec![
        iso(
            "planning",
            "Planning",
            "Focus",
            "2026-05-13T17:00:00.000Z",
            "2026-05-13T18:00:00.000Z",
        ),
        iso(
            "late-sync",
            "Late sync",
            "Company",
            "2026-05-13T22:00:00.000Z",
            "2026-05-14T00:00:00.000Z",
        ),
        iso(
            "evening-review",
            "Evening review",
            "Design",
            "2026-05-14T20:00:00.000Z",
            "2026-05-15T00:00:00.000Z",
        ),
    ]
}

// Wednesday May 13 needs five levels and Friday May 15 four, so both trade their third
// level for "+N more"; the other days fit and paint every chip.
fn busy_month_events() -> Vec<CalendarEvent> {
    let one_hour = |id: &str, title: &str, category: &str, day: u8, hour: u8| {
        fixture(id, title, category, day, (hour, 0), (hour + 1, 0))
    };
    vec![
        iso(
            "conference",
            "Design conference",
            "Design",
            "2026-05-10",
            "2026-05-13",
        ),
        iso(
            "hack-week",
            "Hack week",
            "Launch",
            "2026-05-10",
            "2026-05-12",
        ),
        iso(
            "offsite",
            "Team offsite",
            "Company",
            "2026-05-11",
            "2026-05-13",
        ),
        one_hour("standup", "Standup", "Company", 13, 9),
        one_hour("incident-review", "Incident review", "Incident", 13, 11),
        one_hour("launch-check", "Launch check", "Launch", 13, 14),
        one_hour("planning", "Planning", "Company", 15, 9),
        one_hour("critique", "Design critique", "Design", 15, 10),
        one_hour("retro", "Weekly retro", "Retro", 15, 13),
        one_hour("focus", "Focus block", "Focus", 15, 15),
        one_hour("one-on-one", "1:1", "Company", 5, 10),
        iso(
            "holiday",
            "Company holiday",
            "Holiday",
            "2026-05-25",
            "2026-05-25",
        ),
    ]
}

// A 48-hour offsite and an exactly 24-hour handoff are all-day spans; a 23-hour-59-minute
// shift and an overnight deploy stay in the day columns; Saturday's retreat has too long
// a title for its times to fit.
fn long_span_events() -> Vec<CalendarEvent> {
    vec![
        iso(
            "offsite",
            "Offsite",
            "Company",
            "2026-05-11T09:00:00.000Z",
            "2026-05-13T09:00:00.000Z",
        ),
        iso(
            "handoff",
            "On-call handoff",
            "Incident",
            "2026-05-14T08:00:00.000Z",
            "2026-05-15T08:00:00.000Z",
        ),
        iso(
            "retreat",
            "Leadership planning retreat",
            "Company",
            "2026-05-16T06:00:00.000Z",
            "2026-05-17T06:00:00.000Z",
        ),
        iso(
            "shift",
            "Long shift",
            "Focus",
            "2026-05-15T00:00:00.000Z",
            "2026-05-15T23:59:00.000Z",
        ),
        iso(
            "deploy",
            "Late deploy",
            "Launch",
            "2026-05-12T22:00:00.000Z",
            "2026-05-13T02:00:00.000Z",
        ),
        fixture("standup", "Standup", "Company", 12, (9, 0), (9, 30)),
        fixture("review", "Design review", "Design", 12, (10, 0), (11, 0)),
    ]
}

/// The details a popover shows: the title, then the category and the id.
fn details(skip: &'static str) -> RenderPopover {
    Rc::new(move |event: &CalendarEvent, _: &mut Window, _: &mut App| {
        (event.id.as_ref() != skip).then(|| {
            Stack::vertical()
                .gap(SpacingStep::S1)
                .child(
                    Text::new(event.title.clone())
                        .text_type(TextType::Label)
                        .weight(TextWeight::Bold),
                )
                .child(
                    Text::new(format!(
                        "{} · {}",
                        event.category.as_deref().unwrap_or("No category"),
                        event.id
                    ))
                    .text_type(TextType::Supporting)
                    .color(TextColor::Secondary),
                )
                .into_any_element()
        })
    })
}

fn weekly(min_hour: f64, max_hour: f64, render_popover: Option<RenderPopover>) -> ScheduleView {
    create_schedule_weekly_view(ScheduleWeeklyViewOptions {
        week_starts_on: 0,
        time_grid: ScheduleTimeGridViewOptions {
            min_hour,
            max_hour,
            render_popover,
            ..Default::default()
        },
    })
}

/// A story's state: its view, made once, and the date it shows.
struct Story {
    view: ScheduleView,
    date: Instant,
}

fn story(
    key: &str,
    date: Instant,
    view: impl FnOnce() -> ScheduleView,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Story> {
    window.use_keyed_state(
        ElementId::Name(format!("schedule-example-{key}").into()),
        cx,
        |_, _| Story { view: view(), date },
    )
}

/// A Schedule that pages its story's date.
fn controlled(
    key: &str,
    state: &Entity<Story>,
    events: impl Into<ScheduleEventSource>,
    timezone_id: &str,
    cx: &App,
) -> Schedule {
    let Story { view, date } = state.read(cx);
    let state = state.clone();
    Schedule::new(
        ElementId::Name(format!("schedule-{key}").into()),
        view.clone(),
        events,
        *date,
    )
    .categories(categories())
    .timezone_id(timezone_id.to_string())
    .on_change_date(move |date, _, cx| {
        state.update(cx, |s, cx| {
            s.date = date;
            cx.notify();
        })
    })
}

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

/// Upstream's stories, top to bottom.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let focus = focus_date();
    let fixed = fixture_date();
    let mut column = Stack::vertical().gap(SpacingStep::S8);

    let state = story(
        "monthly",
        focus,
        || create_schedule_monthly_view(ScheduleMonthlyViewOptions::default()),
        window,
        cx,
    );
    column = column.child(example(
        "Monthly",
        controlled("monthly", &state, events(), STORY_TIMEZONE, cx).focus_date(focus),
    ));

    let state = story("weekly", focus, || weekly(7., 19., None), window, cx);
    column = column.child(example(
        "Weekly",
        controlled("weekly", &state, events(), STORY_TIMEZONE, cx),
    ));

    let state = story("weekly-fixed", focus, || weekly(7., 19., None), window, cx);
    column = column.child(example(
        "Weekly, fixed height",
        div()
            .h(px(520.))
            .flex()
            .flex_col()
            .child(controlled("weekly-fixed", &state, events(), STORY_TIMEZONE, cx).h_full()),
    ));

    let state = story(
        "day",
        focus,
        || create_schedule_day_view(ScheduleDayViewOptions::default()),
        window,
        cx,
    );
    column = column.child(example(
        "Day",
        controlled("day", &state, events(), STORY_TIMEZONE, cx).focus_date(focus),
    ));

    let state = story(
        "list",
        focus,
        || create_schedule_list_view(ScheduleListViewOptions::default()),
        window,
        cx,
    );
    column = column.child(example(
        "List",
        controlled("list", &state, events(), STORY_TIMEZONE, cx).focus_date(focus),
    ));

    // The list in a right-to-left locale: its 24-hour ranges read start first.
    let state = story(
        "rtl",
        focus,
        || create_schedule_list_view(ScheduleListViewOptions::default()),
        window,
        cx,
    );
    let hebrew = controlled("rtl", &state, events(), STORY_TIMEZONE, cx).focus_date(focus);
    column = column.child(example(
        "Right-to-left locale",
        astryx_i18n::provider(I18n::new("he-IL"), hebrew),
    ));

    let state = story("async", focus, || weekly(7., 19., None), window, cx);
    let loader = window.use_keyed_state("schedule-example-loader", cx, |_, _| {
        ScheduleEventSource::Loader(Rc::new(|start, end, cx: &mut App| {
            let delay = cx.background_executor().timer(Duration::from_millis(300));
            cx.background_spawn(async move {
                delay.await;
                Ok(create_async_events(start, end))
            })
        }))
    });
    let loader = loader.read(cx).clone();
    column = column.child(example(
        "Async loader",
        controlled("async", &state, loader, STORY_TIMEZONE, cx).focus_date(focus),
    ));

    let state = story("overlapping", fixed, || weekly(8., 18., None), window, cx);
    column = column.child(example(
        "Overlapping events",
        controlled(
            "overlapping",
            &state,
            overlapping_events(),
            FIXTURE_TIMEZONE,
            cx,
        )
        .focus_date(fixed),
    ));

    // Each event is a button that opens the view's popover; "Quick check-in" has no content.
    let state = story(
        "popover",
        fixed,
        || weekly(8., 18., Some(details("quarter"))),
        window,
        cx,
    );
    column = column.child(example(
        "Event popover",
        controlled(
            "popover",
            &state,
            overlapping_events(),
            FIXTURE_TIMEZONE,
            cx,
        )
        .focus_date(fixed),
    ));

    // Weeks and days whose midnight daylight saving skips keep their own dates.
    let mut zones = Stack::vertical().gap(SpacingStep::S6);
    for (key, timezone_id, date, first_hour) in [
        (
            "santiago",
            "America/Santiago",
            at(
                PlainDate {
                    year: 2026,
                    month: 9,
                    day: 6,
                },
                "UTC",
                15,
                0,
            ),
            ("2026-09-06T04:30:00.000Z", "2026-09-06T05:30:00.000Z"),
        ),
        (
            "havana",
            "America/Havana",
            at(
                PlainDate {
                    year: 2026,
                    month: 3,
                    day: 8,
                },
                "UTC",
                15,
                0,
            ),
            ("2026-03-08T05:30:00.000Z", "2026-03-08T06:30:00.000Z"),
        ),
    ] {
        let first = vec![iso(
            &format!("{timezone_id}-first-hour"),
            "First hour",
            "Focus",
            first_hour.0,
            first_hour.1,
        )];
        let week = story(
            &format!("{key}-week"),
            date,
            || weekly(0., 4., None),
            window,
            cx,
        );
        let day = story(
            &format!("{key}-day"),
            date,
            || create_schedule_day_view(grid(0., 4.)),
            window,
            cx,
        );
        zones = zones.child(
            Stack::vertical()
                .gap(SpacingStep::S2)
                .child(
                    Text::new(timezone_id)
                        .text_type(TextType::Label)
                        .weight(TextWeight::Bold),
                )
                .child(
                    controlled(
                        &format!("{key}-week"),
                        &week,
                        first.clone(),
                        timezone_id,
                        cx,
                    )
                    .focus_date(date),
                )
                .child(
                    controlled(&format!("{key}-day"), &day, first, timezone_id, cx)
                        .focus_date(date),
                ),
        );
    }
    column = column.child(example("Skipped midnight", zones));

    // Timed events that end exactly at midnight paint to the bottom of their day.
    let state = story("midnight", fixed, || weekly(16., 24., None), window, cx);
    column = column.child(example(
        "Event ending at midnight",
        controlled("midnight", &state, midnight_events(), FIXTURE_TIMEZONE, cx).focus_date(fixed),
    ));

    // A week row paints at most three levels; a busy day shows two and "+N more".
    let state = story(
        "overflow",
        fixed,
        || create_schedule_monthly_view(ScheduleMonthlyViewOptions::default()),
        window,
        cx,
    );
    column = column.child(example(
        "Month overflow",
        controlled(
            "overflow",
            &state,
            busy_month_events(),
            FIXTURE_TIMEZONE,
            cx,
        )
        .focus_date(fixed),
    ));

    // Chips with content open the month's one popover; the holiday stays static text.
    let state = story(
        "month-popover",
        fixed,
        || {
            create_schedule_monthly_view(ScheduleMonthlyViewOptions {
                week_starts_on: 0,
                render_popover: Some(details("holiday")),
            })
        },
        window,
        cx,
    );
    column = column.child(example(
        "Month event popover",
        controlled(
            "month-popover",
            &state,
            busy_month_events(),
            FIXTURE_TIMEZONE,
            cx,
        )
        .focus_date(fixed),
    ));

    // The pagination controls and a view selector together.
    let views = window.use_keyed_state("schedule-example-views", cx, |_, _| {
        vec![
            ScheduleViewSelectorOption::new(
                create_schedule_monthly_view(ScheduleMonthlyViewOptions::default()),
                "Month",
            ),
            ScheduleViewSelectorOption::new(weekly(7., 19., None), "Week"),
            ScheduleViewSelectorOption::new(create_schedule_day_view(grid(8., 18.)), "Day"),
            ScheduleViewSelectorOption::new(
                create_schedule_list_view(ScheduleListViewOptions::default()),
                "List",
            ),
        ]
    });
    let options = views.read(cx).clone();
    let week = options[1].view.clone();
    let state = story("selector", focus, move || week, window, cx);
    let change = state.clone();
    let selector = schedule_view_selector_plugin(
        options,
        ScheduleViewSelectorPluginOptions {
            on_change_view: Some(Rc::new(move |view, _, cx| {
                change.update(cx, |s, cx| {
                    s.view = view;
                    cx.notify();
                })
            })),
            ..Default::default()
        },
    );
    column = column.child(example(
        "View selector plugin",
        controlled("selector", &state, events(), STORY_TIMEZONE, cx)
            .focus_date(focus)
            .plugins([schedule_pagination_plugin(Default::default()), selector]),
    ));

    let state = story(
        "long",
        fixed,
        || {
            let title: RenderPopover =
                Rc::new(|event: &CalendarEvent, _: &mut Window, _: &mut App| {
                    Some(
                        Text::new(event.title.clone())
                            .text_type(TextType::Label)
                            .weight(TextWeight::Bold)
                            .into_any_element(),
                    )
                });
            weekly(0., 24., Some(title))
        },
        window,
        cx,
    );
    column = column.child(example(
        "Long timed events",
        controlled("long", &state, long_span_events(), FIXTURE_TIMEZONE, cx).focus_date(fixed),
    ));

    column.into_any_element()
}

fn grid(min_hour: f64, max_hour: f64) -> ScheduleDayViewOptions {
    ScheduleTimeGridViewOptions {
        min_hour,
        max_hour,
        ..Default::default()
    }
}
