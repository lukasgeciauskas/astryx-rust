//! The ListInput gallery (upstream's `Lab/ListInput` stories).

use gpui::{
    AnyElement, App, Entity, IntoElement, ParentElement, SharedString, Styled, Window, div, px,
};
use rustryx_components::field::InputStatus;
use rustryx_components::input::TextInput;
use rustryx_components::layout::{SpacingStep, Stack};
use rustryx_components::reorderable::{ItemKey, ReorderableOptions, create_reorderable};
use rustryx_components::table::proportional_min;
use rustryx_components::text::{Heading, HeadingLevel};

use super::{ListInput, ListInputColumn, ListInputRenderContext};

/// A tag option (upstream's `TagOption`).
#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    id: SharedString,
    color: SharedString,
    label: SharedString,
}

fn tag(id: &str, color: &str, label: &str) -> Tag {
    Tag {
        id: id.to_string().into(),
        color: color.to_string().into(),
        label: label.to_string().into(),
    }
}

fn initial_tags() -> Vec<Tag> {
    vec![
        tag("tag-new", "blue", "New"),
        tag("tag-blocked", "green", "Blocked"),
        tag("tag-empty", "amber", ""),
        tag("tag-review", "purple", "In review"),
    ]
}

/// Upstream's `hasInvalidCombination`.
fn invalid(tag: &Tag) -> bool {
    let label = tag.label.trim().to_lowercase();
    (tag.color == "green" && label == "blocked") || (tag.color == "red" && label == "success")
}

fn columns() -> Vec<ListInputColumn<Tag>> {
    vec![
        ListInputColumn::new(
            "color",
            "Color",
            |ctx: &ListInputRenderContext<Tag>, _: &mut Window, _: &mut App| {
                let c = ctx.clone();
                let mut input =
                    TextInput::new(ctx.id.clone(), ctx.label.clone(), ctx.item.color.clone())
                        .label_hidden(ctx.is_label_hidden)
                        .disabled(ctx.is_disabled)
                        .status_placement(ctx.status_placement)
                        .on_change(move |color, window, cx| {
                            let mut next = c.item.clone();
                            next.color = color;
                            c.update_item(next, Some("color"), window, cx);
                        });
                if let Some(status) = ctx.status.clone() {
                    input = input.status(status);
                }
                input.into_any_element()
            },
        )
        .width(proportional_min(1., 120.)),
        ListInputColumn::new(
            "label",
            "Label",
            |ctx: &ListInputRenderContext<Tag>, _: &mut Window, _: &mut App| {
                let c = ctx.clone();
                let mut input =
                    TextInput::new(ctx.id.clone(), ctx.label.clone(), ctx.item.label.clone())
                        .label_hidden(ctx.is_label_hidden)
                        .disabled(ctx.is_disabled)
                        .loading(ctx.is_loading)
                        .status_placement(ctx.status_placement)
                        .on_change(move |label, window, cx| {
                            let mut next = c.item.clone();
                            next.label = label;
                            c.update_item(next, Some("label"), window, cx);
                        });
                if let Some(status) = ctx.status.clone() {
                    input = input.status(status);
                }
                input.into_any_element()
            },
        )
        .width(proportional_min(2., 200.)),
    ]
}

fn tags_input(
    id: &'static str,
    tags: &Entity<Vec<Tag>>,
    reorder: bool,
    disabled: bool,
    loading: bool,
    cx: &App,
) -> ListInput<Tag> {
    let set = tags.clone();
    let mut input = ListInput::new(
        id,
        "Tag options",
        tags.read(cx).clone(),
        |t: &Tag| t.id.clone(),
        {
            let counter = std::cell::Cell::new(0usize);
            move || {
                counter.set(counter.get() + 1);
                tag(&format!("tag-added-{}", counter.get()), "blue", "")
            }
        },
        columns(),
    )
    .item_name("tag")
    .description(if reorder {
        "Add at least five tags and drag rows to set their order."
    } else {
        "Add at least five tags. Items remain in the order they were added."
    })
    .field_status(|tag, column, _| {
        (column == "label" && invalid(tag))
            .then(|| InputStatus::error("That color and label conflict."))
    })
    .item_status(|tag, _| {
        tag.label
            .trim()
            .is_empty()
            .then(|| InputStatus::warning("This tag has no label yet."))
    })
    .disabled(disabled)
    .loading(loading)
    .on_change(move |next, _, _, cx| {
        set.update(cx, |t, cx| {
            *t = next;
            cx.notify();
        })
    });
    if reorder {
        let set = tags.clone();
        input = input.reorderable(create_reorderable(ReorderableOptions::new(
            move |m, _, cx| {
                let ItemKey::Text(key) = &m.key else {
                    return;
                };
                let (key, to) = (key.clone(), m.to);
                set.update(cx, |records, cx| {
                    if let Some(from) = records.iter().position(|r| r.id == key) {
                        let moved = records.remove(from);
                        records.insert(to.min(records.len()), moved);
                        cx.notify();
                    }
                });
            },
        )));
    }
    input
}

fn story(title: &'static str, width: f32, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(Heading::new(HeadingLevel::H3, title))
        .child(div().w(px(width)).max_w_full().child(body))
        .into_any_element()
}

/// The ListInput examples: tag options with and without reordering, an empty list, a
/// narrow list whose records stack, and the disabled and loading states.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let reorderable = window.use_keyed_state("list-input-tags", cx, |_, _| initial_tags());
    let plain = window.use_keyed_state("list-input-plain", cx, |_, _| initial_tags());
    let empty = window.use_keyed_state("list-input-empty", cx, |_, _| Vec::<Tag>::new());
    let narrow = window.use_keyed_state("list-input-narrow", cx, |_, _| initial_tags());
    let disabled = window.use_keyed_state("list-input-disabled", cx, |_, _| initial_tags());
    let loading = window.use_keyed_state("list-input-loading", cx, |_, _| initial_tags());
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(story(
            "Tag options",
            680.,
            tags_input(
                "list-input-tags-input",
                &reorderable,
                true,
                false,
                false,
                cx,
            )
            .max_items(8),
        ))
        .child(story(
            "Non-reorderable",
            680.,
            tags_input("list-input-plain-input", &plain, false, false, false, cx),
        ))
        .child(story(
            "Empty",
            680.,
            tags_input("list-input-empty-input", &empty, false, false, false, cx),
        ))
        .child(story(
            "Responsive",
            360.,
            tags_input("list-input-narrow-input", &narrow, true, false, false, cx),
        ))
        .child(story(
            "Disabled",
            680.,
            tags_input(
                "list-input-disabled-input",
                &disabled,
                true,
                true,
                false,
                cx,
            ),
        ))
        .child(story(
            "Loading",
            680.,
            tags_input("list-input-loading-input", &loading, true, false, true, cx),
        ))
        .into_any_element()
}
