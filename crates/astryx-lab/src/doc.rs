//! The Rust side of each lab component's documentation (see
//! `astryx_components::doc`): `astryx component <Name> --package lab` and
//! `astryx parity` read this list.

pub use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const EXAMPLE: &str = include_str!("examples.rs");

/// Upstream lab `Stat`.
pub const STAT_DOC: ComponentDoc = ComponentDoc {
    name: "Stat",
    rust_type: "astryx::lab::Stat",
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
];
