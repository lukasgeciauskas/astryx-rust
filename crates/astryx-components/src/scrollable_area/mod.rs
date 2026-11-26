//! ScrollableArea. Upstream: `ScrollableArea/` (component spec
//! `ScrollableArea.spec.md`; system spec AST-025) and `hooks/useScrollableArea.ts`
//! (`astryx_primitives::scroll_area`).

pub mod examples;
pub(crate) mod fade;
#[allow(clippy::module_inception)]
mod scrollable_area;

#[cfg(test)]
mod tests;

pub use astryx_primitives::scroll_area::{
    ScrollAxis, ScrollAxisState, ScrollKeyboardAccess, ScrollOverscroll, ScrollRole,
    ScrollStickyContainment, ScrollableAreaState, UseScrollableAreaOptions, use_scrollable_area,
};
pub use scrollable_area::ScrollableArea;

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const SCROLLABLE_AREA_DOC: ComponentDoc = ComponentDoc {
    name: "ScrollableArea",
    rust_type: "astryx::components::scrollable_area::ScrollableArea",
    props: &[
        p("label", RustApi::Argument("label")),
        p("axis", RustApi::Method("axis")),
        p("role", RustApi::Method("role")),
        p("overscroll", RustApi::Method("overscroll")),
        p("width", RustApi::Method("width")),
        p("height", RustApi::Method("height")),
        p("maxWidth", RustApi::Method("max_width")),
        p("minHeight", RustApi::Method("min_height")),
        p("stickyContainment", RustApi::Method("sticky_containment")),
        p("padding", RustApi::Method("padding")),
        p("paddingInline", RustApi::Method("padding_inline")),
        p(
            "paddingInlineStart",
            RustApi::Method("padding_inline_start"),
        ),
        p("paddingInlineEnd", RustApi::Method("padding_inline_end")),
        p("paddingBlock", RustApi::Method("padding_block")),
        p("paddingBlockStart", RustApi::Method("padding_block_start")),
        p("paddingBlockEnd", RustApi::Method("padding_block_end")),
        p("isFullBleed", RustApi::Method("full_bleed")),
        p("children", RustApi::Trait("ParentElement")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_SCROLLABLE_AREA_DOC: ComponentDoc = ComponentDoc {
    name: "useScrollableArea",
    rust_type: "astryx::components::scrollable_area::use_scrollable_area (astryx_primitives::scroll_area)",
    props: &[
        p(
            "options",
            RustApi::Argument("options: UseScrollableAreaOptions"),
        ),
        p("options.axis", RustApi::Argument("axis")),
        p(
            "options.keyboardAccess",
            RustApi::Argument("keyboard_access"),
        ),
        p("options.overscroll", RustApi::Argument("overscroll")),
        p(
            "options.stickyContainment",
            RustApi::Argument("sticky_containment"),
        ),
    ],
    example: include_str!("examples.rs"),
};
