//! The Rust side of each lab component's documentation (see
//! `rustryx_components::doc`): `rustryx component <Name> --package lab` and
//! `rustryx parity` read this list.

pub use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const EXAMPLE: &str = include_str!("examples.rs");

/// Upstream lab `Stat`.
pub const STAT_DOC: ComponentDoc = ComponentDoc {
    name: "Stat",
    rust_type: "rustryx::lab::Stat",
    props: &[
        p("label", RustApi::Argument("label")),
        p("value", RustApi::Argument("value")),
        p("delta", RustApi::Method("delta")),
        p("description", RustApi::Method("description")),
        p("media", RustApi::Method("media")),
        p("size", RustApi::Method("size")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: EXAMPLE,
};

/// Every ported lab component with an upstream doc (lab InfoTip, a deprecated re-export
/// of Core's, and SelectableCardGroupItem have none of their own).
pub const REGISTRY: &[ComponentDoc] = &[
    STAT_DOC,
    crate::line_clamp::LINE_CLAMP_DOC,
    crate::circle_check_indicator::CIRCLE_CHECK_INDICATOR_DOC,
    crate::circular_progress::CIRCULAR_PROGRESS_DOC,
    crate::selectable_card_group::SELECTABLE_CARD_GROUP_DOC,
    crate::page_control::PAGE_CONTROL_DOC,
    crate::dialog_hero_header::DIALOG_HERO_HEADER_DOC,
    crate::chat_reasoning::CHAT_REASONING_DOC,
    crate::log_stream::LOG_STREAM_DOC,
    crate::comment_pin::COMMENT_PIN_DOC,
    crate::tour::TOUR_DOC,
    crate::tab_bar::TAB_BAR_DOC,
    crate::tab_bar_more_list::TAB_BAR_MORE_LIST_DOC,
    crate::navigation_stack::NAVIGATION_STACK_DOC,
    crate::radial::RADIAL_CHART_DOC,
    crate::sankey::SANKEY_CHART_DOC,
    crate::transfer_list::TRANSFER_LIST_SELECTOR_DOC,
    crate::list_input::LIST_INPUT_DOC,
    crate::chat::CHAT_TYPING_INDICATOR_DOC,
    crate::chat::CHAT_UNREAD_DIVIDER_DOC,
    crate::chat::CHAT_VIRTUALIZER_DOC,
    crate::chat::CHAT_EMOJI_PICKER_DOC,
    crate::chat::CHAT_REACTION_BAR_DOC,
    crate::emoji_picker::EMOJI_PICKER_DOC,
    crate::comments::COMMENTS_DOC,
    crate::comments::COMMENT_THREAD_DOC,
    crate::comments::COMMENT_COMPOSER_DOC,
    crate::schedule::SCHEDULE_DOC,
    crate::lab_chart::CHART_AREA_DOC,
    crate::lab_chart::CHART_BAR_DOC,
    crate::lab_chart::CHART_CANDLESTICK_DOC,
    crate::lab_chart::CHART_DOT_DOC,
    crate::lab_chart::CHART_DOT_GL_DOC,
    crate::lab_chart::CHART_DOT_GL_INTERACTIVE_DOC,
    crate::lab_chart::CHART_HEATMAP_GL_DOC,
    crate::lab_chart::CHART_LINE_DOC,
    crate::lab_chart::CHART_STREAM_GL_DOC,
    crate::code_editor::CODE_EDITOR_DOC,
    crate::diff_viewer::DIFF_VIEWER_DOC,
    crate::svg_icon::SVG_ICON_DOC,
    crate::svg_icon::WEIGHTED_SVG_ICON_DOC,
    crate::three_d::THREE_D_CHART_DOC,
    crate::three_d::THREE_D_BAR_DOC,
    crate::three_d::THREE_D_SCATTER_DOC,
    crate::three_d::THREE_D_SCATTER_GL_DOC,
    crate::three_d::THREE_D_SURFACE_DOC,
    crate::media_player::MEDIA_PLAYER_DOC,
];
