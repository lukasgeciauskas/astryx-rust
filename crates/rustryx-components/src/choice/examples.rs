//! Choice examples: the gallery's Choices page and the doc example of
//! CheckboxInput, Switch, the indicators and FieldStatus.

use std::time::Duration;

use gpui::{AnyElement, App, Entity, IntoElement, ParentElement, SharedString, Task, Window};

use crate::choice::{
    CheckboxInput, CheckboxList, CheckboxListItem, LabelPosition, LabelSpacing, RadioList,
    RadioListItem, Switch,
};
use crate::field::{FieldStatus, FieldStatusVariant, InputStatus, StatusType};
use crate::indicator::{
    CheckIndicator, CheckboxIndicator, ChoiceSize, IndicatorState, RadioIndicator,
};
use crate::item::ItemDensity;
use crate::layout::{Orientation, SpacingStep, Stack, StackAlign};
use crate::slider::{Slider, SliderMark, SliderValue, ValueDisplay};
use crate::text::{Text, TextType};

/// Stores a new value and redraws.
fn store(entity: Entity<bool>) -> impl Fn(bool, &mut Window, &mut App) + 'static {
    move |next, window, cx| {
        entity.update(cx, |v, _| *v = next);
        window.refresh();
    }
}

/// A one-second save, then the new value.
fn slow_save(entity: Entity<bool>) -> impl Fn(bool, &mut Window, &mut App) -> Task<()> + 'static {
    move |next, window, cx| {
        let wait = cx.background_executor().timer(Duration::from_secs(1));
        let entity = entity.clone();
        window.spawn(cx, async move |cx| {
            wait.await;
            cx.update(|window, cx| {
                entity.update(cx, |v, _| *v = next);
                window.refresh();
            })
            .ok();
        })
    }
}

pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let label = |text: &'static str| Text::new(text).text_type(TextType::Supporting);
    // The page keeps its own values between frames.
    let terms = window.use_keyed_state("terms", cx, |_, _| false);
    let news = window.use_keyed_state("news", cx, |_, _| true);
    let wifi = window.use_keyed_state("wifi", cx, |_, _| true);
    let sync = window.use_keyed_state("sync", cx, |_, _| false);
    let dark = window.use_keyed_state("dark", cx, |_, _| false);
    let plan = window.use_keyed_state("plan", cx, |_, _| SharedString::from("pro"));
    let size = window.use_keyed_state("size", cx, |_, _| SharedString::from("m"));
    let toppings =
        window.use_keyed_state("toppings", cx, |_, _| vec![SharedString::from("cheese")]);
    let volume = window.use_keyed_state("volume", cx, |_, _| SliderValue::Single(40.));
    let price = window.use_keyed_state("price", cx, |_, _| SliderValue::Range(20., 80.));
    let set_value = |entity: Entity<SliderValue>| {
        move |next: SliderValue, window: &mut Window, cx: &mut App| {
            entity.update(cx, |v, _| *v = next);
            window.refresh();
        }
    };
    let set_text = |entity: Entity<SharedString>| {
        move |next: SharedString, window: &mut Window, cx: &mut App| {
            entity.update(cx, |v, _| *v = next);
            window.refresh();
        }
    };

    Stack::vertical()
        .gap(SpacingStep::S4)
        .child(label("CheckboxInput"))
        .child(
            Stack::vertical()
                .gap(SpacingStep::S3)
                .child(
                    CheckboxInput::new("terms", "I accept the terms", *terms.read(cx))
                        .required(true)
                        .on_change(store(terms.clone())),
                )
                .child(
                    CheckboxInput::new("news", "Product news (1 s save)", *news.read(cx))
                        .description("A short email, at most once a month.")
                        .change_action(slow_save(news.clone())),
                )
                .child(
                    CheckboxInput::new("all", "Select all", IndicatorState::Indeterminate)
                        .size(ChoiceSize::Sm),
                )
                .child(CheckboxInput::new("locked", "Managed by your admin", true).disabled(true))
                .child(
                    CheckboxInput::new("consent", "Share usage data", false)
                        .status(InputStatus::error("Choose whether to share usage data.")),
                ),
        )
        .child(label("Switch"))
        .child(
            Stack::vertical()
                .gap(SpacingStep::S3)
                .child(Switch::new("wifi", "Wi-Fi", *wifi.read(cx)).on_change(store(wifi.clone())))
                .child(
                    Switch::new("sync", "Sync (1 s save)", *sync.read(cx))
                        .size(ChoiceSize::Sm)
                        .change_action(slow_save(sync.clone())),
                )
                .child(
                    Switch::new("dark", "Dark mode", *dark.read(cx))
                        .label_position(LabelPosition::Start)
                        .label_spacing(LabelSpacing::Spread)
                        .width(gpui::px(320.))
                        .on_change(store(dark.clone())),
                )
                .child(Switch::new("off", "Unavailable", false).disabled(true)),
        )
        .child(label("RadioList"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S8)
                .child(
                    RadioList::new("plan", "Plan", plan.read(cx).clone())
                        .description("You can change it later.")
                        .on_change(set_text(plan.clone()))
                        .item(RadioListItem::new("free", "Free").description("Up to 3 projects"))
                        .item(RadioListItem::new("pro", "Pro").description("Unlimited projects"))
                        .item(
                            RadioListItem::new("team", "Team")
                                .description("Contact sales")
                                .disabled(true),
                        ),
                )
                .child(
                    RadioList::new("size", "Size", size.read(cx).clone())
                        .orientation(Orientation::Horizontal)
                        .size(ChoiceSize::Sm)
                        .on_change(set_text(size.clone()))
                        .items(["s", "m", "l"].map(|v| RadioListItem::new(v, v.to_uppercase()))),
                ),
        )
        .child(label("CheckboxList"))
        .child(
            CheckboxList::new("toppings", "Toppings")
                .description("Pick any.")
                .width(gpui::px(360.))
                .density(ItemDensity::Compact)
                .dividers(true)
                .value(toppings.read(cx).clone())
                .on_change({
                    let toppings = toppings.clone();
                    move |next, window, cx| {
                        toppings.update(cx, |v, _| *v = next);
                        window.refresh();
                    }
                })
                .item(CheckboxListItem::new("Cheese").value("cheese"))
                .item(CheckboxListItem::new("Mushrooms").value("mushrooms"))
                .item(
                    CheckboxListItem::new("Truffle")
                        .value("truffle")
                        .description("Seasonal")
                        .disabled(true),
                ),
        )
        .child(label("Slider"))
        .child(
            Stack::vertical()
                .gap(SpacingStep::S4)
                .width(gpui::px(360.))
                .child(
                    Slider::new("volume", "Volume", *volume.read(cx))
                        .value_display(ValueDisplay::Text)
                        .on_change(set_value(volume.clone())),
                )
                .child(
                    Slider::new("price", "Price range", *price.read(cx))
                        .step(5.)
                        .min_steps_between_thumbs(2.)
                        .value_display(ValueDisplay::Text)
                        .marks(
                            [0., 25., 50., 75., 100.]
                                .map(|v| SliderMark::new(v).label(format!("${v}"))),
                        )
                        .on_change(set_value(price.clone())),
                )
                .child(Slider::new("locked", "Locked", 30.).disabled(true)),
        )
        .child(label("Indicators"))
        .child(
            Stack::horizontal()
                .gap(SpacingStep::S4)
                .v_align(StackAlign::Center)
                .child(CheckboxIndicator::new(IndicatorState::Unchecked))
                .child(CheckboxIndicator::new(IndicatorState::Checked))
                .child(CheckboxIndicator::new(IndicatorState::Indeterminate))
                .child(RadioIndicator::new(false))
                .child(RadioIndicator::new(true))
                .child(CheckIndicator::new(true)),
        )
        .child(label("FieldStatus"))
        .child(
            Stack::vertical()
                .gap(SpacingStep::S2)
                .child(
                    FieldStatus::new(
                        "warning",
                        StatusType::Warning,
                        "This will be visible to others.",
                    )
                    .variant(FieldStatusVariant::Detached),
                )
                .child(
                    FieldStatus::new("success", StatusType::Success, "Username is available.")
                        .variant(FieldStatusVariant::Detached),
                ),
        )
        .into_any_element()
}
