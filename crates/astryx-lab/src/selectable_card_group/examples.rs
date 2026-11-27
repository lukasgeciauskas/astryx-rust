//! The SelectableCardGroup gallery (upstream's `SelectableCardGroup` stories): a plan
//! picked as a radio group, add-ons as checkboxes, a vertical stack, and a disabled
//! group.

use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Text, TextColor, TextType, TextWeight};
use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Window, px};

use super::{SelectableCardGroup, SelectableCardGroupItem, SelectableCardGroupOrientation};

const PLANS: [(&str, &str, &str); 3] = [
    ("basic", "Basic", "$9/mo"),
    ("pro", "Pro", "$29/mo"),
    ("team", "Team", "$99/mo"),
];

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

fn plan(id: &str, name: &'static str, price: &'static str) -> SelectableCardGroupItem {
    SelectableCardGroupItem::new(SharedString::from(id.to_string()), name).child(
        Stack::vertical()
            .gap(SpacingStep::S1)
            .child(Text::new(name).weight(TextWeight::Bold))
            .child(
                Text::new(price)
                    .text_type(TextType::Supporting)
                    .color(TextColor::Secondary),
            ),
    )
}

/// The SelectableCardGroup examples.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let chosen = window.use_keyed_state("scg-plan", cx, |_, _| Some(SharedString::from("basic")));
    let add_ons = window.use_keyed_state("scg-add-ons", cx, |_, _| vec![SharedString::from("pro")]);
    let stacked = window.use_keyed_state("scg-stacked", cx, |_, _| None::<SharedString>);
    let single = SelectableCardGroup::new("scg-single", "Plan", chosen.read(cx).clone())
        .description("Billed monthly")
        .on_change({
            let chosen = chosen.clone();
            move |value, _, cx| {
                chosen.update(cx, |v, cx| {
                    *v = Some(value);
                    cx.notify();
                })
            }
        })
        .items(PLANS.map(|(id, name, price)| plan(id, name, price).width(px(160.))));
    let multiple =
        SelectableCardGroup::multiple("scg-multiple", "Add-ons", add_ons.read(cx).clone())
            .on_change_values({
                let add_ons = add_ons.clone();
                move |values, _, cx| {
                    add_ons.update(cx, |v, cx| {
                        *v = values;
                        cx.notify();
                    })
                }
            })
            .items(PLANS.map(|(id, name, price)| plan(id, name, price).width(px(160.))));
    let vertical = SelectableCardGroup::new("scg-vertical", "Plan", stacked.read(cx).clone())
        .orientation(SelectableCardGroupOrientation::Vertical)
        .on_change({
            let stacked = stacked.clone();
            move |value, _, cx| {
                stacked.update(cx, |v, cx| {
                    *v = Some(value);
                    cx.notify();
                })
            }
        })
        .items(PLANS.map(|(id, name, price)| plan(id, name, price).max_width(px(320.))));
    let disabled = SelectableCardGroup::new("scg-disabled", "Plan", Some("pro"))
        .disabled(true)
        .items(PLANS.map(|(id, name, price)| plan(id, name, price).width(px(160.))));
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example("Single choice (radio group)", single))
        .child(example("Multiple choice (checkboxes)", multiple))
        .child(example("Vertical", vertical))
        .child(example("Disabled", disabled))
        .into_any_element()
}
