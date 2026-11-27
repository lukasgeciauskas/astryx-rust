//! DateTimeInput examples: the gallery's Dates page and DateTimeInput's doc
//! example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};
use rustryx_i18n::date::{PlainDate, PlainDateTime, PlainTime};

use crate::date_input::PickerPresentation;
use crate::date_time_input::{DateTimeInput, TimeOptionInterval};
use crate::layout::{SpacingStep, Stack};

fn at(y: i32, m: u8, d: u8, h: u8, min: u8) -> PlainDateTime {
    PlainDateTime::new(
        PlainDate::new(y, m, d).expect("a real date"),
        PlainTime::new(h, min, 0).expect("a real time"),
    )
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let meeting = window.use_keyed_state("dti-meeting", cx, |_, _| Some(at(2026, 11, 18, 14, 30)));
    let launch = window.use_keyed_state("dti-launch", cx, |_, _| None::<PlainDateTime>);
    let pickup = window.use_keyed_state("dti-pickup", cx, |_, _| Some(at(2026, 12, 2, 9, 15)));
    let store = |state: gpui::Entity<Option<PlainDateTime>>| {
        move |value: Option<PlainDateTime>, window: &mut Window, cx: &mut App| {
            state.update(cx, |v, _| *v = value);
            window.refresh();
        }
    };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .w(px(440.))
        .child(
            DateTimeInput::new("meeting", "Meeting", *meeting.read(cx))
                .description("Pick a day, then a time from the list or type one")
                .time_option_interval(TimeOptionInterval::Thirty)
                .clear(true)
                .on_change(store(meeting.clone())),
        )
        .child(
            DateTimeInput::new("launch", "Launch", *launch.read(cx))
                .min(at(2026, 11, 20, 9, 0))
                .max(at(2026, 12, 31, 17, 0))
                .seconds(true)
                .on_change(store(launch.clone())),
        )
        .child(
            DateTimeInput::new("pickup", "Pickup", *pickup.read(cx))
                .description("Picked in a bottom sheet")
                .presentation(PickerPresentation::BottomSheet)
                .min(at(2026, 11, 26, 8, 0))
                .clear(true)
                .on_change(store(pickup.clone())),
        )
        .into_any_element()
}
