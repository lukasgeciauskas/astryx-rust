//! List examples: the gallery's Collections page and the doc example.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, px};

use crate::containers::Card;
use crate::icon::{Icon, IconSize};
use crate::item::ItemDensity;
use crate::layout::{SpacingStep, Stack};
use crate::list::{List, ListEdge, ListItem, ListStyle};
use crate::overlay::MenuItem;
use crate::status::Badge;
use crate::text::{Heading, HeadingLevel, Text, TextType};

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    let selected = window.use_keyed_state("list-selected", cx, |_, _| 0usize);
    let current = *selected.read(cx);
    let pick = |i: usize| {
        let selected = selected.clone();
        move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
            selected.update(cx, |s, _| *s = i);
            window.refresh();
        }
    };
    Stack::vertical()
        .gap(SpacingStep::S6)
        .child(label("List"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S6)
                .child(
                    List::new("settings")
                        .header(Heading::new(HeadingLevel::H4, "Settings"))
                        .label("Settings")
                        .dividers(true)
                        .w(px(280.))
                        .child(
                            ListItem::new("notifications", "Notifications")
                                .description("Manage your alerts")
                                .start_content(Icon::new("info").size(IconSize::Sm))
                                .selected(current == 0)
                                .on_click(pick(0)),
                        )
                        .child(
                            ListItem::new("privacy", "Privacy")
                                .description("Control your data")
                                .start_content(Icon::new("eyeSlash").size(IconSize::Sm))
                                .selected(current == 1)
                                .on_click(pick(1)),
                        )
                        .child(
                            ListItem::new("updates", "Updates")
                                .end_content(Badge::new("3"))
                                .selected(current == 2)
                                .on_click(pick(2)),
                        )
                        .child(
                            ListItem::new("archive", "Archive")
                                .disabled(true)
                                .on_click(|_, _, _| {}),
                        ),
                )
                .child(
                    List::new("steps")
                        .list_style(ListStyle::Decimal)
                        .density(ItemDensity::Compact)
                        .start(8)
                        .w(px(240.))
                        .child(ListItem::new("s8", "Unpack the parts"))
                        .child(ListItem::new("s9", "Attach the legs"))
                        .child(ListItem::new("s10", "Tighten every bolt"))
                        .child(ListItem::new("s11", "Turn it upright")),
                )
                .child(
                    List::new("bullets")
                        .list_style(ListStyle::Disc)
                        .w(px(220.))
                        .child(ListItem::new("b1", "Ceramic mug"))
                        .child(
                            ListItem::new("b2", "Linen napkins")
                                .description("A set of four, in sand"),
                        )
                        .child(ListItem::new("b3", "Walnut tray").context_menu([
                            MenuItem::new("edit", "Edit").into(),
                            MenuItem::new("remove", "Remove").into(),
                        ])),
                ),
        )
        .child(label(
            "Edge treatments in a Card: padded, flush, outset, bleed",
        ))
        .child(
            Stack::horizontal().gap(SpacingStep::S4).children(
                [
                    ListEdge::Padded,
                    ListEdge::Flush,
                    ListEdge::Outset,
                    ListEdge::Bleed,
                ]
                .into_iter()
                .map(|edge| {
                    Card::new()
                        .w(px(200.))
                        .child(Heading::new(HeadingLevel::H5, edge.as_str()))
                        .child(
                            List::new(format!("edge-{}", edge.as_str()))
                                .edge(edge)
                                .dividers(true)
                                .child(ListItem::new("a", "Solstice mug").on_click(|_, _, _| {}))
                                .child(
                                    ListItem::new("b", "Linen napkins")
                                        .selected(true)
                                        .on_click(|_, _, _| {}),
                                ),
                        )
                }),
            ),
        )
        .child(label(
            "Reorderable: drag a handle, or press it and use the arrows",
        ))
        .child(reorderable(window, cx))
        .child(label(
            "Animated: with an animatedLayout handle, added rows enter, removed rows collapse and the rest glide",
        ))
        .child(animated(window, cx))
        .child(label(
            "Windowed: 10,000 rows, opened at row 5,000; only rows in view render",
        ))
        .child(windowed(window, cx))
        .into_any_element()
}

