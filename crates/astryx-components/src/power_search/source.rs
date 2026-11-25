//! The main field's rows, upstream `usePowerSearchSource` and FR27-FR29:
//! fields, fields with an operator, field-prefixed values, the content
//! search, and value-only matches with their "More" rows.

use astryx_i18n::I18n;
use astryx_primitives::typeahead::fold;
use gpui::SharedString;

use super::model::{
    FilterValue, OperatorValue, PowerSearchConfig, PowerSearchField, PowerSearchOperator,
};
use crate::search::Searchable;

/// What a row does when chosen (upstream `PowerSearchAuxData`).
#[derive(Clone, Debug, PartialEq)]
pub struct RowData {
    pub field_key: SharedString,
    pub operator_key: Option<SharedString>,
    /// A row that carries its value commits it.
    pub filter_value: Option<FilterValue>,
    pub group: Option<SharedString>,
    /// A "More" row: opens the editor with the typed text (FR29).
    pub more_query: Option<SharedString>,
    /// A token's filter.
    pub filter_index: Option<usize>,
}

/// One row of the main field's list (upstream `PowerSearchItem`).
#[derive(Clone, Debug, PartialEq)]
pub struct PowerSearchItem {
    pub id: SharedString,
    pub label: SharedString,
    pub data: RowData,
}

impl Searchable for PowerSearchItem {
    fn id(&self) -> SharedString {
        self.id.clone()
    }

    fn label(&self) -> SharedString {
        self.label.clone()
    }

    fn group(&self) -> Option<SharedString> {
        self.data.group.clone()
    }
}

/// A field's value-only matches beyond this show behind its "More" row.
pub const VALUE_ROWS_PER_FIELD: usize = 3;

fn item(id: String, label: String, data: RowData) -> PowerSearchItem {
    PowerSearchItem {
        id: id.into(),
        label: label.into(),
        data,
    }
}

fn data(
    field: &PowerSearchField,
    operator: Option<&PowerSearchOperator>,
    value: Option<FilterValue>,
) -> RowData {
    RowData {
        field_key: field.key.clone(),
        operator_key: operator.map(|o| o.key.clone()),
        filter_value: value,
        group: None,
        more_query: None,
        filter_index: None,
    }
}

/// Upstream `buildFieldItems`: every field with its default operator,
/// ungrouped fields first, then each group in order of appearance.
pub fn field_rows(config: &PowerSearchConfig) -> Vec<PowerSearchItem> {
    let rows: Vec<PowerSearchItem> = config
        .fields
        .iter()
        .map(|f| {
            let mut d = data(f, f.default_op(), None);
            d.group = f.group.clone();
            item(f.key.to_string(), f.label.to_string(), d)
        })
        .collect();
    let mut out: Vec<PowerSearchItem> = rows
        .iter()
        .filter(|r| r.data.group.is_none())
        .cloned()
        .collect();
    let mut groups: Vec<SharedString> = Vec::new();
    for r in &rows {
        if let Some(g) = &r.data.group
            && !groups.contains(g)
        {
            groups.push(g.clone());
        }
    }
    for g in groups {
        out.extend(
            rows.iter()
                .filter(|r| r.data.group.as_ref() == Some(&g))
                .cloned(),
        );
    }
    out
}

