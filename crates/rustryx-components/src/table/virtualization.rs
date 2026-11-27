//! Virtualization (upstream `useTableRowsVirtualization` and
//! `useTableColumnsVirtualization`): geometry the table windows with. Rows
//! window on `rustryx_primitives::list_window` inside the table's own block
//! viewport; columns mount only near the inline viewport. Whatever the table
//! cannot window safely renders in full.

use crate::doc::{ComponentDoc, PropMapping, RustApi};

/// Fixed-pitch row windowing (upstream `UseTableRowsVirtualizationConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowsVirtualization {
    /// The body row's pitch, padding and dividers included; rows never
    /// measure shorter (taller equal rows become the pitch, FR17).
    pub row_height: f32,
    /// The viewport height the first window plans for; it does not size the
    /// table, which takes its height from its parent.
    pub initial_viewport_height: f32,
}

/// Column windowing (upstream `UseTableColumnsVirtualizationConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnsVirtualization {
    /// The inline viewport width the first window plans for.
    pub initial_viewport_width: f32,
}

/// What a table windows (upstream `TableVirtualizationCapability`, or a list
/// of one of each).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TableVirtualization {
    pub(crate) rows: Option<RowsVirtualization>,
    pub(crate) columns: Option<ColumnsVirtualization>,
}

impl TableVirtualization {
    /// Both capabilities: rows and columns.
    pub fn with(self, other: TableVirtualization) -> Self {
        Self {
            rows: self.rows.or(other.rows),
            columns: self.columns.or(other.columns),
        }
    }
}

/// Upstream `useTableRowsVirtualization`: keep the full data and a stable
/// `id_key` on the table.
pub fn use_table_rows_virtualization(
    row_height: f32,
    initial_viewport_height: f32,
) -> TableVirtualization {
    TableVirtualization {
        rows: Some(RowsVirtualization {
            row_height,
            initial_viewport_height,
        }),
        columns: None,
    }
}

/// Upstream `useTableColumnsVirtualization`: keep the full columns on the
/// table; pinned columns and a focused cell's column stay mounted.
pub fn use_table_columns_virtualization(initial_viewport_width: f32) -> TableVirtualization {
    TableVirtualization {
        rows: None,
        columns: Some(ColumnsVirtualization {
            initial_viewport_width,
        }),
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_ROWS_VIRTUALIZATION_DOC: ComponentDoc = ComponentDoc {
    name: "useTableRowsVirtualization",
    rust_type: "rustryx::components::table::use_table_rows_virtualization",
    props: &[
        p("rowHeight", RustApi::Argument("row_height")),
        p(
            "initialViewportHeight",
            RustApi::Argument("initial_viewport_height"),
        ),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_TABLE_COLUMNS_VIRTUALIZATION_DOC: ComponentDoc = ComponentDoc {
    name: "useTableColumnsVirtualization",
    rust_type: "rustryx::components::table::use_table_columns_virtualization",
    props: &[p(
        "initialViewportWidth",
        RustApi::Argument("initial_viewport_width"),
    )],
    example: include_str!("examples.rs"),
};
