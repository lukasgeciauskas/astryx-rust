//! Row expansion (upstream `useTableRowExpansion`): a chevron column that
//! opens a full-width detail panel below its row, with the same toggle in
//! the row's right-click menu. The caller owns the expanded keys.
//!
//! The panel takes its row's divider while open (FR5). With an `animated_layout`
//! handle (FR15, AST-105) an opening panel reveals from its block-end edge while the
//! rows after it glide down with that edge, and a closing panel hides at once while
//! its space is held and shrinks, so the rows after it glide up into it; the chevron
//! turns on the same motion. A table with column lines changes at once. Without a
//! handle the panel shows and hides at once and the chevron turns on the theme's
//! `--duration-fast`.

use std::collections::HashSet;
use std::rc::Rc;

use astryx_i18n::{LogicalStyled, current as i18n, use_direction};
use astryx_primitives::{control_focus, is_focus_visible, use_context};
use astryx_theme::current;
use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use super::cells::{CellDensity, TableDensity, density_padding};
use super::model::{TableColumn, TableItem, pixel};
use super::plugin::{
    BodyCellProps, BodyRowProps, HeaderCellProps, RowModel, TableContextAction,
    TableContextActions, TablePlugin, TableRowContext, WholeRowOwner,
};
use crate::animated_layout::adopt::{indicator_turn, reveal_content};
use crate::animated_layout::{AnimatedLayout, use_edge_reveal};
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::icon::{Icon, IconSize, TABLE_ROW_EXPAND, component_icon};

/// The chevron column's key.
pub const EXPANSION_COLUMN_KEY: &str = "__expansion";
const EXPANSION_COLUMN_WIDTH: f32 = 40.;

type KeyFn<T> = Rc<dyn Fn(&T) -> SharedString>;
type ToggleFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
type Predicate<T> = Rc<dyn Fn(&T) -> bool>;
type PressFn = Rc<dyn Fn(&mut Window, &mut App)>;
type PanelFn<T> = Rc<dyn Fn(&TableItem<T>, &mut Window, &mut App) -> AnyElement>;

/// The panel's surface (upstream `panelVariant`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelVariant {
    /// Nothing painted: the surface behind the table shows.
    #[default]
    Transparent,
    Muted,
}

/// The plugin (upstream `useTableRowExpansion`).
pub struct TableRowExpansion<T> {
    expanded: Rc<HashSet<SharedString>>,
    on_toggle: ToggleFn,
    row_key: KeyFn<T>,
    render_expanded: PanelFn<T>,
    row_label: Option<KeyFn<T>>,
    is_expandable: Option<Predicate<T>>,
    panel_variant: PanelVariant,
    row_click_expansion: bool,
    animated_layout: Option<AnimatedLayout>,
}

impl<T> Clone for TableRowExpansion<T> {
    fn clone(&self) -> Self {
        Self {
            expanded: self.expanded.clone(),
            on_toggle: self.on_toggle.clone(),
            row_key: self.row_key.clone(),
            render_expanded: self.render_expanded.clone(),
            row_label: self.row_label.clone(),
            is_expandable: self.is_expandable.clone(),
            panel_variant: self.panel_variant,
            row_click_expansion: self.row_click_expansion,
            animated_layout: self.animated_layout.clone(),
        }
    }
}

impl<T: 'static> TableRowExpansion<T> {
    /// Upstream `expandedKeys`, `onToggle`, `getRowKey` and
    /// `renderExpanded`.
    pub fn new(
        expanded_keys: impl IntoIterator<Item = SharedString>,
        on_toggle: impl Fn(SharedString, &mut Window, &mut App) + 'static,
        row_key: impl Fn(&T) -> SharedString + 'static,
        render_expanded: impl Fn(&TableItem<T>, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self {
            expanded: Rc::new(expanded_keys.into_iter().collect()),
            on_toggle: Rc::new(on_toggle),
            row_key: Rc::new(row_key),
            render_expanded: Rc::new(render_expanded),
            row_label: None,
            is_expandable: None,
            panel_variant: PanelVariant::Transparent,
            row_click_expansion: false,
            animated_layout: None,
        }
    }

    /// Upstream `getRowLabel`: the chevron is named "Expand" or "Collapse"
    /// and this (trimmed); blank keeps the generic name. The menu's action
    /// keeps the generic wording (FR4).
    pub fn row_label(mut self, f: impl Fn(&T) -> SharedString + 'static) -> Self {
        self.row_label = Some(Rc::new(f));
        self
    }

    /// Upstream `getIsItemExpandable`: every row by default.
    pub fn is_item_expandable(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.is_expandable = Some(Rc::new(f));
        self
    }

    /// Upstream `panelVariant` (transparent by default).
    pub fn panel_variant(mut self, variant: PanelVariant) -> Self {
        self.panel_variant = variant;
        self
    }

    /// Upstream `hasRowClickExpansion`: a pointer click anywhere on the row
    /// toggles it (presses on its controls do not).
    pub fn row_click_expansion(mut self, on: bool) -> Self {
        self.row_click_expansion = on;
        self
    }

    /// Upstream `animatedLayout`: the handle from [`use_animated_layout`]. With it, a
    /// panel that opens reveals while the rows after it glide to their new places, and
    /// one that closes hides at once while the rows after it glide up into its space;
    /// the chevron turns on the same motion (FR15). A table with column lines
    /// (`TableDividers::Grid` or `Columns`) changes at once, as without it.
    ///
    /// [`use_animated_layout`]: crate::animated_layout::use_animated_layout
    pub fn animated_layout(mut self, layout: AnimatedLayout) -> Self {
        self.animated_layout = Some(layout);
        self
    }

    fn expandable(&self, item: &T) -> bool {
        self.is_expandable.as_ref().is_none_or(|f| f(item))
    }
}

