//! PowerSearch's data model, upstream `PowerSearch/types.ts`: the config
//! (fields and their operators), the value each operator takes, and the
//! filters a search holds.

use std::fmt;
use std::rc::Rc;

use gpui::{AnyElement, App, SharedString, Window};

use crate::icon::IconSource;
use crate::search::{SearchItem, SearchSource};

/// A search over items, shared by the operators that use it.
pub type ValueSource = Rc<dyn SearchSource<SearchItem>>;
/// Renders an entity row (upstream `renderItem`).
pub type RenderEntityFn = Rc<dyn Fn(&SearchItem, &mut Window, &mut App) -> AnyElement>;
/// A custom value's editor: the value, its change, the placeholder and
/// whether it is disabled (upstream `Editor`).
pub type CustomEditorFn = Rc<
    dyn Fn(
        Option<SharedString>,
        Rc<dyn Fn(Option<SharedString>, &mut Window, &mut App)>,
        SharedString,
        bool,
        &mut Window,
        &mut App,
    ) -> AnyElement,
>;
/// A custom value's text (upstream `getString`).
pub type CustomTextFn = Rc<dyn Fn(&str) -> String>;

/// How a list value splits pasted text (upstream `OperatorTokenizationConfig`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tokenization {
    /// Upstream `regex`: the separators.
    pub pattern: Option<SharedString>,
    /// Upstream `sort`.
    pub sort: bool,
}

/// One choice of an `enum` value (upstream `EnumItem`).
#[derive(Clone, Debug, PartialEq)]
pub struct EnumItem {
    pub value: SharedString,
    pub label: SharedString,
    pub icon: Option<IconSource>,
}

impl EnumItem {
    pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        EnumItem {
            value: value.into(),
            label: label.into(),
            icon: None,
        }
    }

    /// Upstream `icon`.
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

/// A relative date's unit (upstream `DateTimeRangePart.unit`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelativeUnit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

impl RelativeUnit {
    /// Upstream `SECONDS_BY_UNIT`: a month is 30 days and a year 365.
    pub fn seconds(self) -> i64 {
        match self {
            RelativeUnit::Second => 1,
            RelativeUnit::Minute => 60,
            RelativeUnit::Hour => 3_600,
            RelativeUnit::Day => 86_400,
            RelativeUnit::Week => 604_800,
            RelativeUnit::Month => 2_592_000,
            RelativeUnit::Year => 31_536_000,
        }
    }
}

/// One end of a date range (upstream `DateTimeRangePart`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DateTimeRangePart {
    Now,
    Absolute {
        unix_seconds: i64,
    },
    Relative {
        back_value: i64,
        unit: RelativeUnit,
        anchor_key: Option<SharedString>,
    },
}

impl DateTimeRangePart {
    /// Upstream `resolveDateTimeRangePart`: the instant, in Unix seconds.
    pub fn resolve(&self, now_seconds: f64) -> i64 {
        match self {
            DateTimeRangePart::Now => now_seconds.floor() as i64,
            DateTimeRangePart::Absolute { unix_seconds } => *unix_seconds,
            DateTimeRangePart::Relative {
                back_value, unit, ..
            } => (now_seconds - (*back_value * unit.seconds()) as f64).floor() as i64,
        }
    }
}

/// Upstream `DateTimeRange`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateTimeRange {
    pub start: DateTimeRangePart,
    pub end: DateTimeRangePart,
}

/// Upstream `DateRangeFilterPreset`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateRangePreset {
    pub label: SharedString,
    pub value: DateTimeRange,
}

/// Upstream `RelativeDateFilterPreset`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelativeDatePreset {
    pub label: SharedString,
    pub value: SharedString,
}