/// A list whose row updates animate (FR12): add a row, remove one, or shuffle them.
fn animated(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::button::Button;
    let layout = crate::animated_layout::use_animated_layout("list-animated-layout", window, cx);
    let rows = window.use_keyed_state("list-animated-rows", cx, |_, _| {
        (4usize, vec![0usize, 1, 2, 3])
    });
    let (add, remove, shuffle) = (rows.clone(), rows.clone(), rows.clone());
    let current = rows.read(cx).1.clone();
    let list = List::new("animated")
        .label("Tasks")
        .dividers(true)
        .animated_layout(layout.clone())
        .children(current.iter().map(|n| {
            ListItem::new(("task", *n), format!("Task {}", n + 1))
                .description("Added to the list")
                .item_key(*n)
        }));
    layout
        .container(
            gpui::div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .w(px(320.))
                .child(
                    Stack::horizontal()
                        .gap(SpacingStep::S2)
                        .child(Button::new("list-animated-add", "Add").on_click(
                            move |_, window, cx| {
                                add.update(cx, |(next, rows), _| {
                                    rows.insert(rows.len().min(1), *next);
                                    *next += 1;
                                });
                                window.refresh();
                            },
                        ))
                        .child(Button::new("list-animated-remove", "Remove").on_click(
                            move |_, window, cx| {
                                remove.update(cx, |(_, rows), _| {
                                    if !rows.is_empty() {
                                        rows.remove(rows.len() / 2);
                                    }
                                });
                                window.refresh();
                            },
                        ))
                        .child(Button::new("list-animated-shuffle", "Reverse").on_click(
                            move |_, window, cx| {
                                shuffle.update(cx, |(_, rows), _| rows.reverse());
                                window.refresh();
                            },
                        )),
                )
                .child(Card::new().child(list))
                .child(
                    Text::new("This note glides as the list above it changes.")
                        .text_type(TextType::Supporting),
                ),
            window,
            cx,
        )
        .into_any_element()
}

/// A reorderable list (AST-103): each row's handle drags it, or toggles
/// reorder mode for the arrow keys; the button starts the mode from the app.
fn reorderable(window: &mut Window, cx: &mut App) -> AnyElement {
    use crate::button::Button;
    use crate::reorderable::{ItemKey, ReorderableOptions, create_reorderable};
    let items = window.use_keyed_state("list-reorder-items", cx, |_, _| {
        vec!["Eggs", "Milk", "Bread", "Butter", "Jam"]
    });
    let order = items.clone();
    let object = create_reorderable(ReorderableOptions::new(move |m, window, cx| {
        order.update(cx, |items, _| {
            if let ItemKey::Text(key) = &m.key
                && let Some(from) = items.iter().position(|i| *i == key.as_ref())
            {
                let item = items.remove(from);
                items.insert(m.to, item);
            }
        });
        window.refresh();
    }));
    let start = object.clone();
    Stack::vertical()
        .gap(SpacingStep::S3)
        .child(
            Stack::horizontal().child(
                Button::new("list-reorder-start", "Move Bread")
                    .on_click(move |_, window, cx| start.start("Bread", window, cx)),
            ),
        )
        .child(
            Card::new().w(px(320.)).child(
                List::new("groceries")
                    .label("Groceries")
                    .dividers(true)
                    .reorderable(object)
                    .children(
                        items
                            .read(cx)
                            .iter()
                            .map(|name| ListItem::new(*name, *name).item_key(*name)),
                    ),
            ),
        )
        .into_any_element()
}

/// A long list through the shared windowing module: the rows that show,
/// plus four past each edge, render; the rest are gaps of their size.
fn windowed(window: &mut Window, cx: &mut App) -> AnyElement {
    use astryx_primitives::{ListAnchor, ListPitch, ListWindowKey, WindowedList};
    use gpui::{InteractiveElement, StatefulInteractiveElement};
    let theme = astryx_theme::current(window, cx);
    let t = theme.tokens();
    let keys: std::rc::Rc<[ListWindowKey]> = (0..10_000usize).map(ListWindowKey::from).collect();
    let scroll = window
        .use_keyed_state("list-windowed-scroll", cx, |_, _| gpui::ScrollHandle::new())
        .read(cx)
        .clone();
    gpui::div()
        .id("list-windowed-scroller")
        .w(px(320.))
        .h(px(280.))
        .border_1()
        .border_color(t.color.border)
        .rounded(t.radius.container)
        .overflow_y_scroll()
        .track_scroll(&scroll)
        .child(
            List::new("windowed").label("Ten thousand rows").child(
                WindowedList::new(
                    "windowed-rows",
                    keys,
                    ListPitch::Estimated(px(48.)),
                    |i, _, _| {
                        ListItem::new(("row", i), format!("Row {}", i + 1))
                            .description(format!("Item {} of 10,000", i + 1))
                            .on_click(|_, _, _| {})
                            .into_any_element()
                    },
                )
                .scroll(&scroll)
                .anchor(ListAnchor::Item(ListWindowKey::from(4999usize)))
                .initial_viewport(px(280.))
                .w_full(),
            ),
        )
        .into_any_element()
}