/// Upstream `useTableRowExpansion`.
pub fn use_table_row_expansion<T: 'static>(
    expanded_keys: impl IntoIterator<Item = SharedString>,
    on_toggle: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    row_key: impl Fn(&T) -> SharedString + 'static,
    render_expanded: impl Fn(&TableItem<T>, &mut Window, &mut App) -> AnyElement + 'static,
) -> TableRowExpansion<T> {
    TableRowExpansion::new(expanded_keys, on_toggle, row_key, render_expanded)
}

/// The row's disclosure button.
#[derive(IntoElement)]
struct ExpansionChevron {
    id: ElementId,
    label: SharedString,
    expanded: bool,
    on_toggle: PressFn,
    layout: Option<AnimatedLayout>,
}

/// Whether the table being drawn has column lines: it does not glide (FR15).
fn column_lines(cx: &App) -> bool {
    use_context::<CellDensity>(cx).is_some_and(|d| d.1.columns())
}

impl RenderOnce for ExpansionChevron {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let rtl = use_direction(cx).is_rtl();
        let focus = control_focus(&self.id, window, cx);
        let radius = t.radius.inner.to_pixels(rem);
        // The spacing step, never wider than its column.
        let size = t.spacing.s6.to_pixels(rem).min(px(EXPANSION_COLUMN_WIDTH));
        let (hover_bg, hover_fg) = (t.color.overlay_hover, t.color.icon_primary);
        let selector = format!("{}", self.id);
        let turn_key = format!("{}-turn", self.id);
        let toggle = self.on_toggle;
        let mut el = div()
            .id(self.id)
            .debug_selector(move || selector)
            .track_focus(&focus)
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(size)
            .rounded(radius)
            .cursor_pointer()
            .text_color(t.color.icon_secondary)
            .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
            .role(Role::Button)
            .aria_label(self.label)
            .aria_expanded(self.expanded)
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                toggle(window, cx)
            });
        if is_focus_visible(&focus, window) {
            el = el.child(crate::focus::focus_ring(t, radius, rem));
        }
        let state = if self.expanded {
            "expanded"
        } else {
            "collapsed"
        };
        // The glyph points into the row (mirrored under RTL) and turns a quarter to
        // point down: on the handle's motion, or the theme's own turn with column lines
        // or without a handle.
        let layout = self.layout.filter(|_| !column_lines(cx));
        let turn = indicator_turn(
            turn_key,
            self.expanded,
            layout.as_ref(),
            (t.duration.fast, t.ease.standard),
            window,
            cx,
        );
        if let Some(icon) = component_icon(TABLE_ROW_EXPAND, state, "chevronRight".into(), cx) {
            let (base, quarter) = if rtl {
                (std::f32::consts::PI, -std::f32::consts::FRAC_PI_2)
            } else {
                (0., std::f32::consts::FRAC_PI_2)
            };
            el = el.child(
                Icon::new(icon)
                    .size(IconSize::Xsm)
                    .rotation(base + quarter * turn),
            );
        }
        el
    }
}

/// The open panel: one cell across the row, its content lined up with the
/// first real column, carrying the row's divider.
#[derive(IntoElement)]
struct Panel {
    id: ElementId,
    content: AnyElement,
    muted: bool,
    /// Its row is the table's last: no divider under it.
    last: bool,
}

