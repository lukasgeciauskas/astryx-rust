//! The table's data model (upstream `types.ts`, `columnUtils.ts` and
//! `tableIdentity.ts`): cell values and records, column definitions and
//! their widths, row identity, the default cell text and generated columns.

use std::rc::Rc;

use gpui::{AbsoluteLength, AnyElement, App, GridTrack, SharedString, Window, px};

pub use astryx_primitives::ListWindowKey as RowKey;

// ---- Values and records ----------------------------------------------------

/// A cell's value: what upstream reads as `item[key]`.
#[derive(Clone, Debug, PartialEq)]
pub enum TableValue {
    /// `null` or `undefined`: an empty cell.
    Null,
    Text(SharedString),
    Number(f64),
    Bool(bool),
    /// A `Date` as milliseconds since the Unix epoch; `None` is an Invalid
    /// Date, which Table treats as missing (FR25).
    Date(Option<i64>),
}

impl TableValue {
    /// Whether the value is missing (FR25): null, or an Invalid Date.
    pub fn is_missing(&self) -> bool {
        matches!(self, TableValue::Null | TableValue::Date(None))
    }

    /// The default cell text (upstream `defaultCellRenderer`): a missing
    /// value is empty, a date is its ISO text, a number and a boolean are
    /// written as JavaScript writes them.
    pub fn display_text(&self) -> SharedString {
        match self {
            TableValue::Null | TableValue::Date(None) => SharedString::default(),
            TableValue::Text(t) => t.clone(),
            TableValue::Number(n) => js_number(*n).into(),
            TableValue::Bool(b) => if *b { "true" } else { "false" }.into(),
            TableValue::Date(Some(ms)) => iso_date(*ms).into(),
        }
    }
}

impl From<&str> for TableValue {
    fn from(s: &str) -> Self {
        TableValue::Text(SharedString::from(s.to_string()))
    }
}

impl From<String> for TableValue {
    fn from(s: String) -> Self {
        TableValue::Text(s.into())
    }
}

impl From<SharedString> for TableValue {
    fn from(s: SharedString) -> Self {
        TableValue::Text(s)
    }
}

macro_rules! number_value {
    ($($t:ty),*) => {$(
        impl From<$t> for TableValue {
            fn from(n: $t) -> Self {
                TableValue::Number(n as f64)
            }
        }
    )*};
}
number_value!(f64, f32, i64, i32, u32, u64, usize);

impl From<bool> for TableValue {
    fn from(b: bool) -> Self {
        TableValue::Bool(b)
    }
}

impl<V: Into<TableValue>> From<Option<V>> for TableValue {
    fn from(v: Option<V>) -> Self {
        v.map_or(TableValue::Null, Into::into)
    }
}

/// A number as JavaScript's `String(n)` writes it.
pub(crate) fn js_number(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0. { "Infinity" } else { "-Infinity" }.into();
    }
    if n == 0. {
        return "0".into();
    }
    let a = n.abs();
    if (1e-6..1e21).contains(&a) {
        if n.fract() == 0. {
            format!("{}", n as i128)
        } else {
            format!("{n}")
        }
    } else {
        // Exponential form: 1e+21, 1.5e-7.
        let s = format!("{n:e}");
        match s.split_once('e') {
            Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
            _ => s,
        }
    }
}

/// A Unix time in milliseconds as `Date.toISOString()` writes it.
pub(crate) fn iso_date(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    let (h, mi, s, milli) = (
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1000 % 60,
        rem % 1000,
    );
    let year = if (0..=9999).contains(&y) {
        format!("{y:04}")
    } else if y < 0 {
        format!("-{:06}", -y)
    } else {
        format!("+{y:06}")
    };
    format!("{year}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{milli:03}Z")
}

