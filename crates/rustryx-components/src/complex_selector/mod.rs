//! ComplexSelector (M6). Upstream: `ComplexSelector/`.
//!
//! - A combobox trigger (input, secondary or ghost) whose popup is a dialog
//!   holding the caller's content: a RadioList, a calendar, a tree, a staged
//!   editor. The content gets the value, a commit helper (`on_change`, then
//!   `change_action`), a close helper and the shell's state (FR1).
//! - Click toggles, ArrowDown opens, the content or a
//!   `ComplexSelectorController` closes; every route reports through
//!   `on_open_change` (FR8). Focus enters the content on open and comes back
//!   to the trigger only when it would be lost (a caller's control always).
//! - Busy (`loading` or a running action) shows a spinner once announced;
//!   status, disabled and its reason follow the other fields (FR5-FR7, FR13).

mod component;
pub mod examples;

#[cfg(test)]
mod tests;

pub use component::{
    ComplexCommit, ComplexSelector, ComplexSelectorController, ComplexSelectorState, ComplexTrigger,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const COMPLEX_SELECTOR_DOC: ComponentDoc = ComponentDoc {
    name: "ComplexSelector",
    rust_type: "rustryx::components::complex_selector::ComplexSelector",
    props: &[
        p("label", RustApi::Argument("label")),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("changeAction", RustApi::Method("change_action")),
        p("children", RustApi::Method("content")),
        p("triggerLabel", RustApi::Method("trigger_label")),
        p("placeholder", RustApi::Method("placeholder")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("labelTooltip", RustApi::Method("label_tooltip")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("isDisabled", RustApi::Method("disabled")),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("isLoading", RustApi::Method("loading")),
        p("status", RustApi::Method("status")),
        p("statusVariant", RustApi::Method("status_placement")),
        p("size", RustApi::Method("size")),
        p("variant", RustApi::Method("variant")),
        p("startIcon", RustApi::Method("start_icon")),
        p("width", RustApi::Method("width")),
        p("placement", RustApi::Method("placement")),
        p("alignment", RustApi::Method("alignment")),
        p("renderTrigger", RustApi::Method("render_trigger")),
        p(
            "handleRef",
            RustApi::Method("controller (a ComplexSelectorController)"),
        ),
        p("onOpenChange", RustApi::Method("on_open_change")),
        p("contentXstyle", RustApi::Method("content_style")),
        p("onClick", RustApi::Method("on_click")),
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
