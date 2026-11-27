//! Carousel and useSnapPaging. Upstream: `Carousel/`, `SnapPaging/` (component spec
//! `Carousel`).

#[allow(clippy::module_inception)]
mod carousel;
pub mod examples;
mod stops;

#[cfg(test)]
mod tests;

pub use carousel::{
    Carousel, CarouselHandle, CarouselSnap, SnapPaging, SnapPagingPages, use_snap_paging,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const CAROUSEL_DOC: ComponentDoc = ComponentDoc {
    name: "Carousel",
    rust_type: "rustryx::components::carousel::Carousel",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("gap", RustApi::Method("gap")),
        p("hasButtons", RustApi::Method("buttons")),
        p("hasEdgeFade", RustApi::Method("edge_fade")),
        p("hasLoop", RustApi::Method("looping")),
        p("hasSnap", RustApi::Method("snap")),
        p("padding", RustApi::Method("padding")),
        p("aria-label", RustApi::Method("label")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("handleRef", RustApi::Method("handle (CarouselHandle)")),
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

pub const USE_SNAP_PAGING_DOC: ComponentDoc = ComponentDoc {
    name: "useSnapPaging",
    rust_type: "rustryx::components::carousel::use_snap_paging -> SnapPaging",
    props: &[p("options", RustApi::Argument("pages (SnapPagingPages)"))],
    example: include_str!("examples.rs"),
};
