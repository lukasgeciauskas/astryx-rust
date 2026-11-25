//! Table (upstream `Table.tsx` with `BaseTable.tsx`): a data-driven table,
//! or the caller's composition of TableHeader, TableBody and TableFooter.
//!
//! Layout: a data-driven table uses upstream's fixed layout, columns from
//! their definitions, never from content (FR13), as one grid template that
//! every row repeats, so each row keeps its own box. Children mode uses
//! upstream's automatic layout, each column as wide as its content needs
//! (FR24), as one shared grid whose rows lay out their cells as its own
//! (`display: contents`); there each cell paints its row's fill.

use std::rc::Rc;

use astryx_i18n::{current as i18n, use_direction};
use astryx_primitives::{provide, use_context};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, FocusHandle, GridTrack, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, Pixels, RenderOnce, Role, ScrollHandle, SharedString,
    StatefulInteractiveElement, StyleRefinement, Styled, Window, div, point, prelude::*, px,
};
use smallvec::SmallVec;

use super::cells::{
    CellPlace, RowState, TableDensity, TableDividers, TableStyle, TableTextOverflow,
    TableVerticalAlign, cell, row_background, row_theme_props,
};
use super::menu;
use super::model::{
    ColumnHeader, IdKey, RowKey, TableColumn, TableItem, TableRecord, column_tracks,
    generate_columns, resolve_identity, resolve_table_min_width,
};
use super::plugin::{
    BodyCellProps, BodyRowProps, HeaderCellProps, HeaderRowProps, PlanRow, RowKind,
    ScrollWrapperProps, SyntheticRow, TableMode, TablePlugin, TableRenderContext, TableRenderProps,
    TableRowContext, admitted_row_model, order_plugins,
};
use crate::containers::EmptyState;
use crate::layout::{ContainerChild, ContainerCorners, ContainerPadding};
use crate::text::Text;

enum EmptyChoice {
    Default,
    Custom(AnyElement),
    Off,
}

/// A table (upstream `Table`): rows from `data` through `columns`, or the
/// caller's composition (children mode, [`Table::composed`]).
#[derive(IntoElement)]
pub struct Table<T: TableRecord> {
    id: ElementId,
    data: Option<Rc<[T]>>,
    columns: Option<Vec<TableColumn<T>>>,
    id_key: Option<IdKey<T>>,
    density: TableDensity,
    dividers: TableDividers,
    striped: bool,
    hover: bool,
    vertical_align: TableVerticalAlign,
    text_overflow: TableTextOverflow,
    plugins: Vec<(SharedString, Rc<dyn TablePlugin<T>>)>,
    row_index_start: Option<usize>,
    row_count: Option<usize>,
    empty: EmptyChoice,
    label: Option<SharedString>,
    children: SmallVec<[AnyElement; 3]>,
    style: StyleRefinement,
}

impl Table<super::model::Record> {
    /// A table the caller composes from TableHeader, TableBody and
    /// TableFooter (upstream children mode).
    pub fn composed(id: impl Into<ElementId>) -> Self {
        Table::new(id)
    }
}

impl<T: TableRecord> Table<T> {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Table {
            id: id.into(),
            data: None,
            columns: None,
            id_key: None,
            density: TableDensity::Balanced,
            dividers: TableDividers::Rows,
            striped: false,
            hover: false,
            vertical_align: TableVerticalAlign::Middle,
            text_overflow: TableTextOverflow::Wrap,
            plugins: Vec::new(),
            row_index_start: None,
            row_count: None,
            empty: EmptyChoice::Default,
            label: None,
            children: SmallVec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Upstream `data`: one row per item.
    pub fn data(mut self, data: impl Into<Rc<[T]>>) -> Self {
        self.data = Some(data.into());
        self
    }

    /// Upstream `columns`; generated from the first item's keys without.
    pub fn columns(mut self, columns: impl IntoIterator<Item = TableColumn<T>>) -> Self {
        self.columns = Some(columns.into_iter().collect());
        self
    }

    /// Upstream `idKey` as a field: its string or finite number, as a string.
    pub fn id_key(mut self, field: impl Into<SharedString>) -> Self {
        self.id_key = Some(IdKey::Field(field.into()));
        self
    }

    /// Upstream `idKey` as a function: a string or finite number, kept as
    /// it is.
    pub fn id_key_fn(mut self, f: impl Fn(&T) -> super::model::TableValue + 'static) -> Self {
        self.id_key = Some(IdKey::Fn(Rc::new(f)));
        self
    }

    /// Upstream `idKey` as a ready key (a field or a function), or none:
    /// what a state hook hands back.
    pub fn id_key_with(mut self, key: Option<IdKey<T>>) -> Self {
        self.id_key = key;
        self
    }

    /// Upstream `density`.
    pub fn density(mut self, density: TableDensity) -> Self {
        self.density = density;
        self
    }

    /// Upstream `dividers`.
    pub fn dividers(mut self, dividers: TableDividers) -> Self {
        self.dividers = dividers;
        self
    }