/// A table row: the values a Table reads from it by column key (upstream's
/// `T extends Record<string, unknown>`).
pub trait TableRecord: 'static {
    /// The value at `key`, as upstream reads `item[key]`.
    fn value(&self, key: &str) -> TableValue;

    /// Its keys in order, for columns generated from the data (upstream
    /// `Object.keys` of the first item). None by default.
    fn keys(&self) -> Vec<SharedString> {
        Vec::new()
    }
}

/// A row of named values, for tables over loose data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record(Vec<(SharedString, TableValue)>);

impl Record {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `key` to `value`, replacing an earlier value.
    pub fn with(mut self, key: impl Into<SharedString>, value: impl Into<TableValue>) -> Self {
        let (key, value) = (key.into(), value.into());
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
        self
    }

    pub fn get(&self, key: &str) -> Option<&TableValue> {
        self.0
            .iter()
            .find(|(k, _)| k.as_ref() == key)
            .map(|(_, v)| v)
    }
}

impl TableRecord for Record {
    fn value(&self, key: &str) -> TableValue {
        self.get(key).cloned().unwrap_or(TableValue::Null)
    }

    fn keys(&self) -> Vec<SharedString> {
        self.0.iter().map(|(k, _)| k.clone()).collect()
    }
}

// ---- Columns ---------------------------------------------------------------

/// The default floor (px) of an explicit `proportional()` column.
pub const DEFAULT_MIN_COLUMN_WIDTH: f32 = 120.;
/// The compact floor (px) of a flexible column with no width (FR13, DEC-2).
pub const DEFAULT_FLEXIBLE_COLUMN_MIN_WIDTH: f32 = 60.;

/// A column's width: a share of the free space, or a fixed length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColumnWidth {
    /// Shares the space with other flexible columns by `value`, never
    /// narrower than `min_width`.
    Proportional { value: f32, min_width: Option<f32> },
    /// A fixed width.
    Pixel(f32),
}

/// A flexible width, `value` shares of the space, with the 120px floor
/// (upstream `proportional(value)`).
pub fn proportional(value: f32) -> ColumnWidth {
    ColumnWidth::Proportional {
        value,
        min_width: Some(DEFAULT_MIN_COLUMN_WIDTH),
    }
}

/// A flexible width with its own floor (`proportional(value, {minWidth})`).
pub fn proportional_min(value: f32, min_width: f32) -> ColumnWidth {
    ColumnWidth::Proportional {
        value,
        min_width: Some(min_width),
    }
}

/// A fixed width in pixels (upstream `pixel(value)`).
pub fn pixel(value: f32) -> ColumnWidth {
    ColumnWidth::Pixel(value)
}

/// A column's horizontal alignment, on its header and its cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableColumnAlign {
    #[default]
    Start,
    Center,
    End,
}

/// A column's sort configuration (upstream `sortable`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SortableColumn {
    /// The key it sorts by; the column's key when absent.
    pub sort_key: Option<SharedString>,
}

/// A column's filter: a PowerSearch field, with its default operator or a
/// given one (upstream `filter`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableFilterRef {
    Field(SharedString),
    FieldOperator {
        field: SharedString,
        operator: SharedString,
    },
}

pub(crate) type CellFn<T> = Rc<dyn Fn(&TableItem<T>, &mut Window, &mut App) -> AnyElement>;
/// Decorates a cell's content, its renderer's or the default text (a tree's
/// indent and expander).
pub(crate) type CellWrapFn<T> =
    Rc<dyn Fn(&TableItem<T>, AnyElement, &mut Window, &mut App) -> AnyElement>;

/// One row of a table's data, as a handle a callback can keep (upstream
/// hands the item object itself to cells and plugins). It reads as the row
/// (`Deref`) and keeps the table's data alive.
pub struct TableItem<T> {
    data: Rc<[T]>,
    index: usize,
}

impl<T> Clone for TableItem<T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            index: self.index,
        }
    }
}

