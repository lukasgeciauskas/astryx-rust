//! useContainerReveal examples for the gallery.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use super::{ContentRevealOptions, use_container_reveal};
use crate::button::{Button, ButtonVariant};
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextType};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let reveal = use_container_reveal("reveal-row", Default::default(), window, cx);
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(label(
            "useContainerReveal: the row's action shows on hover or keyboard focus",
        ))
        .child(
            reveal
                .container(Default::default())
                .flex()
                .items_center()
                .gap(px(8.))
                .w(px(360.))
                .child(Text::new("Quarterly report.pdf"))
                .child(
                    reveal
                        .content(ContentRevealOptions {
                            layout_preserved: true,
                            ..Default::default()
                        })
                        .child(Button::new("reveal-edit", "Rename").variant(ButtonVariant::Ghost)),
                ),
        )
        .into_any_element()
}
