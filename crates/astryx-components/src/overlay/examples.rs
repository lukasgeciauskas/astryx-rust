//! Overlay examples: the gallery's Overlays page and the doc example of
//! Tooltip and InfoTip.

use astryx_primitives::{LayerAlign, Placement};
use gpui::{AnyElement, App, IntoElement, ParentElement, Window};

use crate::button::{Button, ButtonVariant, IconButton};
use crate::choice::Switch;
use crate::field::InputStatus;
use crate::icon::Icon;
use crate::input::TextInput;
use crate::layout::{SpacingStep, Stack};
use crate::overlay::{
    AlertDialog, Dialog, DialogHeader, DialogPurpose, HoverCard, InfoTip, Overlay, OverlayPosition,
    OverlayShowOn, Popover, PopoverRole, Tooltip,
};
use crate::text::{Text, TextType};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let archived = window.use_keyed_state("popover-archived", cx, |_, _| false);
    let shown_archived = *archived.read(cx);
    let rename_open = window.use_keyed_state("dialog-rename-open", cx, |_, _| false);
    let delete_open = window.use_keyed_state("dialog-delete-open", cx, |_, _| false);
    let toggle = |state: &gpui::Entity<bool>, open: bool| {
        let state = state.clone();
        move |window: &mut Window, cx: &mut App| {
            state.update(cx, |v, _| *v = open);
            window.refresh();
        }
    };
    let (open_rename, close_rename) = (toggle(&rename_open, true), toggle(&rename_open, false));
    let (open_delete, close_delete) = (toggle(&delete_open, true), toggle(&delete_open, false));
    let close_rename_header = close_rename.clone();
    let close_rename_cancel = close_rename.clone();
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
        .child(label("Popover"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    Popover::new("filters", "Filters")
                        .content(
                            Stack::vertical()
                                .gap(SpacingStep::S3)
                                .width(gpui::px(240.))
                                .child(Text::new("Filters").text_type(TextType::Label))
                                .child(
                                    Switch::new("archived", "Show archived", shown_archived)
                                        .on_change({
                                            let archived = archived.clone();
                                            move |on, window, cx| {
                                                archived.update(cx, |v, _| *v = on);
                                                window.refresh();
                                            }
                                        }),
                                )
                                .child(TextInput::new("owner", "Owner", "")),
                        )
                        .child(Button::new("filters-button", "Filters").variant(ButtonVariant::Secondary)),
                )
                .child(
                    Popover::new("about", "About this report")
                        .placement(Placement::End)
                        .content(
                            Text::new(
                                "Totals refresh every hour. Numbers for today are estimates until midnight.",
                            )
                            .text_type(TextType::Body),
                        )
                        .width(gpui::px(260.))
                        .child(Button::new("about-button", "About").variant(ButtonVariant::Ghost)),
                )
                .child(
                    Popover::new("actions", "")
                        .role(PopoverRole::None)
                        .padding(SpacingStep::S1)
                        .content(
                            Stack::vertical()
                                .child(Button::new("rename", "Rename").variant(ButtonVariant::Ghost))
                                .child(Button::new("duplicate", "Duplicate").variant(ButtonVariant::Ghost)),
                        )
                        .child(Button::new("actions-button", "Actions").variant(ButtonVariant::Secondary)),
                ),
        )
        .child(label("Dialog and AlertDialog"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    Button::new("rename-button", "Rename file")
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_, window, cx| open_rename(window, cx)),
                )
                .child(
                    Button::new("delete-button", "Delete project")
                        .variant(ButtonVariant::Destructive)
                        .on_click(move |_, window, cx| open_delete(window, cx)),
                ),
        )
        .child(
            Dialog::new("rename", *rename_open.read(cx))
                .purpose(DialogPurpose::Form)
                .on_open_change(move |open, window, cx| {
                    if !open {
                        close_rename(window, cx)
                    }
                })
                .child(
                    DialogHeader::new("Rename file")
                        .subtitle("Names show in the sidebar and in links.")
                        .on_open_change(move |_, window, cx| close_rename_header(window, cx)),
                )
                .child(
                    Stack::vertical()
                        .gap(SpacingStep::S4)
                        .child(TextInput::new("file-name", "Name", "Quarterly report"))
                        .child(
                            Stack::horizontal()
                                .gap(SpacingStep::S2)
                                .h_align(crate::layout::StackAlign::End)
                                .child(
                                    Button::new("rename-cancel", "Cancel")
                                        .variant(ButtonVariant::Ghost)
                                        .on_click(move |_, window, cx| close_rename_cancel(window, cx)),
                                )
                                .child(Button::new("rename-save", "Rename")),
                        ),
                ),
        )
        .child(
            AlertDialog::new(
                "delete",
                *delete_open.read(cx),
                "Delete project?",
                "The project and its 12 files will be deleted. This cannot be undone.",
                "Delete project",
            )
            .on_action({
                let close = close_delete.clone();
                move |window, cx| close(window, cx)
            })
            .on_open_change(move |open, window, cx| {
                if !open {
                    close_delete(window, cx)
                }
            }),
        )
        .child(label("Overlay"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S3)
                .child(
                    Overlay::new("cover-hover")
                        .show_on(OverlayShowOn::Hover)
                        .position(OverlayPosition::Bottom)
                        .content(Text::new("Sunset over the bay").text_type(TextType::Label))
                        .child(cover_art(t_swatch(window, cx, 0))),
                )
                .child(
                    Overlay::new("cover-always")
                        .content(Text::new("12 photos").text_type(TextType::Label))
                        .child(cover_art(t_swatch(window, cx, 1))),
                ),
        )
        .child(label("HoverCard"))
        .child(
            Text::new("Reviewed by ")
                .text_type(TextType::Body)
                .child(
                    HoverCard::new("reviewer")
                        .label("Ada Lovelace")
                        .content(
                            Stack::vertical()
                                .gap(SpacingStep::S2)
                                .width(gpui::px(240.))
                                .child(Text::new("Ada Lovelace").text_type(TextType::Label))
                                .child(
                                    Text::new("Analyst. Wrote the first published program.")
                                        .text_type(TextType::Supporting),
                                )
                                .child(Button::new("reviewer-follow", "Follow")),
                        )
                        .text("Ada Lovelace"),
                )
                .child(" two days ago."),
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

/// A swatch color for an example cover.
fn t_swatch(window: &mut Window, cx: &mut App, i: usize) -> gpui::Hsla {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    if i == 0 {
        t.color.accent
    } else {
        t.color.success
    }
}

/// A stand-in for a photo: a colored 200 x 120 card.
fn cover_art(color: gpui::Hsla) -> impl IntoElement {
    use gpui::Styled as _;
    gpui::div()
        .w(gpui::px(200.))
        .h(gpui::px(120.))
        .rounded(gpui::px(8.))
        .bg(color)
}
