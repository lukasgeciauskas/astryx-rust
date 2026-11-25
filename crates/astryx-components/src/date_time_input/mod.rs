//! DateTimeInput. Upstream: `DateTimeInput/` (component spec
//! `DateTimeInput.spec.md`; system specs AST-043, AST-111, AST-117, AST-118).

#[allow(clippy::module_inception)]
mod date_time_input;
pub mod examples;

#[cfg(test)]
mod tests;

pub use date_time_input::{DateTimeBound, DateTimeInput, TimeOptionInterval};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const LATER_SURFACES: &str =
    "the bottom sheet and the platform pickers; the pointer field shows meanwhile";

pub const DATE_TIME_INPUT_DOC: ComponentDoc = ComponentDoc {
    name: "DateTimeInput",
    rust_type: "astryx::components::date_time_input::DateTimeInput",
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
        p("hasSeconds", RustApi::Method("seconds")),
        p("hourFormat", RustApi::Method("hour_format")),
        p("timeIncrement", RustApi::Method("time_increment")),
        p(
            "timeOptionInterval",
            RustApi::Method("time_option_interval"),
        ),
        p("hasClear", RustApi::Method("clear")),
        p("placeholder", RustApi::Method("placeholder")),
        p("calendar", RustApi::Method("calendar")),
        p("timePlaceholder", RustApi::Method("time_placeholder")),
        p("timeLabel", RustApi::Method("time_label")),
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
        p("labelTooltip", RustApi::Method("label_tooltip")),
        p("numberOfMonths", RustApi::Method("number_of_months")),
        p("weekStartsOn", RustApi::Method("week_starts_on")),
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