/// What an operator's value is and how it is edited (upstream
/// `OperatorValue`).
#[derive(Clone)]
pub enum OperatorValue {
    /// No value: `is true`, `is empty`.
    Empty,
    String {
        search_source: Option<ValueSource>,
        arbitrary_allowed: bool,
    },
    StringList {
        search_source: Option<ValueSource>,
        arbitrary_allowed: bool,
        tokenization: Option<Tokenization>,
    },
    Integer {
        min: Option<i64>,
        max: Option<i64>,
        units: Option<SharedString>,
    },
    Float {
        min: Option<f64>,
        max: Option<f64>,
        units: Option<SharedString>,
    },
    /// A time of day, `HH:MM`.
    Time {
        min: Option<SharedString>,
        max: Option<SharedString>,
    },
    /// Upstream `isDateOnly`: unset or true, a calendar day (DEC-5).
    DateAbsolute {
        date_only: Option<bool>,
    },
    DateRelative {
        past_allowed: Option<bool>,
        future_allowed: Option<bool>,
    },
    DateRange {
        interval_presets: Vec<DateRangePreset>,
        relative_presets: Vec<RelativeDatePreset>,
    },
    Enum {
        values: Vec<EnumItem>,
    },
    EnumList {
        values: Vec<EnumItem>,
    },
    EntityList {
        search_source: Option<ValueSource>,
        arbitrary_allowed: bool,
        tokenization: Option<Tokenization>,
        render_item: Option<RenderEntityFn>,
    },
    Custom {
        editor: CustomEditorFn,
        text: CustomTextFn,
    },
    /// A group of filters.
    Nested,
}

impl OperatorValue {
    /// A free-text `string` value.
    pub fn string() -> Self {
        OperatorValue::String {
            search_source: None,
            arbitrary_allowed: false,
        }
    }

    /// A `string_list` value of free text.
    pub fn string_list() -> Self {
        OperatorValue::StringList {
            search_source: None,
            arbitrary_allowed: false,
            tokenization: None,
        }
    }

    /// A `float` value.
    pub fn float() -> Self {
        OperatorValue::Float {
            min: None,
            max: None,
            units: None,
        }
    }

    /// An `integer` value.
    pub fn integer() -> Self {
        OperatorValue::Integer {
            min: None,
            max: None,
            units: None,
        }
    }

    /// The upstream `type` name.
    pub fn kind(&self) -> &'static str {
        match self {
            OperatorValue::Empty => "empty",
            OperatorValue::String { .. } => "string",
            OperatorValue::StringList { .. } => "string_list",
            OperatorValue::Integer { .. } => "integer",
            OperatorValue::Float { .. } => "float",
            OperatorValue::Time { .. } => "time",
            OperatorValue::DateAbsolute { .. } => "date_absolute",
            OperatorValue::DateRelative { .. } => "date_relative",
            OperatorValue::DateRange { .. } => "date_range",
            OperatorValue::Enum { .. } => "enum",
            OperatorValue::EnumList { .. } => "enum_list",
            OperatorValue::EntityList { .. } => "entity_list",
            OperatorValue::Custom { .. } => "custom",
            OperatorValue::Nested => "nested",
        }
    }

    /// Its search source, when its editor searches one.
    pub fn search_source(&self) -> Option<&ValueSource> {
        match self {
            OperatorValue::String { search_source, .. }
            | OperatorValue::StringList { search_source, .. }
            | OperatorValue::EntityList { search_source, .. } => search_source.as_ref(),
            _ => None,
        }
    }

    /// The enum choices, for `enum` and `enum_list`.
    pub fn enum_values(&self) -> Option<&[EnumItem]> {
        match self {
            OperatorValue::Enum { values } | OperatorValue::EnumList { values } => Some(values),
            _ => None,
        }
    }
}

impl fmt::Debug for OperatorValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind())
    }
}

/// An entity in an `entity_list` value (upstream `PowerSearchEntity`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerSearchEntity {
    pub id: SharedString,
    pub label: SharedString,
    pub photo: Option<SharedString>,
}

/// A filter's value (upstream `FilterValue`).
#[derive(Clone, Debug, PartialEq)]
pub enum FilterValue {
    Empty,
    String(SharedString),
    StringList(Vec<SharedString>),
    Integer(i64),
    Float(f64),
    Time(SharedString),
    DateAbsolute { unix_seconds: i64 },
    DateRelative(SharedString),
    DateRange(DateTimeRange),
    Enum(SharedString),
    EnumList(Vec<SharedString>),
    EntityList(Vec<PowerSearchEntity>),
    Custom(SharedString),
    Nested(Vec<PowerSearchFilter>),
}

