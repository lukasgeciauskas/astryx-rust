//! Row selection (upstream `useTableSelection`, `useTableSelectionState`
//! and `TableSelectionToolbar`): a checkbox column with select-all, each
//! row's selected state and wash, and the bulk-actions toolbar.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, Refineable,
    RenderOnce, SharedString, StyleRefinement, Styled, Window, div,
};
use rustryx_i18n::current as i18n;
use rustryx_primitives::ElementSize;
use rustryx_theme::current;

use super::model::{IdKey, TableColumn, TableItem, TableRecord, pixel};
use super::plugin::{BodyRowProps, RowModel, TablePlugin, TableRowContext};
use crate::button::{Button, ButtonVariant};
use crate::choice::CheckboxInput;
use crate::containers::{SectionSide, SectionVariant};
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::indicator::{ChoiceSize, IndicatorState};
use crate::layout::{Orientation, SpacingStep};
use crate::toolbar::Toolbar;

/// The selection column's key, prefixed to stay clear of the caller's.
pub const SELECTION_COLUMN_KEY: &str = "__xds_selection";

type ItemPredicate<T> = Rc<dyn Fn(&T) -> bool>;
type SelectItemFn<T> = Rc<dyn Fn(&T, bool, &mut Window, &mut App)>;
type SelectAllFn = Rc<dyn Fn(bool, &mut Window, &mut App)>;
type Query = Rc<dyn Fn() -> bool>;
type RowLabelFn<T> = Rc<dyn Fn(&T) -> SharedString>;
type KeysFn = Rc<dyn Fn(HashSet<SharedString>, &mut Window, &mut App)>;
type ClearFn = Rc<dyn Fn(&mut Window, &mut App)>;
type SelectionLabelFn = Rc<dyn Fn(usize) -> AnyElement>;

/// The selection plugin (upstream `useTableSelection`): a 36 px checkbox
/// column first, select-all in its header, and each selected row's
/// `aria-selected`, theme `selected` state and wash.
pub struct TableSelection<T> {
    is_selected: ItemPredicate<T>,
    on_select_item: SelectItemFn<T>,
    on_select_all: SelectAllFn,
    is_all_selected: Query,
    is_indeterminate: Option<Query>,
    is_selectable: Option<ItemPredicate<T>>,
    is_enabled: Option<ItemPredicate<T>>,
    row_label: Option<RowLabelFn<T>>,
    highlight: bool,
}

impl<T> Clone for TableSelection<T> {
    fn clone(&self) -> Self {
        Self {
            is_selected: self.is_selected.clone(),
            on_select_item: self.on_select_item.clone(),
            on_select_all: self.on_select_all.clone(),
            is_all_selected: self.is_all_selected.clone(),
            is_indeterminate: self.is_indeterminate.clone(),
            is_selectable: self.is_selectable.clone(),
            is_enabled: self.is_enabled.clone(),
            row_label: self.row_label.clone(),
            highlight: self.highlight,
        }
    }
}

