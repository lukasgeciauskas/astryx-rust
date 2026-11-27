//! Upstream lab `TransferList`: `TransferListSelector`, the selector field, and
//! `TransferList`, the dual-panel content it hosts.

mod list;
pub(crate) mod responsive;
mod selector;

pub mod examples;
#[cfg(test)]
mod tests;

use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};

pub use list::*;
pub use selector::*;

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `TransferListSelector` (its doc's props, in its order).
pub const TRANSFER_LIST_SELECTOR_DOC: ComponentDoc = ComponentDoc {
    name: "TransferListSelector",
    rust_type: "rustryx::lab::transfer_list::TransferListSelector",
    props: &[
        p("label", RustApi::Argument("label")),
        p("options", RustApi::Argument("options (TransferListOption)")),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("commitBehavior", RustApi::Method("commit_behavior")),
        p("triggerLabel", RustApi::Method("trigger_label")),
        p("description", RustApi::Method("description")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("selectedLabel", RustApi::Method("selected_label")),
        p("availableLabel", RustApi::Method("available_label")),
        p("hasSearch", RustApi::Method("search")),
        p("searchLabel", RustApi::Method("search_label")),
        p("searchPlaceholder", RustApi::Method("search_placeholder")),
        p("reorderable", RustApi::Method("reorderable")),
        p("hasSelectAll", RustApi::Method("select_all")),
        p("hasClear", RustApi::Method("clear")),
        p("renderOption", RustApi::Method("render_option")),
        p("selectedEmptyText", RustApi::Method("selected_empty_text")),
        p(
            "availableEmptyText",
            RustApi::Method("available_empty_text"),
        ),
        p("noResultsText", RustApi::Method("no_results_text")),
        p("applyLabel", RustApi::Method("apply_label")),
        p("cancelLabel", RustApi::Method("cancel_label")),
        p("size", RustApi::Method("size")),
        p("width", RustApi::Method("width")),
        p("placement", RustApi::Method("placement")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isLoading", RustApi::Method("loading")),
        p("status", RustApi::Method("status")),
        p("statusVariant", RustApi::Method("status_placement")),
        p("labelTooltip", RustApi::Method("label_tooltip")),
        p("changeAction", RustApi::Method("change_action")),
        p("controlXstyle", RustApi::Method("control_style")),
        p(
            "controlClassName",
            RustApi::Waived("GPUI has no class names: control_style"),
        ),
        p("controlStyle", RustApi::Method("control_style")),
        p(
            "data-testid",
            RustApi::Waived("a DOM test hook; GPUI tests use debug selectors"),
        ),
    ],
    example: include_str!("examples.rs"),
};

/// Upstream lab `TransferList` (the doc's `components` entry).
pub const TRANSFER_LIST_DOC: ComponentDoc = ComponentDoc {
    name: "TransferList",
    rust_type: "rustryx::lab::transfer_list::TransferList",
    props: &[
        p("label", RustApi::Argument("label")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("options", RustApi::Argument("options (TransferListOption)")),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("selectedLabel", RustApi::Method("selected_label")),
        p("availableLabel", RustApi::Method("available_label")),
        p("hasSearch", RustApi::Method("search")),
        p("isDisabled", RustApi::Method("disabled")),
        p("searchLabel", RustApi::Method("search_label")),
        p("searchPlaceholder", RustApi::Method("search_placeholder")),
        p("reorderable", RustApi::Method("reorderable")),
        p("hasSelectAll", RustApi::Method("select_all")),
        p("hasClear", RustApi::Method("clear")),
        p("renderOption", RustApi::Method("render_option")),
        p("selectedEmptyText", RustApi::Method("selected_empty_text")),
        p(
            "availableEmptyText",
            RustApi::Method("available_empty_text"),
        ),
        p("noResultsText", RustApi::Method("no_results_text")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