    /// Upstream `isStriped`: a wash on the even-numbered body rows.
    pub fn striped(mut self, striped: bool) -> Self {
        self.striped = striped;
        self
    }

    /// Upstream `hasHover`: rows highlight under a pointer.
    pub fn hover(mut self, hover: bool) -> Self {
        self.hover = hover;
        self
    }

    /// Upstream `verticalAlign`.
    pub fn vertical_align(mut self, align: TableVerticalAlign) -> Self {
        self.vertical_align = align;
        self
    }

    /// Upstream `textOverflow`.
    pub fn text_overflow(mut self, overflow: TableTextOverflow) -> Self {
        self.text_overflow = overflow;
        self
    }

    /// Upstream `plugins`: a named plugin. Known names run in their
    /// canonical order, others after them as added (FR9).
    pub fn plugin(
        mut self,
        name: impl Into<SharedString>,
        plugin: impl TablePlugin<T> + 'static,
    ) -> Self {
        self.plugins.push((name.into(), Rc::new(plugin)));
        self
    }

    /// A named plugin already shared.
    pub fn plugin_rc(
        mut self,
        name: impl Into<SharedString>,
        plugin: Rc<dyn TablePlugin<T>>,
    ) -> Self {
        self.plugins.push((name.into(), plugin));
        self
    }

    /// Upstream `rowIndexStart`: the ARIA row index of the first body row,
    /// which opts the table into row indexes.
    pub fn row_index_start(mut self, start: usize) -> Self {
        self.row_index_start = Some(start);
        self
    }

    /// Upstream `rowCount`: every body row across pages.
    pub fn row_count(mut self, count: usize) -> Self {
        self.row_count = Some(count);
        self
    }

    /// Upstream `emptyState`: what shows when `data` is empty.
    pub fn empty_state(mut self, empty: impl IntoElement) -> Self {
        self.empty = EmptyChoice::Custom(empty.into_any_element());
        self
    }

    /// Upstream `emptyState={false}`: no empty state.
    pub fn no_empty_state(mut self) -> Self {
        self.empty = EmptyChoice::Off;
        self
    }

    /// The table's name (upstream `aria-label`); its scroll region takes it.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl<T: TableRecord> Styled for Table<T> {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<T: TableRecord> ParentElement for Table<T> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

fn id_text(id: &ElementId) -> String {
    match id {
        ElementId::Name(n) => n.to_string(),
        other => format!("{other:?}"),
    }
}

fn key_text(key: &RowKey) -> String {
    match key {
        RowKey::Number(n) => format!("n{n}"),
        RowKey::Text(t) => format!("s{t}"),
    }
}

/// The scroll region's state: its scroll and its focus.
struct Region {
    scroll: ScrollHandle,
    focus: FocusHandle,
}

impl<T: TableRecord> RenderOnce for Table<T> {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let i18n = i18n(cx);
        let host = use_context::<ContainerPadding>(cx);
        let corners = use_context::<ContainerCorners>(cx).unwrap_or_default();
        let place = use_context::<ContainerChild>(cx);
        let style = TableStyle {
            density: self.density,
            dividers: self.dividers,
            striped: self.striped,
            hover: self.hover,
            vertical_align: self.vertical_align,
            text_overflow: self.text_overflow,
            host,
            dir,
        };
        let name = id_text(&self.id);
        let region = window.use_keyed_state(
            ElementId::Name(format!("{name}-table-region").into()),
            cx,
            |_, cx| Region {
                scroll: ScrollHandle::new(),
                focus: cx.focus_handle().tab_stop(false),
            },
        );
        let (scroll, focus) = {
            let r = region.read(cx);
            (r.scroll.clone(), r.focus.clone())
        };
        let plugins: Vec<Rc<dyn TablePlugin<T>>> = order_plugins(&self.plugins);
        let label = self.label.clone();
        let data_mode = self.data.is_some() || self.columns.is_some() || self.children.is_empty();

        // ---- The table ---------------------------------------------------
        let (table, render_context) = if data_mode {
            let data = self.data.clone().unwrap_or_else(|| Rc::from(Vec::new()));
            let (table, context) = render_data(
                &mut self, &data, &plugins, &style, &scroll, &focus, window, cx,
            );
            // What a cell's lazily rendered controls read (a tree's expander).
            (
                provide(
                    super::cells::CellDensity(style.density, style.dividers),
                    table,
                )
                .into_any_element(),
                context,
            )
        } else {
            let context = TableRenderContext {
                table: name.clone().into(),
                mode: TableMode::Children,
                row_model: admitted_row_model(&plugins),
                logical_row_count: 0,
                header_row_count: 0,
                viewport: scroll.clone(),
                viewport_focus: Some(focus.clone()),
                whole_row_owners: 0,
            };
            let tracks = match self.text_overflow {
                TableTextOverflow::Wrap => vec![GridTrack::Auto],
                TableTextOverflow::Truncate => vec![GridTrack::MinMax(
                    Box::new(GridTrack::Length(gpui::AbsoluteLength::Pixels(px(0.)))),
                    Box::new(GridTrack::Fr(1.)),
                )],
            };
            let mut table = div()
                .id(ElementId::Name(format!("{name}-table").into()))
                .grid()
                .grid_auto_columns(tracks)
                .w_full()
                .role(Role::Table);
            if let Some(label) = &label {
                table = table.aria_label(label.clone());
            }
            theme.refine(table.style(), "table", &[], None);
            let children = std::mem::take(&mut self.children);
            let table = provide(
                super::parts::TableParts {
                    style,
                    table: name.clone().into(),
                },
                table.children(children),
            )
            .into_any_element();
            (table, context)
        };

