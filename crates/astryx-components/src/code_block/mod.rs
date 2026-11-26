//! CodeBlock. Upstream: `CodeBlock/` (component spec `CodeBlock`). The syntax engine is
//! `astryx_syntax`; colors come from `syntax_theme`.

#[allow(clippy::module_inception)]
mod code_block;
pub mod examples;

#[cfg(test)]
mod tests;

pub use code_block::{
    CODE_BLOCK_COPY, CodeBlock, CodeBlockContainer, CodeBlockSize, HighlightMode,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const CODE_BLOCK_DOC: ComponentDoc = ComponentDoc {
    name: "CodeBlock",
    rust_type: "astryx::components::code_block::CodeBlock",
    props: &[
        p("code", RustApi::Argument("code")),
        p("language", RustApi::Method("language")),
        p("title", RustApi::Method("title")),
        p("hasLanguageLabel", RustApi::Method("language_label")),
        p("hasLineNumbers", RustApi::Method("line_numbers")),
        p("startLineNumber", RustApi::Method("start_line_number")),
        p("highlightLines", RustApi::Method("highlight_lines")),
        p("isDiff", RustApi::Method("diff")),
        p("hasCopyButton", RustApi::Method("copy_button")),
        p("onCopy", RustApi::Method("on_copy")),
        p("onCopyError", RustApi::Method("on_copy_error")),
        p("endContent", RustApi::Method("end_content")),
        p("isWrapped", RustApi::Method("wrapped")),
        p("maxHeight", RustApi::Method("max_height")),
        p("size", RustApi::Method("size")),
        p("width", RustApi::Method("width")),
        p("container", RustApi::Method("container")),
        p("tokenizer", RustApi::Method("tokenizer (deprecated)")),
        p("syntaxTheme", RustApi::Method("syntax_theme")),
        p("highlightMode", RustApi::Method("highlight_mode")),
        p("isCollapsible", RustApi::Method("collapsible")),
        p(
            "collapsibleThreshold",
            RustApi::Method("collapsible_threshold"),
        ),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("xstyle", RustApi::Trait("Styled")),
        p(
            "className",
            RustApi::Waived("GPUI has no class names; use Styled"),
        ),
        p("style", RustApi::Trait("Styled")),
        p(
            "data-testid",
            RustApi::Waived("GPUI has no DOM test ids; tests use debug selectors"),
        ),
    ],
    example: include_str!("examples.rs"),
};
