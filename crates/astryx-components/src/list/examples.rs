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
        .into_any_element()
}
