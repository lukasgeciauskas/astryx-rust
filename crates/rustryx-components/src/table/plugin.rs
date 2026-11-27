//! The plugin pipeline (upstream `TablePlugin` and the render props it
//! transforms, `types.ts`): a plugin rewrites the columns, then each
//! structural level's props as the table renders.
//!
//! Plugins compose in order: each transform receives what the plugins before
//! it returned (FR10). The order is the named record's (FR9): the known names
//! in the sequence `columnSettings`, `sort`, `tree`, `selection`,
//! `pagination`, then every other name in insertion order. Context
//! transforms run in reverse, so the first plugin wraps outermost (FR11).
//!
//! Upstream's props carry HTML attributes and event handlers; here a
//! plugin adds handlers and attributes to an element through its props'
//! `hooks`, each a function of the element the table builds.

use std::rc::Rc;

use gpui::{
    AnyElement, App, Div, FocusHandle, ScrollHandle, SharedString, Stateful, StyleRefinement,
    Window,
};

use super::model::{IdentityStatus, RowKey, TableColumn, TableItem};
use crate::icon::IconSource;

pub(crate) use rustryx_primitives::dev_warn_once;

/// A change to an element the table builds: handlers, attributes.
pub type ElementHook = Box<dyn FnOnce(Stateful<Div>) -> Stateful<Div>>;

/// A plugin that claims a whole row's press (upstream `wholeRowOwner`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WholeRowOwner {
    Activation,
    Expansion,
    Navigation,
}

/// The branch a table renders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableMode {
    /// Rows from `data`.
    Data,
    /// The caller's composition.
    Children,
}

/// The row model the plugins admit (FR17).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowModel {
    Flat,
    Tree,
    Grouped,
    Detail,
    Custom,
}

/// What a row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// A row of a flat dataset.
    Product,
    Synthetic,
    Unknown,
}

/// Facts about the table the row, cell and wrapper transforms read
/// (upstream `TableRenderContext`).
#[derive(Clone)]
pub struct TableRenderContext {
    /// The table's id, as text: a prefix for names that must be unique in
    /// the window (accessibility relations).
    pub table: SharedString,
    pub mode: TableMode,
    pub row_model: RowModel,
    /// The product rows in the complete data.
    pub logical_row_count: usize,
    /// 1 when the table renders a header, else 0.
    pub header_row_count: usize,
    /// The scroll container around the table.
    pub viewport: ScrollHandle,
    /// The table windows its rows in its own block viewport (it scrolls
    /// both ways, sized by its parent).
    pub block_viewport: bool,
    /// The scroll container's focus: where focus goes when chrome that held
    /// it goes away (infinite scroll's exhausted command).
    pub viewport_focus: Option<FocusHandle>,
    /// The plugins that claim a whole row's action (activation, row
    /// expansion): more than one neutralizes activation (FR17).
    pub whole_row_owners: usize,
}

/// Facts about one body row (upstream `TableRowContext`).
#[derive(Clone)]
pub struct TableRowContext {
    /// Its `id_key` value, or its index without one.
    pub key: Option<RowKey>,
    pub identity_status: IdentityStatus,
    pub kind: RowKind,
    /// Its position in the complete data.
    pub logical_index: usize,
    pub logical_row_count: usize,
    pub header_row_count: usize,
    pub render: TableRenderContext,
}

type SelectFn = Rc<dyn Fn(&mut Window, &mut App)>;
type ActionsFn = Rc<dyn Fn(&mut App) -> Vec<TableContextAction>>;

/// One right-click action a plugin contributes (upstream
/// `TableContextAction`).
#[derive(Clone)]
pub struct TableContextAction {
    /// Unique within a menu.
    pub id: SharedString,
    pub label: SharedString,
    pub icon: Option<IconSource>,
    pub on_select: SelectFn,
    pub disabled: bool,
    /// Related actions cluster, with a divider between groups, in first-seen
    /// order; actions without one form a trailing group.
    pub group: Option<SharedString>,
    /// Drawn checked (the active sort direction).
    pub checked: Option<bool>,
    /// In the error colour (Delete row).
    pub destructive: bool,
}