        // ---- The scroll region (FR1) ----------------------------------------
        let mut wrapper = ScrollWrapperProps {
            style: StyleRefinement::default(),
            before_table: Vec::new(),
            after_table: Vec::new(),
            hooks: Vec::new(),
        };
        for p in &plugins {
            wrapper = p.transform_scroll_wrapper(wrapper, &render_context, window, cx);
        }
        // A tab stop once it scrolls, with the arrows scrolling it.
        let scrollable = scroll.max_offset().x > px(0.);
        let region_name = label
            .clone()
            .unwrap_or_else(|| i18n.t("@astryx.table.label"));
        let step = px(40.);
        let keys_scroll = scroll.clone();
        let mut region_el = div()
            .id(ElementId::Name(format!("{name}-scroll").into()))
            .track_focus(&focus.clone().tab_stop(scrollable))
            .track_scroll(&scroll)
            .overflow_x_scroll()
            .relative()
            .role(Role::Group)
            .aria_label(region_name)
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let max = keys_scroll.max_offset().x;
                let offset = keys_scroll.offset();
                let x = match event.keystroke.key.as_str() {
                    "left" => offset.x + step,
                    "right" => offset.x - step,
                    "home" => px(0.),
                    "end" => -max,
                    _ => return,
                };
                keys_scroll.set_offset(point(x.min(px(0.)).max(-max), offset.y));
                cx.stop_propagation();
                window.refresh();
            });
        if astryx_primitives::is_focus_visible(&focus, window) {
            region_el = region_el.child(crate::focus::focus_ring_inset(t, px(0.), rem));
        }
        // It bleeds to its padding host's edges (containerBleed).
        if let Some(host) = host {
            region_el = region_el
                .margin_start_logical(dir, -host.inline_start)
                .margin_end_logical(dir, -host.inline_end);
            if place.is_some_and(|p| p.first) {
                region_el = region_el
                    .mt(-host.block_start)
                    .rounded_tl(corners.start_start)
                    .rounded_tr(corners.start_end);
            }
            if place.is_some_and(|p| p.last) {
                region_el = region_el
                    .mb(-host.block_end)
                    .rounded_bl(corners.end_start)
                    .rounded_br(corners.end_end);
            }
        }
        theme.refine(region_el.style(), "table-scroll-wrapper", &[], None);
        region_el.style().refine(&wrapper.style);
        for hook in wrapper.hooks {
            region_el = hook(region_el);
        }
        let mut out: AnyElement = region_el
            .children(wrapper.before_table)
            .child(table)
            .children(wrapper.after_table)
            .into_any_element();
        // Providers: the first plugin wraps outermost (FR11).
        for p in plugins.iter().rev() {
            out = p.transform_table_context(out, window, cx);
        }
        let mut root = div().w_full().min_w_0();
        root.style().refine(&self.style);
        root.child(out)
    }
}

/// A row's fill and hover, as its pinned cells replay them.
type RowBackdrop =
    Rc<std::cell::Cell<Option<(Option<gpui::Hsla>, Option<(SharedString, gpui::Hsla)>)>>>;

/// The row's fill under a pinned cell's content, read once the row has
/// resolved it (the cell is built first).
#[derive(IntoElement)]
struct PinnedBackdrop(RowBackdrop);

impl RenderOnce for PinnedBackdrop {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (fill, hover) = self.0.take().unwrap_or((None, None));
        self.0.set(Some((fill, hover.clone())));
        let mut el = div().absolute().top_0().left_0().size_full();
        if let Some(fill) = fill {
            el = el.bg(fill);
        }
        if let Some((group, hover)) = hover {
            el = el.group_hover(group, move |s| s.bg(hover));
        }
        el
    }
}

