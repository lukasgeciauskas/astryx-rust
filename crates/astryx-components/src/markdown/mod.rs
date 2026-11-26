//! Markdown. Upstream: `Markdown/` (component spec `Markdown`). The parser is
//! `astryx_markdown`, a port of upstream's own.

pub mod examples;
#[allow(clippy::module_inception)]
mod markdown;

#[cfg(test)]
mod tests;

pub use markdown::{
    Markdown, MarkdownAutolink, MarkdownCodeProps, MarkdownComponents, MarkdownContentAlign,
    MarkdownDensity, MarkdownHeadingProps, MarkdownImageProps, MarkdownLinkProps,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const MARKDOWN_DOC: ComponentDoc = ComponentDoc {
    name: "Markdown",
    rust_type: "astryx::components::markdown::Markdown",
    props: &[
        p("children", RustApi::Argument("source")),
        p("display", RustApi::Method("display")),
        p("density", RustApi::Method("density")),
        p("headingLevelStart", RustApi::Method("heading_level_start")),
        p(
            "isStreaming",
            RustApi::Pending("is_streaming", "M10 stage 4: streaming parse and fade-in"),
        ),
        p("onLinkClick", RustApi::Method("on_link_click")),
        p(
            "sources",
            RustApi::Pending("sources", "M10 stage 4: citation chips"),
        ),
        p(
            "citationStyle",
            RustApi::Pending("citation_style", "M10 stage 4: citation chips"),
        ),
        p("contentWidth", RustApi::Method("content_width")),
        p("contentAlign", RustApi::Method("content_align")),
        p(
            "plugins",
            RustApi::Pending("plugins", "M10 stage 4: syntax, transforms and renderers"),
        ),
        p(
            "inlinePlugins",
            RustApi::Pending("inline_plugins", "M10 stage 4"),
        ),
        p("autolink", RustApi::Method("autolink")),
        p(
            "codeLanguages",
            RustApi::Pending("code_languages", "M10 stage 4"),
        ),
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
