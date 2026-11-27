//! The lab InfoTip gallery (upstream's `InfoTip` stories), drawn through Core's
//! component, where InfoTip lives now.

use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::overlay::InfoTip;
use astryx_components::text::{Text, TextType};
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

/// The InfoTip examples: alone, and next to a label.
pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example(
            "Default",
            InfoTip::new(
                "info-tip-default",
                "Editors can change this field; viewers cannot.",
            ),
        ))
        .child(example(
            "Next to a label",
            div().p(px(16.)).child(
                Stack::horizontal()
                    .gap(SpacingStep::S1)
                    .child(Text::new("Rolling average"))
                    .child(
                        InfoTip::new(
                            "info-tip-label",
                            "30-day rolling average, recomputed nightly.",
                        )
                        .label("About this metric"),
                    ),
            ),
        ))
        .into_any_element()
}
