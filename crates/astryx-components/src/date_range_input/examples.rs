//! DateRangeInput examples: the gallery's Dates page and DateRangeInput's
//! doc example.

use astryx_i18n::date::{DateRange, PlainDate};
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use crate::date_range_input::{DateRangeInput, DateRangePreset};
use crate::layout::{SpacingStep, Stack};

fn last(days: i64) -> DateRange {
    let today = PlainDate::today();
    DateRange::new(today.add_days(1 - days), today)
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let report = window.use_keyed_state("dri-report", cx, |_, _| Some(last(7)));
    let stay = window.use_keyed_state("dri-stay", cx, |_, _| None::<DateRange>);
    let store = |state: gpui::Entity<Option<DateRange>>| {
        move |value: Option<DateRange>, window: &mut Window, cx: &mut App| {
            state.update(cx, |v, _| *v = value);
            window.refresh();
        }
    };
    let today = PlainDate::today();
    Stack::vertical()
        .gap(SpacingStep::S4)
        .w(px(360.))
        .child(
            DateRangeInput::new("report", "Reporting period", *report.read(cx))
                .description("Pick a preset or two days")
                .max(today)
                .presets([
                    DateRangePreset::new("Last 7 days", || last(7)),
                    DateRangePreset::new("Last 30 days", || last(30)),
                    DateRangePreset::new("Last 90 days", || last(90)),
                    DateRangePreset::new("Next 7 days", || {
                        let today = PlainDate::today();
                        DateRange::new(today, today.add_days(6))
                    }),
                ])
                .on_change(store(report.clone())),
        )
        .child(
            DateRangeInput::new("stay", "Stay", *stay.read(cx))
                .description("Two to fourteen nights, weekdays only")
                .min(today)
                .min_range_span(2)
                .max_range_span(14)
                .number_of_months(1)
                .date_constraint(|date| date.weekday().index() % 6 != 0)
                .on_change(store(stay.clone())),
        )
        .into_any_element()
}
