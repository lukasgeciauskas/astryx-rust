//! DateInput examples: the gallery's Dates page and DateInput's doc example.

use astryx_i18n::CalendarSystem;
use astryx_i18n::date::{PlainDate, SharedDateFormat, Weekday};
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use crate::date_input::{DateInput, DateInputEntryError, PickerPresentation};
use crate::field::InputStatus;
use crate::layout::{SpacingStep, Stack};

fn date(y: i32, m: u8, d: u8) -> PlainDate {
    PlainDate::new(y, m, d).expect("a real date")
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let due = window.use_keyed_state("date-input-due", cx, |_, _| Some(date(2026, 11, 18)));
    let start = window.use_keyed_state("date-input-start", cx, |_, _| None::<PlainDate>);
    let persian = window.use_keyed_state("date-input-persian", cx, |_, _| Some(date(2026, 10, 30)));
    let trip = window.use_keyed_state("date-input-trip", cx, |_, _| Some(date(2026, 12, 4)));
    let visit = window.use_keyed_state("date-input-visit", cx, |_, _| Some(date(2026, 12, 15)));
    let error = window.use_keyed_state("date-input-error", cx, |_, _| None::<DateInputEntryError>);
    let store = |state: gpui::Entity<Option<PlainDate>>| {
        move |value: Option<PlainDate>, window: &mut Window, cx: &mut App| {
            state.update(cx, |v, _| *v = value);
            window.refresh();
        }
    };
    let status = error
        .read(cx)
        .as_ref()
        .filter(|e| e.kind == crate::date_input::DateEntryErrorKind::OutOfRange)
        .map(|_| InputStatus::error("Pick a day this year"));

    Stack::vertical()
        .gap(SpacingStep::S4)
        .w(px(320.))
        .child(
            DateInput::new("due", "Due date", *due.read(cx))
                .description("Type a date or pick one")
                .clear(true)
                .on_change(store(due.clone())),
        )
        .child({
            let mut field = DateInput::new("start", "Start date", *start.read(cx))
                .format(SharedDateFormat::Date)
                .min(date(2026, 1, 1))
                .max(date(2026, 12, 31))
                .week_starts_on(Weekday::Monday)
                .on_entry_error_change({
                    let error = error.clone();
                    move |e, window, cx| {
                        error.update(cx, |v, _| *v = e);
                        window.refresh();
                    }
                })
                .on_change(store(start.clone()));
            if let Some(status) = status {
                field = field.status(status);
            }
            field
        })
        .child(
            DateInput::new("trip", "Trip day", *trip.read(cx))
                .description("Picked in a bottom sheet")
                .presentation(PickerPresentation::BottomSheet)
                .min(date(2026, 11, 1))
                .max(date(2027, 6, 30))
                .clear(true)
                .on_change(store(trip.clone())),
        )
        .child(
            DateInput::new("visit", "Visit", *visit.read(cx))
                .description("The platform's own picker, where it has one")
                .presentation(PickerPresentation::Native)
                .min(date(2026, 11, 1))
                .clear(true)
                .on_change(store(visit.clone())),
        )
        .child(
            DateInput::new("persian", "Persian date", *persian.read(cx))
                .calendar(CalendarSystem::Persian)
                .week_starts_on(Weekday::Saturday)
                .on_change(store(persian.clone())),
        )
        .child(
            DateInput::new("off", "Locked date", Some(date(2026, 3, 21)))
                .disabled(true)
                .disabled_message("Ask an editor"),
        )
        .into_any_element()
}
