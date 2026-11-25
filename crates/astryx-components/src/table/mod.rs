//! Table (M7): a data-driven table with a plugin pipeline, or the caller's
//! composition of sections, rows and cells. Upstream: `Table/`.

mod activation;
mod cells;
mod column_resize;
mod column_settings;
pub mod examples;
mod filtering;
mod grouped;
mod infinite_scroll;
pub(crate) mod menu;
pub mod model;
mod pagination;
mod parts;
pub mod plugin;
mod row_expansion;
mod row_index;
mod row_status;
mod selection;
mod sortable;
mod sticky;
#[allow(clippy::module_inception)]
mod table;
mod tree;

pub use activation::{TableActivation, USE_TABLE_ACTIVATION_DOC, use_table_activation};
pub use cells::{TableDensity, TableDividers, TableTextOverflow, TableVerticalAlign};
pub use column_resize::{
    ColumnResizeConfig, TableColumnResize, USE_TABLE_COLUMN_RESIZE_DOC, resolve_column_min_width,
    use_table_column_resize,
};
pub use column_settings::{
    ColumnSettingsConfig, ColumnSettingsOption, ColumnSettingsState, TableColumnSettings,
    USE_TABLE_COLUMN_SETTINGS_DOC, USE_TABLE_COLUMN_SETTINGS_STATE_DOC, use_table_column_settings,
    use_table_column_settings_state,
};
pub use filtering::{
    TableFilterState, TableFilterStateHandle, TableFilterValue, TableFilterVariant, TableFiltering,
    USE_TABLE_FILTER_STATE_DOC, USE_TABLE_FILTERING_DOC, to_search_filters, use_table_filter_state,
    use_table_filtering,
};
pub use grouped::{
    GroupedRows, GroupedRowsConfig, TableGroupedRows, USE_TABLE_GROUPED_ROWS_DOC,
    use_table_grouped_rows,
};
pub use infinite_scroll::{
    TableInfiniteScroll, USE_TABLE_INFINITE_SCROLL_DOC, use_table_infinite_scroll,
};
pub use model::{
    ColumnHeader, ColumnWidth, IdKey, IdentityStatus, Record, RowKey, SortableColumn, TableColumn,
    TableColumnAlign, TableFilterRef, TableItem, TableRecord, TableValue, capitalize,
    generate_columns, pixel, proportional, proportional_min, resolve_column_floor_widths,
    resolve_table_min_width,
};
pub use pagination::{
    TablePagination, TablePaginationAlign, TablePaginationPosition, USE_TABLE_PAGINATION_DOC,
    paginate_data, use_table_pagination,
};
pub use parts::{TableBody, TableCell, TableFooter, TableHeader, TableHeaderCell, TableRow};
pub use plugin::{
    BodyCellProps, BodyRowProps, CellPin, ElementHook, HeaderCellProps, HeaderRowProps, PinEdge,
    PlanRow, RowAction, RowKind, RowModel, RowPaint, ScrollWrapperProps, SyntheticRow,
    SyntheticRowFn, SyntheticRowProps, TableContextAction, TableContextActions, TableMode,
    TablePlugin, TableRenderContext, TableRenderProps, TableRowContext, WholeRowOwner,
};
pub use row_expansion::{
    EXPANSION_COLUMN_KEY, PanelVariant, TableRowExpansion, USE_TABLE_ROW_EXPANSION_DOC,
    use_table_row_expansion,
};
pub use row_index::{
    ROW_INDEX_COLUMN_KEY, TableRowIndex, USE_TABLE_ROW_INDEX_DOC, use_table_row_index,
};
pub use row_status::{
    ROW_STATUS_COLUMN_KEY, StatusColor, TableRowSemanticStatus, TableRowStatus,
    TableRowStatusColor, TableRowStatusPlugin, USE_TABLE_ROW_STATUS_DOC, use_table_row_status,
};
pub use selection::{
    SELECTION_COLUMN_KEY, SelectionState, SelectionStateConfig, TABLE_SELECTION_TOOLBAR_DOC,
    TableSelection, TableSelectionState, TableSelectionToolbar, USE_TABLE_SELECTION_DOC,
    USE_TABLE_SELECTION_STATE_DOC, use_table_selection_state,
};
pub use sortable::{
    SortDirection, SortEntry, SortableState, SortableStateConfig, TableSortable,
    USE_TABLE_SORTABLE_DOC, USE_TABLE_SORTABLE_STATE_DOC, next_direction, next_sort,
    use_table_sortable_state,
};
pub use sticky::{TableStickyColumns, USE_TABLE_STICKY_COLUMNS_DOC, use_table_sticky_columns};
pub use table::Table;
pub use tree::{
    AllExpanded, TableTreeData, TableTreeRowMeta, TreeIndent, TreeState, TreeStateConfig,
    USE_TABLE_TREE_DATA_DOC, USE_TABLE_TREE_STATE_DOC, use_table_tree_data, use_table_tree_state,
};

#[cfg(test)]
mod column_tests;
#[cfg(test)]
mod data_tests;
#[cfg(test)]
mod row_tests;
#[cfg(test)]
mod structure_tests;
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