impl<T: 'static> TableSelection<T> {
    /// Upstream `getIsItemSelected`, `onSelectItem` (the item and its
    /// new state), `onSelectAll` (the new all-selected state) and
    /// `getIsAllSelected`.
    pub fn new(
        is_selected: impl Fn(&T) -> bool + 'static,
        on_select_item: impl Fn(&T, bool, &mut Window, &mut App) + 'static,
        on_select_all: impl Fn(bool, &mut Window, &mut App) + 'static,
        is_all_selected: impl Fn() -> bool + 'static,
    ) -> Self {
        Self {
            is_selected: Rc::new(is_selected),
            on_select_item: Rc::new(on_select_item),
            on_select_all: Rc::new(on_select_all),
            is_all_selected: Rc::new(is_all_selected),
            is_indeterminate: None,
            is_selectable: None,
            is_enabled: None,
            row_label: None,
            highlight: true,
        }
    }

    /// Upstream `getIsIndeterminate`: a partial selection; select-all shows
    /// its mixed state.
    pub fn is_indeterminate(mut self, f: impl Fn() -> bool + 'static) -> Self {
        self.is_indeterminate = Some(Rc::new(f));
        self
    }

    /// Upstream `getIsItemSelectable`: a row it rejects has no checkbox (an
    /// empty cell). The caller's select-all leaves such rows out.
    pub fn is_item_selectable(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.is_selectable = Some(Rc::new(f));
        self
    }

    /// Upstream `getIsItemEnabled`: a row it rejects shows a disabled
    /// checkbox that keeps its state.
    pub fn is_item_enabled(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.is_enabled = Some(Rc::new(f));
        self
    }

    /// Upstream `getRowLabel`: the checkbox's hidden name is "Select" and
    /// this; "Select row" without.
    pub fn row_label(mut self, f: impl Fn(&T) -> SharedString + 'static) -> Self {
        self.row_label = Some(Rc::new(f));
        self
    }

    /// Upstream `hasRowHighlight`: the accent wash on selected rows (on by
    /// default); `aria-selected` stays either way.
    pub fn row_highlight(mut self, highlight: bool) -> Self {
        self.highlight = highlight;
        self
    }

    fn column(&self) -> TableColumn<T> {
        let (all, indeterminate, select_all) = (
            self.is_all_selected.clone(),
            self.is_indeterminate.clone(),
            self.on_select_all.clone(),
        );
        let this = self.clone();
        TableColumn::new(SELECTION_COLUMN_KEY)
            .fixed_height()
            .width(pixel(36.))
            .resizable(false)
            .header_element(move |_, cx| {
                let all_selected = all();
                let state = if all_selected {
                    IndicatorState::Checked
                } else if indeterminate.as_ref().is_some_and(|f| f()) {
                    IndicatorState::Indeterminate
                } else {
                    IndicatorState::Unchecked
                };
                let select_all = select_all.clone();
                centered(
                    CheckboxInput::new(
                        "select-all",
                        i18n(cx).t("@astryx.table.selection.selectAllRows"),
                        state,
                    )
                    .label_hidden(true)
                    .size(ChoiceSize::Sm)
                    .on_change(move |_, window, cx| select_all(!all_selected, window, cx)),
                )
            })
            .render_cell_item(move |item, _, cx| {
                if this.is_selectable.as_ref().is_some_and(|f| !f(item)) {
                    return div().into_any_element();
                }
                let selected = (this.is_selected)(item);
                let enabled = this.is_enabled.as_ref().is_none_or(|f| f(item));
                let t = i18n(cx);
                let label = match &this.row_label {
                    Some(f) => t.format(
                        "@astryx.table.selection.selectRowNamed",
                        &[("label", f(item).as_ref().into())],
                    ),
                    None => t.t("@astryx.table.selection.selectRow"),
                };
                let (on_select, row) = (this.on_select_item.clone(), item.clone());
                centered(
                    CheckboxInput::new(("select", item.index()), label, selected)
                        .label_hidden(true)
                        .size(ChoiceSize::Sm)
                        .disabled(!enabled)
                        .on_change(move |_, window, cx| on_select(&row, !selected, window, cx)),
                )
            })
    }
}

fn centered(content: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .w_full()
        .child(content)
        .into_any_element()
}

impl<T: 'static> TablePlugin<T> for TableSelection<T> {
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
        columns.insert(0, self.column());
        columns
    }

    fn transform_body_row(
        &self,
        mut props: BodyRowProps,
        item: &TableItem<T>,
        _: &TableRowContext,
        _: &mut Window,
        _: &mut App,
    ) -> BodyRowProps {
        // Published on every row, so the wash comes off when it changes.
        props.paint.selected = (self.is_selected)(item);
        props.paint.highlight = self.highlight;
        props
    }
}

// ----------------------------------------------------------------------------
// useTableSelectionState

/// The selection shared with UI outside the plugin (upstream
/// `TableSelectionState`): the complete selection, keys outside the current
/// view included, and a command that clears all of it.
#[derive(Clone)]
pub struct TableSelectionState {
    pub selected_keys: Rc<HashSet<SharedString>>,
    pub selected_count: usize,
    pub has_selection: bool,
    clear: ClearFn,
}

impl TableSelectionState {
    /// A state the app builds, for a selection across server pages.
    pub fn new(
        selected_keys: impl IntoIterator<Item = SharedString>,
        clear_selection: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let keys: HashSet<SharedString> = selected_keys.into_iter().collect();
        Self {
            selected_count: keys.len(),
            has_selection: !keys.is_empty(),
            selected_keys: Rc::new(keys),
            clear: Rc::new(clear_selection),
        }
    }

    /// Upstream `clearSelection`: every key, those out of view too.
    pub fn clear_selection(&self, window: &mut Window, cx: &mut App) {
        (self.clear)(window, cx)
    }
}

