//! Calendar examples: the gallery's Dates page and Calendar's doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div};
use rustryx_i18n::CalendarSystem;
use rustryx_i18n::date::{DateRange, PlainDate, Weekday};

use crate::calendar::{Calendar, CalendarDaysOptions, CalendarMode, use_calendar_days};
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextType};

fn date(y: i32, m: u8, d: u8) -> PlainDate {
    PlainDate::new(y, m, d).expect("a real date")
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let picked = window.use_keyed_state("calendar-picked", cx, |_, _| Some(date(2026, 11, 18)));
    let trip = window.use_keyed_state("calendar-trip", cx, |_, _| {
        Some(DateRange::new(date(2026, 11, 20), date(2026, 12, 3)))
    });
    let value = *picked.read(cx);
    let range = *trip.read(cx);

    // A custom month view from the public hook: the weekday names and the
    // day numbers of Aban 1405 in the Persian calendar.
    let days = use_calendar_days(
        CalendarDaysOptions {
            year: 1405,
            month: 8,
            week_starts_on: Weekday::Saturday,
            calendar: CalendarSystem::Persian,
            ..Default::default()
        },
        cx,
    );
    let custom = Stack::vertical()
        .gap(SpacingStep::S1)
        .children(days.weeks.iter().take(2).map(|week| {
            Text::new(
                week.iter()
                    .map(|d| {
                        if d.is_outside {
                            "·".to_string()
                        } else {
                            d.day_number.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
            )
            .text_type(TextType::Supporting)
        }));

    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("Single date"))
        .child(
            div().flex().flex_row().flex_wrap().gap_6().child(
                Calendar::new("single")
                    .value(value)
                    .on_change(move |d, window, cx| {
                        picked.update(cx, |p, _| *p = Some(d));
                        window.refresh();
                    }),
            ),
        )
        .child(label(
            "Range over two months, at most 21 days, weekdays only",
        ))
        .child(
            Calendar::new("range")
                .mode(CalendarMode::Range)
                .range_value(range)
                .number_of_months(2)
                .max_range_span(21_u32)
                .date_constraint(|d| !matches!(d.weekday(), Weekday::Saturday | Weekday::Sunday))
                .on_range_change(move |r, window, cx| {
                    trip.update(cx, |t, _| *t = Some(r));
                    window.refresh();
                }),
        )
        .child(label("Bounds, week numbers, weeks from Monday"))
        .child(
            Calendar::new("bounded")
                .default_value(date(2026, 11, 10))
                .min(date(2026, 11, 3))
                .max(date(2026, 12, 15))
                .has_week_numbers(true)
                .week_starts_on("mon"),
        )
        .child(label("Persian and Hebrew calendars; values stay Gregorian"))
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_6()
                .child(
                    Calendar::new("persian")
                        .calendar(CalendarSystem::Persian)
                        .default_value(date(2026, 10, 30))
                        .week_starts_on(Weekday::Saturday),
                )
                .child(
                    Calendar::new("hebrew")
                        .calendar(CalendarSystem::Hebrew)
                        .default_value(date(2024, 2, 20))
                        .has_variable_row_count(true),
                ),
        )
        .child(label(
            "A custom month view from use_calendar_days (Aban 1405)",
        ))
        .child(custom)
        .into_any_element()
}
