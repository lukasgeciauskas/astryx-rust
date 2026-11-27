//! The EmojiPicker gallery (upstream's `EmojiPicker` stories): the inline showcase that names
//! the last pick, the picker in a popover that appends to a line of text, and a custom
//! set of the viewer's recent emoji and one group of their own.

use gpui::prelude::*;
use gpui::{AnyElement, App, SharedString, Window, div, px, relative};
use rustryx_components::button::{ButtonVariant, IconButton};
use rustryx_components::containers::Card;
use rustryx_components::layout::{SpacingStep, Stack, StackAlign};
use rustryx_components::overlay::Popover;
use rustryx_components::text::{Text, TextColor, TextType};

use super::{EmojiCategory, EmojiOption, EmojiPicker};

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

/// Upstream's `Inline` (the `EmojiPickerShowcase` block): an inline picker in a card,
/// naming the last pick.
fn inline(window: &mut Window, cx: &mut App) -> AnyElement {
    let picked = window.use_keyed_state("emoji-example-picked", cx, |_, _| None::<EmojiOption>);
    let shown: SharedString = match picked.read(cx) {
        None => "Nothing yet".into(),
        Some(o) => format!("{} {}", o.emoji, o.name).into(),
    };
    Stack::vertical()
        .gap(SpacingStep::S3)
        .w(px(352.))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S2)
                .align(StackAlign::Center)
                .child(Text::new("Picked:").text_type(TextType::Label))
                .child(Text::new(shown).color(TextColor::Secondary)),
        )
        .child(
            Card::new()
                .padding(SpacingStep::S0)
                .width(relative(1.))
                .child(EmojiPicker::new("emoji-example-inline").on_select(
                    move |_, option, _, cx| {
                        let option = option.clone();
                        picked.update(cx, |p, cx| {
                            *p = Some(option);
                            cx.notify();
                        })
                    },
                )),
        )
        .into_any_element()
}

/// Upstream's `InAPopover`: opened from a button, the picker focuses its search, and each
/// pick goes on the end of the line.
fn in_a_popover(window: &mut Window, cx: &mut App) -> AnyElement {
    let state = window.use_keyed_state("emoji-example-popover", cx, |_, _| (false, String::new()));
    let (open, text) = state.read(cx).clone();
    let (change, pick) = (state.clone(), state.clone());
    Stack::horizontal()
        .gap(SpacingStep::S2)
        .align(StackAlign::Center)
        .child(
            Popover::new("emoji-example-add", "Add emoji")
                .open(open)
                .on_open_change(move |open, _, cx| {
                    change.update(cx, |s, cx| {
                        s.0 = open;
                        cx.notify();
                    })
                })
                .padding(SpacingStep::S0)
                .width(px(352.))
                // The Popover's own autofocus lands on the picker's search field.
                .content_fn(move |_, _| {
                    EmojiPicker::new("emoji-example-popover-picker")
                        .on_select(move |emoji, _, _, cx| {
                            pick.update(cx, |s, cx| {
                                s.1.push_str(&emoji);
                                s.0 = false;
                                cx.notify();
                            })
                        })
                        .into_any_element()
                })
                .child(
                    IconButton::new(
                        "emoji-example-add-button",
                        "Add emoji",
                        div().inert(true).child("🙂"),
                    )
                    .variant(ButtonVariant::Secondary),
                ),
        )
        .child(
            Text::new(if text.is_empty() {
                "Pick some emoji".to_string()
            } else {
                text
            })
            .text_type(TextType::Large),
        )
        .into_any_element()
}

/// Upstream's `CustomSet`: the viewer's recent emoji and a custom group.
fn custom_set() -> AnyElement {
    Card::new()
        .padding(SpacingStep::S0)
        .width(px(352.))
        .child(
            EmojiPicker::new("emoji-example-custom")
                .frequent_emojis([
                    EmojiOption::new("🚀", "Rocket"),
                    EmojiOption::new("✅", "Check mark button"),
                ])
                .categories([EmojiCategory::new(
                    "Status",
                    [
                        EmojiOption::new("🟢", "Green circle").keywords(["ok", "healthy"]),
                        EmojiOption::new("🟡", "Yellow circle").keywords(["warning"]),
                        EmojiOption::new("🔴", "Red circle").keywords(["error", "down"]),
                    ],
                )])
                .on_select(|_, _, _, _| {}),
        )
        .into_any_element()
}

/// The EmojiPicker examples.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .padding(SpacingStep::S6)
        .child(example("Inline", inline(window, cx)))
        .child(example("In a popover", in_a_popover(window, cx)))
        .child(example("Custom set", custom_set()))
        .into_any_element()
}
