//! Text entry: TextInput, with TextArea, NumberInput, InputGroup,
//! FormLayout and FileInput to follow (M4). The editing itself is
//! `astryx_primitives::editing`; this module is the Astryx chrome around it.

mod control;
pub mod examples;
mod text_input;

#[cfg(test)]
mod tests;

pub use text_input::{InputType, TextInput};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

const NO_FORMS: &str = "GPUI apps have no HTML forms; read the value from your state";
const NO_TOUCH_KEYBOARD: &str =
    "a touch keyboard hint; GPUI's platforms have no touch keyboard to configure";

pub const TEXT_INPUT_DOC: ComponentDoc = ComponentDoc {
    name: "TextInput",
    rust_type: "astryx::components::input::TextInput",
    props: &[
        p("type", RustApi::Method("input_type")),
        p("label", RustApi::Argument("label")),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("changeAction", RustApi::Method("change_action")),
        p("size", RustApi::Method("size")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("onEnter", RustApi::Method("on_enter")),
        p("onKeyDown", RustApi::Method("on_key_down")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isReadOnly", RustApi::Method("read_only")),
        p(
            "disabledMessage",
            RustApi::Pending(
                "disabled_message",
                "describes the field now; the tooltip lands with Tooltip in M5",
            ),
        ),
        p("isLoading", RustApi::Method("loading")),
        p("placeholder", RustApi::Method("placeholder")),
        p(
            "labelTooltip",
            RustApi::Pending("label_tooltip", "the InfoTip lands with Tooltip in M5"),
        ),
        p("startIcon", RustApi::Method("start_icon")),
        p("status", RustApi::Method("status")),
        p(
            "statusVariant",
            RustApi::Method("status_placement (tooltip lands with Tooltip in M5)"),
        ),
        p("hasClear", RustApi::Method("clear")),
        p("hasAutoFocus", RustApi::Method("auto_focus")),
        p("htmlName", RustApi::Waived(NO_FORMS)),
        p("width", RustApi::Method("width")),
        p(
            "autoComplete",
            RustApi::Waived("browser autofill has no GPUI counterpart"),
        ),
        p("enterKeyHint", RustApi::Waived(NO_TOUCH_KEYBOARD)),
        p("inputMode", RustApi::Waived(NO_TOUCH_KEYBOARD)),
        p("autoCapitalize", RustApi::Waived(NO_TOUCH_KEYBOARD)),
        p("hasAutoCorrect", RustApi::Waived(NO_TOUCH_KEYBOARD)),
        p(
            "hasSpellCheck",
            RustApi::Waived("GPUI has no spell checker to turn on or off"),
        ),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("containerXstyle", RustApi::Trait("Styled")),
        p("containerClassName", RustApi::Trait("Styled")),
        p("containerStyle", RustApi::Trait("Styled")),
        p("controlXstyle", RustApi::Method("control_style")),
        p("controlClassName", RustApi::Method("control_style")),
        p("controlStyle", RustApi::Method("control_style")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