/// Pins a cell to its edge: it follows the region's sideways scroll,
/// covers what scrolls under it with an opaque base (the row's fill on top),
/// and casts a shadow from its run's inner edge while content is under it
/// (upstream's `position: sticky` cells).
fn pin_cell(
    el: gpui::Stateful<gpui::Div>,
    pin: super::plugin::CellPin,
    viewport: &ScrollHandle,
    backdrop: Option<RowBackdrop>,
    theme: &astryx_theme::ResolvedTheme,
) -> gpui::Stateful<gpui::Div> {
    use super::plugin::PinEdge;
    let t = theme.tokens();
    let scrolled = -viewport.offset().x;
    let max = viewport.max_offset().x;
    let (shift, under) = match pin.edge {
        PinEdge::Start => (scrolled, scrolled > px(0.5)),
        PinEdge::End => (scrolled - max, scrolled < max - px(0.5)),
    };
    let mut el = el.left(shift).bg(t.color.background_card);
    // Its shadow bleeds past its edge.
    el.style().overflow.x = Some(gpui::Overflow::Visible);
    el.style().overflow.y = Some(gpui::Overflow::Visible);
    if let Some(backdrop) = backdrop {
        el = el.child(PinnedBackdrop(backdrop));
    }
    if pin.boundary && under {
        let width = px(6.);
        let shadow = t.color.shadow;
        let clear = gpui::Hsla { a: 0., ..shadow };
        let strip = div().absolute().top_0().bottom_0().w(width);
        el = el.child(match pin.edge {
            PinEdge::Start => strip.right(-width).bg(gpui::linear_gradient(
                90.,
                gpui::linear_color_stop(shadow, 0.),
                gpui::linear_color_stop(clear, 1.),
            )),
            PinEdge::End => strip.left(-width).bg(gpui::linear_gradient(
                270.,
                gpui::linear_color_stop(shadow, 0.),
                gpui::linear_color_stop(clear, 1.),
            )),
        });
    }
    el
}

/// A plugin's synthetic row: one cell across every column.
#[allow(clippy::too_many_arguments)]
fn synthetic_row(
    table: &str,
    row: &SyntheticRow,
    context: &TableRenderContext,
    tracks: Vec<GridTrack>,
    count: usize,
    aria_row_index: Option<usize>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let props = (row.render)(context, window, cx);
    let selector = format!("{table}-row-g{}", row.key);
    let mut el = div()
        .id(ElementId::Name(selector.clone().into()))
        .debug_selector(move || selector.clone())
        .grid()
        .grid_template_columns(tracks)
        .w_full()
        .role(Role::Row);
    if let Some(i) = aria_row_index {
        el = el.aria_row_index(i);
    }
    el.style().refine(&props.style);
    for hook in props.hooks {
        el = hook(el);
    }
    let mut cell = div()
        .id(ElementId::Name(format!("{table}-cell-g{}", row.key).into()))
        .col_start(1)
        .col_end((count.max(1) + 1) as i16)
        .min_w_0()
        .role(Role::Cell);
    cell.style().refine(&props.cell_style);
    el.child(cell.child(props.content)).into_any_element()
}

/// Logical margins, for the bleed.
trait LogicalMargins: Styled + Sized {
    fn margin_start_logical(self, dir: astryx_i18n::Direction, v: Pixels) -> Self {
        if dir.is_rtl() { self.mr(v) } else { self.ml(v) }
    }

    fn margin_end_logical(self, dir: astryx_i18n::Direction, v: Pixels) -> Self {
        if dir.is_rtl() { self.ml(v) } else { self.mr(v) }
    }
}

impl<E: Styled> LogicalMargins for E {}

