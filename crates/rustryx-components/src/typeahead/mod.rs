//! Typeahead, BaseTypeahead and TypeaheadItem (M6). Upstream: `Typeahead/`.

mod base;
pub(crate) mod engine;
pub mod examples;
mod field;
mod item;

#[cfg(test)]
mod tests;

pub use base::{
    BaseTypeahead, DialogMenuInfo, ListboxMenuInfo, TypeaheadController, TypeaheadMenu,
};
pub use field::Typeahead;
pub use item::TypeaheadItem;

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const SOURCE: &str =
    "source (Typeahead::new, or from_rc; source_key names a source built each render)";

pub const TYPEAHEAD_DOC: ComponentDoc = ComponentDoc {
    name: "Typeahead",
    rust_type: "rustryx::components::typeahead::Typeahead",
    props: &[
        p("label", RustApi::Argument("label")),
        p("searchSource", RustApi::Argument(SOURCE)),
        p("value", RustApi::Method("value")),
        p("onChange", RustApi::Method("on_change")),
        p("placeholder", RustApi::Method("placeholder")),
        p("hasEntriesOnFocus", RustApi::Method("entries_on_focus")),
        p("hasClear", RustApi::Method("clear")),
        p("isDisabled", RustApi::Method("disabled")),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("maxMenuItems", RustApi::Method("max_menu_items")),
        p("minQueryLength", RustApi::Method("min_query_length")),
        p("status", RustApi::Method("status")),
        p("statusVariant", RustApi::Method("status_placement")),
        p("renderItem", RustApi::Method("render_item")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("isRequired", RustApi::Method("required")),
        p("isOptional", RustApi::Method("optional")),
        p("labelTooltip", RustApi::Method("label_tooltip")),
        p("emptySearchText", RustApi::Method("empty_search_text")),
        p("hasAutoFocus", RustApi::Method("auto_focus")),
        p("size", RustApi::Method("size")),
        p("debounceMs", RustApi::Method("debounce")),
        p("onChangeQuery", RustApi::Method("on_change_query")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("menu", RustApi::Method("menu")),
        p(
            "handleRef",
            RustApi::Method("controller (a TypeaheadController)"),
        ),
        p("width", RustApi::Method("width")),
        p("startIcon", RustApi::Method("start_icon")),
        p("containerXstyle", RustApi::Trait("Styled")),
        p("containerClassName", RustApi::Trait("Styled")),
        p("containerStyle", RustApi::Trait("Styled")),
        p("controlXstyle", RustApi::Method("control_style")),
        p("controlClassName", RustApi::Method("control_style")),
        p("controlStyle", RustApi::Method("control_style")),
    ],
    example: include_str!("examples.rs"),
};

pub const BASE_TYPEAHEAD_DOC: ComponentDoc = ComponentDoc {
    name: "BaseTypeahead",
    rust_type: "rustryx::components::typeahead::BaseTypeahead",
    props: &[
        p("searchSource", RustApi::Argument(SOURCE)),
        p("value", RustApi::Method("value")),
        p("onChange", RustApi::Method("on_change")),
        p("renderItem", RustApi::Method("render_item")),
        p("placeholder", RustApi::Method("placeholder")),
        p("hasEntriesOnFocus", RustApi::Method("entries_on_focus")),
        p("maxMenuItems", RustApi::Method("max_menu_items")),
        p("menuWidth", RustApi::Method("menu_width")),
        p("minQueryLength", RustApi::Method("min_query_length")),
        p("emptySearchText", RustApi::Method("empty_search_text")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isFocusableDisabled", RustApi::Method("focusable_disabled")),
        p("hasAutoFocus", RustApi::Method("auto_focus")),
        p("debounceMs", RustApi::Method("debounce")),
        p("size", RustApi::Method("size")),
        p(
            "anchorRef",
            RustApi::Method("chrome (the list anchors to the chrome it returns)"),
        ),
        p("inputXStyle", RustApi::Trait("Styled")),
        p("xstyle", RustApi::Trait("Styled")),
        p(
            "inputTabIndex",
            RustApi::Waived("GPUI's tab order follows paint order"),
        ),
        p("onKeyDown", RustApi::Method("on_key_down")),
        p("onChangeQuery", RustApi::Method("on_change_query")),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("menu", RustApi::Method("menu")),
        p(
            "handleRef",
            RustApi::Method("controller (a TypeaheadController)"),
        ),
        p(
            "inputId",
            RustApi::Waived("GPUI ties names to elements through the accessibility tree, not ids"),
        ),
        p("ariaDescribedBy", RustApi::Method("description")),
        p("ariaLabelledBy", RustApi::Method("label")),
    ],
    example: include_str!("examples.rs"),
};

pub const TYPEAHEAD_ITEM_DOC: ComponentDoc = ComponentDoc {
    name: "TypeaheadItem",
    rust_type: "rustryx::components::typeahead::TypeaheadItem",
    props: &[
        p(
            "item",
            RustApi::Argument("label (TypeaheadItem::new, or for_item; element for item.element)"),
        ),
        p("icon", RustApi::Method("icon")),
        p("description", RustApi::Method("description")),
        p("isDisabled", RustApi::Method("disabled")),
        p(
            "group",
            RustApi::Waived("no effect upstream either: group results with Searchable::group"),
        ),
    ],
    example: include_str!("examples.rs"),
};
