//! Link examples: the gallery's Links page and the doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Window};

use crate::layout::{SpacingStep, Stack, StackAlign};
use crate::link::Link;
use crate::text::{Text, TextColor, TextType};

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let row = || {
        Stack::horizontal()
            .gap(SpacingStep::S4)
            .v_align(StackAlign::Center)
    };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("Colors"))
        .child(
            row()
                .child(Link::new("accent", "Accent").href("https://example.com"))
                .child(
                    Link::new("primary", "Primary")
                        .href("https://example.com")
                        .color(TextColor::Primary),
                )
                .child(
                    Link::new("secondary", "Secondary")
                        .href("https://example.com")
                        .color(TextColor::Secondary),
                ),
        )
        .child(label("Underlined, external, a button link, disabled"))
        .child(
            row()
                .child(
                    Link::new("underlined", "Release notes")
                        .href("https://example.com/notes")
                        .underline(true),
                )
                .child(
                    Link::new("external", "Astryx on GitHub")
                        .href("https://github.com/facebook/astryx")
                        .external(true),
                )
                .child(Link::new("button", "Show details").on_click(|_, _, _| {}))
                .child(
                    Link::new("disabled", "Unavailable")
                        .href("https://example.com")
                        .disabled(true),
                ),
        )
        .child(label("Standalone"))
        .child(
            Link::new("standalone", "View all projects")
                .href("/projects")
                .standalone(true),
        )
        .into_any_element()
}