/// A data-driven table's element and render context.
#[allow(clippy::too_many_arguments)]
fn render_data<T: TableRecord>(
    this: &mut Table<T>,
    data: &Rc<[T]>,
    plugins: &[Rc<dyn TablePlugin<T>>],
    style: &TableStyle,
    scroll: &ScrollHandle,
    focus: &FocusHandle,
    window: &mut Window,
    cx: &mut App,
) -> (AnyElement, TableRenderContext) {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let name = id_text(&this.id);

    // ---- Columns ---------------------------------------------------------
    let mut columns = this
        .columns
        .take()
        .unwrap_or_else(|| generate_columns(data));
    for p in plugins {
        columns = p.transform_columns(columns, window, cx);
    }
    let tracks = column_tracks(&columns);
    let min_width = resolve_table_min_width(&columns);
    let count = columns.len();
    let row_model = admitted_row_model(plugins);
    // Canonical identity when a plugin needs it (FR16): id_key values with
    // missing, invalid and duplicate keys marked.
    let canonical = plugins.iter().any(|p| p.needs_canonical_identity());
    let identity = resolve_identity(data, this.id_key.as_ref(), canonical);
    let header_rows = usize::from(count > 0);
    let context = TableRenderContext {
        table: name.clone().into(),
        mode: TableMode::Data,
        row_model,
        logical_row_count: data.len(),
        header_row_count: header_rows,
        viewport: scroll.clone(),
        viewport_focus: Some(focus.clone()),
        whole_row_owners: plugins
            .iter()
            .filter(|p| p.whole_row_owner().is_some())
            .count(),
    };
    // A loading source counts only where the table admits loading: flat
    // product rows (FR22).
    let admitted = row_model == super::plugin::RowModel::Flat;
    let loading = admitted && plugins.iter().any(|p| p.is_loading(cx) == Some(true));
    let has_more = admitted && plugins.iter().any(|p| p.has_more(cx) == Some(true));

    // ---- The table's props -------------------------------------------------
    let mut table_props = TableRenderProps::default();
    for p in plugins {
        table_props = p.transform_table(table_props, window, cx);
    }

    // ---- The header (FR2, FR11) ------------------------------------------
    let header_row = (count > 0).then(|| {
        let mut cells: Vec<(AnyElement, bool)> = columns
            .iter()
            .enumerate()
            .map(|(i, col)| {
                let content = match &col.header {
                    Some(ColumnHeader::Text(text)) => text.clone().into_any_element(),
                    Some(ColumnHeader::Element(f)) => f(window, cx),
                    None => col.key.clone().into_any_element(),
                };
                let mut props = HeaderCellProps {
                    style: StyleRefinement::default(),
                    before: Vec::new(),
                    content: Some(content),
                    after: Vec::new(),
                    overlay: Vec::new(),
                    below: Vec::new(),
                    context_menu: Vec::new(),
                    aria_label: None,
                    pinned: None,
                    column_index: i,
                    hooks: Vec::new(),
                };
                for p in plugins {
                    props = p.transform_header_cell(props, col, &columns, window, cx);
                }
                let pinned = props.pinned.is_some();
                (
                    header_cell(&name, col, props, i, count, style, scroll, window, cx),
                    pinned,
                )
            })
            .collect();
        // Pinned cells paint over the cells that scroll under them.
        cells.sort_by_key(|(_, pinned)| *pinned);
        let mut props = HeaderRowProps {
            style: StyleRefinement::default(),
            children: cells.into_iter().map(|(cell, _)| cell).collect(),
            hooks: Vec::new(),
        };
        for p in plugins {
            props = p.transform_header_row(props, window, cx);
        }
        let mut row = div()
            .id(ElementId::Name(format!("{name}-header-row").into()))
            .grid()
            .grid_template_columns(tracks.clone())
            .w_full()
            .role(Role::Row);
        theme.refine(row.style(), "table-row", &[], None);
        row.style().refine(&props.style);
        for hook in props.hooks {
            row = hook(row);
        }
        row.children(props.children)
    });

    // ---- The body ----------------------------------------------------------
    let aria_indexes = this.row_index_start.is_some() || this.row_count.is_some();
    let first_index = this.row_index_start.unwrap_or(1);
    let mut body: Vec<AnyElement> = Vec::new();
    // The body's rows: a plugin's plan (synthetic rows among the product
    // rows), else every product row in order.
    let plan: Rc<[PlanRow]> = plugins
        .iter()
        .find_map(|p| p.row_plan())
        .unwrap_or_else(|| (0..data.len()).map(PlanRow::Product).collect());
    let rows_shown = plan.len();
    for (display, entry) in plan.iter().enumerate() {
        let i = match entry {
            PlanRow::Product(i) if *i < data.len() => *i,
            PlanRow::Product(_) => continue,
            PlanRow::Synthetic(row) => {
                body.push(synthetic_row(
                    &name,
                    row,
                    &context,
                    tracks.clone(),
                    count,
                    aria_indexes.then_some(first_index + display),
                    window,
                    cx,
                ));
                continue;
            }
        };
        let id = &identity[i];
        let item = &TableItem::new(data.clone(), i);
        let row_context = TableRowContext {
            key: id.key.clone(),
            identity_status: id.status,
            kind: if row_model == super::plugin::RowModel::Flat {
                RowKind::Product
            } else {
                RowKind::Unknown
            },
            logical_index: i,
            logical_row_count: data.len(),
            header_row_count: header_rows,
            render: context.clone(),
        };
        let row_key = id.key.as_ref().map_or_else(|| format!("i{i}"), key_text);
        // The row's fill, for its pinned cells: set when the row resolves it.
        let backdrop: RowBackdrop = Rc::default();
        let mut cells: Vec<(AnyElement, bool)> = columns
            .iter()
            .enumerate()
            .map(|(ci, col)| {
                let mut props = BodyCellProps {
                    style: StyleRefinement::default(),
                    context_menu: Vec::new(),
                    column_index: ci,
                    content_suppressed: false,
                    pinned: None,
                    hooks: Vec::new(),
                };
                for p in plugins {
                    props =
                        p.transform_body_cell(props, col, item, &columns, &row_context, window, cx);
                }
                body_cell(
                    &name,
                    &row_key,
                    col,
                    item,
                    props,
                    ci,
                    count,
                    display + 1 == rows_shown,
                    style,
                    scroll,
                    &backdrop,
                    window,
                    cx,
                )
            })
            .collect();
        cells.sort_by_key(|(_, pinned)| *pinned);
        let cells: Vec<AnyElement> = cells.into_iter().map(|(cell, _)| cell).collect();
        let mut props = BodyRowProps {
            style: StyleRefinement::default(),
            children: cells,
            after_row: Vec::new(),
            aria_row_index: aria_indexes.then_some(first_index + display),
            paint: super::plugin::RowPaint::default(),
            tab_stop: None,
            key_shortcuts: Vec::new(),
            row_action: None,
            hooks: Vec::new(),
        };
        for p in plugins {
            props = p.transform_body_row(props, item, &row_context, window, cx);
        }
        let after_row = std::mem::take(&mut props.after_row);
        let row = body_row(
            &name,
            &row_key,
            display,
            tracks.clone(),
            props,
            style,
            &backdrop,
            window,
            cx,
        );
        body.push(row.into_any_element());
        body.extend(after_row);
    }
    // Empty when no row shows: collapsed groups' headers still count.
    let empty = if rows_shown == 0 {
        match std::mem::replace(&mut this.empty, EmptyChoice::Off) {
            EmptyChoice::Custom(el) => Some(el),
            // While the first rows load, "No data" would be wrong (FR22).
            EmptyChoice::Default if !loading => Some(
                EmptyState::new(i18n(cx).t("@astryx.table.noData"))
                    .compact(true)
                    .into_any_element(),
            ),
            _ => None,
        }
    } else {
        None
    };

    // ---- Assembly ----------------------------------------------------------
    let mut table = div()
        .id(ElementId::Name(format!("{name}-table").into()))
        .flex()
        .flex_col()
        .w_full()
        .min_w(px(min_width))
        .role(Role::Table);
    if let Some(label) = &this.label {
        table = table.aria_label(label.clone());
    }
    // An unknown total (ARIA's -1, while more rows may come or with no
    // count given) has no AccessKit form: no count is exposed then.
    if let Some(total) = this.row_count.filter(|_| !has_more) {
        table = table.aria_row_count(total + header_rows);
    }
    theme.refine(table.style(), "table", &[], None);
    table.style().refine(&table_props.style);
    for hook in table_props.hooks {
        table = hook(table);
    }
    let mut header = div()
        .id(ElementId::Name(format!("{name}-header").into()))
        .w_full()
        .role(Role::RowGroup);
    theme.refine(header.style(), "table-header", &[], None);
    let mut body_el = div()
        .id(ElementId::Name(format!("{name}-body").into()))
        .flex()
        .flex_col()
        .w_full()
        .role(Role::RowGroup);
    theme.refine(body_el.style(), "table-body", &[], None);
    let body_el = match empty {
        Some(empty) => body_el.child({
            let selector = format!("{name}-empty");
            div()
                .debug_selector(move || selector)
                .w_full()
                .py(t.spacing.s4.to_pixels(rem))
                .child(empty)
        }),
        None => body_el.children(body),
    };
    let table = table
        .when_some(header_row, |table, row| {
            header = header.child(row);
            table.child(header)
        })
        .child(body_el);
    (table.into_any_element(), context)
}

