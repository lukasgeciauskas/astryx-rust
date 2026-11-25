//! MultiSelector examples: the gallery's Collections page and the doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled, Window, px};

use crate::button::IconButton;
use crate::icon::Icon;
use crate::layout::{SpacingStep, Stack};
use crate::multi_selector::{MultiSelector, TriggerDisplay};
use crate::selector::{SelectorEntry, SelectorOptionData, SelectorVariant};
use crate::text::{Text, TextType};

fn columns() -> Vec<SelectorEntry> {
    ["Name", "Owner", "Status", "Priority", "Due date", "Labels"]
        .into_iter()
        .map(SelectorEntry::from)
        .collect()
}

fn produce() -> Vec<SelectorEntry> {
    vec![
        SelectorEntry::section("Fruit", ["Apple", "Banana", "Cherry"]),
        SelectorEntry::section("Vegetables", ["Carrot", "Leek", "Pepper"]),
    ]
}

fn views() -> Vec<SelectorEntry> {
    ["Board", "Calendar", "Timeline"]
        .into_iter()
        .map(|view| {
            SelectorEntry::Option(SelectorOptionData::new(view).action(move |_, _| {
                IconButton::new(
                    SharedString::from(format!("more-{view}")),
                    format!("More for {view}"),
                    Icon::new("moreHorizontal"),
                )
                .into_any_element()
            }))
        })
        .collect()
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let state = |key: &'static str, initial: &[&'static str], window: &mut Window, cx: &mut App| {
        let initial: Vec<SharedString> = initial.iter().map(|v| SharedString::from(*v)).collect();
        window.use_keyed_state(key, cx, move |_, _| initial)
    };
    let shown = state("multi-columns", &["Name", "Status"], window, cx);
    let tags = state("multi-tags", &["Owner"], window, cx);
    let picked = state(
        "multi-produce",
        &["Apple", "Leek", "Pepper", "Carrot"],
        window,
        cx,
    );
    let open_views = state("multi-views", &["Board"], window, cx);
    let store = |entity: &gpui::Entity<Vec<SharedString>>| {
        let entity = entity.clone();
        move |values: Vec<SharedString>, _, window: &mut Window, cx: &mut App| {
            entity.update(cx, |v, _| *v = values);
            window.refresh();
        }
    };
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("MultiSelector"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .child(
                    MultiSelector::new("columns", "Columns", columns(), shown.read(cx).clone())
                        .select_all(true)
                        .clear(true)
                        .on_change(store(&shown))
                        .w(px(240.)),
                )
                .child(
                    MultiSelector::new("labels", "Labels", columns(), tags.read(cx).clone())
                        .trigger_display(TriggerDisplay::Badges)
                        .search(true)
                        .create(true)
                        .on_change(store(&tags))
                        .w(px(280.)),
                )
                .child(
                    MultiSelector::new("produce", "Produce", produce(), picked.read(cx).clone())
                        .trigger_display(TriggerDisplay::Labels)
                        .search(true)
                        .on_change(store(&picked))
                        .w(px(280.)),
                )
                .child(
                    MultiSelector::new("views", "Views", views(), open_views.read(cx).clone())
                        .variant(SelectorVariant::Secondary)
                        .label_hidden(true)
                        .on_change(store(&open_views)),
                ),
        )
        .into_any_element()
}
