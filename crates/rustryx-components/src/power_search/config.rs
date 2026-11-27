//! Upstream `usePowerSearchConfig`: a config from typed field
//! definitions, with the standard operators for each type, and
//! `apply_filters` to filter rows in memory.

use std::collections::HashMap;

use gpui::SharedString;

use super::model::{
    EnumItem, FilterValue, OperatorValue, PowerSearchConfig, PowerSearchField, PowerSearchFilter,
    PowerSearchOperator,
};

/// A field's type in [`create_power_search_config`] (upstream `FieldType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    String,
    Number,
    Boolean,
    Date,
    Enum,
    EnumList,
    StringList,
}

/// One field to build (upstream `FieldDefinition`).
#[derive(Clone, Debug)]
pub struct FieldDefinition {
    pub key: SharedString,
    pub kind: FieldKind,
    /// The label; the key without one.
    pub label: Option<SharedString>,
    pub enum_values: Vec<EnumItem>,
}

impl FieldDefinition {
    pub fn new(key: impl Into<SharedString>, kind: FieldKind) -> Self {
        FieldDefinition {
            key: key.into(),
            kind,
            label: None,
            enum_values: Vec::new(),
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn enum_values(mut self, values: impl IntoIterator<Item = EnumItem>) -> Self {
        self.enum_values = values.into_iter().collect();
        self
    }
}

fn op(key: &'static str, i18n: &'static str, value: OperatorValue) -> PowerSearchOperator {
    PowerSearchOperator::keyed(key, format!("@astryx.powersearch.operator.{i18n}"), value)
}

fn build_field(def: &FieldDefinition) -> PowerSearchField {
    let label = def.label.clone().unwrap_or_else(|| def.key.clone());
    let field = PowerSearchField::new(def.key.clone(), label);
    let values = def.enum_values.clone();
    match def.kind {
        FieldKind::String => field.default_operator("contains").operators([
            op("contains", "contains", OperatorValue::string()),
            op("not_contains", "notContains", OperatorValue::string()),
            op("starts_with", "startsWith", OperatorValue::string()),
            op("not_starts_with", "notStartsWith", OperatorValue::string()),
            op("ends_with", "endsWith", OperatorValue::string()),
            op("not_ends_with", "notEndsWith", OperatorValue::string()),
            op("is", "is", OperatorValue::string()),
            op("is_not", "isNot", OperatorValue::string()),
        ]),
        FieldKind::Number => field.default_operator("equals").operators([
            op("equals", "equals", OperatorValue::float()),
            op("not_equals", "notEquals", OperatorValue::float()),
            op("greater_than", "greaterThan", OperatorValue::float()),
            op("less_than", "lessThan", OperatorValue::float()),
            op(
                "greater_than_or_equal",
                "greaterThanOrEqual",
                OperatorValue::float(),
            ),
            op(
                "less_than_or_equal",
                "lessThanOrEqual",
                OperatorValue::float(),
            ),
        ]),
        FieldKind::Date => field.default_operator("after").operators([
            op(
                "before",
                "before",
                OperatorValue::DateAbsolute { date_only: None },
            ),
            op(
                "after",
                "after",
                OperatorValue::DateAbsolute { date_only: None },
            ),
            op(
                "between",
                "between",
                OperatorValue::DateRange {
                    interval_presets: Vec::new(),
                    relative_presets: Vec::new(),
                },
            ),
        ]),
        FieldKind::Boolean => field.default_operator("is_true").operators([
            op("is_true", "isTrue", OperatorValue::Empty),
            op("is_false", "isFalse", OperatorValue::Empty),
        ]),
        FieldKind::Enum => field.default_operator("is").operators([
            op(
                "is",
                "is",
                OperatorValue::Enum {
                    values: values.clone(),
                },
            ),
            op(
                "is_not",
                "isNot",
                OperatorValue::Enum {
                    values: values.clone(),
                },
            ),
            op(
                "is_any_of",
                "isAnyOf",
                OperatorValue::EnumList {
                    values: values.clone(),
                },
            ),
            op("is_none_of", "isNoneOf", OperatorValue::EnumList { values }),
        ]),
        FieldKind::EnumList => field.default_operator("is_any_of").operators([
            op(
                "is_any_of",
                "isAnyOf",
                OperatorValue::EnumList {
                    values: values.clone(),
                },
            ),
            op("is_none_of", "isNoneOf", OperatorValue::EnumList { values }),
        ]),
        FieldKind::StringList => field.default_operator("is_any_of").operators([
            op("is_any_of", "isAnyOf", OperatorValue::string_list()),
            op("is_none_of", "isNoneOf", OperatorValue::string_list()),
        ]),
    }
}

/// A row's value for one field, for [`PowerSearchConfigured::apply_filters`].
#[derive(Clone, Debug, PartialEq)]
pub enum RowValue {
    String(SharedString),
    Number(f64),
    Boolean(bool),
    /// Unix seconds.
    Date(i64),
    Strings(Vec<SharedString>),
}

/// A row `apply_filters` can read (upstream: a record keyed by field).
pub trait FilterRow {
    fn value(&self, field: &str) -> Option<RowValue>;
}

impl FilterRow for HashMap<SharedString, RowValue> {
    fn value(&self, field: &str) -> Option<RowValue> {
        self.get(field).cloned()
    }
}

/// A built config and its in-memory filter (upstream
/// `createPowerSearchConfig`'s result).
#[derive(Clone, Debug)]
pub struct PowerSearchConfigured {
    pub config: PowerSearchConfig,
}

impl PowerSearchConfigured {
    /// Upstream `applyFilters`: the rows every filter keeps.
    pub fn apply_filters<R: FilterRow + Clone>(
        &self,
        filters: &[PowerSearchFilter],
        rows: &[R],
    ) -> Vec<R> {
        let now = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.);
        rows.iter()
            .filter(|row| filters.iter().all(|f| matches_filter(*row, f, now)))
            .cloned()
            .collect()
    }
}

/// Upstream `createPowerSearchConfig` (and `usePowerSearchConfig`).
pub fn create_power_search_config(
    definitions: impl IntoIterator<Item = FieldDefinition>,
    name: Option<&str>,
) -> PowerSearchConfigured {
    let fields = definitions.into_iter().map(|d| build_field(&d)).collect();
    PowerSearchConfigured {
        config: PowerSearchConfig {
            name: name.unwrap_or("PowerSearchConfig").to_string().into(),
            fields,
            content_search_field_key: None,
        },
    }
}

fn strings(value: Option<RowValue>) -> Option<Vec<SharedString>> {
    match value {
        Some(RowValue::String(s)) => Some(vec![s]),
        Some(RowValue::Strings(list)) => Some(list),
        _ => None,
    }
}

/// Upstream `matchesFilter`.
pub(crate) fn matches_filter(row: &impl FilterRow, filter: &PowerSearchFilter, now: f64) -> bool {
    let value = row.value(&filter.field);
    let op = filter.operator.as_ref();
    match &filter.value {
        FilterValue::Empty => match op {
            "is_true" => truthy(&value),
            "is_false" => !truthy(&value),
            _ => true,
        },
        FilterValue::String(target) => {
            let Some(RowValue::String(s)) = value else {
                return false;
            };
            let (s, t) = (s.to_lowercase(), target.to_lowercase());
            match op {
                "contains" => s.contains(&t),
                "not_contains" => !s.contains(&t),
                "starts_with" => s.starts_with(&t),
                "not_starts_with" => !s.starts_with(&t),
                "ends_with" => s.ends_with(&t),
                "not_ends_with" => !s.ends_with(&t),
                "is" => s == t,
                "is_not" => s != t,
                _ => true,
            }
        }
        FilterValue::Integer(_) | FilterValue::Float(_) => {
            let Some(RowValue::Number(n)) = value else {
                return false;
            };
            let t = match &filter.value {
                FilterValue::Integer(i) => *i as f64,
                FilterValue::Float(f) => *f,
                _ => unreachable!(),
            };
            match op {
                "equals" => n == t,
                "not_equals" => n != t,
                "greater_than" => n > t,
                "less_than" => n < t,
                "greater_than_or_equal" => n >= t,
                "less_than_or_equal" => n <= t,
                _ => true,
            }
        }
        FilterValue::DateAbsolute { unix_seconds } => {
            let Some(RowValue::Date(ts)) = value else {
                return false;
            };
            match op {
                "before" => ts < *unix_seconds,
                "after" => ts > *unix_seconds,
                _ => true,
            }
        }
        FilterValue::DateRange(range) => {
            let Some(RowValue::Date(ts)) = value else {
                return false;
            };
            if op == "between" {
                ts >= range.start.resolve(now) && ts <= range.end.resolve(now)
            } else {
                true
            }
        }
        FilterValue::Enum(target) => {
            let Some(RowValue::String(s)) = value else {
                return false;
            };
            match op {
                "is" => &s == target,
                "is_not" => &s != target,
                _ => true,
            }
        }
        FilterValue::EnumList(targets) | FilterValue::StringList(targets) => {
            let Some(values) = strings(value) else {
                return false;
            };
            match op {
                "is_any_of" => values.iter().any(|v| targets.contains(v)),
                "is_none_of" => values.iter().all(|v| !targets.contains(v)),
                _ => true,
            }
        }
        FilterValue::Time(_)
        | FilterValue::DateRelative(_)
        | FilterValue::EntityList(_)
        | FilterValue::Custom(_)
        | FilterValue::Nested(_) => true,
    }
}

/// JavaScript's `Boolean(value)`.
fn truthy(value: &Option<RowValue>) -> bool {
    match value {
        None => false,
        Some(RowValue::Boolean(b)) => *b,
        Some(RowValue::String(s)) => !s.is_empty(),
        Some(RowValue::Number(n)) => *n != 0. && !n.is_nan(),
        Some(RowValue::Date(_)) | Some(RowValue::Strings(_)) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power_search::model::{DateTimeRange, DateTimeRangePart, RelativeUnit};

    fn row(pairs: &[(&str, RowValue)]) -> HashMap<SharedString, RowValue> {
        pairs
            .iter()
            .map(|(k, v)| (SharedString::from(k.to_string()), v.clone()))
            .collect()
    }

    #[test]
    fn fields_get_the_standard_operators() {
        let built = create_power_search_config(
            [
                FieldDefinition::new("title", FieldKind::String).label("Title"),
                FieldDefinition::new("pages", FieldKind::Number),
                FieldDefinition::new("genre", FieldKind::Enum)
                    .enum_values([EnumItem::new("fiction", "Fiction")]),
            ],
            None,
        );
        let config = &built.config;
        assert_eq!(config.name, "PowerSearchConfig");
        let title = config.find_field("title").unwrap();
        assert_eq!(title.operators.len(), 8);
        assert_eq!(title.default_op().unwrap().key, "contains");
        assert_eq!(
            config.find_field("pages").unwrap().label,
            "pages",
            "the key labels it"
        );
        let genre = config.find_field("genre").unwrap();
        assert_eq!(
            genre
                .operators
                .iter()
                .map(|o| o.value.kind())
                .collect::<Vec<_>>(),
            ["enum", "enum", "enum_list", "enum_list"]
        );
    }

    #[test]
    fn apply_filters_keeps_the_rows_every_filter_keeps() {
        let built =
            create_power_search_config([FieldDefinition::new("title", FieldKind::String)], None);
        let rows = vec![
            row(&[
                ("title", RowValue::String("Dune".into())),
                ("pages", RowValue::Number(412.)),
            ]),
            row(&[
                ("title", RowValue::String("Emma".into())),
                ("pages", RowValue::Number(300.)),
            ]),
        ];
        let kept = built.apply_filters(
            &[PowerSearchFilter::new(
                "title",
                "contains",
                FilterValue::String("du".into()),
            )],
            &rows,
        );
        assert_eq!(kept.len(), 1);
        let kept = built.apply_filters(
            &[PowerSearchFilter::new(
                "pages",
                "greater_than_or_equal",
                FilterValue::Float(300.),
            )],
            &rows,
        );
        assert_eq!(kept.len(), 2);
        let kept = built.apply_filters(
            &[
                PowerSearchFilter::new("title", "not_starts_with", FilterValue::String("e".into())),
                PowerSearchFilter::new("pages", "less_than", FilterValue::Float(500.)),
            ],
            &rows,
        );
        assert_eq!(kept.len(), 1);
        assert!(built.apply_filters(&[], &rows).len() == 2);
    }

    #[test]
    fn lists_booleans_and_dates_match() {
        let r = row(&[
            ("tags", RowValue::Strings(vec!["a".into(), "b".into()])),
            ("done", RowValue::Boolean(true)),
            ("at", RowValue::Date(1_000)),
        ]);
        let any = PowerSearchFilter::new(
            "tags",
            "is_any_of",
            FilterValue::StringList(vec!["b".into()]),
        );
        let none = PowerSearchFilter::new(
            "tags",
            "is_none_of",
            FilterValue::StringList(vec!["b".into()]),
        );
        assert!(matches_filter(&r, &any, 0.));
        assert!(!matches_filter(&r, &none, 0.));
        assert!(matches_filter(
            &r,
            &PowerSearchFilter::new("done", "is_true", FilterValue::Empty),
            0.
        ));
        assert!(matches_filter(
            &r,
            &PowerSearchFilter::new(
                "at",
                "before",
                FilterValue::DateAbsolute {
                    unix_seconds: 2_000
                }
            ),
            0.
        ));
        let range = FilterValue::DateRange(DateTimeRange {
            start: DateTimeRangePart::Relative {
                back_value: 1,
                unit: RelativeUnit::Hour,
                anchor_key: None,
            },
            end: DateTimeRangePart::Now,
        });
        assert!(matches_filter(
            &r,
            &PowerSearchFilter::new("at", "between", range.clone()),
            4_000.
        ));
        assert!(!matches_filter(
            &r,
            &PowerSearchFilter::new("at", "between", range),
            9_000.
        ));
    }
}