impl TableContextAction {
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        on_select: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            on_select: Rc::new(on_select),
            disabled: false,
            group: None,
            checked: None,
            destructive: false,
        }
    }

    pub fn icon(mut self, icon: impl Into<IconSource>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub fn destructive(mut self, destructive: bool) -> Self {
        self.destructive = destructive;
        self
    }
}

/// A cell's actions: a list, or a getter run when the menu opens (upstream
/// `TableContextActions`).
#[derive(Clone)]
pub enum TableContextActions {
    List(Vec<TableContextAction>),
    Lazy(ActionsFn),
}

impl TableContextActions {
    pub fn lazy(f: impl Fn(&mut App) -> Vec<TableContextAction> + 'static) -> Self {
        TableContextActions::Lazy(Rc::new(f))
    }

    pub(crate) fn resolve(&self, cx: &mut App) -> Vec<TableContextAction> {
        match self {
            TableContextActions::List(list) => list.clone(),
            TableContextActions::Lazy(f) => f(cx),
        }
    }
}

/// The `<table>` element's props.
#[derive(Default)]
pub struct TableRenderProps {
    pub style: StyleRefinement,
    pub hooks: Vec<ElementHook>,
}

/// The header row's props: its cells, built.
pub struct HeaderRowProps {
    pub style: StyleRefinement,
    pub children: Vec<AnyElement>,
    pub hooks: Vec<ElementHook>,
}

/// A header cell's props, in named slots so plugins do not collide
/// (upstream `HeaderCellRenderProps`): `before` the label (a selection
/// checkbox), the label `content` (a sort control may wrap it), `after` it
/// (a sort indicator), an `overlay` laid over the cell (a resize handle,
/// never part of the header's name) and content `below` it (inline filters).
#[derive(Default)]
pub struct HeaderCellProps {
    pub style: StyleRefinement,
    pub before: Vec<AnyElement>,
    pub content: Option<AnyElement>,
    pub after: Vec<AnyElement>,
    pub overlay: Vec<AnyElement>,
    pub below: Vec<AnyElement>,
    pub context_menu: Vec<TableContextActions>,
    /// The header's accessible name, when a plugin names it.
    pub aria_label: Option<SharedString>,
    /// Pinned to an edge (sticky columns).
    pub pinned: Option<CellPin>,
    /// Its place among the final columns.
    pub column_index: usize,
    pub hooks: Vec<ElementHook>,
}

/// Which edge a pinned cell stays at (upstream `useTableStickyColumns`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinEdge {
    Start,
    End,
}

/// A pinned cell: it stays at its edge while the table scrolls sideways,
/// over the cells that scroll under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellPin {
    pub edge: PinEdge,
    /// The pinned run's inner edge: it casts the shadow over scrolled
    /// content.
    pub boundary: bool,
}

/// A body row's props: its cells, built (upstream `BodyRowRenderProps`).
#[derive(Default)]
pub struct BodyRowProps {
    pub style: StyleRefinement,
    pub children: Vec<AnyElement>,
    /// Rendered right after the row: a detail panel.
    pub after_row: Vec<AnyElement>,
    pub aria_row_index: Option<usize>,
    /// What the row plugins publish for its paint (FR21): one resolver
    /// paints it from every plugin's state.
    pub paint: RowPaint,
    /// Upstream `tabIndex`: `Some(true)` makes the row a tab stop,
    /// `Some(false)` focusable but out of the tab order (`-1`), `None`
    /// leaves it unfocusable.
    pub tab_stop: Option<bool>,
    /// Upstream `aria-keyshortcuts`, one token each.
    pub key_shortcuts: Vec<SharedString>,
    /// The one whole-row action (activation, FR17): a primary click on the
    /// row or a non-repeated Enter or Space while the row itself is
    /// focused runs it, after the row's other handlers, unless one called
    /// `window.prevent_default()`. Presses on a nested control never do.
    pub row_action: Option<RowAction>,
    pub hooks: Vec<ElementHook>,
}