/// What [`use_table_selection_state`] reads (upstream
/// `UseTableSelectionStateConfig`).
pub struct SelectionStateConfig<T> {
    data: Rc<[T]>,
    id_key: IdKey<T>,
    controlled: Option<(Rc<HashSet<SharedString>>, KeysFn)>,
    is_selectable: Option<ItemPredicate<T>>,
    is_enabled: Option<ItemPredicate<T>>,
}

impl<T> SelectionStateConfig<T> {
    /// Upstream `data` and `idKey` (a field name, or [`IdKey::func`]).
    pub fn new(data: impl Into<Rc<[T]>>, id_key: impl Into<IdKey<T>>) -> Self {
        Self {
            data: data.into(),
            id_key: id_key.into(),
            controlled: None,
            is_selectable: None,
            is_enabled: None,
        }
    }

    /// Upstream `selectedKeys` with `setSelectedKeys`: a selection the
    /// caller owns. Without it the hook keeps its own.
    pub fn selected_keys(
        mut self,
        keys: impl IntoIterator<Item = SharedString>,
        set_selected_keys: impl Fn(HashSet<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.controlled = Some((
            Rc::new(keys.into_iter().collect()),
            Rc::new(set_selected_keys),
        ));
        self
    }

    /// Upstream `getIsItemSelectable`: rows it rejects have no checkbox and
    /// stay out of select-all.
    pub fn is_item_selectable(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.is_selectable = Some(Rc::new(f));
        self
    }

    /// Upstream `getIsItemEnabled`: rows it rejects are frozen; select-all
    /// keeps their state.
    pub fn is_item_enabled(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.is_enabled = Some(Rc::new(f));
        self
    }
}

/// The hook's result (upstream `UseTableSelectionStateResult`).
pub struct SelectionState<T> {
    /// The plugin, ready for the table (upstream `selectionConfig`).
    pub plugin: TableSelection<T>,
    /// For the toolbar and other UI (upstream `selectionState`).
    pub state: TableSelectionState,
}

/// Upstream `useTableSelectionState`: the selection's bookkeeping over
/// `data`. Select-all takes every selectable, enabled row and keeps frozen
/// ones (disabled but selected) as they are; it reads as all selected only
/// with at least one actionable row.
pub fn use_table_selection_state<T: TableRecord>(
    id: impl Into<ElementId>,
    config: SelectionStateConfig<T>,
    window: &mut Window,
    cx: &mut App,
) -> SelectionState<T> {
    let id = id.into();
    let own = window.use_keyed_state(
        ElementId::Name(format!("{id:?}-selection").into()),
        cx,
        |_, _| Rc::new(HashSet::<SharedString>::new()),
    );
    let (selected, set): (Rc<HashSet<SharedString>>, KeysFn) = match config.controlled {
        Some(controlled) => controlled,
        None => (
            own.read(cx).clone(),
            Rc::new(move |next, window, cx| {
                own.update(cx, |keys, _| *keys = Rc::new(next));
                window.refresh();
            }),
        ),
    };
    let id_key = config.id_key.clone();
    let get_id: Rc<dyn Fn(&T) -> SharedString> = Rc::new(move |item| id_key.string_key(item));
    let (is_selectable, is_enabled) = (config.is_selectable.clone(), config.is_enabled.clone());
    let actionable = |item: &T| {
        is_selectable.as_ref().is_none_or(|f| f(item))
            && is_enabled.as_ref().is_none_or(|f| f(item))
    };
    // The actionable rows' keys, the frozen selected keys outside them, and
    // what select-all selects: every actionable key plus the frozen ones.
    let selectable_ids: HashSet<SharedString> = config
        .data
        .iter()
        .filter(|i| actionable(i))
        .map(|i| get_id(i))
        .collect();
    let frozen: HashSet<SharedString> = selected
        .iter()
        .filter(|k| !selectable_ids.contains(*k))
        .cloned()
        .collect();
    let all_selectable: HashSet<SharedString> = selected
        .iter()
        .chain(selectable_ids.iter())
        .cloned()
        .collect();
    // With no actionable rows (a filter that matches nothing), a selection
    // never reads as all selected.
    let all_selected = !selectable_ids.is_empty() && all_selectable.len() == selected.len();
    let selected_actionable = config
        .data
        .iter()
        .filter(|i| actionable(i) && selected.contains(&get_id(i)))
        .count();
    let indeterminate = selected_actionable > 0 && selected_actionable < selectable_ids.len();

    let (sel, gid) = (selected.clone(), get_id.clone());
    let (set_item, sel_item, gid_item) = (set.clone(), selected.clone(), get_id.clone());
    let set_all = set.clone();
    let mut plugin = TableSelection::new(
        move |item: &T| sel.contains(&gid(item)),
        move |item: &T, on, window, cx| {
            let mut next = (*sel_item).clone();
            let key = gid_item(item);
            if on {
                next.insert(key);
            } else {
                next.remove(&key);
            }
            set_item(next, window, cx);
        },
        move |all, window, cx| {
            set_all(
                if all {
                    all_selectable.clone()
                } else {
                    frozen.clone()
                },
                window,
                cx,
            )
        },
        move || all_selected,
    )
    .is_indeterminate(move || indeterminate);
    if let Some(f) = config.is_selectable {
        plugin.is_selectable = Some(f);
    }
    if let Some(f) = config.is_enabled {
        plugin.is_enabled = Some(f);
    }
    let state = TableSelectionState {
        selected_count: selected.len(),
        has_selection: !selected.is_empty(),
        selected_keys: selected,
        clear: Rc::new(move |window, cx| set(HashSet::new(), window, cx)),
    };
    SelectionState { plugin, state }
}

// ----------------------------------------------------------------------------
// TableSelectionToolbar

/// The bulk-actions bar for a selection (upstream `TableSelectionToolbar`):
/// the caller's actions first, then the count and a clear button. It renders
/// nothing while the selection is empty.
#[derive(IntoElement)]
pub struct TableSelectionToolbar {
    id: ElementId,
    selection: TableSelectionState,
    start_content: Option<AnyElement>,
    label: Option<SharedString>,
    render_selection_label: Option<SelectionLabelFn>,
    clear_label: Option<SharedString>,
    size: ElementSize,
    variant: SectionVariant,
    gap: Option<SpacingStep>,
    orientation: Option<Orientation>,
    center_content: Option<AnyElement>,
    dividers: Vec<SectionSide>,
    style: StyleRefinement,
}

impl TableSelectionToolbar {
    /// Upstream `selection`: the state from [`use_table_selection_state`],
    /// or one the app builds ([`TableSelectionState::new`]).
    pub fn new(id: impl Into<ElementId>, selection: TableSelectionState) -> Self {
        Self {
            id: id.into(),
            selection,
            start_content: None,
            label: None,
            render_selection_label: None,
            clear_label: None,
            size: ElementSize::Sm,
            variant: SectionVariant::Muted,
            gap: None,
            orientation: None,
            center_content: None,
            dividers: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Toolbar's `gap`, passed through (FR3).
    pub fn gap(mut self, gap: SpacingStep) -> Self {
        self.gap = Some(gap);
        self
    }

    /// Toolbar's `orientation`, passed through.
    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.orientation = Some(orientation);
        self
    }

    /// Toolbar's `centerContent`, passed through. Its end lane holds the
    /// count and the clear button (no `endContent`).
    pub fn center_content(mut self, content: impl IntoElement) -> Self {
        self.center_content = Some(content.into_any_element());
        self
    }

    /// Toolbar's `dividers`, passed through.
    pub fn dividers(mut self, sides: impl IntoIterator<Item = SectionSide>) -> Self {
        self.dividers = sides.into_iter().collect();
        self
    }

    /// Upstream `startContent`: the product's bulk actions.
    pub fn start_content(mut self, content: impl IntoElement) -> Self {
        self.start_content = Some(content.into_any_element());
        self
    }

    /// Upstream `label`: the toolbar's name ("Bulk actions").
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Upstream `renderSelectionLabel`: the visible count ("3 selected").
    pub fn render_selection_label(mut self, f: impl Fn(usize) -> AnyElement + 'static) -> Self {
        self.render_selection_label = Some(Rc::new(f));
        self
    }

    /// Upstream `clearLabel`: the clear button's label ("Unselect All").
    pub fn clear_label(mut self, label: impl Into<SharedString>) -> Self {
        self.clear_label = Some(label.into());
        self
    }

    rustryx_primitives::setters! {
        /// Upstream `size` (`sm` by default): reaches its controls.
        size: ElementSize,
        /// Upstream `variant` (`muted` by default): the surface.
        variant: SectionVariant,
    }
}

impl Styled for TableSelectionToolbar {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for TableSelectionToolbar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if !self.selection.has_selection {
            return gpui::Empty.into_any_element();
        }
        let theme = current(window, cx);
        let t = theme.tokens();
        let tr = i18n(cx);
        let label = self
            .label
            .unwrap_or_else(|| tr.t("@astryx.table.selection.bulkActionsLabel"));
        let clear_label = self
            .clear_label
            .unwrap_or_else(|| tr.t("@astryx.table.selection.clearAll"));
        let count = self.selection.selected_count;
        let count_label = match &self.render_selection_label {
            Some(f) => f(count),
            None => tr
                .format(
                    "@astryx.table.selection.selectedCount",
                    &[("count", count.into())],
                )
                .into_any_element(),
        };
        let selection = self.selection.clone();
        let (count_selector, clear_selector) = (
            format!("{}-selection-count", self.id),
            format!("{}-clear", self.id),
        );
        let mut bar = Toolbar::new(self.id.clone(), label)
            .size(self.size)
            .variant(self.variant)
            .end_content(
                div()
                    .flex()
                    .items_center()
                    .gap(t.spacing.s1.to_pixels(window.rem_size()))
                    .child(
                        div()
                            .debug_selector(move || count_selector.clone())
                            .child(count_label),
                    )
                    .child(
                        div().debug_selector(move || clear_selector.clone()).child(
                            Button::new(
                                ElementId::Name(format!("{:?}-clear", self.id).into()),
                                clear_label,
                            )
                            .variant(ButtonVariant::Ghost)
                            .on_click(move |_, window, cx| selection.clear_selection(window, cx)),
                        ),
                    ),
            );
        if let Some(content) = self.start_content {
            bar = bar.start_content(content);
        }
        if let Some(content) = self.center_content {
            bar = bar.center_content(content);
        }
        if let Some(gap) = self.gap {
            bar = bar.gap(gap);
        }
        if let Some(orientation) = self.orientation {
            bar = bar.orientation(orientation);
        }
        if !self.dividers.is_empty() {
            bar = bar.dividers(self.dividers);
        }
        bar.style().refine(&self.style);
        bar.into_any_element()
    }
}

// ----------------------------------------------------------------------------
// Docs

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_SELECTION_DOC: ComponentDoc = ComponentDoc {
    name: "useTableSelection",
    rust_type: "rustryx::components::table::TableSelection",
    props: &[
        p(
            "getIsItemSelected",
            RustApi::Method("new (its first argument)"),
        ),
        p(
            "onSelectItem",
            RustApi::Method("new (the item and its new state)"),
        ),
        p(
            "onSelectAll",
            RustApi::Method("new (the new all-selected state)"),
        ),
        p(
            "getIsAllSelected",
            RustApi::Method("new (its last argument)"),
        ),
        p("getIsIndeterminate", RustApi::Method("is_indeterminate")),
        p("getIsItemSelectable", RustApi::Method("is_item_selectable")),
        p("getIsItemEnabled", RustApi::Method("is_item_enabled")),
        p("getRowLabel", RustApi::Method("row_label")),
        p("hasRowHighlight", RustApi::Method("row_highlight")),
    ],
    example: include_str!("examples.rs"),
};

pub const USE_TABLE_SELECTION_STATE_DOC: ComponentDoc = ComponentDoc {
    name: "useTableSelectionState",
    rust_type: "rustryx::components::table::use_table_selection_state",
    props: &[
        p("data", RustApi::Method("SelectionStateConfig::new")),
        p(
            "idKey",
            RustApi::Method("SelectionStateConfig::new (a field, or IdKey::func)"),
        ),
        p("selectedKeys", RustApi::Method("selected_keys")),
        p(
            "setSelectedKeys",
            RustApi::Method("selected_keys (its setter)"),
        ),
        p("getIsItemSelectable", RustApi::Method("is_item_selectable")),
        p("getIsItemEnabled", RustApi::Method("is_item_enabled")),
    ],
    example: include_str!("examples.rs"),
};

pub const TABLE_SELECTION_TOOLBAR_DOC: ComponentDoc = ComponentDoc {
    name: "TableSelectionToolbar",
    rust_type: "rustryx::components::table::TableSelectionToolbar",
    props: &[
        p("selection", RustApi::Method("new")),
        p("startContent", RustApi::Method("start_content")),
        p("label", RustApi::Method("label")),
        p(
            "renderSelectionLabel",
            RustApi::Method("render_selection_label"),
        ),
        p("clearLabel", RustApi::Method("clear_label")),
        p("size", RustApi::Method("size")),
        p("variant", RustApi::Method("variant")),
        p("xstyle", RustApi::Trait("Styled")),
    ],
    example: include_str!("examples.rs"),
};
