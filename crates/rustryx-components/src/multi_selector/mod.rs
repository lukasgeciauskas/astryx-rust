//! MultiSelector (M6). Upstream: `MultiSelector/`.
//!
//! - A trigger (input, secondary or ghost) showing the selection as a
//!   count, labels or badges, over a listbox of checkbox rows that stays
//!   open as options toggle.
//! - Optional select-all row, search with counted announcements, and a
//!   Create row for unmatched queries (FR9).
//! - Keys (FR12): ArrowDown opens; arrows move; Home and End jump (PageUp and
//!   PageDown with search); Enter and Space toggle; Escape and Tab close;
//!   Delete clears a closed, clearable trigger. Opening never lands on a
//!   disabled option (FR18).
//! - Each change is announced (FR14). A `MultiSelectorController` opens,
//!   closes and toggles it, and `render_trigger` hangs it off any control
//!   (FR8).

mod component;
pub mod examples;

#[cfg(test)]
mod tests;

pub use component::{
    MultiSelector, MultiSelectorChange, MultiSelectorController, MultiTrigger, TriggerDisplay,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const MULTI_SELECTOR_DOC: ComponentDoc = ComponentDoc {
    name: "MultiSelector",
    rust_type: "rustryx::components::multi_selector::MultiSelector",
    props: &[
        p("label", RustApi::Argument("label")),
        p(
            "options",
            RustApi::Argument("options (SelectorEntry: options, dividers, sections)"),
        ),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("changeAction", RustApi::Method("change_action")),
        p("placeholder", RustApi::Method("placeholder")),
        p("size", RustApi::Method("size")),
        p("variant", RustApi::Method("variant")),
        p("triggerDisplay", RustApi::Method("trigger_display")),
        p("formatValue", RustApi::Method("format_value")),
        p("maxBadges", RustApi::Method("max_badges")),
        p("hasSelectAll", RustApi::Method("select_all")),
        p("selectAllLabel", RustApi::Method("select_all_label")),
        p("hasSearch", RustApi::Method("search")),
        p("searchPlaceholder", RustApi::Method("search_placeholder")),
        p("emptyText", RustApi::Method("empty_text")),
        p("emptySearchText", RustApi::Method("empty_search_text")),
        p("hasCreate", RustApi::Method("create")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isReadOnly", RustApi::Method("read_only")),
        p(
            "htmlName",
            RustApi::Waived("GPUI apps have no HTML forms; read the value from your state"),
        ),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("isLoading", RustApi::Method("loading")),
        p("status", RustApi::Method("status")),
        p("statusVariant", RustApi::Method("status_placement")),
        p("renderOption", RustApi::Method("render_option")),
        p("indicatorPosition", RustApi::Method("indicator_position")),
        p("presentation", RustApi::Method("presentation")),
        p("width", RustApi::Method("width")),
        p("startIcon", RustApi::Method("start_icon")),
        p("hasClear", RustApi::Method("clear")),
        p("isDefaultOpen", RustApi::Method("default_open")),
        p("renderTrigger", RustApi::Method("render_trigger")),
        p(
            "handleRef",
            RustApi::Method("controller (a MultiSelectorController)"),
        ),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("onClick", RustApi::Method("on_click")),
        p("onFocus", RustApi::Method("on_focus")),
        p("onKeyDown", RustApi::Method("on_key_down")),
        p("containerXstyle", RustApi::Trait("Styled")),
        p("containerClassName", RustApi::Trait("Styled")),
        p("containerStyle", RustApi::Trait("Styled")),
        p("controlXstyle", RustApi::Method("control_style")),
        p("controlClassName", RustApi::Method("control_style")),
        p("controlStyle", RustApi::Method("control_style")),
    ],
    example: include_str!("examples.rs"),
};