/// A row a plugin adds among the product rows (a group's header): one cell
/// across every column. Other plugins' row and cell transforms never see it.
#[derive(Clone)]
pub struct SyntheticRow {
    /// Unique among the synthetic rows (its selector reads `g` and this).
    pub key: SharedString,
    pub render: SyntheticRowFn,
}

/// Builds a synthetic row's props.
pub type SyntheticRowFn =
    Rc<dyn Fn(&TableRenderContext, &mut Window, &mut App) -> SyntheticRowProps>;

/// What a synthetic row renders: its row's style and hooks, and its one
/// cell's style and content.
pub struct SyntheticRowProps {
    pub style: StyleRefinement,
    pub cell_style: StyleRefinement,
    pub content: AnyElement,
    pub hooks: Vec<ElementHook>,
}

/// One body row of a plan (upstream's render plan): a product row by its
/// index in the data, or a plugin's synthetic row.
#[derive(Clone)]
pub enum PlanRow {
    Product(usize),
    Synthetic(SyntheticRow),
}

/// A whole row's action ([`BodyRowProps::row_action`]).
pub type RowAction = Rc<dyn Fn(&mut Window, &mut App)>;

/// What the row plugins publish for a row's paint (upstream `rowPaint.ts`,
/// FR21): selection's wash wins while the row is selected with
/// `highlight`; otherwise the active row's fill; then hover and the stripe.
/// The active row also carries its inline-start marker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowPaint {
    /// Selected (`aria-selected`, the theme's `selected` state).
    pub selected: bool,
    /// Selection paints its wash on a selected row (`hasRowHighlight`).
    pub highlight: bool,
    /// The current item (activation: `aria-current`, fill and marker).
    pub active: bool,
}

/// A body cell's props (upstream `BodyCellRenderProps`).
#[derive(Default)]
pub struct BodyCellProps {
    pub style: StyleRefinement,
    pub context_menu: Vec<TableContextActions>,
    pub column_index: usize,
    /// The cell renders empty: neither the column's renderer nor the default
    /// one runs (a row a plugin replaces wholesale).
    pub content_suppressed: bool,
    /// Pinned to an edge (sticky columns).
    pub pinned: Option<CellPin>,
    pub hooks: Vec<ElementHook>,
}

/// The scroll wrapper's props: the container around the table (upstream
/// `ScrollWrapperRenderProps`).
pub struct ScrollWrapperProps {
    pub style: StyleRefinement,
    /// Chrome inside the scroll container, before and after the table.
    pub before_table: Vec<AnyElement>,
    pub after_table: Vec<AnyElement>,
    pub hooks: Vec<ElementHook>,
    /// Turns the scroll-edge fade off on an edge that marks its overflow another
    /// way (upstream `scrollEdgeFadeOff`): pinned columns' shadows.
    pub fade_off_start: bool,
    pub fade_off_end: bool,
}

/// A table plugin (upstream `TablePlugin<T>`): every transform is optional
/// and receives what the plugins before it produced.
#[allow(unused_variables)]
pub trait TablePlugin<T> {
    /// The row model its body transforms keep (`Flat` for one product row
    /// per item). A body transform without one makes the model `Custom`.
    fn row_model(&self) -> Option<RowModel> {
        None
    }

    /// Whether it transforms body rows or cells (for the row model).
    fn transforms_body(&self) -> bool {
        false
    }

    /// The body's rows, when it plans them (grouped rows): product rows by
    /// index, with its synthetic rows among them. The first plugin with a
    /// plan decides.
    fn row_plan(&self) -> Option<Rc<[PlanRow]>> {
        None
    }

    /// The whole-row action it claims, if any (FR17): more than one owner
    /// neutralizes activation.
    fn whole_row_owner(&self) -> Option<WholeRowOwner> {
        None
    }

