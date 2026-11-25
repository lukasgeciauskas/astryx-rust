//! Table (M7): a data-driven table with a plugin pipeline, or the caller's
//! composition of sections, rows and cells. Upstream: `Table/`.

mod cells;
pub mod examples;
pub(crate) mod menu;
pub mod model;
mod parts;
pub mod plugin;
#[allow(clippy::module_inception)]
mod table;

pub use cells::{TableDensity, TableDividers, TableTextOverflow, TableVerticalAlign};
pub use model::{
    ColumnHeader, ColumnWidth, IdKey, IdentityStatus, Record, RowKey, SortableColumn, TableColumn,
    TableColumnAlign, TableFilterRef, TableRecord, TableValue, capitalize, generate_columns, pixel,
    proportional, proportional_min, resolve_column_floor_widths, resolve_table_min_width,
};
pub use parts::{TableBody, TableCell, TableFooter, TableHeader, TableHeaderCell, TableRow};
pub use plugin::{
    BodyCellProps, BodyRowProps, ElementHook, HeaderCellProps, HeaderRowProps, RowKind, RowModel,
    RowPaint, ScrollWrapperProps, TableContextAction, TableContextActions, TableMode, TablePlugin,
    TableRenderContext, TableRenderProps, TableRowContext,
};
pub use table::Table;

#[cfg(test)]
mod tests;

use crate::doc::{ComponentDoc, PropMapping, RustApi};

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const TABLE_DOC: ComponentDoc = ComponentDoc {
    name: "Table",
    rust_type: "astryx::components::table::Table",
    props: &[
        p("data", RustApi::Method("data")),
        p("columns", RustApi::Method("columns (TableColumn::new)")),
        p("idKey", RustApi::Method("id_key / id_key_fn")),
        p("density", RustApi::Method("density")),
        p("dividers", RustApi::Method("dividers")),
        p("isStriped", RustApi::Method("striped")),
        p("hasHover", RustApi::Method("hover")),
        p("verticalAlign", RustApi::Method("vertical_align")),
        p("textOverflow", RustApi::Method("text_overflow")),
        p(
            "virtualization",
            RustApi::Pending(
                "virtualization",
                "row and column windowing come later in M7",
            ),
        ),
        p("plugins", RustApi::Method("plugin (named; TablePlugin)")),
        p("rowIndexStart", RustApi::Method("row_index_start")),
        p("rowCount", RustApi::Method("row_count")),
        p(
            "emptyState",
            RustApi::Method("empty_state / no_empty_state"),
        ),
        p(
            "children",
            RustApi::Trait("ParentElement (Table::composed)"),
        ),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const TABLE_HEADER_DOC: ComponentDoc = ComponentDoc {
    name: "TableHeader",
    rust_type: "astryx::components::table::TableHeader",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const TABLE_BODY_DOC: ComponentDoc = ComponentDoc {
    name: "TableBody",
    rust_type: "astryx::components::table::TableBody",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const TABLE_FOOTER_DOC: ComponentDoc = ComponentDoc {
    name: "TableFooter",
    rust_type: "astryx::components::table::TableFooter",
    props: &[
        p("children", RustApi::Trait("ParentElement")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};

pub const TABLE_ROW_DOC: ComponentDoc = ComponentDoc {
    name: "TableRow",
    rust_type: "astryx::components::table::TableRow",
    props: &[p("children", RustApi::Trait("ParentElement"))],
    example: include_str!("examples.rs"),
};

pub const TABLE_CELL_DOC: ComponentDoc = ComponentDoc {
    name: "TableCell",
    rust_type: "astryx::components::table::TableCell",
    props: &[p("children", RustApi::Trait("ParentElement"))],
    example: include_str!("examples.rs"),
};

pub const TABLE_HEADER_CELL_DOC: ComponentDoc = ComponentDoc {
    name: "TableHeaderCell",
    rust_type: "astryx::components::table::TableHeaderCell",
    props: &[p("children", RustApi::Trait("ParentElement"))],
    example: include_str!("examples.rs"),
};
