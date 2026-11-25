//! OverflowList examples: the gallery's Collections page and the doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use crate::layout::{SpacingStep, Stack};
use crate::overflow::{CollapseFrom, OverflowList};
use crate::status::Badge;
use crate::text::{Text, TextType};
use crate::token::Token;

const TAGS: [&str; 8] = [
    "Design",
    "Engineering",
    "Research",
    "Marketing",
    "Sales",
    "Support",
    "Legal",
    "Finance",
];

fn tags(id: &'static str) -> OverflowList {
    let mut list = OverflowList::new(id);
    for tag in TAGS {
        list = list.keyed_item(tag, move |_, _| {
            Token::new(format!("{id}-{tag}"), tag).into_any_element()
        });
    }
    list.overflow_renderer(|hidden, _, _| {
        Badge::new(format!("+{}", hidden.len())).into_any_element()
    })
}

pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("OverflowList: one row, from the start, two rows"))
        .child(tags("tags").w(px(420.)))
        .child(
            tags("tags-start")
                .collapse_from(CollapseFrom::Start)
                .min_visible_items(1)
                .w(px(300.)),
        )
        .child(tags("tags-rows").max_rows(2).w(px(360.)))
        .into_any_element()
}
