//! List and ListItem (M6). Upstream: `List/`.

pub mod examples;
mod item;
#[allow(clippy::module_inception)]
mod list;

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

const MOTION: &str = "not yet: animated row changes land with the motion layer (M11)";
const REORDER: &str = "not yet: reordering lands later in M6, with TreeList";
const SWIPE: &str = "not yet: swipe actions land with the touch layer (M11)";

pub const LIST_DOC: ComponentDoc = ComponentDoc {
    name: "List",
    rust_type: "astryx::components::list::List",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("density", RustApi::Method("density")),
        p("hasDividers", RustApi::Method("dividers")),
        p("animatedLayout", RustApi::Waived(MOTION)),
        p("reorderable", RustApi::Waived(REORDER)),
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
    rust_type: "astryx::components::list::ListItem",
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
        p("swipe", RustApi::Waived(SWIPE)),
        p("swipeActions", RustApi::Waived(SWIPE)),
        p("swipeBehavior", RustApi::Waived(SWIPE)),
    ],
    example: include_str!("examples.rs"),
};
