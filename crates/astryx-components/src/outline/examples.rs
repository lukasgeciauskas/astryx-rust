//! Outline examples: the gallery's Shell page.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use super::{Outline, OutlineDensity, use_outline_from_container};
use crate::layout::{SpacingStep, Stack};
use crate::scrollable_area::ScrollableArea;
use crate::text::{Heading, HeadingLevel, Text, TextType};

fn section(id: &'static str, level: HeadingLevel, title: &'static str) -> impl IntoElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Heading::new(level, title).id(id))
        .children((0..3).map(|_| {
            Text::new(
                "Each heading with an id is an anchor: the Outline lists it, follows the \
                 scroll, and scrolls back to it.",
            )
            .text_type(TextType::Body)
        }))
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens().clone();
    let source = use_outline_from_container("outline-example-article", window, cx);
    let items = source.items(cx);
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(
            Text::new("Outline: the headings of the article beside it (useOutlineFromDOM)")
                .text_type(TextType::Supporting),
        )
        .child(
            div()
                .flex()
                .gap(t.spacing.s6)
                .child(
                    div()
                        .w(px(220.))
                        .child(Outline::new("outline-example", items.clone())),
                )
                .child(
                    div()
                        .flex_1()
                        .h(px(320.))
                        .border_1()
                        .border_color(t.color.border)
                        .rounded(t.radius.container)
                        .child(
                            ScrollableArea::new("outline-example-scroll", "Article").child(
                                source.container(
                                    Stack::vertical()
                                        .gap(SpacingStep::S6)
                                        .p(t.spacing.s4)
                                        .child(section(
                                            "ol-intro",
                                            HeadingLevel::H1,
                                            "Introduction",
                                        ))
                                        .child(section(
                                            "ol-install",
                                            HeadingLevel::H2,
                                            "Installing",
                                        ))
                                        .child(section("ol-theme", HeadingLevel::H3, "Theming"))
                                        .child(section("ol-api", HeadingLevel::H2, "API"))
                                        .child(section("ol-faq", HeadingLevel::H2, "Questions")),
                                ),
                            ),
                        ),
                )
                .child(div().w(px(180.)).child(
                    Outline::new("outline-example-compact", items).density(OutlineDensity::Compact),
                )),
        )
        .into_any_element()
}
