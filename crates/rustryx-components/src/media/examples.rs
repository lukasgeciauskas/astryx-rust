//! Media examples: the gallery's Media page and Thumbnail's doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Window};

use crate::layout::{SpacingStep, Stack, StackAlign};
use crate::media::{ShowRemoveOn, Thumbnail};
use crate::text::{Text, TextType};

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label(
            "Placeholder, uploading (skeleton), removable, clickable, disabled",
        ))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .v_align(StackAlign::Center)
                .child(Thumbnail::new("empty").label("draft.png"))
                .child(Thumbnail::new("loading").label("photo.jpg").loading(true))
                .child(
                    Thumbnail::new("removable")
                        .label("notes.png")
                        .show_remove_on(ShowRemoveOn::Always)
                        .on_remove(|_, _, _| {}),
                )
                .child(
                    Thumbnail::new("clickable")
                        .label("open.png")
                        .on_click(|_, _, _| {}),
                )
                .child(
                    Thumbnail::new("disabled")
                        .label("locked.png")
                        .disabled(true),
                ),
        )
        .into_any_element()
}