impl RenderOnce for Panel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let dir = use_direction(cx);
        let (density, dividers) = use_context::<CellDensity>(cx).map_or(
            (TableDensity::Balanced, super::cells::TableDividers::Rows),
            |d| (d.0, d.1),
        );
        let (_, inline) = density_padding(&theme, density, rem);
        let selector = format!("{}", self.id);
        let mut cell = div()
            .id(ElementId::Name(format!("{}-cell", self.id).into()))
            .role(Role::Cell)
            .w_full()
            .py(t.spacing.s4.to_pixels(rem))
            .padding_end(dir, t.spacing.s5.to_pixels(rem))
            .padding_start(dir, px(EXPANSION_COLUMN_WIDTH) + inline);
        if self.muted {
            cell = cell.bg(t.color.background_muted);
        }
        if dividers.rows() && !self.last {
            cell = cell
                .border_b(t.border.width.to_pixels(rem))
                .border_color(t.color.border);
        }
        div()
            .id(self.id)
            .debug_selector(move || selector)
            .w_full()
            .role(Role::Row)
            .child(cell.child(self.content))
    }
}

/// A row's panel under an `animated_layout` handle, open or not: it plays its reveal,
/// and holds its space while it closes (FR15).
#[derive(IntoElement)]
struct ExpansionPanel {
    id: ElementId,
    open: bool,
    layout: AnimatedLayout,
    /// The panel, while open.
    panel: Option<AnyElement>,
}

/// The panel's height when last open.
struct HeldHeight(gpui::Pixels);

impl RenderOnce for ExpansionPanel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let layout = (!column_lines(cx)).then_some(&self.layout);
        let reveal = use_edge_reveal(
            ElementId::Name(format!("{}-reveal", self.id).into()),
            self.open,
            layout,
            window,
            cx,
        );
        let held = window.use_keyed_state(
            ElementId::Name(format!("{}-held", self.id).into()),
            cx,
            |_, _| HeldHeight(px(0.)),
        );
        match self.panel {
            Some(panel) => {
                let measure = held.clone();
                let panel = div()
                    .child(panel)
                    .on_children_prepainted(move |bounds, _, cx| {
                        if let Some(b) = bounds.first() {
                            let h = b.size.height;
                            if measure.read(cx).0 != h {
                                measure.update(cx, |m, _| m.0 = h);
                            }
                        }
                    });
                reveal_content(reveal, panel, false, window, cx)
                    .unwrap_or_else(|| div().into_any_element())
            }
            // Closing: the panel is gone at once; its space shrinks on the motion.
            None if reveal.is_moving() => {
                div().h(held.read(cx).0 * reveal.shown()).into_any_element()
            }
            None => div().into_any_element(),
        }
    }
}

