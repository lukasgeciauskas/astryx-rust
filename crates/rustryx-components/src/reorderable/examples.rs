//! Reordering examples: an app's own cards in a wrapping grid, attached
//! with shift feedback (the List's own example is in `list/examples.rs`).

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};

use crate::icon::{Icon, IconSize};
use crate::layout::{SpacingStep, Stack};
use crate::reorderable::{FeedbackBehavior, ItemKey, ReorderableOptions, create_reorderable};
use crate::text::{Text, TextType};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = rustryx_theme::current(window, cx);
    let t = theme.tokens();
    let cards = window.use_keyed_state("reorder-cards", cx, |_, _| {
        vec!["Ace", "King", "Queen", "Jack", "Ten", "Nine", "Eight"]
    });
    let order = cards.clone();
    let attached = create_reorderable(
        ReorderableOptions::new(move |m, window, cx| {
            order.update(cx, |cards, _| {
                if let ItemKey::Text(key) = &m.key
                    && let Some(from) = cards.iter().position(|c| *c == key.as_ref())
                {
                    let card = cards.remove(from);
                    cards.insert(m.to, card);
                }
            });
            window.refresh();
        })
        .feedback_behavior(FeedbackBehavior::Shift),
    )
    .attach("reorder-cards", window, cx);
    let names = cards.read(cx).clone();
    let tiles: Vec<AnyElement> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let grip = div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(24.))
                .rounded(t.radius.element)
                .child(Icon::new("menu").size(IconSize::Sm));
            let tile = div()
                .w(px(112.))
                .h(px(72.))
                .p(t.spacing.s2)
                .flex()
                .justify_between()
                .items_start()
                .rounded(t.radius.container)
                .border_1()
                .border_color(t.color.border)
                .bg(t.color.background_surface)
                .child(Text::new(*name))
                .child(attached.handle(*name, grip));
            attached.item(i, *name, *name, tile, cx)
        })
        .collect();
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(
            Text::new(
                "createReorderable: attached cards, shift feedback (the cards slide aside on springs.settle, and the drop settles into the gap)",
            )
                .text_type(TextType::Supporting),
        )
        .child(
            attached.container(
                div()
                    .flex()
                    .flex_wrap()
                    .w(px(380.))
                    .gap(t.spacing.s3)
                    .children(tiles),
                names.len(),
            ),
        )
        .into_any_element()
}
