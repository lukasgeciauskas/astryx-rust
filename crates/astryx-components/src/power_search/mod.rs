//! PowerSearch (M6), in progress. Upstream: `PowerSearch/`.
//!
//! A structured filter bar: each token is a filter (a field, an operator
//! and a value). Done so far: the model, the config builder, the text a
//! filter reads as, the main field's rows, the value editors and the
//! filter editor; the field itself and its sheet follow.

pub mod config;
pub mod editor;
#[cfg(test)]
mod editor_tests;
pub mod format;
pub mod model;
pub mod source;
pub(crate) mod value_editor;

pub use config::{
    FieldDefinition, FieldKind, FilterRow, PowerSearchConfigured, RowValue,
    create_power_search_config,
};
pub use editor::{EditorMode, PowerSearchFilterEditor};
pub use model::{
    DateRangePreset, DateTimeRange, DateTimeRangePart, EnumItem, FilterValue, OperatorLabel,
    OperatorValue, PartialFilter, PowerSearchChangeType, PowerSearchConfig, PowerSearchEntity,
    PowerSearchField, PowerSearchFilter, PowerSearchOperator, RelativeDatePreset, RelativeUnit,
    Tokenization,
};
