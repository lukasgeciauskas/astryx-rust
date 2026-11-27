//! Resizable: `use_resizable` and `ResizeHandle`. Upstream: `Resizable/` (component
//! spec `Resizable.spec.md`; system spec AST-010 for its sizes).

mod controller;
pub mod examples;
mod handle;
mod hook;
mod model;

#[cfg(test)]
mod tests;

pub use handle::{PillPlacement, ResizeHandle, ResizeHandlePosition};
pub(crate) use hook::SlideCurve;
pub use hook::{
    ResizableBasis, ResizableProps, ResizableRegion, ResizableRegions, UseResizable,
    UseResizableRegions, use_resizable, use_resizable_basis, use_resizable_regions,
};
pub use model::{ResizableSize, ResizeDirection};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const RESIZABLE_DOC: ComponentDoc = ComponentDoc {
    name: "Resizable",
    rust_type: "rustryx::components::resizable (use_resizable, ResizeHandle)",
    props: &[],
    example: include_str!("examples.rs"),
};

pub const USE_RESIZABLE_DOC: ComponentDoc = ComponentDoc {
    name: "useResizable",
    rust_type: "rustryx::components::resizable::use_resizable (use_resizable_regions for named regions)",
    props: &[
        p("defaultSize", RustApi::Method("default_size")),
        p("minSize", RustApi::Method("min_size")),
        p("maxSize", RustApi::Method("max_size")),
        p("containerRef", RustApi::Method("container")),
        p("direction", RustApi::Method("direction")),
        p("collapsible", RustApi::Method("collapsible")),
        p("snaps", RustApi::Method("snaps")),
        p("autoSaveId", RustApi::Method("auto_save_id")),
        p(
            "defaultIsCollapsed",
            RustApi::Method("default_is_collapsed"),
        ),
        p("isCollapsed", RustApi::Method("is_collapsed")),
        p("onCollapseChange", RustApi::Method("on_collapse_change")),
    ],
    example: include_str!("examples.rs"),
};