    /// Whether it needs the rows' canonical identity: `id_key` values, with
    /// missing, invalid and duplicate keys reported (FR16).
    fn needs_canonical_identity(&self) -> bool {
        false
    }

    /// Whether the table's data is loading (upstream `renderState`): an
    /// empty table that loads shows no default empty state.
    fn is_loading(&self, cx: &App) -> Option<bool> {
        None
    }

    /// Whether more rows may come (upstream `renderState.hasMore`).
    fn has_more(&self, cx: &App) -> Option<bool> {
        None
    }

    /// Filters, reorders or adds columns, before any element transform.
    fn transform_columns(
        &self,
        columns: Vec<TableColumn<T>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<TableColumn<T>> {
        columns
    }

    fn transform_table(
        &self,
        props: TableRenderProps,
        window: &mut Window,
        cx: &mut App,
    ) -> TableRenderProps {
        props
    }

    fn transform_header_row(
        &self,
        props: HeaderRowProps,
        window: &mut Window,
        cx: &mut App,
    ) -> HeaderRowProps {
        props
    }

    fn transform_header_cell(
        &self,
        props: HeaderCellProps,
        column: &TableColumn<T>,
        columns: &[TableColumn<T>],
        window: &mut Window,
        cx: &mut App,
    ) -> HeaderCellProps {
        props
    }

    fn transform_body_row(
        &self,
        props: BodyRowProps,
        item: &TableItem<T>,
        context: &TableRowContext,
        window: &mut Window,
        cx: &mut App,
    ) -> BodyRowProps {
        props
    }

    #[allow(clippy::too_many_arguments)]
    fn transform_body_cell(
        &self,
        props: BodyCellProps,
        column: &TableColumn<T>,
        item: &TableItem<T>,
        columns: &[TableColumn<T>],
        context: &TableRowContext,
        window: &mut Window,
        cx: &mut App,
    ) -> BodyCellProps {
        props
    }

    fn transform_scroll_wrapper(
        &self,
        props: ScrollWrapperProps,
        context: &TableRenderContext,
        window: &mut Window,
        cx: &mut App,
    ) -> ScrollWrapperProps {
        props
    }

    /// Wraps the table in providers.
    fn transform_table_context(
        &self,
        child: AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        child
    }
}

/// The canonical sequence of the known plugin names (FR9).
pub const KNOWN_PLUGIN_ORDER: [&str; 5] =
    ["columnSettings", "sort", "tree", "selection", "pagination"];

/// The plugins in pipeline order: the known names in their sequence, then
/// the others as given (FR9).
pub(crate) fn order_plugins<P: Clone>(named: &[(SharedString, P)]) -> Vec<P> {
    let mut out = Vec::with_capacity(named.len());
    for known in KNOWN_PLUGIN_ORDER {
        out.extend(
            named
                .iter()
                .filter(|(n, _)| n.as_ref() == known)
                .map(|(_, p)| p.clone()),
        );
    }
    out.extend(
        named
            .iter()
            .filter(|(n, _)| !KNOWN_PLUGIN_ORDER.contains(&n.as_ref()))
            .map(|(_, p)| p.clone()),
    );
    out
}

/// The row model the plugins admit (upstream `resolveTableAdmission`).
pub(crate) fn admitted_row_model<T>(plugins: &[Rc<dyn TablePlugin<T>>]) -> RowModel {
    let modifier = plugins.iter().find(|p| {
        p.row_model() != Some(RowModel::Flat) && (p.row_model().is_some() || p.transforms_body())
    });
    match modifier {
        None => RowModel::Flat,
        Some(p) => p.row_model().unwrap_or(RowModel::Custom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr9_known_names_first_in_their_sequence_then_the_rest_as_given() {
        let named: Vec<(SharedString, &str)> =
            ["mine", "pagination", "sort", "other", "columnSettings"]
                .iter()
                .map(|n| (SharedString::from(*n), *n))
                .collect();
        assert_eq!(
            order_plugins(&named),
            ["columnSettings", "sort", "pagination", "mine", "other"]
        );
    }
}
