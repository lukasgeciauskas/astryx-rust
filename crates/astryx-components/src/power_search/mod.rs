//! PowerSearch (M6), in progress. Upstream: `PowerSearch/`.
//!
//! A structured filter bar: each token is a filter (a field, an operator
//! and a value). This stage holds the model, the config builder and the
//! text a filter reads as; the field, its editors and the sheet follow.

pub mod config;
pub mod format;
pub mod model;
pub mod source;

pub use config::{
    FieldDefinition, FieldKind, FilterRow, PowerSearchConfigured, RowValue,
    create_power_search_config,
};
pub use model::{
    DateRangePreset, DateTimeRange, DateTimeRangePart, EnumItem, FilterValue, OperatorLabel,
    OperatorValue, PartialFilter, PowerSearchChangeType, PowerSearchConfig, PowerSearchEntity,
    PowerSearchField, PowerSearchFilter, PowerSearchOperator, RelativeDatePreset, RelativeUnit,
    Tokenization,
};
