//! ScrollableArea examples: the gallery's Shell page.

use astryx_theme::{ThemeStyled, current};
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use crate::layout::{SpacingStep, Stack};
use crate::scrollable_area::{ScrollAxis, ScrollOverscroll, ScrollableArea};
use crate::text::{Text, TextType};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens().clone();
    let row = |i: usize| {
        div()
            .py(t.spacing.s2)
            .border_b_1()
            .border_color(t.color.border)
            .text_style(&t.text.body)
            .text_color(t.color.text_primary)
            .child(format!("Activity {i}"))
    };
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label(
            "ScrollableArea: block scrolling, padded, a tab stop while it scrolls",
        ))
        .child(
            ScrollableArea::new("activity", "Activity history")
                .height(px(180.))
                .width(px(360.))
                .padding(SpacingStep::S3)
                .border_1()
                .border_color(t.color.border)
                .rounded(t.radius.container)
                .children((1..=20).map(row)),
        )
        .child(label("Inline scrolling, contained at its edges"))
        .child(
            ScrollableArea::new("timeline", "Timeline")
                .axis(ScrollAxis::Inline)
                .overscroll(ScrollOverscroll::Contain)
                .width(px(360.))
                .border_1()
                .border_color(t.color.border)
                .rounded(t.radius.container)
                .child(
                    div()
                        .flex()
                        .gap(t.spacing.s2)
                        .p(t.spacing.s2)
                        .children((1..=16).map(|i| {
                            div()
                                .w(px(72.))
                                .h(px(56.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(t.radius.element)
                                .border_1()
                                .border_color(t.color.border)
                                .text_style(&t.text.label)
                                .text_color(t.color.text_primary)
                                .child(format!("Day {i}"))
                        })),
                ),
        )
        .child(label("Content that fits: no tab stop, no scrollbar"))
        .child(
            ScrollableArea::new("fits", "Short list")
                .height(px(120.))
                .width(px(360.))
                .border_1()
                .border_color(t.color.border)
                .rounded(t.radius.container)
                .children((1..=2).map(row)),
        )
        .into_any_element()
}