impl FilterValue {
    /// The upstream `type` name.
    pub fn kind(&self) -> &'static str {
        match self {
            FilterValue::Empty => "empty",
            FilterValue::String(_) => "string",
            FilterValue::StringList(_) => "string_list",
            FilterValue::Integer(_) => "integer",
            FilterValue::Float(_) => "float",
            FilterValue::Time(_) => "time",
            FilterValue::DateAbsolute { .. } => "date_absolute",
            FilterValue::DateRelative(_) => "date_relative",
            FilterValue::DateRange(_) => "date_range",
            FilterValue::Enum(_) => "enum",
            FilterValue::EnumList(_) => "enum_list",
            FilterValue::EntityList(_) => "entity_list",
            FilterValue::Custom(_) => "custom",
            FilterValue::Nested(_) => "nested",
        }
    }
}

/// An operator's name: its own text, or a catalog key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperatorLabel {
    /// Upstream `label`.
    Text(SharedString),
    /// Upstream `i18nKey`.
    Key(SharedString),
}

/// One way to compare a field (upstream `PowerSearchOperator`).
#[derive(Clone, Debug)]
pub struct PowerSearchOperator {
    pub key: SharedString,
    pub label: OperatorLabel,
    pub value: OperatorValue,
    /// Upstream `isValueOnlyMatchAllowed`: a query of only a value offers
    /// this operator's rows (FR27).
    pub value_only_match: bool,
}

impl PowerSearchOperator {
    /// An operator named by its own `label`.
    pub fn new(
        key: impl Into<SharedString>,
        label: impl Into<SharedString>,
        value: OperatorValue,
    ) -> Self {
        PowerSearchOperator {
            key: key.into(),
            label: OperatorLabel::Text(label.into()),
            value,
            value_only_match: false,
        }
    }

    /// An operator named by a catalog key (upstream `i18nKey`).
    pub fn keyed(
        key: impl Into<SharedString>,
        i18n_key: impl Into<SharedString>,
        value: OperatorValue,
    ) -> Self {
        PowerSearchOperator {
            key: key.into(),
            label: OperatorLabel::Key(i18n_key.into()),
            value,
            value_only_match: false,
        }
    }

    /// Upstream `isValueOnlyMatchAllowed` (FR27).
    pub fn value_only_match(mut self, allowed: bool) -> Self {
        self.value_only_match = allowed;
        self
    }

    /// Upstream `resolveOperatorLabel`.
    pub fn label_text(&self, cx: &App) -> SharedString {
        match &self.label {
            OperatorLabel::Text(text) => text.clone(),
            OperatorLabel::Key(key) => astryx_i18n::current(cx).t(key),
        }
    }
}

/// A field a filter can test (upstream `PowerSearchField`).
#[derive(Clone, Debug)]
pub struct PowerSearchField {
    pub key: SharedString,
    pub label: SharedString,
    pub operators: Vec<PowerSearchOperator>,
    pub icon: Option<IconSource>,
    pub default_operator: Option<SharedString>,
    pub group: Option<SharedString>,
    pub description: Option<SharedString>,
    pub typeahead_aliases: Vec<SharedString>,
    pub typeahead_min_query_length: Option<usize>,
    /// Upstream `isValueMatchAllowed`: `Some(false)` turns off every
    /// value suggestion of the field.
    pub value_match_allowed: Option<bool>,
}

impl PowerSearchField {
    pub fn new(key: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        PowerSearchField {
            key: key.into(),
            label: label.into(),
            operators: Vec::new(),
            icon: None,
            default_operator: None,
            group: None,
            description: None,
            typeahead_aliases: Vec::new(),
            typeahead_min_query_length: None,
            value_match_allowed: None,
        }
    }

