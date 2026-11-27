//! Grouped rows (upstream `useTableGroupedRows`): a flat list in collapsible
//! sections. Each group's header is a synthetic row across the table (a
//! chevron, the key and its count); collapsing hides the group's rows.

use std::collections::HashSet;
use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use rustryx_i18n::{LogicalStyled, current as i18n, use_direction};
use rustryx_primitives::{control_focus, is_focus_visible};
use rustryx_theme::current;

use super::model::{IdKey, TableValue};
use super::plugin::{
    PlanRow, RowModel, SyntheticRow, SyntheticRowProps, TablePlugin, TableRenderContext,
};
use crate::doc::{ComponentDoc, PropMapping, RustApi};
use crate::icon::{Icon, IconSize};

type GroupFn<T> = Rc<dyn Fn(&T) -> SharedString>;
type ToggleFn = Rc<dyn Fn(SharedString, &mut Window, &mut App)>;
type PressFn = Rc<dyn Fn(&mut Window, &mut App)>;
type HeaderFn = Rc<dyn Fn(&SharedString, usize, bool) -> AnyElement>;

/// What [`use_table_grouped_rows`] reads (upstream
/// `UseTableGroupedRowsConfig`).
pub struct GroupedRowsConfig<T> {
    data: Rc<[T]>,
    group_by: GroupFn<T>,
    collapsed: Rc<HashSet<SharedString>>,
    on_toggle: ToggleFn,
    render_header: Option<HeaderFn>,
    row_key: Option<GroupFn<T>>,
    group_order: Vec<SharedString>,
}

impl<T> GroupedRowsConfig<T> {
    /// Upstream `data`, `groupBy`, `collapsedGroups` and `onToggleGroup`.
    pub fn new(
        data: impl Into<Rc<[T]>>,
        group_by: impl Fn(&T) -> SharedString + 'static,
        collapsed_groups: impl IntoIterator<Item = SharedString>,
        on_toggle_group: impl Fn(SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            data: data.into(),
            group_by: Rc::new(group_by),
            collapsed: Rc::new(collapsed_groups.into_iter().collect()),
            on_toggle: Rc::new(on_toggle_group),
            render_header: None,
            row_key: None,
            group_order: Vec::new(),
        }
    }

    /// Upstream `renderGroupHeader`: the header's content after the chevron
    /// (the key, its count and whether it is collapsed); "key (count)" by
    /// default.
    pub fn render_group_header(
        mut self,
        f: impl Fn(&SharedString, usize, bool) -> AnyElement + 'static,
    ) -> Self {
        self.render_header = Some(Rc::new(f));
        self
    }

    /// Upstream `getRowKey`: a stable key for a row; rows are keyed by
    /// position without it.
    pub fn row_key(mut self, f: impl Fn(&T) -> SharedString + 'static) -> Self {
        self.row_key = Some(Rc::new(f));
        self
    }

    /// Upstream `groupOrder`: these groups first; the rest keep their
    /// first-seen order.
    pub fn group_order(mut self, keys: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.group_order = keys.into_iter().map(Into::into).collect();
        self
    }
}

/// The plugin: it plans the body's rows with each group's header.
#[derive(Clone)]
pub struct TableGroupedRows {
    plan: Rc<[PlanRow]>,
}

/// The hook's result (upstream `UseTableGroupedRowsResult`).
pub struct GroupedRows<T> {
    pub plugin: TableGroupedRows,
    /// The rows to show, groups in order, collapsed groups left out: the
    /// table's `data`.
    pub data: Rc<[T]>,
    /// The table's key: `row_key`'s, or none (positions).
    pub id_key: Option<IdKey<T>>,
}

/// Upstream `useTableGroupedRows`: group the rows (first-seen order, then
/// `group_order`), and pass `data`, `id_key` and `plugin` to the table.
/// Grouping runs on the rows it gets: filter, sort and slice first.
pub fn use_table_grouped_rows<T: Clone + 'static>(config: GroupedRowsConfig<T>) -> GroupedRows<T> {
    let mut keys: Vec<SharedString> = Vec::new();
    let mut groups: std::collections::HashMap<SharedString, Vec<T>> = Default::default();
    for item in config.data.iter() {
        let key = (config.group_by)(item);
        match groups.get_mut(&key) {
            Some(bucket) => bucket.push(item.clone()),
            None => {
                keys.push(key.clone());
                groups.insert(key, vec![item.clone()]);
            }
        }
    }
    if !config.group_order.is_empty() {
        let pinned: Vec<SharedString> = config
            .group_order
            .iter()
            .filter(|k| groups.contains_key(*k))
            .cloned()
            .collect();
        let rest: Vec<SharedString> = keys
            .iter()
            .filter(|k| !config.group_order.contains(k))
            .cloned()
            .collect();
        keys = pinned.into_iter().chain(rest).collect();
    }
    let mut data: Vec<T> = Vec::new();
    let mut plan: Vec<PlanRow> = Vec::new();
    for key in keys {
        let rows = groups.remove(&key).unwrap_or_default();
        let collapsed = config.collapsed.contains(&key);
        plan.push(PlanRow::Synthetic(header_row(
            key.clone(),
            rows.len(),
            collapsed,
            config.on_toggle.clone(),
            config.render_header.clone(),
        )));
        if !collapsed {
            for row in rows {
                plan.push(PlanRow::Product(data.len()));
                data.push(row);
            }
        }
    }
    let id_key = config
        .row_key
        .map(|f| IdKey::Fn(Rc::new(move |item: &T| TableValue::Text(f(item)))));
    GroupedRows {
        plugin: TableGroupedRows { plan: plan.into() },
        data: data.into(),
        id_key,
    }
}