impl<T: 'static> TablePlugin<T> for TableRowExpansion<T> {
    fn row_model(&self) -> Option<RowModel> {
        Some(RowModel::Detail)
    }

    fn transforms_body(&self) -> bool {
        true
    }

    fn whole_row_owner(&self) -> Option<WholeRowOwner> {
        self.row_click_expansion.then_some(WholeRowOwner::Expansion)
    }

    fn transform_columns(
        &self,
        mut columns: Vec<TableColumn<T>>,
        _: &mut Window,
        _: &mut App,
    ) -> Vec<TableColumn<T>> {
        let this = self.clone();
        columns.insert(
            0,
            TableColumn::new(EXPANSION_COLUMN_KEY)
                // Only chevrons: the header is named below.
                .header_element(|_, _| div().into_any_element())
                .width(pixel(EXPANSION_COLUMN_WIDTH))
                .resizable(false)
                .render_cell_item(move |item, _, cx| {
                    if !this.expandable(item) {
                        return div().into_any_element();
                    }
                    let key = (this.row_key)(item);
                    let expanded = this.expanded.contains(&key);
                    let t = i18n(cx);
                    let label = this
                        .row_label
                        .as_ref()
                        .map(|f| SharedString::from(f(item).trim().to_string()))
                        .filter(|l| !l.is_empty());
                    let label = match (label, expanded) {
                        (Some(l), true) => t.format(
                            "@astryx.tableRowExpansion.collapseRowNamed",
                            &[("label", l.as_ref().into())],
                        ),
                        (Some(l), false) => t.format(
                            "@astryx.tableRowExpansion.expandRowNamed",
                            &[("label", l.as_ref().into())],
                        ),
                        (None, true) => t.t("@astryx.tableRowExpansion.collapseRow"),
                        (None, false) => t.t("@astryx.tableRowExpansion.expandRow"),
                    };
                    let toggle = this.on_toggle.clone();
                    let toggle_key = key.clone();
                    let layout = this.animated_layout.clone();
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .w_full()
                        .child(ExpansionChevron {
                            id: ElementId::Name(format!("expansion-{key}").into()),
                            label,
                            expanded,
                            on_toggle: Rc::new(move |window, cx| {
                                toggle(toggle_key.clone(), window, cx)
                            }),
                            layout,
                        })
                        .into_any_element()
                }),
        );
        columns
    }

    fn transform_header_cell(
        &self,
        mut props: HeaderCellProps,
        column: &TableColumn<T>,
        _: &[TableColumn<T>],
        _: &mut Window,
        cx: &mut App,
    ) -> HeaderCellProps {
        if column.key.as_ref() == EXPANSION_COLUMN_KEY {
            props.aria_label = Some(i18n(cx).t("@astryx.tableRowExpansion.columnHeader"));
        }
        props
    }

    fn transform_body_cell(
        &self,
        mut props: BodyCellProps,
        _: &TableColumn<T>,
        item: &TableItem<T>,
        _: &[TableColumn<T>],
        _: &TableRowContext,
        _: &mut Window,
        _: &mut App,
    ) -> BodyCellProps {
        if !self.expandable(item) {
            return props;
        }
        let key = (self.row_key)(item);
        let open = self.expanded.contains(&key);
        // An open row's panel carries the divider below it (FR5).
        if open {
            props.style.border_widths.bottom = Some(px(0.).into());
        }
        // The same toggle in every cell's menu, read when the menu opens.
        let toggle = self.on_toggle.clone();
        props
            .context_menu
            .push(TableContextActions::lazy(move |cx| {
                let t = i18n(cx);
                let (toggle, key) = (toggle.clone(), key.clone());
                vec![
                    TableContextAction::new(
                        "row-expansion-toggle",
                        t.t(if open {
                            "@astryx.tableRowExpansion.collapseRow"
                        } else {
                            "@astryx.tableRowExpansion.expandRow"
                        }),
                        move |window, cx| toggle(key.clone(), window, cx),
                    )
                    .group("row-expansion")
                    .icon(if open { "chevronDown" } else { "chevronRight" }),
                ]
            }));
        props
    }

    fn transform_body_row(
        &self,
        mut props: BodyRowProps,
        item: &TableItem<T>,
        context: &TableRowContext,
        window: &mut Window,
        cx: &mut App,
    ) -> BodyRowProps {
        if !self.expandable(item) {
            return props;
        }
        let key = (self.row_key)(item);
        if self.row_click_expansion {
            let (toggle, click_key) = (self.on_toggle.clone(), key.clone());
            props.hooks.push(Box::new(move |row| {
                row.cursor_pointer().on_click(move |event, window, cx| {
                    // Pointer only; a nested control's press stops here.
                    if event.is_keyboard() || window.default_prevented() {
                        return;
                    }
                    toggle(click_key.clone(), window, cx)
                })
            }));
        }
        let open = self.expanded.contains(&key);
        let panel_id = ElementId::Name(format!("{}-panel-{key}", context.render.table).into());
        let panel = open.then(|| {
            let last = context.logical_index + 1 == context.logical_row_count;
            Panel {
                id: panel_id.clone(),
                content: (self.render_expanded)(item, window, cx),
                muted: self.panel_variant == PanelVariant::Muted,
                last,
            }
            .into_any_element()
        });
        match &self.animated_layout {
            // FR15: every expandable row keeps its panel's place, so a change plays.
            Some(layout) => props.after_row.push(
                ExpansionPanel {
                    id: ElementId::Name(format!("{panel_id}-motion").into()),
                    open,
                    layout: layout.clone(),
                    panel,
                }
                .into_any_element(),
            ),
            None => props.after_row.extend(panel),
        }
        props
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_ROW_EXPANSION_DOC: ComponentDoc = ComponentDoc {
    name: "useTableRowExpansion",
    rust_type: "astryx::components::table::TableRowExpansion",
    props: &[
        p("expandedKeys", RustApi::Argument("expanded_keys")),
        p("onToggle", RustApi::Argument("on_toggle")),
        p("getRowKey", RustApi::Argument("row_key")),
        p("getRowLabel", RustApi::Method("row_label")),
        p("renderExpanded", RustApi::Argument("render_expanded")),
        p("getIsItemExpandable", RustApi::Method("is_item_expandable")),
        p("panelVariant", RustApi::Method("panel_variant")),
        p(
            "hasRowClickExpansion",
            RustApi::Method("row_click_expansion"),
        ),
        p("animatedLayout", RustApi::Method("animated_layout")),
    ],
    example: include_str!("examples.rs"),
};
