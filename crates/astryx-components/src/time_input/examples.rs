//! TimeInput examples: the gallery's Dates page and TimeInput's doc example.

use astryx_i18n::date::{HourFormat, PlainTime};
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use crate::layout::{SpacingStep, Stack};
use crate::time_input::TimeInput;

fn time(h: u8, m: u8) -> PlainTime {
    PlainTime::new(h, m, 0).expect("a real time")
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let start = window.use_keyed_state("time-input-start", cx, |_, _| Some(time(9, 30)));
    let alarm = window.use_keyed_state("time-input-alarm", cx, |_, _| None::<PlainTime>);
    let store = |state: gpui::Entity<Option<PlainTime>>| {
        move |value: Option<PlainTime>, window: &mut Window, cx: &mut App| {
            state.update(cx, |v, _| *v = value);
            window.refresh();
        }
    };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .w(px(320.))
        .child(
            TimeInput::new("start", "Start time", *start.read(cx))
                .description("Business hours, in steps of 15 minutes")
                .min(time(9, 0))
                .max(time(17, 0))
                .increment(15)
                .clear(true)
                .on_change(store(start.clone())),
        )
        .child(
            TimeInput::new("alarm", "Alarm", *alarm.read(cx))
                .hour_format(HourFormat::H24)
                .seconds(true)
                .on_change(store(alarm.clone())),
        )
        .into_any_element()
}