/// A group's header row: a click anywhere toggles it; the chevron is the
/// keyboard control.
fn header_row(
    key: SharedString,
    count: usize,
    collapsed: bool,
    on_toggle: ToggleFn,
    render: Option<HeaderFn>,
) -> SyntheticRow {
    SyntheticRow {
        key: key.clone(),
        render: Rc::new(
            move |context: &TableRenderContext, window: &mut Window, cx: &mut App| {
                let theme = current(window, cx);
                let t = theme.tokens();
                let rem = window.rem_size();
                let dir = use_direction(cx);
                let style = div()
                    .cursor_pointer()
                    .bg(t.color.background_muted)
                    .border_b(t.border.width.to_pixels(rem))
                    .border_color(t.color.border)
                    .style()
                    .clone();
                let cell_style = div()
                    .py(t.spacing.s2.to_pixels(rem))
                    .padding_end(dir, t.spacing.s3.to_pixels(rem))
                    .style()
                    .clone();
                let toggle_row = on_toggle.clone();
                let row_key = key.clone();
                let tr = i18n(cx);
                let label = tr.format(
                    if collapsed {
                        "@astryx.tableGroupedRows.expandGroup"
                    } else {
                        "@astryx.tableGroupedRows.collapseGroup"
                    },
                    &[("groupKey", key.as_ref().into())],
                );
                let content: AnyElement = match &render {
                    Some(f) => f(&key, count, collapsed),
                    None => div()
                        .flex()
                        .gap(px(4.))
                        .font_weight(t.font.weight.semibold)
                        .text_color(t.color.text_primary)
                        .child(key.clone())
                        .child(
                            div()
                                .font_weight(t.font.weight.normal)
                                .text_color(t.color.text_secondary)
                                .child(format!("({count})")),
                        )
                        .into_any_element(),
                };
                // The built-in heading stays in view as the table scrolls
                // sideways (upstream pins it); a custom one keeps the full width
                // and only its chevron would pin.
                let pinned = render.is_none();
                let offset = if pinned {
                    -context.viewport.offset().x
                } else {
                    px(0.)
                };
                let toggle = on_toggle.clone();
                let chevron_key = key.clone();
                let inner = div()
                    .relative()
                    .left(offset)
                    .flex()
                    .items_center()
                    .gap(t.spacing.s1.to_pixels(rem))
                    .padding_start(dir, t.spacing.s1.to_pixels(rem))
                    .child(Chevron {
                        id: ElementId::Name(format!("group-{key}-toggle").into()),
                        label,
                        collapsed,
                        on_toggle: Rc::new(move |window, cx| {
                            toggle(chevron_key.clone(), window, cx)
                        }),
                    })
                    .child(content)
                    .into_any_element();
                SyntheticRowProps {
                    style,
                    cell_style,
                    content: inner,
                    hooks: vec![Box::new(move |row| {
                        row.on_click(move |_, window, cx| toggle_row(row_key.clone(), window, cx))
                    })],
                }
            },
        ),
    }
}

/// The group's disclosure button: borderless, flush with the table's edge.
#[derive(IntoElement)]
struct Chevron {
    id: ElementId,
    label: SharedString,
    collapsed: bool,
    on_toggle: PressFn,
}

impl RenderOnce for Chevron {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = current(window, cx);
        let t = theme.tokens();
        let rem = window.rem_size();
        let rtl = use_direction(cx).is_rtl();
        let focus = control_focus(&self.id, window, cx);
        let selector = format!("{}", self.id);
        let hover = t.color.icon_primary;
        let toggle = self.on_toggle;
        let radius = t.radius.inner.to_pixels(rem);
        let mut el = div()
            .id(self.id)
            .debug_selector(move || selector)
            .track_focus(&focus)
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded(radius)
            .cursor_pointer()
            .text_color(t.color.icon_secondary)
            .hover(move |s| s.text_color(hover))
            .role(Role::Button)
            .aria_label(self.label)
            .aria_expanded(!self.collapsed)
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                toggle(window, cx)
            });
        if is_focus_visible(&focus, window) {
            el = el.child(crate::focus::focus_ring(t, radius, rem));
        }
        let quarter = std::f32::consts::FRAC_PI_2;
        let turn = match (self.collapsed, rtl) {
            (true, false) => 0.,
            (true, true) => std::f32::consts::PI,
            (false, _) => quarter,
        };
        el.child(Icon::new("chevronRight").size(IconSize::Sm).rotation(turn))
    }
}

impl<T> TablePlugin<T> for TableGroupedRows {
    fn row_model(&self) -> Option<RowModel> {
        Some(RowModel::Grouped)
    }

    fn transforms_body(&self) -> bool {
        true
    }

    fn row_plan(&self) -> Option<Rc<[PlanRow]>> {
        Some(self.plan.clone())
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

pub const USE_TABLE_GROUPED_ROWS_DOC: ComponentDoc = ComponentDoc {
    name: "useTableGroupedRows",
    rust_type: "rustryx::components::table::use_table_grouped_rows",
    props: &[
        p("data", RustApi::Argument("GroupedRowsConfig::new")),
        p("groupBy", RustApi::Argument("GroupedRowsConfig::new")),
        p(
            "collapsedGroups",
            RustApi::Argument("GroupedRowsConfig::new"),
        ),
        p("onToggleGroup", RustApi::Argument("GroupedRowsConfig::new")),
        p("renderGroupHeader", RustApi::Method("render_group_header")),
        p("getRowKey", RustApi::Method("row_key")),
        p("groupOrder", RustApi::Method("group_order")),
    ],
    example: include_str!("examples.rs"),
};
