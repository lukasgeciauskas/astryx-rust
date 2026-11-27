//! The row-number column (upstream `useTableRowIndex`): a right-aligned,
//! monospaced number before every row, 1-based by default.

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, FontFeatures, IntoElement, ParentElement, SharedString, Styled, Window, div,
};
use rustryx_theme::{ThemeStyled, current};

use super::model::{TableColumn, TableColumnAlign, TableItem, pixel};
use super::plugin::{BodyCellProps, RowModel, TablePlugin, TableRowContext};
use crate::doc::{ComponentDoc, PropMapping, RustApi};

/// The index column's key.
pub const ROW_INDEX_COLUMN_KEY: &str = "__rowIndex";

type Lookup<T> = Rc<dyn Fn(&TableItem<T>) -> Option<usize>>;
type LabelFn = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

#[derive(Clone)]
enum IndexLabel {
    Text(SharedString),
    Element(LabelFn),
}

/// The plugin (upstream `useTableRowIndex`): numbers follow the rows the
/// table renders (its current sort, filter or page), or the order of
/// `data` when given.
pub struct TableRowIndex<T> {
    lookup: Option<Lookup<T>>,
    label: IndexLabel,
    start_from: i64,
    /// The table's position for each row, as the body transform sees it.
    positions: Rc<RefCell<HashMap<usize, usize>>>,
}

impl<T: 'static> Default for TableRowIndex<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: 'static> TableRowIndex<T> {
    /// Numbers from the table's row positions, from 1, under "#".
    pub fn new() -> Self {
        Self {
            lookup: None,
            label: IndexLabel::Text("#".into()),
            start_from: 1,
            positions: Rc::default(),
        }
    }

    /// Upstream `data` without `getRowKey`: numbers follow this order, a
    /// row matching its item (the same row of the same data, else an equal
    /// one; upstream matches by reference).
    pub fn data(mut self, data: impl Into<Rc<[T]>>) -> Self
    where
        T: PartialEq,
    {
        let data: Rc<[T]> = data.into();
        self.lookup = Some(Rc::new(move |item| {
            if Rc::ptr_eq(item.data(), &data) {
                Some(item.index())
            } else {
                data.iter().rposition(|d| d == &**item)
            }
        }));
        self
    }

    /// Upstream `data` with `getRowKey`: rows match by their unique key.
    pub fn data_keyed(
        mut self,
        data: impl Into<Rc<[T]>>,
        get_row_key: impl Fn(&T) -> SharedString + 'static,
    ) -> Self {
        let data: Rc<[T]> = data.into();
        let map: OnceCell<HashMap<SharedString, usize>> = OnceCell::new();
        self.lookup = Some(Rc::new(move |item| {
            map.get_or_init(|| {
                data.iter()
                    .enumerate()
                    .map(|(i, d)| (get_row_key(d), i))
                    .collect()
            })
            .get(&get_row_key(item))
            .copied()
        }));
        self
    }

    /// Upstream `label` as text (`#` by default).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = IndexLabel::Text(label.into());
        self
    }

    /// Upstream `label` as any element.
    pub fn label_element(
        mut self,
        label: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.label = IndexLabel::Element(Rc::new(label));
        self
    }

    /// Upstream `startFrom`: the first number (1 by default).
    pub fn start_from(mut self, start: i64) -> Self {
        self.start_from = start;
        self
    }
}

impl<T> TableRowIndex<T> {
    /// The number a row shows, if it has one.
    #[cfg(test)]
    pub(crate) fn number_for(&self, item: &TableItem<T>) -> Option<i64> {
        number(&self.lookup, &self.positions, self.start_from, item)
    }
}

fn number<T>(
    lookup: &Option<Lookup<T>>,
    positions: &RefCell<HashMap<usize, usize>>,
    start: i64,
    item: &TableItem<T>,
) -> Option<i64> {
    let index = match lookup {
        Some(lookup) => lookup(item),
        None => positions.borrow().get(&item.index()).copied(),
    };
    index.map(|i| i as i64 + start)
}

/// Upstream `useTableRowIndex` with its defaults.
pub fn use_table_row_index<T: 'static>() -> TableRowIndex<T> {
    TableRowIndex::new()
}

fn index_text(n: i64, window: &mut Window, cx: &mut App) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    div()
        .text_style(&t.text.supporting)
        .font_stack(&t.font.family.code)
        .font_features(FontFeatures(Arc::new(vec![("tnum".into(), 1)])))
        .text_color(t.color.text_secondary)
        .child(n.to_string())
        .into_any_element()
}

impl<T: 'static> TablePlugin<T> for TableRowIndex<T> {
    fn row_model(&self) -> Option<RowModel> {
        Some(RowModel::Flat)
    }

    fn transforms_body(&self) -> bool {
        true
    }

    fn transform_columns(
        &self,
        mut columns: Vec<TableColumn<T>>,
        _: &mut Window,
        _: &mut App,
    ) -> Vec<TableColumn<T>> {
        let (lookup, positions, start) =
            (self.lookup.clone(), self.positions.clone(), self.start_from);
        let column = TableColumn::new(ROW_INDEX_COLUMN_KEY)
            .width(pixel(48.))
            .align(TableColumnAlign::End)
            .resizable(false)
            .render_cell_item(move |item, window, cx| {
                match number(&lookup, &positions, start, item) {
                    Some(n) => index_text(n, window, cx),
                    None => div().into_any_element(),
                }
            });
        // Only a count from 1 keeps the rows windowed (upstream).
        let column = if self.start_from == 1 {
            column.fixed_height()
        } else {
            column
        };
        let column = match &self.label {
            IndexLabel::Text(text) => column.header(text.clone()),
            IndexLabel::Element(f) => {
                let f = f.clone();
                column.header_element(move |window, cx| f(window, cx))
            }
        };
        columns.insert(0, column);
        columns
    }

    fn transform_body_cell(
        &self,
        props: BodyCellProps,
        column: &TableColumn<T>,
        item: &TableItem<T>,
        _: &[TableColumn<T>],
        context: &TableRowContext,
        _: &mut Window,
        _: &mut App,
    ) -> BodyCellProps {
        // Without data, the number is the row's place in the complete data.
        if self.lookup.is_none() && column.key.as_ref() == ROW_INDEX_COLUMN_KEY {
            self.positions
                .borrow_mut()
                .insert(item.index(), context.logical_index);
        }
        props
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_ROW_INDEX_DOC: ComponentDoc = ComponentDoc {
    name: "useTableRowIndex",
    rust_type: "rustryx::components::table::TableRowIndex",
    props: &[
        p("data", RustApi::Method("data / data_keyed")),
        p(
            "getRowKey",
            RustApi::Method("data_keyed (its key function)"),
        ),
        p("label", RustApi::Method("label / label_element")),
        p("startFrom", RustApi::Method("start_from")),
    ],
    example: include_str!("examples.rs"),
};
