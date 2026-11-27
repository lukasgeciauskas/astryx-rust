//! Sticky columns (upstream `useTableStickyColumns`): contiguous runs of
//! columns pinned to the start and end edges while the table scrolls
//! sideways, with a shadow where content passes under them.
//!
//! GPUI has no `position: sticky`: a pinned cell follows the scroll
//! region's offset and paints over the cells that scroll under it (see
//! `table.rs`'s `pin_cell`).

use gpui::{App, SharedString, Window};

use super::model::{TableColumn, TableItem};
use super::plugin::{
    BodyCellProps, CellPin, HeaderCellProps, PinEdge, TablePlugin, TableRowContext,
};
use crate::doc::{ComponentDoc, PropMapping, RustApi};

/// The plugin (upstream `useTableStickyColumns`). With neither run it pins
/// nothing.
#[derive(Clone, Debug, Default)]
pub struct TableStickyColumns {
    start_keys: Vec<SharedString>,
    end_keys: Vec<SharedString>,
}

impl TableStickyColumns {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `startKeys`: the run from the first column through the last
    /// of these is pinned to the start edge.
    pub fn start_keys(mut self, keys: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.start_keys = keys.into_iter().map(Into::into).collect();
        self
    }

    /// Upstream `endKeys`: the run from the first of these through the last
    /// column is pinned to the end edge.
    pub fn end_keys(mut self, keys: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.end_keys = keys.into_iter().map(Into::into).collect();
        self
    }

    /// The pinned runs as column counts; a column in both runs pins to the
    /// start (upstream `countPinnedRuns`).
    fn runs<T>(&self, columns: &[TableColumn<T>]) -> (usize, usize) {
        let start = columns
            .iter()
            .rposition(|c| self.start_keys.contains(&c.key))
            .map_or(0, |i| i + 1);
        let end = columns
            .iter()
            .position(|c| self.end_keys.contains(&c.key))
            .map_or(0, |first| columns.len() - first.max(start));
        (start, end)
    }

    /// How the column at `index` is pinned.
    pub(crate) fn pin<T>(&self, index: usize, columns: &[TableColumn<T>]) -> Option<CellPin> {
        let (start, end) = self.runs(columns);
        if index < start {
            return Some(CellPin {
                edge: PinEdge::Start,
                boundary: index + 1 == start,
            });
        }
        let first_end = columns.len() - end;
        (end > 0 && index >= first_end).then_some(CellPin {
            edge: PinEdge::End,
            boundary: index == first_end,
        })
    }
}

/// Upstream `useTableStickyColumns`.
pub fn use_table_sticky_columns(
    start_keys: impl IntoIterator<Item = impl Into<SharedString>>,
    end_keys: impl IntoIterator<Item = impl Into<SharedString>>,
) -> TableStickyColumns {
    TableStickyColumns::new()
        .start_keys(start_keys)
        .end_keys(end_keys)
}

impl<T> TablePlugin<T> for TableStickyColumns {
    /// Pinned runs mark their overflow with a shadow, so their edges do not fade
    /// (upstream `scrollEdgeFadeOff`).
    fn transform_scroll_wrapper(
        &self,
        mut props: super::plugin::ScrollWrapperProps,
        _context: &super::plugin::TableRenderContext,
        _window: &mut Window,
        _cx: &mut App,
    ) -> super::plugin::ScrollWrapperProps {
        props.fade_off_start |= !self.start_keys.is_empty();
        props.fade_off_end |= !self.end_keys.is_empty();
        props
    }

    fn transform_header_cell(
        &self,
        mut props: HeaderCellProps,
        _: &TableColumn<T>,
        columns: &[TableColumn<T>],
        _: &mut Window,
        _: &mut App,
    ) -> HeaderCellProps {
        if let Some(pin) = self.pin(props.column_index, columns) {
            props.pinned = Some(pin);
        }
        props
    }

    fn transform_body_cell(
        &self,
        mut props: BodyCellProps,
        _: &TableColumn<T>,
        _: &TableItem<T>,
        columns: &[TableColumn<T>],
        _: &TableRowContext,
        _: &mut Window,
        _: &mut App,
    ) -> BodyCellProps {
        if let Some(pin) = self.pin(props.column_index, columns) {
            props.pinned = Some(pin);
        }
        props
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_STICKY_COLUMNS_DOC: ComponentDoc = ComponentDoc {
    name: "useTableStickyColumns",
    rust_type: "rustryx::components::table::TableStickyColumns",
    props: &[
        p("startKeys", RustApi::Method("start_keys")),
        p("endKeys", RustApi::Method("end_keys")),
    ],
    example: include_str!("examples.rs"),
};
