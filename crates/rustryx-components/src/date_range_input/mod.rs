//! DateRangeInput. Upstream: `DateRangeInput/` (component spec
//! `DateRangeInput.spec.md`; system specs AST-020, AST-085, AST-111).

#[allow(clippy::module_inception)]
mod date_range_input;
pub mod examples;

#[cfg(test)]
mod tests;

pub use date_range_input::{DateRangeInput, DateRangePreset};
pub use rustryx_i18n::date::DateRange;

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const DATE_RANGE_INPUT_DOC: ComponentDoc = ComponentDoc {
    name: "DateRangeInput",
    rust_type: "rustryx::components::date_range_input::DateRangeInput",
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
        p("maxRangeSpan", RustApi::Method("max_range_span")),
        p("minRangeSpan", RustApi::Method("min_range_span")),
        p("presets", RustApi::Method("presets")),
        p("hasClear", RustApi::Method("clear")),
        p("placeholder", RustApi::Method("placeholder")),
        p("size", RustApi::Method("size")),
        p("status", RustApi::Method("status")),
        p("statusVariant", RustApi::Method("status_placement")),
        p("labelTooltip", RustApi::Method("label_tooltip")),
        p("numberOfMonths", RustApi::Method("number_of_months")),
        p("weekStartsOn", RustApi::Method("week_starts_on")),
        p("calendar", RustApi::Method("calendar")),
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
