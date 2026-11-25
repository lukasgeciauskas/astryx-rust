//! Tokenizer examples: the gallery's Collections page and the doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use crate::layout::{SpacingStep, Stack};
use crate::search::{SearchItem, static_source};
use crate::text::{Text, TextType};
use crate::tokenizer::{TokenOverflowBehavior, Tokenizer};

fn tags() -> Vec<SearchItem> {
    [
        "Design",
        "Engineering",
        "Research",
        "Marketing",
        "Sales",
        "Support",
        "Legal",
    ]
    .into_iter()
    .map(|t| SearchItem::new(t.to_lowercase(), t))
    .collect()
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let chosen = window.use_keyed_state("tokenizer-chosen", cx, |_, _| {
        vec![
            SearchItem::new("design", "Design"),
            SearchItem::new("research", "Research"),
        ]
    });
    let folded = window.use_keyed_state("tokenizer-folded", cx, |_, _| tags());
    let (a, b) = (chosen.clone(), folded.clone());
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("Tokenizer"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .child(
                    Tokenizer::new(
                        "teams",
                        "Teams",
                        static_source(tags()),
                        chosen.read(cx).clone(),
                    )
                    .entries_on_focus(true)
                    .clear(true)
                    .create(|text| SearchItem::new(text.to_lowercase(), text))
                    .placeholder("Add teams…")
                    .on_change(move |items, _, window, cx| {
                        a.update(cx, |v, _| *v = items);
                        window.refresh();
                    })
                    .w(px(320.)),
                )
                .child(
                    Tokenizer::new(
                        "all",
                        "Every team",
                        static_source(tags()),
                        folded.read(cx).clone(),
                    )
                    .token_overflow(TokenOverflowBehavior::UnfocusedInline)
                    .description("Folds into one row when blurred")
                    .on_change(move |items, _, window, cx| {
                        b.update(cx, |v, _| *v = items);
                        window.refresh();
                    })
                    .w(px(320.)),
                ),
        )
        .into_any_element()
}