impl<T> TableItem<T> {
    /// The row at `index` of `data`.
    pub fn new(data: Rc<[T]>, index: usize) -> Self {
        assert!(
            index < data.len(),
            "TableItem: index {index} is out of range"
        );
        Self { data, index }
    }

    /// Its position in the data the table renders.
    pub fn index(&self) -> usize {
        self.index
    }

    /// The data it belongs to.
    pub fn data(&self) -> &Rc<[T]> {
        &self.data
    }
}

impl<T> std::ops::Deref for TableItem<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.data[self.index]
    }
}
pub(crate) type HeaderFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// A column's header: text, or any element.
#[derive(Clone)]
pub enum ColumnHeader {
    Text(SharedString),
    Element(HeaderFn),
}

/// A column of a data-driven table (upstream `TableColumn<T>`).
pub struct TableColumn<T> {
    pub key: SharedString,
    pub(crate) header: Option<ColumnHeader>,
    pub width: Option<ColumnWidth>,
    pub align: Option<TableColumnAlign>,
    pub sortable: Option<SortableColumn>,
    /// Whether its header border resizes it (upstream `resizable`, true by
    /// default).
    pub resizable: bool,
    pub filter: Option<TableFilterRef>,
    pub(crate) render_cell: Option<CellFn<T>>,
    pub(crate) wrappers: Vec<CellWrapFn<T>>,
}

impl<T> Clone for TableColumn<T> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            header: self.header.clone(),
            width: self.width,
            align: self.align,
            sortable: self.sortable.clone(),
            resizable: self.resizable,
            filter: self.filter.clone(),
            render_cell: self.render_cell.clone(),
            wrappers: self.wrappers.clone(),
        }
    }
}

impl<T> TableColumn<T> {
    /// A column reading `key`; its header defaults to the key, capitalized.
    pub fn new(key: impl Into<SharedString>) -> Self {
        Self {
            key: key.into(),
            header: None,
            width: None,
            align: None,
            sortable: None,
            resizable: true,
            filter: None,
            render_cell: None,
            wrappers: Vec::new(),
        }
    }

    /// Upstream `header` as text.
    pub fn header(mut self, header: impl Into<SharedString>) -> Self {
        self.header = Some(ColumnHeader::Text(header.into()));
        self
    }

    /// Upstream `header` as any element.
    pub fn header_element(
        mut self,
        header: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.header = Some(ColumnHeader::Element(Rc::new(header)));
        self
    }