/// Upstream `resolveValueMatches`: the values a field-prefixed query names.
fn prefixed_values(op: &PowerSearchOperator, raw: &str) -> Vec<(String, FilterValue, bool)> {
    let lower = raw.to_lowercase();
    match &op.value {
        OperatorValue::String { .. } => vec![(
            raw.to_string(),
            FilterValue::String(raw.to_string().into()),
            true,
        )],
        OperatorValue::StringList { .. } => {
            vec![(
                raw.to_string(),
                FilterValue::StringList(vec![raw.to_string().into()]),
                true,
            )]
        }
        OperatorValue::Enum { values } => values
            .iter()
            .filter(|v| v.label.to_lowercase().contains(&lower))
            .map(|v| {
                (
                    v.label.to_string(),
                    FilterValue::Enum(v.value.clone()),
                    false,
                )
            })
            .collect(),
        OperatorValue::EnumList { values } => values
            .iter()
            .filter(|v| v.label.to_lowercase().contains(&lower))
            .map(|v| {
                (
                    v.label.to_string(),
                    FilterValue::EnumList(vec![v.value.clone()]),
                    false,
                )
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A row's text: the field, the operator and the value in one catalog
/// message, quoted for typed text (FR27).
pub fn value_row_label(
    field: &str,
    operator: &str,
    value: &str,
    quoted: bool,
    t18: &I18n,
) -> String {
    let key = if quoted {
        "@astryx.powersearch.valueMatchText"
    } else {
        "@astryx.powersearch.valueMatch"
    };
    t18.format(
        key,
        &[
            ("field", field.into()),
            ("operator", operator.into()),
            ("value", value.into()),
        ],
    )
    .to_string()
}

/// Whether `query` names only a value (FR27): it neither equals a field's
/// label nor starts with one and a space, ignoring case.
pub fn is_value_only(config: &PowerSearchConfig, query: &str) -> bool {
    let lower = query.trim().to_lowercase();
    !lower.is_empty()
        && !config.fields.iter().any(|f| {
            let label = f.label.to_lowercase();
            lower == label || lower.starts_with(&format!("{label} "))
        })
}

/// Whether an opted-in operator edits free text: a `string` or
/// `string_list` with no source, or a `string_list` that takes any text.
pub fn takes_free_text(op: &OperatorValue) -> bool {
    match op {
        OperatorValue::String { search_source, .. } => search_source.is_none(),
        OperatorValue::StringList {
            search_source,
            arbitrary_allowed,
            ..
        } => search_source.is_none() || *arbitrary_allowed,
        _ => false,
    }
}

/// Whether a field may add value-only rows for `query` (FR27).
pub fn field_takes_value_rows(field: &PowerSearchField, query: &str) -> bool {
    if field.value_match_allowed == Some(false) {
        return false;
    }
    let length = query.trim().chars().count();
    length >= field.typeahead_min_query_length.unwrap_or(2)
}

/// The tier of an enum label for a typed text (FR27): 0 the whole label,
/// 1 its start, 2 a later word's start; none when no word starts with it.
pub fn word_match_tier(label: &str, typed: &str) -> Option<u8> {
    let (label, typed) = (fold(label), fold(typed.trim()));
    if typed.is_empty() {
        return None;
    }
    if label == typed {
        return Some(0);
    }
    if label.starts_with(&typed) {
        return Some(1);
    }
    let mut previous: Option<char> = None;
    for (at, ch) in label.char_indices() {
        let boundary = previous.is_some_and(|p| {
            p.is_whitespace()
                || p.is_ascii_punctuation()
                || (!p.is_alphanumeric() && !p.is_whitespace())
        });
        if boundary && label[at..].starts_with(&typed) {
            return Some(2);
        }
        previous = Some(ch);
    }
    None
}

/// The rows a field offers a value-only query without a source (FR27), in
/// operator order: each opted-in operator's free-text row, then its enum
/// rows by tier. `content_default` is the content-search field's default
/// operator while the content-search row shows; it adds none.
pub fn value_only_rows(
    field: &PowerSearchField,
    query: &str,
    content_default: Option<(&str, &str)>,
    cx: &gpui::App,
) -> Vec<PowerSearchItem> {
    value_only_rows_with(field, query, content_default, &[], 10, cx)
}

/// The items one source answered, keyed by the source's identity.
pub type SourceAnswers = [(usize, Vec<crate::search::SearchItem>)];

/// A source's identity: one object several operators share is one source.
pub fn source_key(source: &super::model::ValueSource) -> usize {
    std::rc::Rc::as_ptr(source) as *const () as usize
}

/// [`value_only_rows`] with the answers of the field's sources (FR28): each
/// opted-in operator with a source adds a row per item its source answered,
/// at most `max_items`, committing the value its editor stores.
pub fn value_only_rows_with(
    field: &PowerSearchField,
    query: &str,
    content_default: Option<(&str, &str)>,
    answers: &SourceAnswers,
    max_items: usize,
    cx: &gpui::App,
) -> Vec<PowerSearchItem> {
    let t18 = astryx_i18n::current(cx);
    let mut rows = Vec::new();
    if !field_takes_value_rows(field, query) {
        return rows;
    }
    for op in field.operators.iter().filter(|o| o.value_only_match) {
        if content_default == Some((field.key.as_ref(), op.key.as_ref())) {
            continue;
        }
        let op_label = op.label_text(cx);
        if takes_free_text(&op.value) {
            let value = match &op.value {
                OperatorValue::String { .. } => FilterValue::String(query.to_string().into()),
                _ => FilterValue::StringList(vec![query.to_string().into()]),
            };
            rows.push(item(
                format!("{}:{}:only:{}", field.key, op.key, query),
                value_row_label(&field.label, &op_label, query, true, &t18),
                data(field, Some(op), Some(value)),
            ));
            continue;
        }
        if let Some(source) = op.value.search_source() {
            let key = source_key(source);
            let items = answers
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, items)| items.as_slice())
                .unwrap_or(&[]);
            for found in items.iter().take(max_items) {
                let (id, label) = (
                    crate::search::Searchable::id(found),
                    crate::search::Searchable::label(found),
                );
                let value = match &op.value {
                    OperatorValue::String { .. } => FilterValue::String(label.clone()),
                    OperatorValue::StringList { .. } => {
                        FilterValue::StringList(vec![label.clone()])
                    }
                    _ => FilterValue::EntityList(vec![super::model::PowerSearchEntity {
                        id: id.clone(),
                        label: label.clone(),
                        photo: found
                            .data_as::<super::value_editor::EntityPhoto>()
                            .map(|p| p.0.clone()),
                    }]),
                };
                rows.push(item(
                    format!("{}:{}:src:{}", field.key, op.key, id),
                    value_row_label(&field.label, &op_label, &label, false, &t18),
                    data(field, Some(op), Some(value)),
                ));
            }
            continue;
        }
        let Some(values) = op.value.enum_values() else {
            continue;
        };
        let mut tiered: Vec<(u8, usize)> = values
            .iter()
            .enumerate()
            .filter_map(|(i, v)| word_match_tier(&v.label, query).map(|tier| (tier, i)))
            .collect();
        tiered.sort();
        for (_, i) in tiered {
            let v = &values[i];
            let value = match &op.value {
                OperatorValue::Enum { .. } => FilterValue::Enum(v.value.clone()),
                _ => FilterValue::EnumList(vec![v.value.clone()]),
            };
            rows.push(item(
                format!("{}:{}:only:{}", field.key, op.key, v.value),
                value_row_label(&field.label, &op_label, &v.label, false, &t18),
                data(field, Some(op), Some(value)),
            ));
        }
    }
    rows
}

/// A field's value rows, at most three, then its "More" row when it has
/// more (FR29). The "More" row opens the editor of the operator of the
/// first row not shown, with the typed text.
pub fn cap_value_rows(
    field: &PowerSearchField,
    rows: Vec<PowerSearchItem>,
    query: &str,
    t18: &I18n,
) -> Vec<PowerSearchItem> {
    if rows.len() <= VALUE_ROWS_PER_FIELD {
        return rows;
    }
    let first_hidden = rows[VALUE_ROWS_PER_FIELD].data.operator_key.clone();
    let mut shown: Vec<PowerSearchItem> = rows.into_iter().take(VALUE_ROWS_PER_FIELD).collect();
    shown.push(PowerSearchItem {
        id: format!("__more__:{}", field.key).into(),
        label: t18.format(
            "@astryx.powersearch.moreMatches",
            &[
                ("field", field.label.as_ref().into()),
                ("query", query.into()),
            ],
        ),
        data: RowData {
            field_key: field.key.clone(),
            operator_key: first_hidden,
            filter_value: None,
            group: None,
            more_query: Some(query.to_string().into()),
            filter_index: None,
        },
    });
    shown
}

/// Whether a field answers a value-only query through a source (FR28): an
/// opted-in operator with a `searchSource`, which FR27 lets add rows.
pub fn is_source_field(field: &PowerSearchField, query: &str) -> bool {
    field_takes_value_rows(field, query)
        && field.operators.iter().any(|o| {
            o.value_only_match && o.value.search_source().is_some() && !takes_free_text(&o.value)
        })
}

/// The rows of a typed query that need no source (FR29): the content-search
/// row, the field and field-and-operator rows and field-prefixed values
/// (at most `max_results`), then each field's value-only rows — the
/// fields without a source in config order. Search-source fields' rows join
/// later, when their sources settle.
pub fn typed_rows(
    config: &PowerSearchConfig,
    query: &str,
    max_results: usize,
    cx: &gpui::App,
) -> Vec<PowerSearchItem> {
    let t18 = astryx_i18n::current(cx);
    let lower = query.to_lowercase();
    let lower = lower.trim();
    if lower.is_empty() {
        return field_rows(config);
    }
    let mut results: Vec<PowerSearchItem> = Vec::new();
    let mut seen: Vec<SharedString> = Vec::new();
    let mut push = |results: &mut Vec<PowerSearchItem>, row: PowerSearchItem| {
        if !seen.contains(&row.id) {
            seen.push(row.id.clone());
            results.push(row);
        }
    };
    for field in &config.fields {
        if field
            .typeahead_min_query_length
            .is_some_and(|min| lower.chars().count() < min)
        {
            continue;
        }
        let matches = field.label.to_lowercase().contains(lower)
            || field
                .typeahead_aliases
                .iter()
                .any(|a| a.to_lowercase().contains(lower));
        if matches {
            push(
                &mut results,
                item(
                    field.key.to_string(),
                    field.label.to_string(),
                    data(field, field.default_op(), None),
                ),
            );
        }
        for op in &field.operators {
            let combined = format!("{} {}", field.label, op.label_text(cx));
            if combined.to_lowercase().contains(lower) {
                push(
                    &mut results,
                    item(
                        format!("{}:{}", field.key, op.key),
                        combined,
                        data(field, Some(op), None),
                    ),
                );
            }
        }
    }
    // Field-prefixed values: "Title contains dune", "Genre fiction".
    for field in config
        .fields
        .iter()
        .filter(|f| f.value_match_allowed != Some(false))
    {
        let label = field.label.to_lowercase();
        let mut exact_operator = false;
        for op in &field.operators {
            let op_label = op.label_text(cx);
            let prefix = format!("{label} {} ", op_label.to_lowercase());
            if lower.starts_with(&prefix) && lower.len() > prefix.len() {
                let raw = &query.trim_start()[prefix.len()..];
                let values = prefixed_values(op, raw);
                exact_operator |= !values.is_empty();
                for (shown, value, quoted) in values {
                    push(
                        &mut results,
                        item(
                            format!("{}:{}:value:{shown}", field.key, op.key),
                            value_row_label(&field.label, &op_label, &shown, quoted, &t18),
                            data(field, Some(op), Some(value)),
                        ),
                    );
                }
            }
        }
        let field_prefix = format!("{label} ");
        if !exact_operator && lower.starts_with(&field_prefix) && lower.len() > field_prefix.len() {
            let remainder = &lower[field_prefix.len()..];
            let operator_prefix = field
                .operators
                .iter()
                .any(|o| o.label_text(cx).to_lowercase().starts_with(remainder));
            if !operator_prefix {
                let raw = &query.trim_start()[field_prefix.len()..];
                for op in &field.operators {
                    let op_label = op.label_text(cx);
                    for (shown, value, quoted) in prefixed_values(op, raw) {
                        push(
                            &mut results,
                            item(
                                format!("{}:{}:value:{shown}", field.key, op.key),
                                value_row_label(&field.label, &op_label, &shown, quoted, &t18),
                                data(field, Some(op), Some(value)),
                            ),
                        );
                    }
                }
            }
        }
    }
    // The content search leads, unless a field or field-and-operator is
    // typed exactly.
    let exact = config.fields.iter().any(|f| {
        f.label.to_lowercase() == lower
            || f.operators
                .iter()
                .any(|o| format!("{} {}", f.label, o.label_text(cx)).to_lowercase() == lower)
    });
    let mut content_default: Option<(SharedString, SharedString)> = None;
    if let Some(key) = &config.content_search_field_key
        && !exact
        && let Some(field) = config.find_field(key)
        && let Some(op) = field.default_op()
    {
        content_default = Some((field.key.clone(), op.key.clone()));
        results.insert(
            0,
            item(
                format!("__content_search__:{query}"),
                format!("\"{query}\""),
                data(
                    field,
                    Some(op),
                    Some(FilterValue::String(query.to_string().into())),
                ),
            ),
        );
    }
    results.truncate(max_results);
    // FR27, FR29: value-only matches, uncounted, field by field.
    if is_value_only(config, query) {
        let content = content_default
            .as_ref()
            .map(|(f, o)| (f.as_ref(), o.as_ref()));
        for field in config.fields.iter().filter(|f| !is_source_field(f, query)) {
            let rows = value_only_rows(field, query, content, cx);
            for row in cap_value_rows(field, rows, query, &t18) {
                if !results.iter().any(|r| r.id == row.id) {
                    results.push(row);
                }
            }
        }
    }
    results
}

/// The suggestions Add filter's content search lists (FR13, FR30): the
/// rows of [`typed_rows`] whose field the config has, then each
/// search-source field's rows that need no source and its "More" row in
/// place of its source rows. Nothing here waits for a source.
pub fn sheet_rows(
    config: &PowerSearchConfig,
    query: &str,
    max_results: usize,
    max_items: usize,
    cx: &gpui::App,
) -> Vec<PowerSearchItem> {
    let t18 = astryx_i18n::current(cx);
    let mut rows: Vec<PowerSearchItem> = typed_rows(config, query, max_results, cx)
        .into_iter()
        .filter(|r| config.find_field(&r.data.field_key).is_some())
        .collect();
    if query.trim().is_empty() || !is_value_only(config, query) {
        return rows;
    }
    let content = content_default(config, query, cx);
    let content = content.as_ref().map(|(f, o)| (f.as_ref(), o.as_ref()));
    for field in config.fields.iter().filter(|f| is_source_field(f, query)) {
        let own = value_only_rows_with(field, query, content, &[], max_items, cx);
        let mut shown: Vec<PowerSearchItem> = own.into_iter().take(VALUE_ROWS_PER_FIELD).collect();
        let source_op = field
            .operators
            .iter()
            .find(|o| {
                o.value_only_match
                    && o.value.search_source().is_some()
                    && !takes_free_text(&o.value)
            })
            .map(|o| o.key.clone());
        shown.push(PowerSearchItem {
            id: format!("__more__:{}", field.key).into(),
            label: t18.format(
                "@astryx.powersearch.moreMatches",
                &[
                    ("field", field.label.as_ref().into()),
                    ("query", query.into()),
                ],
            ),
            data: RowData {
                field_key: field.key.clone(),
                operator_key: source_op,
                filter_value: None,
                group: None,
                more_query: Some(query.to_string().into()),
                filter_index: None,
            },
        });
        for row in shown {
            if !rows.iter().any(|r| r.id == row.id) {
                rows.push(row);
            }
        }
    }
    rows
}

/// The content-search field's default operator while the content-search row
/// shows for `query` (FR27: it adds no value-only row then).
pub fn content_default(
    config: &PowerSearchConfig,
    query: &str,
    cx: &gpui::App,
) -> Option<(SharedString, SharedString)> {
    let lower = query.trim().to_lowercase();
    let exact = config.fields.iter().any(|f| {
        f.label.to_lowercase() == lower
            || f.operators
                .iter()
                .any(|o| format!("{} {}", f.label, o.label_text(cx)).to_lowercase() == lower)
    });
    let key = config.content_search_field_key.as_ref()?;
    let field = config.find_field(key)?;
    let op = field.default_op()?;
    (!exact && !lower.is_empty()).then(|| (field.key.clone(), op.key.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power_search::model::{
        EnumItem, PowerSearchConfig, PowerSearchField, PowerSearchOperator,
    };

    fn config() -> PowerSearchConfig {
        let genres = [
            "Fiction",
            "Science fiction",
            "Non-fiction",
            "Fictional essays",
            "History",
        ]
        .map(|g| EnumItem::new(g.to_lowercase().replace(' ', "_"), g));
        PowerSearchConfig::new("books")
            .content_search_field("text")
            .field(
                PowerSearchField::new("text", "Text").operator(PowerSearchOperator::new(
                    "contains",
                    "contains",
                    OperatorValue::string(),
                )),
            )
            .field(
                PowerSearchField::new("title", "Title")
                    .operator(
                        PowerSearchOperator::new("contains", "contains", OperatorValue::string())
                            .value_only_match(true),
                    )
                    .operator(PowerSearchOperator::new(
                        "is",
                        "is",
                        OperatorValue::string(),
                    )),
            )
            .field(
                PowerSearchField::new("genre", "Genre")
                    .group("Catalog")
                    .operator(
                        PowerSearchOperator::new(
                            "is",
                            "is",
                            OperatorValue::Enum {
                                values: genres.to_vec(),
                            },
                        )
                        .value_only_match(true),
                    ),
            )
    }

    fn labels(rows: &[PowerSearchItem]) -> Vec<String> {
        rows.iter().map(|r| r.label.to_string()).collect()
    }

    #[gpui::test]
    fn an_empty_query_lists_the_fields_ungrouped_first(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            assert_eq!(
                labels(&typed_rows(&config(), "", 10, cx)),
                ["Text", "Title", "Genre"]
            );
        });
    }

    #[gpui::test]
    fn a_field_query_lists_fields_operators_and_prefixed_values(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let rows = typed_rows(&config(), "title", 10, cx);
            assert_eq!(
                labels(&rows),
                ["Title", "Title contains", "Title is"],
                "an exact field: no content search"
            );
            let rows = typed_rows(&config(), "Title contains dune", 10, cx);
            assert_eq!(
                labels(&rows),
                ["\"Title contains dune\"", "Title contains \"dune\""]
            );
            assert_eq!(
                rows[1].data.filter_value,
                Some(FilterValue::String("dune".into()))
            );
            let rows = typed_rows(&config(), "genre hist", 10, cx);
            assert!(labels(&rows).contains(&"Genre is History".to_string()));
        });
    }

    #[gpui::test]
    fn fr27_fr29_value_only_queries_list_matches_after_the_rest(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let rows = typed_rows(&config(), "fiction", 10, cx);
            assert_eq!(
                labels(&rows),
                [
                    "\"fiction\"",
                    "Title contains \"fiction\"",
                    "Genre is Fiction",
                    "Genre is Fictional essays",
                    "Genre is Science fiction",
                    "More Genre matching 'fiction'",
                ],
                "content search first; whole label, then its start, then a later word; three per field"
            );
            let more = rows.last().unwrap();
            assert_eq!(more.data.more_query.as_deref(), Some("fiction"));
            assert_eq!(more.data.operator_key.as_deref(), Some("is"));
            assert!(typed_rows(&config(), "f", 10, cx).iter().all(|r| r.data.more_query.is_none()), "two characters at least");
        });
    }

    #[test]
    fn word_starts_match_after_folding() {
        assert_eq!(word_match_tier("Non-fiction", "fiction"), Some(2));
        assert_eq!(word_match_tier("Café", "cafe"), Some(0));
        assert_eq!(word_match_tier("Nonfiction", "fiction"), None);
        assert_eq!(word_match_tier("Łódź", "lod"), Some(1));
    }
}
