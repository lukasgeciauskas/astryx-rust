//! Text and Heading. Upstream: `packages/core/src/Text/`.

pub mod examples;
mod heading;
#[allow(clippy::module_inception)]
mod text;
mod typography;

#[cfg(test)]
mod tests;

pub use heading::{DisplayType, Heading, HeadingLevel};
pub use text::{Text, TextElement, TruncateTooltip, WordBreak};
pub use typography::{Justify, TextColor, TextSize, TextType, TextWeight, TextWrap};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const NO_INLINE: &str = "GPUI has no inline formatting context; Text is always a block box";
const NO_CAPSIZE: &str = "GPUI has no text-box-trim";

pub const TEXT_DOC: ComponentDoc = ComponentDoc {
    name: "Text",
    rust_type: "astryx::components::text::Text",
    props: &[
        p("type", RustApi::Method("text_type")),
        p("children", RustApi::Argument("text")),
        p("size", RustApi::Method("size")),
        p("color", RustApi::Method("color")),
        p("weight", RustApi::Method("weight")),
        p("display", RustApi::Waived(NO_INLINE)),
        p("as", RustApi::Method("element")),
        p("maxLines", RustApi::Method("max_lines")),
        p("hasTruncateTooltip", RustApi::Method("truncate_tooltip")),
        p("wordBreak", RustApi::Method("word_break")),
        p("textWrap", RustApi::Method("text_wrap")),
        p("justify", RustApi::Method("justify")),
        p("hasCapsize", RustApi::Waived(NO_CAPSIZE)),
        p("hasStrikethrough", RustApi::Method("strikethrough")),
        p("hasTabularNumbers", RustApi::Method("tabular_numbers")),
        p("id", RustApi::Method("id")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const HEADING_DOC: ComponentDoc = ComponentDoc {
    name: "Heading",
    rust_type: "astryx::components::text::Heading",
    props: &[
        p("level", RustApi::Argument("level")),
        p("type", RustApi::Method("display_type")),
        p("weight", RustApi::Method("weight")),
        p("children", RustApi::Argument("text")),
        p("accessibilityLevel", RustApi::Method("accessibility_level")),
        p("color", RustApi::Method("color")),
        p("display", RustApi::Waived(NO_INLINE)),
        p("maxLines", RustApi::Method("max_lines")),
        p("hasTruncateTooltip", RustApi::Method("truncate_tooltip")),
        p("wordBreak", RustApi::Method("word_break")),
        p("textWrap", RustApi::Method("text_wrap")),
        p("justify", RustApi::Method("justify")),
        p("hasCapsize", RustApi::Waived(NO_CAPSIZE)),
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
