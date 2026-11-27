//! Carousel examples: the gallery's Shell page.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use super::{Carousel, SnapPagingPages, use_snap_paging};
use crate::layout::{SpacingStep, Stack};
use crate::text::{Text, TextType};

fn card(i: usize, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let theme = rustryx_theme::current(window, cx);
    let t = theme.tokens().clone();
    div()
        .w(px(180.))
        .h(px(110.))
        .rounded(t.radius.container)
        .bg(t.color.background_muted)
        .border_1()
        .border_color(t.color.border)
        .flex()
        .items_center()
        .justify_center()
        .child(Text::new(format!("Item {}", i + 1)).text_type(TextType::Label))
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let cards = |window: &mut Window, cx: &mut App| {
        (0..12)
            .map(|i| card(i, window, cx).into_any_element())
            .collect::<Vec<_>>()
    };
    let native = cards(window, cx);
    let paging = cards(window, cx);
    let looping = cards(window, cx);
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(
            Text::new("Carousel: native snap, with buttons and edge fades")
                .text_type(TextType::Supporting),
        )
        .child(
            Carousel::new("carousel-native")
                .label("Featured")
                .gap(SpacingStep::S3)
                .padding(SpacingStep::S2)
                .snap(true)
                .children(native),
        )
        .child(
            Text::new("Carousel: snap paging, one item per flick").text_type(TextType::Supporting),
        )
        .child(
            Carousel::new("carousel-paging")
                .label("Photos")
                .gap(SpacingStep::S3)
                .snap(use_snap_paging(SnapPagingPages::Item))
                .children(paging),
        )
        .child(Text::new("Carousel: looping").text_type(TextType::Supporting))
        .child(
            Carousel::new("carousel-loop")
                .label("Team")
                .gap(SpacingStep::S3)
                .looping(true)
                .children(looping),
        )
        .into_any_element()
}
