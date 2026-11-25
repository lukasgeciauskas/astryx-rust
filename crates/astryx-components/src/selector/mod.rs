//! Selector and SelectorOption (M6). Upstream: `Selector/`.
//!
//! - [`Selector`]: one value from a list of options, sections and
//!   dividers, behind a form-field or toolbar trigger.
//! - [`SelectorOption`]: the content row a Selector draws for an option,
//!   for custom `render_option` and `render_value` content.
//! - The combobox keyboard model (`combobox`) is shared with MultiSelector.

pub(crate) mod combobox;
pub mod examples;
mod option;
pub(crate) mod options;
pub(crate) mod search_row;
#[allow(clippy::module_inception)]
mod selector;

#[cfg(test)]
mod tests;

pub use option::SelectorOption;
pub use options::{OptionActionFn, SelectorEntry, SelectorOptionData, selectable_options};
pub(crate) use selector::trigger_line;
pub use selector::{Selector, SelectorIndicatorPosition, SelectorVariant};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const NO_FORMS: &str = "GPUI apps have no HTML forms; read the value from your state";

pub const SELECTOR_DOC: ComponentDoc = ComponentDoc {
    name: "Selector",
    rust_type: "astryx::components::selector::Selector",
    props: &[
        p("label", RustApi::Argument("label")),
        p(
            "options",
            RustApi::Argument(
                "options (strings, SelectorOptionData, SelectorEntry::divider/section)",
            ),
        ),
        p("value", RustApi::Method("value")),
        p("onChange", RustApi::Method("on_change")),
        p("changeAction", RustApi::Method("change_action")),
        p("hasClear", RustApi::Method("clear")),
        p("hasSearch", RustApi::Method("search")),
        p("searchPlaceholder", RustApi::Method("search_placeholder")),
        p("emptyText", RustApi::Method("empty_text")),
        p("emptySearchText", RustApi::Method("empty_search_text")),
        p("placeholder", RustApi::Method("placeholder")),
        p("size", RustApi::Method("size")),
        p("variant", RustApi::Method("variant")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isReadOnly", RustApi::Method("read_only")),
        p("htmlName", RustApi::Waived(NO_FORMS)),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("labelTooltip", RustApi::Method("label_tooltip")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("status", RustApi::Method("status")),
        p("statusVariant", RustApi::Method("status_placement")),
        p("renderOption", RustApi::Method("render_option")),
        p("renderValue", RustApi::Method("render_value")),
        p("indicatorPosition", RustApi::Method("indicator_position")),
        p("presentation", RustApi::Method("presentation")),
        p("placement", RustApi::Method("placement")),
        p("isDefaultOpen", RustApi::Method("default_open")),
        p("width", RustApi::Method("width")),
        p("startIcon", RustApi::Method("start_icon")),
        p("isLoading", RustApi::Method("loading")),
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

pub const SELECTOR_OPTION_DOC: ComponentDoc = ComponentDoc {
    name: "SelectorOption",
    rust_type: "astryx::components::selector::SelectorOption",
    props: &[
        p("icon", RustApi::Method("icon (or icon_element)")),
        p(
            "label",
            RustApi::Argument("label (or SelectorOption::new_element)"),
        ),
        p("description", RustApi::Method("description")),
        p("layout", RustApi::Method("layout")),
        p("endContent", RustApi::Method("end_content")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
