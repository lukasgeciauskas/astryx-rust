//! The TabBarMoreList gallery (upstream's `TabBarMoreList` stories): six sections, the
//! last two in More, one of them disabled.

use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Text, TextType};
use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled, Window, div, px};

use super::TabBarMoreList;
use crate::tab_bar::TabBarItem;

/// The six sections: value, label and icon.
const SIX: [(&str, &str, &str); 6] = [
    ("today", "Today", "calendar"),
    ("recent", "Recent", "clock"),
    ("search", "Search", "search"),
    ("tools", "Tools", "wrench"),
    ("info", "Info", "info"),
    ("files", "Files", "copy"),
];

/// The TabBarMoreList examples.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let chosen = window.use_keyed_state("tbm-chosen", cx, |_, _| SharedString::from("info"));
    let tabs: Vec<TabBarItem> = SIX
        .iter()
        .map(|(v, l, i)| {
            TabBarItem::new(*v, *l, *i)
                .disabled(*v == "files")
                .badge(if *v == "info" { "2" } else { "" })
        })
        .collect();
    let value = chosen.read(cx).clone();
    let list = TabBarMoreList::new("tbm-list", tabs)
        .value(value.clone())
        .on_change(move |next, _, cx| {
            chosen.update(cx, |v, cx| {
                *v = next;
                cx.notify();
            })
        });
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new("More, with Info chosen and Files disabled").text_type(TextType::Large))
        .child(div().w(px(402.)).child(list))
        .child(Text::new(format!("Showing {value}")))
        .into_any_element()
}
