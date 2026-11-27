//! The lab gallery's examples (upstream's lab stories).

use astryx_components::layout::{SpacingStep, Stack};
use astryx_components::text::{Text, TextType};
use gpui::{AnyElement, App, IntoElement, ParentElement, Window};

use crate::stat::{Stat, StatDelta, StatDeltaDirection, StatDeltaSentiment, StatSize};

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(body)
        .into_any_element()
}

/// Stat: a KPI row.
fn stats() -> AnyElement {
    Stack::horizontal()
        .gap(SpacingStep::S8)
        .child(
            Stat::new("Total requests", "2.4M")
                .delta(StatDelta::new("+12.4%", StatDeltaDirection::Up)),
        )
        .child(
            Stat::new("Error rate", "0.42%")
                .delta(
                    StatDelta::new("-0.08%", StatDeltaDirection::Down)
                        .sentiment(StatDeltaSentiment::Positive),
                )
                .description("vs. previous 30 days"),
        )
        .child(
            Stat::new("Active users", "18,204")
                .size(StatSize::Lg)
                .delta(StatDelta::new("0%", StatDeltaDirection::Flat)),
        )
        .child(
            Stat::new("Latency", "212 ms").size(StatSize::Sm).delta(
                StatDelta::new("+18 ms", StatDeltaDirection::Up)
                    .sentiment(StatDeltaSentiment::Negative),
            ),
        )
        .into_any_element()
}

/// The lab gallery.
pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example("Stat", stats()))
        .into_any_element()
}
