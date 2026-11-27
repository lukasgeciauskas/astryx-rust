//! TreeList (M6). Upstream: `TreeList/`.
//!
//! - Data-driven: a list of [`TreeListItemData`] with nested children,
//!   expanded from `expanded` and then by the person (FR1).
//! - Rows: a chevron for branches (or the theme's `tree-disclosure`
//!   indicator, FR10), start content, the label and description, end
//!   content, and guide lines (`TreeListVariant::LineGuides`).
//! - A press outside a row's controls runs its action: its `on_click`, else
//!   a branch toggles, else its link opens (FR7).
//! - The WAI-ARIA tree keys (`rustryx_primitives::tree_focus`) with one tab
//!   stop, typeahead on labels, and focus kept when a branch collapses
//!   around it (AR1, AR2).
//! - Motion (FR8, FR9, AST-105): with an `animated_layout` handle a branch's
//!   children reveal and conceal from their block-end edge while the rows and
//!   content after it glide, and the chevron turns on that motion; a collapsing
//!   group stays drawn, inert and out of the keys, until it is gone. Without a
//!   handle a tree under a coarse pointer plays the same motion inside itself
//!   (content after it takes its place at once), and under a mouse a group
//!   appears and leaves at once.

mod component;
pub mod examples;

#[cfg(test)]
mod tests;

pub use component::{TreeList, TreeListDensity, TreeListItemData, TreeListVariant};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const TREE_LIST_DOC: ComponentDoc = ComponentDoc {
    name: "TreeList",
    rust_type: "rustryx::components::tree_list::TreeList",
    props: &[
        p("items", RustApi::Argument("items (TreeListItemData)")),
        p("density", RustApi::Method("density")),
        p("variant", RustApi::Method("variant")),
        p("animatedLayout", RustApi::Method("animated_layout")),
        p("header", RustApi::Method("header")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
