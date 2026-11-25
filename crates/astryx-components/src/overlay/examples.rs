//! Overlay examples: the gallery's Overlays page and the doc example of
//! Tooltip and InfoTip.

use astryx_primitives::{LayerAlign, Placement};
use gpui::{AnyElement, App, IntoElement, ParentElement, Window};

use crate::button::{Button, ButtonVariant, IconButton};
use crate::field::InputStatus;
use crate::icon::Icon;
use crate::input::TextInput;
use crate::layout::{SpacingStep, Stack};
use crate::overlay::{InfoTip, Tooltip};
use crate::text::{Text, TextType};

pub fn examples(_: &mut Window, _: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let placed = |id: &'static str, placement: Placement, text: &'static str| {
        Tooltip::new(id, text)
            .placement(placement)
            .child(Button::new(format!("{id}-button"), text).variant(ButtonVariant::Secondary))
    };

    Stack::vertical()
        .gap(SpacingStep::S6)
        .child(label("Tooltip"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    Tooltip::new("open", "Shown when the page opens")
                        .default_open(true)
                        .child(Button::new("open-button", "Default open")),
                )
                .child(placed("above", Placement::Above, "Above"))
                .child(placed("below", Placement::Below, "Below"))
                .child(placed("start", Placement::Start, "Start"))
                .child(placed("end", Placement::End, "End")),
        )
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    IconButton::new("copy-link", "Copy link", Icon::new("copy"))
                        .variant(ButtonVariant::Ghost)
                        .tooltip("Copy a link to this page"),
                )
                .child(
                    Tooltip::new("aligned", "Aligned to the start edge")
                        .alignment(LayerAlign::Start)
                        .child(Button::new("aligned-button", "Start aligned")),
                )
                .child(
                    Text::new("Times are shown in ")
                        .text_type(TextType::Body)
                        .child(Tooltip::new("utc", "Coordinated Universal Time").text("UTC")),
                ),
        )
        .child(label("InfoTip"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S2)
                .child(Text::new("Monthly active users").text_type(TextType::Body))
                .child(InfoTip::new(
                    "mau",
                    "People who signed in at least once in the last 30 days.",
                )),
        )
        .child(label("Tooltips in fields"))
        .child(
            Stack::vertical()
                .gap(SpacingStep::S4)
                .width(gpui::px(360.))
                .child(
                    TextInput::new("workspace-name", "Workspace name", "Astryx")
                        .label_tooltip("Shown in the sidebar and in invitations."),
                )
                .child(
                    TextInput::new("billing", "Billing email", "")
                        .disabled(true)
                        .disabled_message("Only owners can change billing."),
                )
                .child(
                    TextInput::new("slug", "Slug", "astryx rust")
                        .status(InputStatus::error("Use letters, numbers and dashes."))
                        .status_placement(crate::field::StatusPlacement::Tooltip),
                ),
        )
        .into_any_element()
}