    /// Upstream `width`.
    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.width = Some(width);
        self
    }

    /// Upstream `align`.
    pub fn align(mut self, align: TableColumnAlign) -> Self {
        self.align = Some(align);
        self
    }

    /// Upstream `sortable: true`.
    pub fn sortable(mut self, sortable: bool) -> Self {
        self.sortable = sortable.then(SortableColumn::default);
        self
    }

    /// Upstream `sortable: {sortKey}`.
    pub fn sort_key(mut self, key: impl Into<SharedString>) -> Self {
        self.sortable = Some(SortableColumn {
            sort_key: Some(key.into()),
        });
        self
    }

    /// Upstream `resizable`.
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Upstream `filter`.
    pub fn filter(mut self, filter: TableFilterRef) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Upstream `renderCell`.
    pub fn render_cell(
        mut self,
        render: impl Fn(&T, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_cell = Some(Rc::new(move |item: &TableItem<T>, window, cx| {
            render(item, window, cx)
        }));
        self
    }

    /// Upstream `renderCell` with the row as a handle a callback can keep
    /// (a button in the cell that acts on its row).
    pub fn render_cell_item(
        mut self,
        render: impl Fn(&TableItem<T>, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.render_cell = Some(Rc::new(render));
        self
    }

    /// Whether it uses the default renderer.
    pub fn has_default_renderer(&self) -> bool {
        self.render_cell.is_none()
    }

    /// The header's text: its text header, else the key (a header element
    /// has none).
    pub fn header_text(&self) -> Option<SharedString> {
        match &self.header {
            Some(ColumnHeader::Text(t)) => Some(t.clone()),
            Some(ColumnHeader::Element(_)) => None,
            None => Some(self.key.clone()),
        }
    }

    /// The key it sorts by, when it is sortable.
    pub fn sort_key_or_key(&self) -> Option<SharedString> {
        self.sortable
            .as_ref()
            .map(|s| s.sort_key.clone().unwrap_or_else(|| self.key.clone()))
    }
}

/// The floor of a flexible column: its own `min_width`, else 120px for an
/// explicit `proportional()`, else 60px with no width.
fn flexible_floor(width: Option<ColumnWidth>) -> f32 {
    match width {
        Some(ColumnWidth::Proportional {
            min_width: Some(m), ..
        }) => m,
        Some(ColumnWidth::Proportional { .. }) => DEFAULT_MIN_COLUMN_WIDTH,
        _ => DEFAULT_FLEXIBLE_COLUMN_MIN_WIDTH,
    }
}

struct SpacePlan {
    total_proportion: f32,
    pixel_total: f32,
    max_proportional_space: f32,
}

fn plan_space<T>(columns: &[TableColumn<T>]) -> SpacePlan {
    let (mut total_proportion, mut pixel_total) = (0., 0.);
    let mut flexible = Vec::new();
    for c in columns {
        match c.width {
            Some(ColumnWidth::Pixel(v)) => pixel_total += v,
            w => {
                let proportion = match w {
                    Some(ColumnWidth::Proportional { value, .. }) => value,
                    _ => 1.,
                };
                total_proportion += proportion;
                flexible.push((proportion, flexible_floor(w)));
            }
        }
    }
    let max_proportional_space = if total_proportion > 0. {
        flexible
            .iter()
            .map(|(p, floor)| floor * total_proportion / p)
            .fold(0., f32::max)
    } else {
        0.
    };
    SpacePlan {
        total_proportion,
        pixel_total,
        max_proportional_space,
    }
}

/// Each column's width while the table sits at its minimum width, the
/// width a horizontally overflowing table renders at (upstream
/// `resolveColumnFloorWidths`).
pub fn resolve_column_floor_widths<T>(columns: &[TableColumn<T>]) -> Vec<f32> {
    let plan = plan_space(columns);
    columns
        .iter()
        .map(|c| match c.width {
            Some(ColumnWidth::Pixel(v)) => v,
            w => {
                let proportion = match w {
                    Some(ColumnWidth::Proportional { value, .. }) => value,
                    _ => 1.,
                };
                if plan.total_proportion > 0. {
                    plan.max_proportional_space * proportion / plan.total_proportion
                } else {
                    0.
                }
            }
        })
        .collect()
}

/// The table's minimum width: the pixel columns, and the least space in
/// which every flexible column meets its floor (upstream `tableMinWidth`).
pub fn resolve_table_min_width<T>(columns: &[TableColumn<T>]) -> f32 {
    let plan = plan_space(columns);
    plan.pixel_total + plan.max_proportional_space
}

/// The grid tracks of a data-driven table's fixed layout: a pixel column
/// its length, a flexible one its share of the free space above its floor.
/// At or above the table's minimum width each flexible column gets exactly
/// its proportion, as upstream's percentage widths give it.
pub(crate) fn column_tracks<T>(columns: &[TableColumn<T>]) -> Vec<GridTrack> {
    columns
        .iter()
        .map(|c| match c.width {
            Some(ColumnWidth::Pixel(v)) => GridTrack::Length(AbsoluteLength::Pixels(px(v))),
            w => {
                let proportion = match w {
                    Some(ColumnWidth::Proportional { value, .. }) => value,
                    _ => 1.,
                };
                GridTrack::MinMax(
                    Box::new(GridTrack::Length(AbsoluteLength::Pixels(px(
                        flexible_floor(w),
                    )))),
                    Box::new(GridTrack::Fr(proportion)),
                )
            }
        })
        .collect()
}

/// `str` with its first letter capitalized (upstream `capitalize`).
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Columns from the keys of the first item, sized from the header and the
/// first five rows (upstream `generateColumns`).
pub fn generate_columns<T: TableRecord>(data: &[T]) -> Vec<TableColumn<T>> {
    let Some(first) = data.first() else {
        return Vec::new();
    };
    let sample = &data[..data.len().min(5)];
    let content_len = |v: &TableValue| match v {
        TableValue::Text(t) => t.chars().count(),
        TableValue::Number(_) | TableValue::Bool(_) => v.display_text().chars().count(),
        _ => 0,
    };
    let longest_word = |v: &TableValue| match v {
        TableValue::Text(_) | TableValue::Number(_) | TableValue::Bool(_) => v
            .display_text()
            .split([' ', '\t', '\n'])
            .map(|w| w.chars().count())
            .max()
            .unwrap_or(0),
        _ => 0,
    };
    const PX_PER_CHAR: f32 = 8.;
    first
        .keys()
        .into_iter()
        .map(|key| {
            let header = capitalize(&key);
            let header_len = header.chars().count();
            let (mut max_content, mut max_word) = (header_len, header_len);
            for row in sample {
                let v = row.value(&key);
                max_content = max_content.max(content_len(&v));
                max_word = max_word.max(longest_word(&v));
            }
            let proportion = if max_content <= 6 {
                1.
            } else if max_content <= 15 {
                2.
            } else {
                3.
            };
            let min_width = (max_word as f32 * PX_PER_CHAR).max(DEFAULT_FLEXIBLE_COLUMN_MIN_WIDTH);
            TableColumn::new(key)
                .header(header)
                .width(proportional_min(proportion, min_width))
        })
        .collect()
}

// ---- Row identity ----------------------------------------------------------

type KeyFn<T> = Rc<dyn Fn(&T) -> TableValue>;

/// A row's canonical identity (upstream `idKey`): a field, whose string or
/// finite number is projected to a string, or a function, whose string or
/// finite number keeps its type.
pub enum IdKey<T> {
    Field(SharedString),
    Fn(KeyFn<T>),
}

impl<T> IdKey<T> {
    /// A key read from the field `name`.
    pub fn field(name: impl Into<SharedString>) -> Self {
        IdKey::Field(name.into())
    }

    /// A key a function derives from the row.
    pub fn func(f: impl Fn(&T) -> TableValue + 'static) -> Self {
        IdKey::Fn(Rc::new(f))
    }

    /// The row's key as selection keeps it: the value as a string.
    pub fn string_key(&self, item: &T) -> SharedString
    where
        T: TableRecord,
    {
        let value = match self {
            IdKey::Field(f) => item.value(f),
            IdKey::Fn(f) => f(item),
        };
        match value {
            TableValue::Text(t) => t,
            other => other.display_text(),
        }
    }
}

impl<T> From<&'static str> for IdKey<T> {
    fn from(name: &'static str) -> Self {
        IdKey::Field(name.into())
    }
}

