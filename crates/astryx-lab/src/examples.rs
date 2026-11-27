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

/// The lab gallery: each component's own examples.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    Stack::vertical()
        .gap(SpacingStep::S8)
        .child(example("Stat", stats()))
        .child(example(
            "LineClamp",
            crate::line_clamp::examples::examples(window, cx),
        ))
        .child(example(
            "CircleCheckIndicator",
            crate::circle_check_indicator::examples::examples(window, cx),
        ))
        .child(example(
            "CircularProgress",
            crate::circular_progress::examples::examples(window, cx),
        ))
        .child(example(
            "SelectableCardGroup",
            crate::selectable_card_group::examples::examples(window, cx),
        ))
        .child(example(
            "PageControl",
            crate::page_control::examples::examples(window, cx),
        ))
        .child(example(
            "InfoTip",
            crate::info_tip::examples::examples(window, cx),
        ))
        .child(example(
            "DialogHeroHeader",
            crate::dialog_hero_header::examples::examples(window, cx),
        ))
        .child(example(
            "ChatReasoning",
            crate::chat_reasoning::examples::examples(window, cx),
        ))
        .child(example(
            "LogStream",
            crate::log_stream::examples::examples(window, cx),
        ))
        .child(example(
            "CommentPin",
            crate::comment_pin::examples::examples(window, cx),
        ))
        .child(example("Tour", crate::tour::examples::examples(window, cx)))
        .child(example(
            "TabBar",
            crate::tab_bar::examples::examples(window, cx),
        ))
        .child(example(
            "TabBarMoreList",
            crate::tab_bar_more_list::examples::examples(window, cx),
        ))
        .child(example(
            "NavigationStack",
            crate::navigation_stack::examples::examples(window, cx),
        ))
        .child(example(
            "RefreshControl",
            crate::refresh_control::examples::examples(window, cx),
        ))
        .child(example(
            "TransferList",
            crate::transfer_list::examples::examples(window, cx),
        ))
        .child(example(
            "ListInput",
            crate::list_input::examples::examples(window, cx),
        ))
        .child(example(
            "RadialChart",
            crate::radial::examples::examples(window, cx),
        ))
        .child(example(
            "SankeyChart",
            crate::sankey::examples::examples(window, cx),
        ))
        .child(example(
            "Chat additions",
            crate::chat::examples::examples(window, cx),
        ))
        .child(example(
            "EmojiPicker",
            crate::emoji_picker::examples::examples(window, cx),
        ))
        .child(example(
            "Comments",
            crate::comments::examples::examples(window, cx),
        ))
        .child(example(
            "Schedule",
            crate::schedule::examples::examples(window, cx),
        ))
        .child(example(
            "Chart (lab)",
            crate::lab_chart::examples::examples(window, cx),
        ))
        .child(example(
            "CodeEditor",
            crate::code_editor::examples::examples(window, cx),
        ))
        .child(example(
            "DiffViewer",
            crate::diff_viewer::examples::examples(window, cx),
        ))
        .child(example(
            "SVGIcon",
            crate::svg_icon::examples::examples(window, cx),
        ))
        .into_any_element()
}