    /// Adds an operator.
    pub fn operator(mut self, operator: PowerSearchOperator) -> Self {
        self.operators.push(operator);
        self
    }

    /// Upstream `operators`.
    pub fn operators(mut self, operators: impl IntoIterator<Item = PowerSearchOperator>) -> Self {
        self.operators.extend(operators);
        self
    }

    /// Upstream `defaultOperator`.
    pub fn default_operator(mut self, key: impl Into<SharedString>) -> Self {
        self.default_operator = Some(key.into());
        self
    }

    /// Upstream `icon`.
    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Upstream `group`.
    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }

    /// Upstream `description`.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Upstream `typeaheadAliases`.
    pub fn aliases(mut self, aliases: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.typeahead_aliases = aliases.into_iter().map(Into::into).collect();
        self
    }

    /// Upstream `typeaheadMinQueryLength`.
    pub fn min_query_length(mut self, length: usize) -> Self {
        self.typeahead_min_query_length = Some(length);
        self
    }

    /// Upstream `isValueMatchAllowed`.
    pub fn value_match_allowed(mut self, allowed: bool) -> Self {
        self.value_match_allowed = Some(allowed);
        self
    }

    /// The operator with `key`.
    pub fn find_operator(&self, key: &str) -> Option<&PowerSearchOperator> {
        self.operators.iter().find(|o| o.key.as_ref() == key)
    }

    /// Upstream `getDefaultOperator`: the named default, else the first.
    pub fn default_op(&self) -> Option<&PowerSearchOperator> {
        match &self.default_operator {
            Some(key) => self.find_operator(key),
            None => self.operators.first(),
        }
    }
}

/// What a search can filter on (upstream `PowerSearchConfig`).
#[derive(Clone, Debug, Default)]
pub struct PowerSearchConfig {
    pub name: SharedString,
    pub fields: Vec<PowerSearchField>,
    /// Upstream `contentSearchFieldKey`: the field free text searches.
    pub content_search_field_key: Option<SharedString>,
}

impl PowerSearchConfig {
    pub fn new(name: impl Into<SharedString>) -> Self {
        PowerSearchConfig {
            name: name.into(),
            fields: Vec::new(),
            content_search_field_key: None,
        }
    }

    /// Adds a field.
    pub fn field(mut self, field: PowerSearchField) -> Self {
        self.fields.push(field);
        self
    }

    /// Upstream `contentSearchFieldKey`.
    pub fn content_search_field(mut self, key: impl Into<SharedString>) -> Self {
        self.content_search_field_key = Some(key.into());
        self
    }

    /// Upstream `getField`.
    pub fn find_field(&self, key: &str) -> Option<&PowerSearchField> {
        self.fields.iter().find(|f| f.key.as_ref() == key)
    }

    /// Upstream `getOperator`.
    pub fn find_operator(&self, field: &str, operator: &str) -> Option<&PowerSearchOperator> {
        self.find_field(field)
            .and_then(|f| f.find_operator(operator))
    }
}

/// One active filter (upstream `PowerSearchFilter`).
#[derive(Clone, Debug, PartialEq)]
pub struct PowerSearchFilter {
    pub field: SharedString,
    pub operator: SharedString,
    pub value: FilterValue,
    /// Upstream `isReadOnly`: shown, never edited or removed.
    pub read_only: bool,
}

impl PowerSearchFilter {
    pub fn new(
        field: impl Into<SharedString>,
        operator: impl Into<SharedString>,
        value: FilterValue,
    ) -> Self {
        PowerSearchFilter {
            field: field.into(),
            operator: operator.into(),
            value,
            read_only: false,
        }
    }

    /// Upstream `isReadOnly`.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }
}

/// A filter being built (upstream `PartialFilter`).
#[derive(Clone, Debug, PartialEq)]
pub struct PartialFilter {
    pub field: SharedString,
    pub operator: Option<SharedString>,
    pub value: Option<FilterValue>,
    pub read_only: bool,
}

/// What a change did (upstream `PowerSearchChangeType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerSearchChangeType {
    Add,
    Edit,
    Remove,
}
