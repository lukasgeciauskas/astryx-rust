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
    ColumnHeader, IdKey, RowKey, TableColumn, TableRecord, column_tracks, generate_columns,
    resolve_identity, resolve_table_min_width,
};
use super::plugin::{
    BodyCellProps, BodyRowProps, HeaderCellProps, HeaderRowProps, RowKind, ScrollWrapperProps,
    TableMode, TablePlugin, TableRenderContext, TableRenderProps, TableRowContext,
    admitted_row_model, order_plugins,
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
            render_data(&mut self, &data, &plugins, &style, &scroll, window, cx)
        } else {
            let context = TableRenderContext {
                mode: TableMode::Children,
                row_model: admitted_row_model(&plugins),
                logical_row_count: 0,
                header_row_count: 0,
                viewport: scroll.clone(),
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
fn render_data<T: TableRecord>(
    this: &mut Table<T>,
    data: &Rc<[T]>,
    plugins: &[Rc<dyn TablePlugin<T>>],
    style: &TableStyle,
    scroll: &ScrollHandle,
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
    let identity = resolve_identity(data, this.id_key.as_ref(), false);
    let header_rows = usize::from(count > 0);
    let context = TableRenderContext {
        mode: TableMode::Data,
        row_model,
        logical_row_count: data.len(),
        header_row_count: header_rows,
        viewport: scroll.clone(),
    };
    let loading = plugins.iter().any(|p| p.is_loading(cx) == Some(true));
    let has_more = plugins.iter().any(|p| p.has_more(cx) == Some(true));

    // ---- The table's props -------------------------------------------------
    let mut table_props = TableRenderProps::default();
    for p in plugins {
        table_props = p.transform_table(table_props, window, cx);
    }

    // ---- The header (FR2, FR11) ------------------------------------------
    let header_row = (count > 0).then(|| {
        let cells: Vec<AnyElement> = columns
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
                    column_index: i,
                    hooks: Vec::new(),
                };
                for p in plugins {
                    props = p.transform_header_cell(props, col, &columns, window, cx);
                }
                header_cell(&name, col, props, i, count, style, window, cx)
            })
            .collect();
        let mut props = HeaderRowProps {
            style: StyleRefinement::default(),
            children: cells,
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
    for (i, item) in data.iter().enumerate() {
        let id = &identity[i];
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
        let cells: Vec<(AnyElement, Vec<super::plugin::TableContextActions>)> = columns
            .iter()
            .enumerate()
            .map(|(ci, col)| {
                let mut props = BodyCellProps {
                    style: StyleRefinement::default(),
                    context_menu: Vec::new(),
                    column_index: ci,
                    content_suppressed: false,
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
                    i + 1 == data.len(),
                    style,
                    window,
                    cx,
                )
            })
            .collect();
        // A row's menu gathers its cells' actions (one menu per row).
        let (cells, actions): (
            Vec<AnyElement>,
            Vec<Vec<super::plugin::TableContextActions>>,
        ) = cells.into_iter().unzip();
        let mut props = BodyRowProps {
            style: StyleRefinement::default(),
            children: cells,
            after_row: Vec::new(),
            aria_row_index: aria_indexes.then_some(first_index + i),
            selected: None,
            hooks: Vec::new(),
        };
        for p in plugins {
            props = p.transform_body_row(props, item, &row_context, window, cx);
        }
        let row = body_row(
            &name,
            &row_key,
            i,
            tracks.clone(),
            props.style,
            props.children,
            props.aria_row_index,
            props.selected,
            props.hooks,
            style,
            window,
            cx,
        );
        body.push(
            menu::with_actions(
                row,
                ElementId::Name(format!("{name}-row-{row_key}-menu").into()),
                actions.into_iter().flatten().collect(),
                None,
                window,
                cx,
            )
            .into_any_element(),
        );
        body.extend(props.after_row);
    }
    let empty = if data.is_empty() {
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
        Some(empty) => body_el.child(div().w_full().py(t.spacing.s4.to_pixels(rem)).child(empty)),
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
    let label = div()
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
    let label_row = if props.after.is_empty() {
        label
    } else {
        div()
            .flex()
            .items_center()
            .gap(t.spacing.s1.to_pixels(rem))
            .min_w_0()
            .child(div().min_w_0().flex_shrink_1().child(label))
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
        .role(Role::ColumnHeader);
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
    item: &T,
    props: BodyCellProps,
    index: usize,
    count: usize,
    last_row: bool,
    style: &TableStyle,
    window: &mut Window,
    cx: &mut App,
) -> (AnyElement, Vec<super::plugin::TableContextActions>) {
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
        .role(Role::Cell);
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
    (el.children(content).into_any_element(), props.context_menu)
}

/// A body row: the column grid, its paint (selection's wash, the hover
/// overlay, the even rows' stripe) and its theme rule.
#[allow(clippy::too_many_arguments)]
fn body_row(
    table: &str,
    key: &str,
    index: usize,
    tracks: Vec<GridTrack>,
    props_style: StyleRefinement,
    children: Vec<AnyElement>,
    aria_row_index: Option<usize>,
    selected: Option<super::plugin::RowPaint>,
    hooks: Vec<super::plugin::ElementHook>,
    style: &TableStyle,
    window: &mut Window,
    cx: &mut App,
) -> gpui::Stateful<gpui::Div> {
    let theme = current(window, cx);
    let t = theme.tokens();
    let selector = format!("{table}-row-{key}");
    let mut row = div()
        .id(ElementId::Name(selector.clone().into()))
        .debug_selector(move || selector.clone())
        .grid()
        .grid_template_columns(tracks)
        .w_full()
        .role(Role::Row);
    let state = RowState {
        index,
        hovered: false,
        selected_wash: selected.is_some_and(|s| s.selected && s.highlight),
    };
    if let Some(bg) = row_background(&theme, style, state) {
        row = row.bg(bg);
    }
    if style.hover && !state.selected_wash {
        let hover = t.color.overlay_hover;
        row = row.hover(move |s| s.bg(hover));
    }
    if let Some(i) = aria_row_index {
        row = row.aria_row_index(i);
    }
    if selected.is_some_and(|s| s.selected) {
        row = row.aria_selected(true);
    }
    let props = row_theme_props(style, Some(index));
    theme.refine(row.style(), "table-row", &props, None);
    row.style().refine(&props_style);
    for hook in hooks {
        row = hook(row);
    }
    row.relative().children(children)
}
