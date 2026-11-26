//! Layout primitives: Stack (HStack, VStack, StackItem), Center, Grid
//! (GridSpan), Divider, AspectRatio and VisuallyHidden.

pub mod examples;
mod grid;
mod misc;
mod regions;
mod spacing;
mod stack;

#[cfg(test)]
mod regions_tests;
#[cfg(test)]
mod tests;

pub use grid::{Grid, GridAlign, GridColumns, GridRepeat, GridSpan, SpanColumns};
pub use misc::{
    AspectFit, AspectRatio, AspectShape, Center, CenterAxis, Divider, DividerVariant, Orientation,
    VisuallyHidden,
};
pub(crate) use regions::padding_container;
pub use regions::{
    Layout, LayoutArea, LayoutContainerKind, LayoutContent, LayoutDividerDefault, LayoutFooter,
    LayoutHeader, LayoutHeight, LayoutPanel, LayoutSlots, PaddingContainer,
};
pub(crate) use spacing::host_children;
pub use spacing::{
    ContainerChild, ContainerCorners, ContainerPadding, Padding, Sizing, SpacingStep,
};
pub(crate) use spacing::{padding_setters, size_setters};
pub use stack::{Stack, StackAlign, StackDirection, StackItem, StackItemSize, StackWrap};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const NO_ELEMENT_TYPE: &str = "GPUI has no HTML tags; the root is always a box";

const PADDING: [PropMapping; 7] = [
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
];

