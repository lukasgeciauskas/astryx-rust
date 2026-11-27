//! List and ListItem (M6). Upstream: `List/`. With an `animated_layout` handle its row
//! updates animate (FR12, [`motion`]).

pub mod examples;
mod item;
#[allow(clippy::module_inception)]
mod list;
mod motion;

#[cfg(test)]
mod tests;

pub use item::ListItem;
pub use list::{List, ListEdge, ListStyle};
#[allow(unused_imports)]
pub(crate) use list::{ListMarker, ListTask, list_marker_scope};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const LIST_DOC: ComponentDoc = ComponentDoc {
    name: "List",
    rust_type: "rustryx::components::list::List",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("density", RustApi::Method("density")),
        p("hasDividers", RustApi::Method("dividers")),
        p("animatedLayout", RustApi::Method("animated_layout")),
        p("reorderable", RustApi::Method("reorderable")),
        p("edge", RustApi::Method("edge")),
        p(
            "edgeCompensation",
            RustApi::Method("edge_compensation (deprecated)"),
        ),
        p("header", RustApi::Method("header (and label, its name)")),
        p("listStyle", RustApi::Method("list_style")),
        p("start", RustApi::Method("start")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const LIST_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "ListItem",
    rust_type: "rustryx::components::list::ListItem",
    props: &[
        p(
            "label",
            RustApi::Argument("label (or ListItem::new_element)"),
        ),
        p(
            "description",
            RustApi::Method("description (or description_element)"),
        ),
        p("startContent", RustApi::Method("start_content")),
        p("endContent", RustApi::Method("end_content")),
        p("onClick", RustApi::Method("on_click")),
        p("interactiveRef", RustApi::Method("interactive")),
        p("href", RustApi::Method("href")),
        p("target", RustApi::Method("target")),
        p("rel", RustApi::Waived("GPUI opens URLs without a referrer")),
        p("align", RustApi::Method("align")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isSelected", RustApi::Method("selected")),
        p("itemKey", RustApi::Method("item_key")),
        p(
            "swipe",
            RustApi::Method("swipe (a create_swipe value; on_activate gets the item_key)"),
        ),
        p(
            "swipeActions",
            RustApi::Method("swipe_actions (deprecated: use swipe)"),
        ),
        p(
            "swipeBehavior",
            RustApi::Method("swipe_behavior (deprecated: CreateSwipeOptions::swipe_behavior)"),
        ),
    ],
    example: include_str!("examples.rs"),
};
