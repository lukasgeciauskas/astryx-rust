//! Button examples: the gallery's Buttons page and the doc example of
//! Button, IconButton, ButtonGroup and ToggleButton.

use std::time::Duration;

use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Window};
use rustryx_primitives::ElementSize;

use crate::button::{
    Button, ButtonGroup, ButtonVariant, IconButton, SelectionMode, ToggleButton, ToggleButtonGroup,
};
use crate::icon::Icon;
use crate::layout::{Orientation, SpacingStep, Stack, StackAlign};
use crate::status::Badge;
use crate::text::{Text, TextType};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let row = || {
        Stack::horizontal()
            .gap(SpacingStep::S2)
            .v_align(StackAlign::Center)
    };
    let variants = [
        ButtonVariant::Primary,
        ButtonVariant::Secondary,
        ButtonVariant::Ghost,
        ButtonVariant::Destructive,
    ];
    // The page keeps its own selection between frames.
    let align = window.use_keyed_state("align-selection", cx, |_, _| {
        vec![SharedString::from("left")]
    });
    let styles = window.use_keyed_state("style-selection", cx, |_, _| Vec::<SharedString>::new());
    let bold = window.use_keyed_state("bold-toggle", cx, |_, _| false);
    let (align_value, styles_value, bold_value) = (
        align.read(cx).clone(),
        styles.read(cx).clone(),
        *bold.read(cx),
    );

    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("Variants"))
        .child(
            row().children(
                variants.into_iter().map(|variant| {
                    Button::new(variant.as_str(), variant.as_str()).variant(variant)
                }),
            ),
        )
        .child(label("Sizes, with an icon"))
        .child(
            row().children(
                [ElementSize::Sm, ElementSize::Md, ElementSize::Lg]
                    .into_iter()
                    .map(|size| {
                        Button::new(size.as_str(), "Search")
                            .size(size)
                            .icon(Icon::new("search"))
                    }),
            ),
        )
        .child(label(
            "Icon-only, end content, loading, a slow action, disabled",
        ))
        .child(
            row()
                .child(
                    IconButton::new("close", "Close", Icon::new("close"))
                        .variant(ButtonVariant::Ghost),
                )
                .child(IconButton::new("more", "More", Icon::new("moreHorizontal")))
                .child(
                    Button::new("inbox", "Inbox")
                        .variant(ButtonVariant::Primary)
                        .end_content(Badge::new("12")),
                )
                .child(Button::new("loading", "Saving").loading(true))
                .child(
                    Button::new("slow", "Sync (2 s)").click_action(|_, window, cx| {
                        let wait = cx.background_executor().timer(Duration::from_secs(2));
                        window.spawn(cx, async move |_| wait.await)
                    }),
                )
                .child(Button::new("disabled", "Disabled").disabled(true)),
        )
        .child(label("ButtonGroup"))
        .child(
            row()
                .child(
                    ButtonGroup::new("history", "History")
                        .child(Button::new("undo", "Undo").icon(Icon::new("chevronLeft")))
                        .child(Button::new("redo", "Redo").icon(Icon::new("chevronRight")))
                        .child(IconButton::new(
                            "history-more",
                            "More",
                            Icon::new("moreHorizontal"),
                        )),
                )
                .child(
                    ButtonGroup::new("primary-group", "Publish")
                        .child(Button::new("publish", "Publish").variant(ButtonVariant::Primary))
                        .child(
                            IconButton::new(
                                "publish-more",
                                "More options",
                                Icon::new("chevronDown"),
                            )
                            .variant(ButtonVariant::Primary),
                        ),
                )
                .child(
                    ButtonGroup::new("vertical", "Zoom")
                        .orientation(Orientation::Vertical)
                        .size(ElementSize::Sm)
                        .child(Button::new("zoom-in", "Zoom in"))
                        .child(Button::new("zoom-out", "Zoom out")),
                ),
        )
        .child(label(
            "ToggleButton and ToggleButtonGroup (single, multiple)",
        ))
        .child(
            row()
                .child(
                    ToggleButton::new("bold", "Bold")
                        .pressed(bold_value)
                        .on_pressed_change({
                            let bold = bold.clone();
                            move |next, _, window, cx| {
                                bold.update(cx, |b, _| *b = next);
                                window.refresh();
                            }
                        }),
                )
                .child(
                    ToggleButtonGroup::new("align", "Alignment", SelectionMode::Single)
                        .value(align_value)
                        .on_change({
                            let align = align.clone();
                            move |next, window, cx| {
                                align.update(cx, |a, _| *a = next);
                                window.refresh();
                            }
                        })
                        .child(ToggleButton::new("left", "Left").value("left"))
                        .child(ToggleButton::new("center", "Center").value("center"))
                        .child(ToggleButton::new("right", "Right").value("right")),
                )
                .child(
                    ToggleButtonGroup::new("styles", "Styles", SelectionMode::Multiple)
                        .value(styles_value)
                        .on_change({
                            let styles = styles.clone();
                            move |next, window, cx| {
                                styles.update(cx, |s, _| *s = next);
                                window.refresh();
                            }
                        })
                        .child(ToggleButton::new("italic", "Italic").value("italic"))
                        .child(ToggleButton::new("underline", "Underline").value("underline")),
                ),
        )
        .into_any_element()
}
