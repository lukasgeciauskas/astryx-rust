//! The CircleCheckIndicator gallery (upstream's `CircleCheckIndicator` stories): every
//! state at both sizes, and circles in rows you lay out, which keep the row's
//! semantics.

use astryx_components::indicator::{ChoiceSize, IndicatorState};
use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Text, TextColor, TextType};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, IntoElement, ParentElement, Role, SharedString, Toggled, Window,
    div,
};

use super::CircleCheckIndicator;

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

/// Every state at both sizes (upstream's `States` story).
fn states() -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S3)
        .children([ChoiceSize::Md, ChoiceSize::Sm].map(|size| {
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .child(
                    Text::new(size.as_str())
                        .text_type(TextType::Supporting)
                        .color(TextColor::Secondary),
                )
                .child(CircleCheckIndicator::new(IndicatorState::Unchecked).size(size))
                .child(CircleCheckIndicator::new(IndicatorState::Checked).size(size))
                .child(CircleCheckIndicator::new(IndicatorState::Indeterminate).size(size))
                .child(
                    CircleCheckIndicator::new(IndicatorState::Unchecked)
                        .size(size)
                        .disabled(true),
                )
                .child(
                    CircleCheckIndicator::new(IndicatorState::Checked)
                        .size(size)
                        .disabled(true),
                )
        }))
        .into_any_element()
}

/// Photos to share: each row is a checkbox that draws the circle (upstream's
/// `MappedCheckboxList` story, laid out by hand).
fn photo_rows(window: &mut Window, cx: &mut App) -> AnyElement {
    let chosen = window.use_keyed_state("circle-check-photos", cx, |_, _| {
        vec![true, false, false, false]
    });
    let photos = ["Beach", "Mountains", "City at night", "Forest trail"];
    let rows = photos.iter().enumerate().map(|(i, label)| {
        let on = chosen.read(cx)[i];
        let group: SharedString = format!("circle-check-photo-{i}").into();
        let chosen = chosen.clone();
        div()
            .id(ElementId::Name(group.clone()))
            .group(group.clone())
            .flex()
            .items_center()
            .gap(gpui::px(8.))
            .cursor_pointer()
            .role(Role::CheckBox)
            .aria_label(SharedString::from(*label))
            .aria_toggled(if on { Toggled::True } else { Toggled::False })
            .on_click(move |_, _, cx| {
                chosen.update(cx, |v, cx| {
                    v[i] = !v[i];
                    cx.notify();
                })
            })
            .child(
                CircleCheckIndicator::new(on)
                    .id(ElementId::Name(format!("{group}-circle").into()))
                    .hover_group(group),
            )
            .child(Text::new(*label))
    });
    Stack::vertical()
        .gap(SpacingStep::S2)
        .children(rows)
        .into_any_element()
}

/// The CircleCheckIndicator examples.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example("States", states()))
        .child(example("In rows you lay out", photo_rows(window, cx)))
        .into_any_element()
}
