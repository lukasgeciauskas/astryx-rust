//! Outline and its source of items. Upstream: `Outline/` (component spec `Outline`).
//! `useOutlineFromMarkdown` needs Markdown's parser and heading slugs, and lands with
//! Markdown (M10).

pub mod examples;
#[allow(clippy::module_inception)]
mod outline;
mod source;

#[cfg(test)]
mod tests;

pub use outline::{Outline, OutlineDensity, OutlineItem};
pub(crate) use source::OutlineCollect;
pub use source::{OutlineSource, use_outline_from_container};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const OUTLINE_DOC: ComponentDoc = ComponentDoc {
    name: "Outline",
    rust_type: "astryx::components::outline::Outline",
    props: &[
        p("items", RustApi::Argument("items")),
        p("activeId", RustApi::Method("active_id")),
        p("onActiveIdChange", RustApi::Method("on_active_id_change")),
        p("label", RustApi::Method("label")),
        p("density", RustApi::Method("density")),
        p("onNavigateStart", RustApi::Method("on_navigate_start")),
        p("onNavigateEnd", RustApi::Method("on_navigate_end")),
        p("offset", RustApi::Method("offset")),
        p("scrollContainerRef", RustApi::Method("scroll_container")),
        p("hasScrollOnClick", RustApi::Method("scroll_on_click")),
        p(
            "data-testid",
            RustApi::Waived("GPUI has no DOM test ids; tests use debug selectors"),
        ),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_OUTLINE_FROM_DOM_DOC: ComponentDoc = ComponentDoc {
    name: "useOutlineFromDOM",
    rust_type: "astryx::components::outline::use_outline_from_container -> OutlineSource",
    props: &[p(
        "containerRef",
        RustApi::Method("OutlineSource::container (wraps the element holding the headings)"),
    )],
    example: include_str!("examples.rs"),
};
