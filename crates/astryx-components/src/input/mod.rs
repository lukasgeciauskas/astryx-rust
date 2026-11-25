//! Text entry: TextInput, with TextArea, NumberInput, InputGroup,
//! FormLayout and FileInput to follow (M4). The editing itself is
//! `astryx_primitives::editing`; this module is the Astryx chrome around it.

mod control;
pub mod examples;
pub(crate) mod file_input;
pub(crate) mod group;
pub(crate) mod number_input;
pub(crate) mod text_area;
mod text_input;

#[cfg(test)]
mod tests;

pub use file_input::{FileInput, FileInputMode, PickedFile, mime_for_extension};
pub use group::{InputGroup, InputGroupText};
pub use number_input::{EntryErrorKind, NumberInput, NumberInputEntryError};
pub use text_area::TextArea;
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

pub const TEXT_AREA_DOC: ComponentDoc = ComponentDoc {
    name: "TextArea",
    rust_type: "astryx::components::input::TextArea",
    props: &[
        p("label", RustApi::Argument("label")),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("changeAction", RustApi::Method("change_action")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
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
        p("rows", RustApi::Method("rows")),
        p("maxLength", RustApi::Method("max_length")),
        p("htmlMaxLength", RustApi::Method("html_max_length")),
        p("status", RustApi::Method("status")),
        p(
            "statusVariant",
            RustApi::Method("status_placement (tooltip lands with Tooltip in M5)"),
        ),
        p(
            "labelTooltip",
            RustApi::Pending("label_tooltip", "the InfoTip lands with Tooltip in M5"),
        ),
        p("startIcon", RustApi::Method("start_icon")),
        p(
            "hasSpellCheck",
            RustApi::Waived("GPUI has no spell checker to turn on or off"),
        ),
        p("hasAutoFocus", RustApi::Method("auto_focus")),
        p("size", RustApi::Method("size")),
        p("onPaste", RustApi::Method("on_paste")),
        p("htmlName", RustApi::Waived(NO_FORMS)),
        p("width", RustApi::Method("width")),
        p("onFocus", RustApi::Method("on_focus")),
        p("onBlur", RustApi::Method("on_blur")),
        p(
            "autoComplete",
            RustApi::Waived("browser autofill has no GPUI counterpart"),
        ),
        p("enterKeyHint", RustApi::Waived(NO_TOUCH_KEYBOARD)),
        p("inputMode", RustApi::Waived(NO_TOUCH_KEYBOARD)),
        p("autoCapitalize", RustApi::Waived(NO_TOUCH_KEYBOARD)),
        p("hasAutoCorrect", RustApi::Waived(NO_TOUCH_KEYBOARD)),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("containerXstyle", RustApi::Trait("Styled")),
        p("containerClassName", RustApi::Trait("Styled")),
        p("containerStyle", RustApi::Trait("Styled")),
        p("controlXstyle", RustApi::Method("control_style")),
        p("controlClassName", RustApi::Method("control_style")),
        p("controlStyle", RustApi::Method("control_style")),
    ],
    example: include_str!("examples.rs"),
};

pub const NUMBER_INPUT_DOC: ComponentDoc = ComponentDoc {
    name: "NumberInput",
    rust_type: "astryx::components::input::NumberInput",
    props: &[
        p("label", RustApi::Argument("label")),
        p("value", RustApi::Argument("value")),
        p("onChange", RustApi::Method("on_change")),
        p("size", RustApi::Method("size")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("isOptional", RustApi::Method("optional")),
        p("onKeyDown", RustApi::Method("on_key_down")),
        p("isRequired", RustApi::Method("required")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isReadOnly", RustApi::Method("read_only")),
        p(
            "disabledMessage",
            RustApi::Pending(
                "disabled_message",
                "describes the field now; the tooltip lands with Tooltip in M5",
            ),
        ),
        p("placeholder", RustApi::Method("placeholder")),
        p(
            "labelTooltip",
            RustApi::Pending("label_tooltip", "the InfoTip lands with Tooltip in M5"),
        ),
        p("startIcon", RustApi::Method("start_icon")),
        p("labelIcon", RustApi::Method("label_icon")),
        p("status", RustApi::Method("status")),
        p(
            "onEntryErrorChange",
            RustApi::Method("on_entry_error_change"),
        ),
        p(
            "entryErrorBehavior",
            RustApi::Waived(
                "a typed number is complete only at commit, so both behaviors are the same (FR14)",
            ),
        ),
        p(
            "statusVariant",
            RustApi::Method("status_placement (tooltip lands with Tooltip in M5)"),
        ),
        p("min", RustApi::Method("min")),
        p("max", RustApi::Method("max")),
        p("step", RustApi::Method("step")),
        p("formatValue", RustApi::Method("format_value")),
        p("isWheelEnabled", RustApi::Method("wheel_enabled")),
        p("hasNumberSteppers", RustApi::Method("number_steppers")),
        p("units", RustApi::Method("units")),
        p("isIntegerOnly", RustApi::Method("integer_only")),
        p("hasClear", RustApi::Method("clear")),
        p("htmlName", RustApi::Waived(NO_FORMS)),
        p(
            "autoComplete",
            RustApi::Waived("browser autofill has no GPUI counterpart"),
        ),
        p("width", RustApi::Method("width")),
        p("hasAutoFocus", RustApi::Method("auto_focus")),
        p("onFocus", RustApi::Method("on_focus")),
        p("onBlur", RustApi::Method("on_blur")),
        p("onEnter", RustApi::Method("on_enter")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("containerXstyle", RustApi::Trait("Styled")),
        p("containerClassName", RustApi::Trait("Styled")),
        p("containerStyle", RustApi::Trait("Styled")),
        p("controlXstyle", RustApi::Method("control_style")),
        p("controlClassName", RustApi::Method("control_style")),
        p("controlStyle", RustApi::Method("control_style")),
    ],
    example: include_str!("examples.rs"),
};

pub const INPUT_GROUP_DOC: ComponentDoc = ComponentDoc {
    name: "InputGroup",
    rust_type: "astryx::components::input::InputGroup",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("label", RustApi::Argument("label")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("isDisabled", RustApi::Method("disabled")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("size", RustApi::Method("size")),
        p("status", RustApi::Method("status")),
        p(
            "labelTooltip",
            RustApi::Pending("label_tooltip", "the InfoTip lands with Tooltip in M5"),
        ),
        p("xstyle", RustApi::Trait("Styled")),
        p(
            "data-testid",
            RustApi::Waived("GPUI tests find elements by id"),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const INPUT_GROUP_TEXT_DOC: ComponentDoc = ComponentDoc {
    name: "InputGroupText",
    rust_type: "astryx::components::input::InputGroupText",
    props: &[
        p("children", RustApi::Argument("content")),
        p("xstyle", RustApi::Trait("Styled")),
        p("className", RustApi::Trait("Styled")),
        p("style", RustApi::Trait("Styled")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
    ],
    example: include_str!("examples.rs"),
};

pub const FILE_INPUT_DOC: ComponentDoc = ComponentDoc {
    name: "FileInput",
    rust_type: "astryx::components::input::FileInput",
    props: &[
        p("label", RustApi::Argument("label")),
        p(
            "value",
            RustApi::Argument("value (a Vec<PickedFile>, empty for none)"),
        ),
        p("onChange", RustApi::Method("on_change")),
        p("changeAction", RustApi::Method("change_action")),
        p("accept", RustApi::Method("accept")),
        p("isMultiple", RustApi::Method("multiple")),
        p("maxSize", RustApi::Method("max_size")),
        p("maxFiles", RustApi::Method("max_files")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("description", RustApi::Method("description")),
        p("isOptional", RustApi::Method("optional")),
        p("isRequired", RustApi::Method("required")),
        p("isDisabled", RustApi::Method("disabled")),
        p(
            "disabledMessage",
            RustApi::Pending(
                "disabled_message",
                "describes the field now; the tooltip lands with Tooltip in M5",
            ),
        ),
        p("isLoading", RustApi::Method("loading")),
        p("placeholder", RustApi::Method("placeholder")),
        p("mode", RustApi::Method("mode")),
        p("status", RustApi::Method("status")),
        p(
            "statusVariant",
            RustApi::Method("status_placement (tooltip lands with Tooltip in M5)"),
        ),
        p(
            "labelTooltip",
            RustApi::Pending("label_tooltip", "the InfoTip lands with Tooltip in M5"),
        ),
        p("width", RustApi::Method("width")),
        p("ref", RustApi::Waived("GPUI elements have no refs")),
        p("containerXstyle", RustApi::Trait("Styled")),
        p("containerClassName", RustApi::Trait("Styled")),
        p("containerStyle", RustApi::Trait("Styled")),
        p("controlXstyle", RustApi::Method("control_style")),
        p("controlClassName", RustApi::Method("control_style")),
        p("controlStyle", RustApi::Method("control_style")),
    ],
    example: include_str!("examples.rs"),
};