/// A header cell from its props' slots: `before`, the label row (the
/// label then `after`), the `overlay` laid over the cell and `below`.
#[allow(clippy::too_many_arguments)]
fn header_cell<T>(
    table: &str,
    col: &TableColumn<T>,
    props: HeaderCellProps,
    index: usize,
    count: usize,
    style: &TableStyle,
    viewport: &ScrollHandle,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let truncate = style.text_overflow == TableTextOverflow::Truncate;
    let place = CellPlace {
        header: true,
        first: index == 0,
        last: index + 1 == count,
        last_row: false,
        align: col.align,
    };
    let content = props
        .content
        .unwrap_or_else(|| col.key.clone().into_any_element());
    // The label fills the cell, so a control around it (sorting) does too.
    let label = div()
        .w_full()
        .min_w_0()
        .when(truncate, |d| d.truncate())
        .child(content);
    // A wrapping table's text heading shows in full on hover (upstream's
    // native `title`); a truncating one discloses a cut heading on focus.
    let title = (!truncate)
        .then(|| match &col.header {
            Some(ColumnHeader::Text(t)) => Some(t.clone()),
            None => Some(col.key.clone()),
            Some(ColumnHeader::Element(_)) => None,
        })
        .flatten()
        .filter(|t| !t.is_empty());
    let label = crate::overlay::with_tooltip(
        &ElementId::Name(format!("{table}-header-{}-title", col.key).into()),
        title,
        false,
        label,
    );
    // A truncating heading is disclosed only while keyboard focus is in its
    // header (its sort control) and its text is cut.
    let cell_focus = window
        .use_keyed_state(
            ElementId::Name(format!("{table}-header-{}-focus", col.key).into()),
            cx,
            |_, cx| cx.focus_handle(),
        )
        .read(cx)
        .clone();
    let label = match (truncate, col.header_text().filter(|t| !t.is_empty())) {
        (true, Some(text)) => {
            let cut = window.use_keyed_state(
                ElementId::Name(format!("{table}-header-{}-cut", col.key).into()),
                cx,
                |_, _| false,
            );
            let probe_cut = cut.clone();
            let measured = text.clone();
            let probe = gpui::canvas(
                move |bounds, window, cx| {
                    let style = window.text_style();
                    let rem = window.rem_size();
                    let size = style.font_size.to_pixels(rem);
                    let runs = [style.to_run(measured.len())];
                    let width = window
                        .text_system()
                        .shape_line(measured.clone(), size, &runs, None)
                        .width;
                    let over = width > bounds.size.width + px(0.5);
                    if *probe_cut.read(cx) != over {
                        probe_cut.update(cx, |c, _| *c = over);
                        window.defer(cx, |window, _| window.refresh());
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full();
            let open = *cut.read(cx)
                && cell_focus.contains_focused(window, cx)
                && astryx_primitives::is_focus_within_visible(&cell_focus, window, cx);
            crate::overlay::Tooltip::new(
                ElementId::Name(format!("{table}-header-{}-cut-tip", col.key).into()),
                text,
            )
            .open(Some(open))
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_w_0()
                    .child(label)
                    .child(probe),
            )
            .into_any_element()
        }
        _ => label,
    };
    let label_row = if props.after.is_empty() {
        label
    } else {
        div()
            .flex()
            .items_center()
            .gap(t.spacing.s1.to_pixels(rem))
            .w_full()
            .min_w_0()
            .child(div().flex_1().min_w_0().child(label))
            .children(props.after)
            .into_any_element()
    };
    let mut el = cell(&theme, style, place, rem)
        .id(ElementId::Name(
            format!("{table}-header-{}", col.key).into(),
        ))
        .debug_selector({
            let s = format!("{table}-header-{}", col.key);
            move || s.clone()
        })
        .track_focus(&cell_focus)
        .col_start(index as i16 + 1)
        .col_end(index as i16 + 2)
        // One row: an explicit line keeps auto-placement from wrapping a
        // pinned cell, which paints last, onto a row of its own.
        .row_start(1)
        .row_end(2)
        .role(Role::ColumnHeader);
    if let Some(pin) = props.pinned {
        el = pin_cell(el, pin, viewport, None, &theme);
    }
    if let Some(label) = props.aria_label.clone().or_else(|| col.header_text()) {
        el = el.aria_label(label);
    }
    theme.refine(
        el.style(),
        "table-header-cell",
        &[("density", style.density.as_str())],
        None,
    );
    el.style().refine(&props.style);
    for hook in props.hooks {
        el = hook(el);
    }
    let el = if props.before.is_empty() {
        el.child(label_row)
    } else {
        el.child(
            div()
                .flex()
                .items_center()
                .gap(t.spacing.s2.to_pixels(rem))
                .min_w_0()
                .children(props.before)
                .child(label_row),
        )
    }
    .children(props.overlay)
    .children(props.below);
    menu::with_actions(
        el,
        ElementId::Name(format!("{table}-header-{}-menu", col.key).into()),
        props.context_menu,
        col.header_text(),
        window,
        cx,
    )
    .into_any_element()
}

/// A body cell: the column's renderer, or the default text (one line with a
/// tooltip when it is cut, under truncation), unless suppressed.
#[allow(clippy::too_many_arguments)]
fn body_cell<T: TableRecord>(
    table: &str,
    row_key: &str,
    col: &TableColumn<T>,
    item: &TableItem<T>,
    props: BodyCellProps,
    index: usize,
    count: usize,
    last_row: bool,
    style: &TableStyle,
    viewport: &ScrollHandle,
    backdrop: &RowBackdrop,
    window: &mut Window,
    cx: &mut App,
) -> (AnyElement, bool) {
    let theme = current(window, cx);
    let rem = window.rem_size();
    let content: Option<AnyElement> = if props.content_suppressed {
        None
    } else {
        match &col.render_cell {
            Some(render) => Some(render(item, window, cx)),
            None => {
                let text = item.value(&col.key).display_text();
                if text.is_empty() {
                    None
                } else if style.text_overflow == TableTextOverflow::Truncate {
                    Some(Text::new(text).max_lines(1).into_any_element())
                } else {
                    Some(text.into_any_element())
                }
            }
        }
    };
    // Plugins' decorations wrap the content, empty or not (a tree's indent
    // and expander).
    let content = if props.content_suppressed || col.wrappers.is_empty() {
        content
    } else {
        let mut el = content.unwrap_or_else(|| div().into_any_element());
        for wrap in &col.wrappers {
            el = wrap(item, el, window, cx);
        }
        Some(el)
    };
    let place = CellPlace {
        header: false,
        first: index == 0,
        last: index + 1 == count,
        last_row,
        align: col.align,
    };
    let selector = format!("{table}-cell-{row_key}-{}", col.key);
    let mut el = cell(&theme, style, place, rem)
        .id(ElementId::Name(selector.clone().into()))
        .debug_selector(move || selector.clone())
        .col_start(index as i16 + 1)
        .col_end(index as i16 + 2)
        // One row: an explicit line keeps auto-placement from wrapping a
        // pinned cell, which paints last, onto a row of its own.
        .row_start(1)
        .row_end(2)
        .role(Role::Cell);
    let pinned = props.pinned;
    if let Some(pin) = pinned {
        el = pin_cell(el, pin, viewport, Some(backdrop.clone()), &theme);
    }
    theme.refine(
        el.style(),
        "table-cell",
        &[("density", style.density.as_str())],
        None,
    );
    el.style().refine(&props.style);
    for hook in props.hooks {
        el = hook(el);
    }
    // The cell's own menu, over the whole cell (TableCell FR4).
    let el = menu::with_actions(
        el.children(content),
        ElementId::Name(format!("{table}-cell-{row_key}-{}-menu", col.key).into()),
        props.context_menu,
        None,
        window,
        cx,
    )
    .into_any_element();
    (el, pinned.is_some())
}

/// A body row: the column grid, its paint (selection's wash, the hover
/// overlay, the even rows' stripe) and its theme rule.
#[allow(clippy::too_many_arguments)]
fn body_row(
    table: &str,
    key: &str,
    index: usize,
    tracks: Vec<GridTrack>,
    props: BodyRowProps,
    style: &TableStyle,
    backdrop: &RowBackdrop,
    window: &mut Window,
    cx: &mut App,
) -> gpui::Stateful<gpui::Div> {
    let theme = current(window, cx);
    let t = theme.tokens();
    let rem = window.rem_size();
    let BodyRowProps {
        style: props_style,
        children,
        aria_row_index,
        paint,
        tab_stop,
        key_shortcuts,
        row_action,
        hooks,
        ..
    } = props;
    let selector = format!("{table}-row-{key}");
    let mut row = div()
        .id(ElementId::Name(selector.clone().into()))
        .debug_selector(move || selector.clone())
        .grid()
        .grid_template_columns(tracks)
        .w_full()
        .role(Role::Row);
    // One resolver paints the row from every plugin's state (FR21).
    let state = RowState {
        index,
        hovered: false,
        selected_wash: paint.selected && paint.highlight,
        active: paint.active,
    };
    let fill = row_background(&theme, style, state);
    if let Some(bg) = fill {
        row = row.bg(bg);
    }
    let group: SharedString = format!("{table}-row-{key}-group").into();
    let hover =
        (style.hover && !state.selected_wash && !state.active).then_some(t.color.overlay_hover);
    if let Some(hover) = hover {
        row = row.group(group.clone()).hover(move |s| s.bg(hover));
    }
    // Its pinned cells replay the same fill over their opaque base.
    backdrop.set(Some((fill, hover.map(|h| (group, h)))));
    if let Some(i) = aria_row_index {
        row = row.aria_row_index(i);
    }
    if paint.selected {
        row = row.aria_selected(true);
    }
    // The active row: `aria-current` and an inline-start marker that does not
    // rely on colour (activation DD1, AR5).
    if paint.active {
        let marker = format!("{table}-row-{key}-active");
        row = row.aria_current(gpui::AriaCurrent::True).child(
            div()
                .debug_selector(move || marker.clone())
                .absolute()
                .top_0()
                .bottom_0()
                .when(style.dir.is_rtl(), |d| d.right_0())
                .when(!style.dir.is_rtl(), |d| d.left_0())
                .w(px(3.))
                .bg(t.color.accent),
        );
    }
    let mut theme_props = row_theme_props(style, Some(index));
    if paint.selected {
        theme_props.push(("selected", "true"));
    }
    theme.refine(row.style(), "table-row", &theme_props, None);
    row.style().refine(&props_style);
    if !key_shortcuts.is_empty() {
        row = row.aria_keyshortcuts(SharedString::from(key_shortcuts.join(" ")));
    }
    // A focusable row: a tab stop, or focusable out of the tab order.
    let focus = (tab_stop.is_some() || row_action.is_some()).then(|| {
        let handle = window
            .use_keyed_state(
                ElementId::Name(format!("{table}-row-{key}-focus").into()),
                cx,
                |_, cx| cx.focus_handle(),
            )
            .read(cx)
            .clone();
        handle.tab_stop(tab_stop != Some(false))
    });
    if let Some(focus) = &focus {
        row = row.track_focus(focus);
        if astryx_primitives::is_focus_visible(focus, window) {
            row = row.child(crate::focus::focus_ring_inset(t, px(0.), rem));
        }
    }
    for hook in hooks {
        row = hook(row);
    }
    // The whole-row action runs after every other handler of the row.
    if let (Some(action), Some(focus)) = (row_action, focus) {
        let (on_click, row_focus) = (action.clone(), focus.clone());
        row = row
            .on_click(move |event, window, cx| {
                // Keyboard presses activate on key down, below.
                if event.is_keyboard() || window.default_prevented() {
                    return;
                }
                // A nested control took the press: it holds the focus.
                if row_focus.contains_focused(window, cx) && !row_focus.is_focused(window) {
                    return;
                }
                on_click(window, cx);
            })
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let stroke = &event.keystroke;
                if !focus.is_focused(window) || stroke.modifiers.modified() {
                    return;
                }
                let space = stroke.key == "space";
                if !space && stroke.key != "enter" {
                    return;
                }
                let cancelled = window.default_prevented();
                // Space never scrolls the page from an activatable row.
                if space {
                    window.prevent_default();
                    cx.stop_propagation();
                }
                if !cancelled && !event.is_held {
                    action(window, cx);
                }
            });
    }
    row.relative().children(children)
}
