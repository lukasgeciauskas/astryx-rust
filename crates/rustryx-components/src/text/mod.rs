//! Text and Heading. Upstream: `packages/core/src/Text/`.

pub mod examples;
mod heading;
mod inline;
#[allow(clippy::module_inception)]
mod text;
mod typography;

#[cfg(test)]
mod tests;

pub use heading::{DisplayType, Heading, HeadingLevel};
pub use inline::{FlowReport, InlineFlow};
pub(crate) use inline::{InlineBuilder, InlineGroup, InlineGroupState, InlineRing, InlineSpan};
pub use text::{Text, TextDisplay, TextElement, TruncateTooltip, WordBreak};
pub use typography::{Justify, TextColor, TextSize, TextType, TextWeight, TextWrap};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const TEXT_DOC: ComponentDoc = ComponentDoc {
    name: "Text",
    rust_type: "rustryx::components::text::Text",
    props: &[
        p("type", RustApi::Method("text_type")),
        p("children", RustApi::Argument("text")),
        p("size", RustApi::Method("size")),
        p("color", RustApi::Method("color")),
        p("weight", RustApi::Method("weight")),
        p("display", RustApi::Method("display")),
        p("as", RustApi::Method("element")),
        p("maxLines", RustApi::Method("max_lines")),
        p("hasTruncateTooltip", RustApi::Method("truncate_tooltip")),
        p("wordBreak", RustApi::Method("word_break")),
        p("textWrap", RustApi::Method("text_wrap")),
        p("justify", RustApi::Method("justify")),
        p("hasCapsize", RustApi::Method("capsize")),
        p("hasStrikethrough", RustApi::Method("strikethrough")),
        p("hasTabularNumbers", RustApi::Method("tabular_numbers")),
        p("id", RustApi::Method("id")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const HEADING_DOC: ComponentDoc = ComponentDoc {
    name: "Heading",
    rust_type: "rustryx::components::text::Heading",
    props: &[
        p("level", RustApi::Argument("level")),
        p("type", RustApi::Method("display_type")),
        p("weight", RustApi::Method("weight")),
        p("children", RustApi::Argument("text")),
        p("accessibilityLevel", RustApi::Method("accessibility_level")),
        p("color", RustApi::Method("color")),
        p("display", RustApi::Method("display")),
        p("maxLines", RustApi::Method("max_lines")),
        p("hasTruncateTooltip", RustApi::Method("truncate_tooltip")),
        p("wordBreak", RustApi::Method("word_break")),
        p("textWrap", RustApi::Method("text_wrap")),
        p("justify", RustApi::Method("justify")),
        p("hasCapsize", RustApi::Method("capsize")),
        p("hasStrikethrough", RustApi::Method("strikethrough")),
        p("id", RustApi::Method("id")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
        p(
            "ref",
            RustApi::Waived("GPUI elements have no refs; give the heading an id"),
        ),
    ],
    example: include_str!("examples.rs"),
};
