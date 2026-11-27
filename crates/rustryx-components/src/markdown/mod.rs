//! Markdown. Upstream: `Markdown/` (component spec `Markdown`). The parser is
//! `rustryx_markdown`, a port of upstream's own.

pub mod examples;
#[allow(clippy::module_inception)]
mod markdown;
mod plugins;
mod streaming;

#[cfg(test)]
mod tests;

pub use markdown::{
    Markdown, MarkdownAutolink, MarkdownBlockquoteProps, MarkdownCitationProps, MarkdownCodeProps,
    MarkdownComponents, MarkdownContentAlign, MarkdownDensity, MarkdownHeadingProps,
    MarkdownHrProps, MarkdownImageProps, MarkdownInlinePlugin, MarkdownLinkProps,
    MarkdownMathDisplay, MarkdownMathProps, MarkdownParagraphProps, MarkdownSourceLines,
};
pub use plugins::{MarkdownExtensionRender, MarkdownPluginNodeRenderer, markdown_extension_render};
pub use streaming::{StreamingTextSpeed, UseStreamingTextOptions, use_streaming_text};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const MARKDOWN_DOC: ComponentDoc = ComponentDoc {
    name: "Markdown",
    rust_type: "rustryx::components::markdown::Markdown",
    props: &[
        p("children", RustApi::Argument("source")),
        p("display", RustApi::Method("display")),
        p("density", RustApi::Method("density")),
        p("headingLevelStart", RustApi::Method("heading_level_start")),
        p("isStreaming", RustApi::Method("streaming")),
        p("onLinkClick", RustApi::Method("on_link_click")),
        p("sources", RustApi::Method("sources")),
        p("citationStyle", RustApi::Method("citation_style")),
        p("contentWidth", RustApi::Method("content_width")),
        p("contentAlign", RustApi::Method("content_align")),
        p("plugins", RustApi::Method("plugins")),
        p("inlinePlugins", RustApi::Method("inline_plugins")),
        p("autolink", RustApi::Method("autolink")),
        p("codeLanguages", RustApi::Method("code_languages")),
        p("components", RustApi::Method("components")),
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

pub const USE_STREAMING_TEXT_DOC: ComponentDoc = ComponentDoc {
    name: "useStreamingText",
    rust_type: "rustryx::components::markdown::use_streaming_text -> SharedString",
    props: &[
        p("targetText", RustApi::Argument("target")),
        p("isStreaming", RustApi::Argument("streaming")),
        p("options", RustApi::Argument("options")),
        p(
            "options.speed",
            RustApi::Method("UseStreamingTextOptions::speed"),
        ),
    ],
    example: include_str!("examples.rs"),
};
