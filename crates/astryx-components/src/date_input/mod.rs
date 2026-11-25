//! DateInput and the presentation vocabulary of the date and time inputs.
//! Upstream: `DateInput/` (component spec `DateInput.spec.md`; system specs
//! AST-043, AST-111, AST-117, AST-118).

#[allow(clippy::module_inception)]
mod date_input;
pub mod examples;
pub(crate) mod presentation;
pub(crate) mod segment;
pub(crate) mod typed;

#[cfg(test)]
mod tests;

pub use date_input::DateInput;
pub use presentation::{InputPresentation, LegacyNativePicker, PickerPresentation};
pub use typed::{DateEntryErrorKind, DateInputEntryError, DateInputFormat};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const LATER_SURFACES: &str =
    "the bottom sheet and the platform picker; the pointer field shows meanwhile";

pub const DATE_INPUT_DOC: ComponentDoc = ComponentDoc {
    name: "DateInput",
    rust_type: "astryx::components::date_input::DateInput",
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
        p("dateConstraints", RustApi::Method("date_constraints")),
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
        p("hasClear", RustApi::Method("clear")),
        p("numberOfMonths", RustApi::Method("number_of_months")),
        p("weekStartsOn", RustApi::Method("week_starts_on")),
        p("format", RustApi::Method("format")),
        p("calendar", RustApi::Method("calendar")),
        p(
            "presentation",
            RustApi::Pending("presentation", LATER_SURFACES),
        ),
        p(
            "nativePicker",
            RustApi::Pending("native_picker", LATER_SURFACES),
        ),
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
