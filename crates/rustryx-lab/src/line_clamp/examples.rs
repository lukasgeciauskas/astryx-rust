//! The LineClamp gallery (upstream's `LineClamp` stories), each in a 320 px column.

use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, Window, div, px};
use rustryx_components::layout::{SpacingStep, Stack};
use rustryx_components::link::Link;
use rustryx_components::text::{Text, TextType, TextWeight};
use rustryx_primitives::Placement;

use super::LineClamp;

const LONG_TEXT: &str = "Astryx is an open source design system that grew inside Meta over the last eight years, where it became the most-used and largest design system in the company — powering thousands of apps and shaped by the engineers, designers, and product teams who depend on it every day.";

fn example(title: &'static str, body: impl IntoElement) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S2)
        .child(Text::new(title).text_type(TextType::Large))
        .child(div().max_w(px(320.)).child(body))
        .into_any_element()
}

/// The LineClamp examples.
pub fn examples(_window: &mut Window, _cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example("Three lines", LineClamp::new("lc-default", 3).child(LONG_TEXT)))
        .child(example("Two lines", LineClamp::new("lc-two", 2).child(LONG_TEXT)))
        .child(example(
            "One line, tooltip below",
            LineClamp::new("lc-one", 1).truncate_tooltip_placement(Placement::Below).child(LONG_TEXT),
        ))
        .child(example(
            "Without tooltip",
            LineClamp::new("lc-no-tooltip", 2).truncate_tooltip(false).child(LONG_TEXT),
        ))
        .child(example(
            "Mixed content",
            LineClamp::new("lc-mixed", 2)
                .child("This card description mixes ")
                .child(Text::new("bold").weight(TextWeight::Bold))
                .child(", ")
                .child(Text::new("italic").italic())
                .child(", and plain text. LineClamp clamps all of it as one block and adds no typography of its own."),
        ))
        .child(example(
            "Short content, not clamped",
            LineClamp::new("lc-short", 3).child("This content is short enough that it never clamps."),
        ))
        .child(example(
            "A link past the clamp leaves the Tab order",
            LineClamp::new("lc-link", 2)
                .child(LONG_TEXT)
                .child(" ")
                .child(Link::new("lc-link-more", "Read the full story").href("https://example.com")),
        ))
        .into_any_element()
}