pub const STACK_DOC: ComponentDoc = ComponentDoc {
    name: "Stack",
    rust_type: "astryx::components::layout::Stack (Stack::horizontal() is HStack, Stack::vertical() is VStack)",
    props: &[
        p("direction", RustApi::Argument("direction")),
        p("hAlign", RustApi::Method("h_align")),
        p("vAlign", RustApi::Method("v_align")),
        p("justify", RustApi::Method("justify")),
        p("align", RustApi::Method("align")),
        p("width", RustApi::Method("width")),
        p("height", RustApi::Method("height")),
        p("maxWidth", RustApi::Method("max_width")),
        p("minHeight", RustApi::Method("min_height")),
        p("gap", RustApi::Method("gap")),
        PADDING[0],
        PADDING[1],
        PADDING[2],
        PADDING[3],
        PADDING[4],
        PADDING[5],
        PADDING[6],
        p("isScrollable", RustApi::Method("scrollable")),
        p("wrap", RustApi::Method("wrap")),
        p("as", RustApi::Waived(NO_ELEMENT_TYPE)),
        p("children", RustApi::Trait("ParentElement")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const CENTER_DOC: ComponentDoc = ComponentDoc {
    name: "Center",
    rust_type: "astryx::components::layout::Center",
    props: &[
        p("axis", RustApi::Method("axis")),
        p("width", RustApi::Method("width")),
        p("height", RustApi::Method("height")),
        p("maxWidth", RustApi::Method("max_width")),
        p("minHeight", RustApi::Method("min_height")),
        PADDING[0],
        PADDING[1],
        PADDING[2],
        PADDING[3],
        PADDING[4],
        PADDING[5],
        PADDING[6],
        p("isInline", RustApi::Method("inline")),
        p("children", RustApi::Trait("ParentElement")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const GRID_DOC: ComponentDoc = ComponentDoc {
    name: "Grid",
    rust_type: "astryx::components::layout::Grid",
    props: &[
        p("columns", RustApi::Argument("columns")),
        p("width", RustApi::Method("width")),
        p("height", RustApi::Method("height")),
        p("maxWidth", RustApi::Method("max_width")),
        p("minHeight", RustApi::Method("min_height")),
        p("gap", RustApi::Method("gap")),
        p("rowGap", RustApi::Method("row_gap")),
        p("columnGap", RustApi::Method("column_gap")),
        p("rowHeight", RustApi::Method("row_height")),
        p("align", RustApi::Method("align")),
        p("justify", RustApi::Method("justify")),
        p(
            "children",
            RustApi::Trait("ParentElement (spans through Grid::span)"),
        ),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const DIVIDER_DOC: ComponentDoc = ComponentDoc {
    name: "Divider",
    rust_type: "astryx::components::layout::Divider",
    props: &[
        p("orientation", RustApi::Method("orientation")),
        p("label", RustApi::Method("label")),
        p("variant", RustApi::Method("variant")),
        p("isFullBleed", RustApi::Method("full_bleed")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const ASPECT_RATIO_DOC: ComponentDoc = ComponentDoc {
    name: "AspectRatio",
    rust_type: "astryx::components::layout::AspectRatio",
    props: &[
        p("ratio", RustApi::Argument("ratio")),
        p("shape", RustApi::Method("shape")),
        p(
            "fit",
            RustApi::Method("fit (stretches children; images set their own object_fit)"),
        ),
        p("children", RustApi::Trait("ParentElement")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const VISUALLY_HIDDEN_DOC: ComponentDoc = ComponentDoc {
    name: "VisuallyHidden",
    rust_type: "astryx::components::layout::VisuallyHidden",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("as", RustApi::Waived(NO_ELEMENT_TYPE)),
    ],
    example: include_str!("examples.rs"),
};

const NO_REFS: &str = "GPUI elements have no refs";

pub const LAYOUT_DOC: ComponentDoc = ComponentDoc {
    name: "Layout",
    rust_type: "astryx::components::layout::Layout",
    props: &[
        p("content", RustApi::Method("content")),
        p("header", RustApi::Method("header")),
        p("footer", RustApi::Method("footer")),
        p("start", RustApi::Method("start")),
        p("end", RustApi::Method("end")),
        p("height", RustApi::Method("height")),
        p("contentWidth", RustApi::Method("content_width")),
        p("padding", RustApi::Method("padding")),
        p("children", RustApi::Trait("ParentElement")),
        p("ref", RustApi::Waived(NO_REFS)),
        p(
            "defaultHasDividers",
            RustApi::Method("default_has_dividers"),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const LAYOUT_HEADER_DOC: ComponentDoc = ComponentDoc {
    name: "LayoutHeader",
    rust_type: "astryx::components::layout::LayoutHeader",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("hasDivider", RustApi::Method("has_divider")),
        p("height", RustApi::Method("height")),
        p("label", RustApi::Method("label")),
        p("padding", RustApi::Method("padding")),
        p("paddingBlockEnd", RustApi::Method("padding_block_end")),
        p("role", RustApi::Method("role")),
        p("ref", RustApi::Waived(NO_REFS)),
    ],
    example: include_str!("examples.rs"),
};

pub const LAYOUT_CONTENT_DOC: ComponentDoc = ComponentDoc {
    name: "LayoutContent",
    rust_type: "astryx::components::layout::LayoutContent",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("padding", RustApi::Method("padding")),
        p("isScrollable", RustApi::Method("scrollable")),
        p("label", RustApi::Method("label")),
        p("role", RustApi::Method("role")),
        p("ref", RustApi::Waived(NO_REFS)),
    ],
    example: include_str!("examples.rs"),
};

pub const LAYOUT_FOOTER_DOC: ComponentDoc = ComponentDoc {
    name: "LayoutFooter",
    rust_type: "astryx::components::layout::LayoutFooter",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("hasDivider", RustApi::Method("has_divider")),
        p("height", RustApi::Method("height")),
        p("label", RustApi::Method("label")),
        p("padding", RustApi::Method("padding")),
        p("role", RustApi::Method("role")),
        p("ref", RustApi::Waived(NO_REFS)),
    ],
    example: include_str!("examples.rs"),
};

pub const LAYOUT_PANEL_DOC: ComponentDoc = ComponentDoc {
    name: "LayoutPanel",
    rust_type: "astryx::components::layout::LayoutPanel",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("hasDivider", RustApi::Method("has_divider")),
        p("padding", RustApi::Method("padding")),
        p("isScrollable", RustApi::Method("scrollable")),
        p("label", RustApi::Method("label")),
        p("role", RustApi::Method("role")),
        p("width", RustApi::Method("width")),
        p("resizable", RustApi::Method("resizable")),
        p("ref", RustApi::Waived(NO_REFS)),
    ],
    example: include_str!("examples.rs"),
};
