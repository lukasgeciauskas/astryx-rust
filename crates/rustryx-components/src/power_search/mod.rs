//! PowerSearch (M6), in progress. Upstream: `PowerSearch/`.
//!
//! A structured filter bar: each token is a filter (a field, an operator
//! and a value). Pick a field from the main rows, set its operator and
//! value in the editor popover; tokens open their editor and remove their
//! filter. On a compact touch screen (or with `presentation`) the field is a
//! trigger that opens a flow of sheets over a draft (`sheet`).

pub mod config;
pub(crate) mod draft;
pub mod editor;
#[cfg(test)]
mod editor_tests;
pub mod examples;
mod field;
#[cfg(test)]
mod field_tests;
pub mod format;
pub mod model;
mod sheet;
#[cfg(test)]
mod sheet_tests;
pub mod source;
pub(crate) mod value_editor;

pub use config::{
    FieldDefinition, FieldKind, FilterRow, PowerSearchConfigured, RowValue,
    create_power_search_config,
};
pub use editor::{EditorMode, PowerSearchFilterEditor};
pub use field::{
    DEFAULT_MAX_SEARCH_RESULTS, PowerSearch, PowerSearchComponentOverride, PowerSearchController,
    PowerSearchEditorProps, PowerSearchTokenProps, ResultCount, SaveFn,
};
pub use model::{
    DateRangePreset, DateTimeRange, DateTimeRangePart, EnumItem, FilterValue, OperatorLabel,
    OperatorValue, PartialFilter, PowerSearchChangeType, PowerSearchConfig, PowerSearchEntity,
    PowerSearchField, PowerSearchFilter, PowerSearchOperator, RelativeDatePreset, RelativeUnit,
    Tokenization,
};

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const POWER_SEARCH_DOC: ComponentDoc = ComponentDoc {
    name: "PowerSearch",
    rust_type: "rustryx::components::power_search::PowerSearch",
    props: &[
        p(
            "config",
            RustApi::Argument("config (Rc<PowerSearchConfig>)"),
        ),
        p("filters", RustApi::Argument("filters")),
        p("onChange", RustApi::Argument("on_change")),
        p("label", RustApi::Method("label")),
        p("isLabelHidden", RustApi::Method("label_hidden")),
        p("placeholder", RustApi::Method("placeholder")),
        p("hasAutoFocus", RustApi::Method("auto_focus")),
        p("hasClear", RustApi::Method("clear")),
        p("isReadOnly", RustApi::Method("read_only")),
        p("isDisabled", RustApi::Method("disabled")),
        p("disabledMessage", RustApi::Method("disabled_message")),
        p("status", RustApi::Method("status")),
        p("startIcon", RustApi::Method("start_icon")),
        p("statusVariant", RustApi::Method("status_placement")),
        p("maxTokenLength", RustApi::Method("max_token_length")),
        p(
            "maxOperatorMenuItems",
            RustApi::Method("max_operator_menu_items"),
        ),
        p("maxSearchResults", RustApi::Method("max_search_results")),
        p("menuWidth", RustApi::Method("menu_width")),
        p("popoverSaveButtonLabel", RustApi::Method("save_label")),
        p("timezoneID", RustApi::Method("timezone_id")),
        p(
            "handleRef",
            RustApi::Method("controller (a PowerSearchController)"),
        ),
        p("endContent", RustApi::Method("end_content")),
        p("resultCount", RustApi::Method("result_count")),
        p("size", RustApi::Method("size")),
        p("components", RustApi::Method("component")),
        p("tokenOverflowBehavior", RustApi::Method("token_overflow")),
        p("onFocus", RustApi::Method("on_focus")),
        p("onBlur", RustApi::Method("on_blur")),
        p("xstyle", RustApi::Trait("Styled")),
        p("presentation", RustApi::Method("presentation")),
    ],
    example: include_str!("examples.rs"),
};