impl<T> From<SharedString> for IdKey<T> {
    fn from(name: SharedString) -> Self {
        IdKey::Field(name)
    }
}

impl<T> From<String> for IdKey<T> {
    fn from(name: String) -> Self {
        IdKey::Field(name.into())
    }
}

impl<T> Clone for IdKey<T> {
    fn clone(&self) -> Self {
        match self {
            IdKey::Field(f) => IdKey::Field(f.clone()),
            IdKey::Fn(f) => IdKey::Fn(f.clone()),
        }
    }
}

/// Whether a row's key is a usable identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityStatus {
    Valid,
    Missing,
    Invalid,
    Duplicate,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowIdentity {
    pub key: Option<RowKey>,
    pub status: IdentityStatus,
}

fn key_of(value: TableValue, keeps_type: bool) -> Option<RowKey> {
    match value {
        TableValue::Text(t) => Some(RowKey::Text(t)),
        TableValue::Number(n) if n.is_finite() => {
            Some(if keeps_type && n.fract() == 0. && n.abs() < 9.0e15 {
                RowKey::Number(n as i64)
            } else {
                RowKey::Text(js_number(n).into())
            })
        }
        _ => None,
    }
}

/// Each row's identity (upstream `resolveTableIdentity`). Without
/// canonical identity, a row's key is its `id_key` value or its index.
pub(crate) fn resolve_identity<T: TableRecord>(
    data: &[T],
    id_key: Option<&IdKey<T>>,
    canonical: bool,
) -> Vec<RowIdentity> {
    let raw = |item: &T| match id_key {
        Some(IdKey::Field(f)) => item.value(f),
        Some(IdKey::Fn(f)) => f(item),
        None => TableValue::Null,
    };
    let keeps_type = matches!(id_key, Some(IdKey::Fn(_)));
    if !canonical {
        return data
            .iter()
            .enumerate()
            .map(|(i, item)| RowIdentity {
                key: match id_key {
                    None => Some(RowKey::Number(i as i64)),
                    Some(_) => key_of(raw(item), keeps_type)
                        .or_else(|| Some(RowKey::Text(raw(item).display_text()))),
                },
                status: IdentityStatus::Valid,
            })
            .collect();
    }
    let mut rows: Vec<RowIdentity> = data
        .iter()
        .map(|item| match id_key {
            None => RowIdentity {
                key: None,
                status: IdentityStatus::Missing,
            },
            Some(_) => match key_of(raw(item), keeps_type) {
                Some(key) => RowIdentity {
                    key: Some(key),
                    status: IdentityStatus::Valid,
                },
                None => RowIdentity {
                    key: None,
                    status: IdentityStatus::Invalid,
                },
            },
        })
        .collect();
    let mut counts = std::collections::HashMap::new();
    for r in &rows {
        if let Some(k) = &r.key {
            *counts.entry(k.clone()).or_insert(0usize) += 1;
        }
    }
    for r in &mut rows {
        if r.key.as_ref().is_some_and(|k| counts[k] > 1) {
            r.status = IdentityStatus::Duplicate;
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(widths: &[Option<ColumnWidth>]) -> Vec<TableColumn<Record>> {
        widths
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let c = TableColumn::new(format!("c{i}"));
                match w {
                    Some(w) => c.width(*w),
                    None => c,
                }
            })
            .collect()
    }

    #[test]
    fn fr13_widthless_columns_share_equally_above_a_60px_floor() {
        let c = cols(&[None, None, None]);
        assert_eq!(resolve_table_min_width(&c), 180.);
        assert_eq!(resolve_column_floor_widths(&c), [60., 60., 60.]);
    }

    #[test]
    fn proportional_columns_keep_their_shares_at_the_minimum() {
        // proportional(2) beside a widthless column: the widthless one rides
        // along at a third of the space its neighbour's 120px floor needs.
        let c = cols(&[Some(proportional(2.)), None, Some(pixel(80.))]);
        // 120 * 3 / 2 = 180 of flexible space, plus 80.
        assert_eq!(resolve_table_min_width(&c), 260.);
        assert_eq!(resolve_column_floor_widths(&c), [120., 60., 80.]);
        let own = cols(&[Some(proportional_min(1., 200.)), Some(proportional(1.))]);
        assert_eq!(resolve_column_floor_widths(&own), [200., 200.]);
    }

    #[test]
    fn fr25_missing_values_render_empty() {
        assert_eq!(TableValue::Null.display_text(), "");
        assert_eq!(TableValue::Date(None).display_text(), "");
        assert!(TableValue::Date(None).is_missing());
        assert_eq!(
            TableValue::Date(Some(0)).display_text(),
            "1970-01-01T00:00:00.000Z"
        );
        assert_eq!(
            TableValue::Date(Some(1_700_000_000_123)).display_text(),
            "2023-11-14T22:13:20.123Z"
        );
        assert_eq!(
            TableValue::Date(Some(-86_400_000)).display_text(),
            "1969-12-31T00:00:00.000Z"
        );
    }

    #[test]
    fn numbers_and_booleans_read_as_javascript_writes_them() {
        assert_eq!(TableValue::from(30).display_text(), "30");
        assert_eq!(TableValue::from(1.5).display_text(), "1.5");
        assert_eq!(TableValue::from(-0.25).display_text(), "-0.25");
        assert_eq!(TableValue::from(1e21).display_text(), "1e+21");
        assert_eq!(TableValue::from(true).display_text(), "true");
        assert_eq!(js_number(f64::NAN), "NaN");
    }

    #[test]
    fn generated_columns_come_from_the_first_item() {
        let data = vec![
            Record::new()
                .with("id", 1)
                .with("name", "Ada Lovelace")
                .with("email", "ada@analytical.engine"),
            Record::new()
                .with("id", 2)
                .with("name", "Grace")
                .with("email", "grace@navy.mil"),
        ];
        let c = generate_columns(&data);
        assert_eq!(
            c.iter().map(|c| c.key.to_string()).collect::<Vec<_>>(),
            ["id", "name", "email"]
        );
        assert_eq!(c[0].header_text().as_deref(), Some("Id"));
        assert_eq!(c[0].width, Some(proportional_min(1., 60.)));
        assert_eq!(c[1].width, Some(proportional_min(2., 64.)));
        assert_eq!(c[2].width, Some(proportional_min(3., 168.)));
        assert!(generate_columns::<Record>(&[]).is_empty());
    }

    #[test]
    fn fr16_identity_resolves_strings_and_finite_numbers() {
        let data = vec![
            Record::new().with("id", "a"),
            Record::new().with("id", 2),
            Record::new().with("id", f64::NAN),
            Record::new(),
            Record::new().with("id", "a"),
        ];
        let field = IdKey::Field("id".into());
        let ids = resolve_identity(&data, Some(&field), true);
        let statuses: Vec<_> = ids.iter().map(|r| r.status).collect();
        assert_eq!(
            statuses,
            [
                IdentityStatus::Duplicate,
                IdentityStatus::Valid,
                IdentityStatus::Invalid,
                IdentityStatus::Invalid,
                IdentityStatus::Duplicate
            ]
        );
        // A field projects a number to a string; a function keeps its type.
        assert_eq!(ids[1].key, Some(RowKey::Text("2".into())));
        let f: IdKey<Record> = IdKey::Fn(Rc::new(|r: &Record| r.value("id")));
        assert_eq!(
            resolve_identity(&data, Some(&f), true)[1].key,
            Some(RowKey::Number(2))
        );
        // Without canonical identity, the index stands in.
        assert_eq!(
            resolve_identity(&data, None, false)[3].key,
            Some(RowKey::Number(3))
        );
        assert_eq!(
            resolve_identity(&data, None, true)[0].status,
            IdentityStatus::Missing
        );
    }

    #[test]
    fn tracks_follow_the_widths() {
        let c = cols(&[Some(pixel(80.)), Some(proportional(2.)), None]);
        let t = column_tracks(&c);
        assert_eq!(t[0], GridTrack::Length(AbsoluteLength::Pixels(px(80.))));
        assert_eq!(
            t[1],
            GridTrack::MinMax(
                Box::new(GridTrack::Length(AbsoluteLength::Pixels(px(120.)))),
                Box::new(GridTrack::Fr(2.))
            )
        );
        assert_eq!(
            t[2],
            GridTrack::MinMax(
                Box::new(GridTrack::Length(AbsoluteLength::Pixels(px(60.)))),
                Box::new(GridTrack::Fr(1.))
            )
        );
    }
}
