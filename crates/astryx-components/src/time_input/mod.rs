//! TimeInput. Upstream: `TimeInput/` (component spec `TimeInput.spec.md`;
//! system specs AST-043, AST-117, AST-118).

pub mod examples;
#[allow(clippy::module_inception)]
mod time_input;
pub(crate) mod typed;

#[cfg(test)]
mod tests;

pub use time_input::TimeInput;
pub use typed::{TimeEntryErrorKind, TimeInputEntryError};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const LATER_SURFACES: &str =
    "the bottom sheet and the platform picker; the typed field shows meanwhile";

pub const TIME_INPUT_DOC: ComponentDoc = ComponentDoc {
    name: "TimeInput",
    rust_type: "astryx::components::time_input::TimeInput",
    props: &[
        p("label", RustApi::Argument("label")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("isDisabled", RustApi::Method("disabled")),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("changeAction", RustApi::Method("change_action")),
        p("isLoading", RustApi::Method("loading")),
        p("min", RustApi::Method("min")),
        p("max", RustApi::Method("max")),
        p("hasSeconds", RustApi::Method("seconds")),
        p("hasClear", RustApi::Method("clear")),
        p("hourFormat", RustApi::Method("hour_format")),
        p("increment", RustApi::Method("increment")),
        p(
            "presentation",
            RustApi::Pending("presentation", LATER_SURFACES),
        ),
        p(
            "nativePicker",
            RustApi::Pending("native_picker", LATER_SURFACES),
        ),
        p("placeholder", RustApi::Method("placeholder")),
        p("size", RustApi::Method("size")),
        p("status", RustApi::Method("status")),
        p(
            "onEntryErrorChange",
            RustApi::Method("on_entry_error_change"),
        ),
        p(
            "entryErrorBehavior",
            RustApi::Method("entry_error_behavior"),
        ),
        p("statusVariant", RustApi::Method("status_placement")),
        p("labelTooltip", RustApi::Method("label_tooltip")),
        p("hasAutoFocus", RustApi::Method("auto_focus")),
        p("width", RustApi::Method("width")),
        p("containerXstyle", RustApi::Trait("Styled")),
        p("containerClassName", RustApi::Trait("Styled")),
        p("containerStyle", RustApi::Trait("Styled")),
        p("controlXstyle", RustApi::Method("control_style")),
        p("controlClassName", RustApi::Method("control_style")),
        p("controlStyle", RustApi::Method("control_style")),
    ],
    example: include_str!("examples.rs"),
};
