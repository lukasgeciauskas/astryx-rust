//! Icon examples: the gallery's Icon page and Icon's doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Window};

use crate::icon::{Icon, IconColor, IconSize};
use crate::layout::{SpacingStep, Stack, StackAlign};
use crate::text::{Text, TextType};

const NAMES: &[&str] = &[
    "close",
    "check",
    "chevronDown",
    "chevronRight",
    "search",
    "info",
    "success",
    "warning",
    "error",
    "calendar",
    "clock",
    "copy",
    "externalLink",
    "menu",
    "moreHorizontal",
    "upload",
];

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let row = || {
        Stack::horizontal()
            .gap(SpacingStep::S4)
            .v_align(StackAlign::Center)
    };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label(
            "Built-in meanings, drawn with the active theme's artwork",
        ))
        .child(row().children(NAMES.iter().map(|name| Icon::new(*name))))
        .child(label("Sizes: xsm, sm, md, lg"))
        .child(
            row().children(
                [IconSize::Xsm, IconSize::Sm, IconSize::Md, IconSize::Lg]
                    .into_iter()
                    .map(|size| Icon::new("search").size(size)),
            ),
        )
        .child(label("Colors"))
        .child(
            row().children(
                [
                    IconColor::Primary,
                    IconColor::Secondary,
                    IconColor::Disabled,
                    IconColor::Accent,
                    IconColor::Info,
                    IconColor::Success,
                    IconColor::Warning,
                    IconColor::Error,
                    IconColor::Blue,
                    IconColor::Purple,
                    IconColor::Pink,
                    IconColor::Teal,
                ]
                .into_iter()
                .map(|color| Icon::new("info").color(color)),
            ),
        )
        .child(label(
            "A labeled, standalone icon is an image for screen readers",
        ))
        .child(
            row().child(
                Icon::new("warning")
                    .color(IconColor::Warning)
                    .label("Unsaved changes"),
            ),
        )
        .into_any_element()
}
