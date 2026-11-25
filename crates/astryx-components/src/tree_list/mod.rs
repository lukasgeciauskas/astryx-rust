//! TreeList (M6). Upstream: `TreeList/`.
//!
//! - Data-driven: a list of [`TreeListItemData`] with nested children,
//!   expanded from `expanded` and then by the person (FR1).
//! - Rows: a chevron for branches (or the theme's `tree-disclosure`
//!   indicator, FR10), start content, the label and description, end
//!   content, and guide lines (`TreeListVariant::LineGuides`).
//! - A press outside a row's controls runs its action: its `on_click`, else
//!   a branch toggles, else its link opens (FR7).
//! - The WAI-ARIA tree keys (`astryx_primitives::tree_focus`) with one tab
//!   stop, typeahead on labels, and focus kept when a branch collapses
//!   around it (AR1, AR2).

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
    rust_type: "astryx::components::tree_list::TreeList",
    props: &[
        p("items", RustApi::Argument("items (TreeListItemData)")),
        p("density", RustApi::Method("density")),
        p("variant", RustApi::Method("variant")),
        p(
            "animatedLayout",
            RustApi::Waived(
                "MOTION: the expand glide and animatedLayout handles come with the motion layer (M11), as List's do",
            ),
        ),
        p("header", RustApi::Method("header")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
