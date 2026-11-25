//! Selector examples: the gallery's Collections page and the doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled, Window, px};

use crate::field::InputStatus;
use crate::layout::{SpacingStep, Stack};
use crate::selector::{
    Selector, SelectorEntry, SelectorOption, SelectorOptionData, SelectorVariant,
};
use crate::text::{Text, TextType};

/// A Selector whose value lives in keyed state.
fn selector(
    id: &'static str,
    label: &'static str,
    options: Vec<SelectorEntry>,
    initial: Option<&'static str>,
    window: &mut Window,
    cx: &mut App,
) -> Selector {
    let value = window.use_keyed_state(format!("{id}-value"), cx, move |_, _| {
        initial.map(SharedString::from)
    });
    let current = value.read(cx).clone();
    Selector::new(id, label, options)
        .value(current)
        .on_change(move |next, window, cx| {
            value.update(cx, |v, _| *v = next);
            window.refresh();
        })
}

fn fruits() -> Vec<SelectorEntry> {
    [
        "Apple", "Banana", "Cherry", "Grape", "Mango", "Orange", "Pear",
    ]
    .into_iter()
    .map(SelectorEntry::from)
    .collect()
}

fn sorts() -> Vec<SelectorEntry> {
    vec![
        SelectorOptionData::new("newest")
            .label("Newest")
            .icon("arrowDown")
            .into(),
        SelectorOptionData::new("oldest")
            .label("Oldest")
            .icon("arrowUp")
            .into(),
        SelectorOptionData::new("relevance")
            .label("Relevance")
            .icon("arrowsUpDown")
            .into(),
    ]
}

fn countries() -> Vec<SelectorEntry> {
    vec![
        SelectorEntry::section(
            "Europe",
            [
                SelectorOptionData::new("fr").label("France"),
                SelectorOptionData::new("de").label("Germany"),
                SelectorOptionData::new("it").label("Italy"),
            ],
        ),
        SelectorEntry::section(
            "Asia",
            [
                SelectorOptionData::new("jp").label("Japan"),
                SelectorOptionData::new("kr")
                    .label("South Korea")
                    .disabled(true),
            ],
        ),
        SelectorEntry::divider(),
        SelectorOptionData::new("other")
            .label("Somewhere else")
            .into(),
    ]
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let people = vec![
        SelectorOptionData::new("ada")
            .label("Ada Lovelace")
            .description("Engineering")
            .into(),
        SelectorOptionData::new("grace")
            .label("Grace Hopper")
            .description("Compilers")
            .into(),
        SelectorOptionData::new("alan")
            .label("Alan Turing")
            .description("Research")
            .into(),
    ];
    Stack::vertical()
        .gap(SpacingStep::S6)
        .child(label("Selector"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .child(
                    selector("fruit", "Fruit", fruits(), Some("Banana"), window, cx)
                        .placeholder("Select a fruit…")
                        .w(px(220.)),
                )
                .child(
                    selector("country", "Country", countries(), None, window, cx)
                        .search(true)
                        .clear(true)
                        .description("Sections, a divider, search and clear")
                        .w(px(240.)),
                )
                .child(
                    selector("owner", "Owner", people, Some("grace"), window, cx)
                        .render_option(|option, _, _| {
                            SelectorOption::new(option.label.clone())
                                .description(option.description.clone().unwrap_or_default())
                                .into_any_element()
                        })
                        .w(px(220.)),
                ),
        )
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .child(
                    selector("sort", "Sort", sorts(), Some("newest"), window, cx)
                        .variant(SelectorVariant::Secondary)
                        .label_hidden(true),
                )
                .child(
                    selector("view", "View", fruits(), Some("Cherry"), window, cx)
                        .variant(SelectorVariant::Ghost)
                        .label_hidden(true),
                )
                .child(
                    selector("size-sm", "Small", fruits(), Some("Pear"), window, cx)
                        .size(astryx_primitives::ElementSize::Sm)
                        .w(px(160.)),
                ),
        )
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .child(
                    selector("required", "Required", fruits(), None, window, cx)
                        .required(true)
                        .status(InputStatus::error("Pick one"))
                        .w(px(200.)),
                )
                .child(
                    selector(
                        "read-only",
                        "Read-only",
                        fruits(),
                        Some("Mango"),
                        window,
                        cx,
                    )
                    .read_only(true)
                    .w(px(200.)),
                )
                .child(
                    selector("disabled", "Disabled", fruits(), Some("Grape"), window, cx)
                        .disabled(true)
                        .disabled_message("Only editors can change this")
                        .w(px(200.)),
                ),
        )
        .into_any_element()
}
