//! The lab gallery's examples (upstream's lab stories).

use gpui::{AnyElement, App, IntoElement, ParentElement, Window};
use rustryx_components::layout::{SpacingStep, Stack};
use rustryx_components::text::{Text, TextType};

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

/// A gallery section: its title and what draws it.
pub type Section = (&'static str, fn(&mut Window, &mut App) -> AnyElement);

/// A section that shows one module's examples under its title.
macro_rules! section {
    ($title:literal, $module:ident) => {
        ($title, |window: &mut Window, cx: &mut App| {
            example($title, crate::$module::examples::examples(window, cx))
        })
    };
}

/// The lab's examples as sections, one per component, in the gallery's order. The gallery
/// draws one at a time: a page of all of them rebuilds every example whenever any one of
/// them animates.
pub const SECTIONS: &[Section] = &[
    ("Stat", |_, _| example("Stat", stats())),
    section!("LineClamp", line_clamp),
    section!("CircleCheckIndicator", circle_check_indicator),
    section!("CircularProgress", circular_progress),
    section!("SelectableCardGroup", selectable_card_group),
    section!("PageControl", page_control),
    section!("InfoTip", info_tip),
    section!("DialogHeroHeader", dialog_hero_header),
    section!("ChatReasoning", chat_reasoning),
    section!("LogStream", log_stream),
    section!("CommentPin", comment_pin),
    section!("Tour", tour),
    section!("TabBar", tab_bar),
    section!("TabBarMoreList", tab_bar_more_list),
    section!("NavigationStack", navigation_stack),
    section!("RefreshControl", refresh_control),
    section!("TransferList", transfer_list),
    section!("ListInput", list_input),
    section!("RadialChart", radial),
    section!("SankeyChart", sankey),
    section!("Chat additions", chat),
    section!("EmojiPicker", emoji_picker),
    section!("Comments", comments),
    section!("Schedule", schedule),
    section!("Chart (lab)", lab_chart),
    section!("CodeEditor", code_editor),
    section!("DiffViewer", diff_viewer),
    section!("SVGIcon", svg_icon),
    section!("MediaPlayer", media_player),
];

/// Every lab example, top to bottom.
pub fn examples(window: &mut Window, cx: &mut App) -> AnyElement {
    let mut stack = Stack::vertical().gap(SpacingStep::S8);
    for (_, section) in SECTIONS {
        stack = stack.child(section(window, cx));
    }
    stack.into_any_element()
}
