//! `SankeyChart`: the root of a Sankey (flow) diagram (upstream lab `SankeyChart`). It
//! lays the nodes out in columns and the links between them (upstream `computeLayout`)
//! and provides the layout to the marks inside it: [`SankeyGrid`], [`SankeyLink`],
//! [`SankeyNode`] and [`SankeyLabel`].
//!
//! - Columns come from `columns` or, without them, a topological sort of the links.
//! - The width follows the container but never goes below `min_column_width` per
//!   column: past it the chart scrolls sideways in a named, focusable region.
//! - The plot is a named image, and a graph of up to 100 links is mirrored in a
//!   visually hidden From / To / Value table.

mod marks;

pub mod examples;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, Role, SharedString, Window, div, px};
use rustryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use rustryx_primitives::scroll_area::{
    ScrollAxis, ScrollKeyboardAccess, ScrollRole, UseScrollableAreaOptions, use_scrollable_area,
};
use rustryx_primitives::{provide, use_context};
use rustryx_theme::authoring::js::js_max;

pub use crate::radial::TextAnchor;
pub use marks::{
    LabelPlacement, NodeLabel, SankeyGrid, SankeyLabel, SankeyLink, SankeyLinkColor, SankeyNode,
    default_format, gradient_offsets, link_path, node_labels, oklch_color, on_surface_dark,
};

use crate::radial::plot::MeasuredPlot;
use crate::radial::{DataTable, table_element};

/// An OKLCH color: lightness, chroma and hue in degrees (upstream's `[number, number,
/// number]`).
pub type Oklch = [f64; 3];

/// A node (upstream `SankeyNodeDatum`).
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyNodeDatum {
    /// Upstream `id`: unique.
    pub id: SharedString,
    /// Upstream `label`.
    pub label: SharedString,
    /// Upstream `value`: the total flowing through it.
    pub value: f64,
    /// Upstream `color`, in OKLCH; a palette color in turn when unset.
    pub color: Option<Oklch>,
}

impl SankeyNodeDatum {
    /// A node `id` labelled `label` with `value`.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>, value: f64) -> Self {
        SankeyNodeDatum {
            id: id.into(),
            label: label.into(),
            value,
            color: None,
        }
    }

    /// Upstream `color`.
    pub fn color(mut self, color: Oklch) -> Self {
        self.color = Some(color);
        self
    }
}

/// A flow between two nodes (upstream `SankeyLinkDatum`).
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyLinkDatum {
    pub source: SharedString,
    pub target: SharedString,
    pub value: f64,
}

impl SankeyLinkDatum {
    /// A flow of `value` from `source` to `target`.
    pub fn new(
        source: impl Into<SharedString>,
        target: impl Into<SharedString>,
        value: f64,
    ) -> Self {
        SankeyLinkDatum {
            source: source.into(),
            target: target.into(),
            value,
        }
    }
}

/// A column (upstream `SankeyColumnDef`): its node ids and an optional header.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SankeyColumnDef {
    pub ids: Vec<SharedString>,
    /// Upstream `label`: shown below the column.
    pub label: Option<SharedString>,
}

/// Upstream `SankeyColumn`: the node ids alone, or a definition with a header.
#[derive(Clone, Debug, PartialEq)]
pub enum SankeyColumn {
    Ids(Vec<SharedString>),
    Def(SankeyColumnDef),
}

impl SankeyColumn {
    /// A column of `ids`.
    pub fn ids(ids: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        SankeyColumn::Ids(ids.into_iter().map(Into::into).collect())
    }

    /// A column of `ids` headed `label`.
    pub fn labeled(
        label: impl Into<SharedString>,
        ids: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        SankeyColumn::Def(SankeyColumnDef {
            ids: ids.into_iter().map(Into::into).collect(),
            label: Some(label.into()),
        })
    }

    fn normalized(&self) -> SankeyColumnDef {
        match self {
            SankeyColumn::Ids(ids) => SankeyColumnDef {
                ids: ids.clone(),
                label: None,
            },
            SankeyColumn::Def(def) => def.clone(),
        }
    }
}

/// A node as laid out (upstream `SankeyNodeLayout`).
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyNodeLayout {
    pub id: SharedString,
    pub label: SharedString,
    pub value: f64,
    pub color: Oklch,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub column: usize,
    /// How far down its outgoing (`source_offset`) and incoming links have filled it.
    pub source_offset: f64,
    pub target_offset: f64,
}

/// A link as laid out (upstream `SankeyLinkLayout`); its ends are indices into the
/// layout's nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyLinkLayout {
    pub source: usize,
    pub target: usize,
    pub value: f64,
    pub height: f64,
    pub source_y: f64,
    pub target_y: f64,
}

/// A column as laid out (upstream `SankeyColumnLayout`).
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyColumnLayout {
    pub x: f64,
    pub label: Option<SharedString>,
    pub ids: Vec<SharedString>,
}

/// Upstream's `LayoutResult`.
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyLayout {
    pub nodes: Vec<SankeyNodeLayout>,
    pub links: Vec<SankeyLinkLayout>,
    pub columns: Vec<SankeyColumnLayout>,
    pub value_scale: f64,
    pub max_value: f64,
}

/// Upstream `DEFAULT_PALETTE`: the colors nodes without one take in turn.
pub const DEFAULT_PALETTE: [Oklch; 8] = [
    [0.65, 0.2, 270.],
    [0.6, 0.17, 235.],
    [0.62, 0.16, 190.],
    [0.64, 0.18, 155.],
    [0.58, 0.15, 40.],
    [0.55, 0.14, 350.],
    [0.54, 0.15, 20.],
    [0.56, 0.13, 300.],
];

/// An ordered map with upstream `Map` semantics: a known key keeps its place.
struct OrderedMap<V>(Vec<(SharedString, V)>);

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        OrderedMap(Vec::new())
    }
}

impl<V: Clone> OrderedMap<V> {
    fn get(&self, key: &str) -> Option<&V> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn index(&self, key: &str) -> Option<usize> {
        self.0.iter().position(|(k, _)| k == key)
    }

    fn set(&mut self, key: SharedString, value: V) {
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
    }
}

/// Upstream's topological column assignment (`autoColumns` and
/// `resolveColumnCount`): sources in column 0, every target one past its furthest
/// source; nodes a cycle leaves unplaced go to column 0.
fn topological_columns(
    nodes: &[SankeyNodeDatum],
    links: &[SankeyLinkDatum],
) -> (OrderedMap<usize>, usize) {
    let mut in_degree: OrderedMap<i64> = OrderedMap::default();
    let mut out_edges: OrderedMap<Vec<SharedString>> = OrderedMap::default();
    for n in nodes {
        in_degree.set(n.id.clone(), 0);
        out_edges.set(n.id.clone(), Vec::new());
    }
    for l in links {
        let d = in_degree.get(&l.target).copied().unwrap_or(0);
        in_degree.set(l.target.clone(), d + 1);
        if let Some(i) = out_edges.index(&l.source) {
            out_edges.0[i].1.push(l.target.clone());
        }
    }
    let mut col: OrderedMap<usize> = OrderedMap::default();
    let mut queue = std::collections::VecDeque::new();
    for n in nodes {
        if in_degree.get(&n.id) == Some(&0) {
            queue.push_back(n.id.clone());
            col.set(n.id.clone(), 0);
        }
    }
    while let Some(id) = queue.pop_front() {
        let c = col.get(&id).copied().unwrap_or(0);
        for tgt in out_edges.get(&id).cloned().unwrap_or_default() {
            let next = (col.get(&tgt).copied().unwrap_or(0)).max(c + 1);
            col.set(tgt.clone(), next);
            let d = in_degree.get(&tgt).copied().unwrap_or(0) - 1;
            in_degree.set(tgt.clone(), d);
            if d == 0 {
                queue.push_back(tgt);
            }
        }
    }
    let max = col.0.iter().map(|(_, c)| *c).max().unwrap_or(0);
    (col, max)
}

/// Upstream `resolveColumnCount`: how many columns the chart has.
pub fn column_count(
    columns: Option<&[SankeyColumn]>,
    nodes: &[SankeyNodeDatum],
    links: &[SankeyLinkDatum],
) -> usize {
    match columns {
        Some(columns) => columns.len(),
        None => topological_columns(nodes, links).1 + 1,
    }
}

/// Upstream `autoColumns`.
fn auto_columns(nodes: &[SankeyNodeDatum], links: &[SankeyLinkDatum]) -> Vec<SankeyColumnDef> {
    let (col, max) = topological_columns(nodes, links);
    let mut columns = vec![SankeyColumnDef::default(); max + 1];
    for n in nodes {
        columns[col.get(&n.id).copied().unwrap_or(0)]
            .ids
            .push(n.id.clone());
    }
    columns
}

/// The layout's sizes (upstream `LayoutOptions`).
#[derive(Clone, Debug)]
pub struct LayoutOptions<'a> {
    pub width: f64,
    pub height: f64,
    pub node_width: f64,
    pub node_gap: f64,
    pub label_margin: f64,
    pub columns: Option<&'a [SankeyColumn]>,
}

/// `value || 0`.
fn or_zero(v: Option<f64>) -> f64 {
    match v {
        Some(v) if v != 0. && !v.is_nan() => v,
        _ => 0.,
    }
}

/// Upstream `computeLayout`: node and link positions. Each column's nodes stack,
/// `node_gap` apart, centered in the height left after the labels and headers; a
/// value is `value_scale` px tall, the scale that fits the largest column; the
/// columns spread across the width; each link takes the next slice of its source's
/// and its target's height, in link order.
pub fn compute_layout(
    nodes: &[SankeyNodeDatum],
    links: &[SankeyLinkDatum],
    options: &LayoutOptions<'_>,
) -> SankeyLayout {
    let LayoutOptions {
        width,
        height,
        node_width,
        node_gap,
        label_margin,
        ..
    } = *options;
    let col_defs: Vec<SankeyColumnDef> = match options.columns {
        Some(columns) => columns.iter().map(SankeyColumn::normalized).collect(),
        None => auto_columns(nodes, links),
    };
    let col_count = col_defs.len();
    let has_headers = col_defs
        .iter()
        .any(|c| c.label.as_ref().is_some_and(|l| !l.is_empty()));
    let header_margin = if has_headers { 20. } else { 0. };
    let mut node_map: OrderedMap<SankeyNodeDatum> = OrderedMap::default();
    for n in nodes {
        node_map.set(n.id.clone(), n.clone());
    }
    let usable = height - label_margin - 16. - header_margin;
    let mut max_col_value = 0.;
    for col in &col_defs {
        let total = col
            .ids
            .iter()
            .fold(0., |s, id| s + or_zero(node_map.get(id).map(|n| n.value)));
        if total > max_col_value {
            max_col_value = total;
        }
    }
    let max_nodes = col_defs
        .iter()
        .map(|c| c.ids.len() as f64)
        .fold(f64::NEG_INFINITY, js_max);
    let value_scale = (usable - (max_nodes - 1.) * node_gap) / max_col_value;
    let col_spacing = if col_count > 1 {
        (width - node_width) / (col_count - 1) as f64
    } else {
        0.
    };

    let mut columns = Vec::new();
    let mut laid: OrderedMap<SankeyNodeLayout> = OrderedMap::default();
    let mut color_index = 0;
    for (ci, col) in col_defs.iter().enumerate() {
        let x = ci as f64 * col_spacing;
        columns.push(SankeyColumnLayout {
            x,
            label: col.label.clone(),
            ids: col.ids.clone(),
        });
        let total_h = col.ids.iter().fold(0., |s, id| {
            s + or_zero(node_map.get(id).map(|n| n.value)) * value_scale
        });
        let total_gap = (col.ids.len() as f64 - 1.) * node_gap;
        let mut y = label_margin + (usable - total_h - total_gap) / 2.;
        for id in &col.ids {
            let Some(node) = node_map.get(id).cloned() else {
                continue;
            };
            let h = node.value * value_scale;
            let color = node
                .color
                .unwrap_or(DEFAULT_PALETTE[color_index % DEFAULT_PALETTE.len()]);
            color_index += 1;
            laid.set(
                id.clone(),
                SankeyNodeLayout {
                    id: id.clone(),
                    label: node.label.clone(),
                    value: node.value,
                    color,
                    x,
                    y,
                    width: node_width,
                    height: h,
                    column: ci,
                    source_offset: 0.,
                    target_offset: 0.,
                },
            );
            y += h + node_gap;
        }
    }

    let mut layout_links = Vec::new();
    for link in links {
        let (Some(s), Some(t)) = (laid.index(&link.source), laid.index(&link.target)) else {
            continue;
        };
        let lh = link.value * value_scale;
        let source_y = laid.0[s].1.y + laid.0[s].1.source_offset;
        let target_y = laid.0[t].1.y + laid.0[t].1.target_offset;
        laid.0[s].1.source_offset += lh;
        laid.0[t].1.target_offset += lh;
        layout_links.push(SankeyLinkLayout {
            source: s,
            target: t,
            value: link.value,
            height: lh,
            source_y,
            target_y,
        });
    }

    SankeyLayout {
        nodes: laid.0.into_iter().map(|(_, n)| n).collect(),
        links: layout_links,
        columns,
        value_scale,
        max_value: max_col_value,
    }
}

/// What a SankeyChart provides to its marks (upstream `SankeyContext`).
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyContext {
    pub nodes: Vec<SankeyNodeLayout>,
    pub links: Vec<SankeyLinkLayout>,
    pub columns: Vec<SankeyColumnLayout>,
    pub width: f64,
    pub height: f64,
    pub value_scale: f64,
    pub max_value: f64,
    pub node_width: f64,
    /// Upstream `nodeColor`: one CSS color for every node.
    pub node_color: Option<SharedString>,
}

/// The sankey context the marks read (upstream `useSankey`).
///
/// # Panics
///
/// Outside a [`SankeyChart`], as upstream throws.
pub fn use_sankey(cx: &App) -> SankeyContext {
    use_context::<SankeyContext>(cx).expect("Sankey components must be used inside <SankeyChart>")
}

/// Above this many links the hidden table is left out (upstream `MAX_TABLE_LINKS`).
pub const MAX_TABLE_LINKS: usize = 100;

/// The hidden From / To / Value table for up to [`MAX_TABLE_LINKS`] links.
pub fn sankey_table(
    nodes: &[SankeyNodeDatum],
    links: &[SankeyLinkDatum],
    label: &str,
) -> Option<DataTable> {
    if links.is_empty() || links.len() > MAX_TABLE_LINKS {
        return None;
    }
    // Upstream's `new Map(nodes.map(n => [n.id, n.label || n.id]))`: a later node wins.
    let name = |id: &SharedString| -> SharedString {
        nodes
            .iter()
            .rev()
            .find(|n| n.id == *id)
            .map(|n| {
                if n.label.is_empty() {
                    n.id.clone()
                } else {
                    n.label.clone()
                }
            })
            .unwrap_or_else(|| id.clone())
    };
    Some(DataTable {
        caption: format!("{label} data").into(),
        columns: vec!["From".into(), "To".into(), "Value".into()],
        rows: links
            .iter()
            .map(|l| {
                vec![
                    name(&l.source),
                    name(&l.target),
                    rustryx_charts::ChartValue::Number(l.value).to_js_string(),
                ]
            })
            .collect(),
    })
}

/// The chart's width for a container `container` px wide (at least
/// `min_column_width` per column), and whether it scrolls.
pub fn chart_width(container: f64, column_count: usize, min_column_width: f64) -> (f64, bool) {
    let width = js_max(container, column_count as f64 * min_column_width);
    (width, container > 0. && width > container)
}

/// The root of a Sankey (flow) diagram (upstream lab `SankeyChart`).
///
/// ```ignore
/// SankeyChart::new("funnel", nodes, links)
///     .columns([SankeyColumn::labeled("Source", ["a", "b"]), SankeyColumn::labeled("Target", ["c", "d"])])
///     .child(SankeyGrid::new())
///     .child(SankeyLink::new())
///     .child(SankeyNode::new())
///     .child(SankeyLabel::new())
/// ```
#[derive(IntoElement)]
pub struct SankeyChart {
    id: ElementId,
    nodes: Rc<[SankeyNodeDatum]>,
    links: Rc<[SankeyLinkDatum]>,
    columns: Option<Rc<[SankeyColumn]>>,
    height: f64,
    node_width: f64,
    node_gap: f64,
    node_color: Option<SharedString>,
    min_column_width: f64,
    label: SharedString,
    children: Vec<AnyElement>,
}

impl SankeyChart {
    /// A diagram of `nodes` (upstream `nodes`) and `links` (upstream `links`).
    pub fn new(
        id: impl Into<ElementId>,
        nodes: impl IntoIterator<Item = SankeyNodeDatum>,
        links: impl IntoIterator<Item = SankeyLinkDatum>,
    ) -> Self {
        SankeyChart {
            id: id.into(),
            nodes: nodes.into_iter().collect(),
            links: links.into_iter().collect(),
            columns: None,
            height: 320.,
            node_width: 20.,
            node_gap: 14.,
            node_color: None,
            min_column_width: 160.,
            label: "Sankey chart".into(),
            children: Vec::new(),
        }
    }

    /// Upstream `columns`: the node ids of each column; detected from the links when
    /// unset.
    pub fn columns(mut self, columns: impl IntoIterator<Item = SankeyColumn>) -> Self {
        self.columns = Some(columns.into_iter().collect());
        self
    }

    /// Upstream `height`, px (320 by default).
    pub fn height(mut self, height: f64) -> Self {
        self.height = height;
        self
    }

    /// Upstream `nodeWidth`: the node bars' width, px (20 by default).
    pub fn node_width(mut self, width: f64) -> Self {
        self.node_width = width;
        self
    }

    /// Upstream `nodeGap`: the gap between a column's nodes, px (14 by default).
    pub fn node_gap(mut self, gap: f64) -> Self {
        self.node_gap = gap;
        self
    }

    /// Upstream `nodeColor`: one CSS color (hex, `rgb()` or a basic name) for every
    /// node bar; the labels on the bars pick their text color for it.
    pub fn node_color(mut self, color: impl Into<SharedString>) -> Self {
        self.node_color = Some(color.into());
        self
    }

    /// Upstream `minColumnWidth`: the narrowest a column may be, px (160 by default);
    /// below it the chart scrolls sideways.
    pub fn min_column_width(mut self, width: f64) -> Self {
        self.min_column_width = width;
        self
    }

    /// Upstream `label`: the accessible name, of the plot and of the scroll region
    /// (`"Sankey chart"` by default).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
}

impl ParentElement for SankeyChart {
    /// Upstream `children`: the marks, drawn in order.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for SankeyChart {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let key = self.id.to_string();
        let table = sankey_table(&self.nodes, &self.links, &self.label)
            .map(|t| table_element(&format!("{key}-sankey"), t));
        let count = column_count(self.columns.as_deref(), &self.nodes, &self.links);
        let height = self.height;
        let plot_height = px(if height.is_finite() && height > 0. {
            height as f32
        } else {
            0.
        });
        let SankeyChart {
            nodes,
            links,
            columns,
            node_width,
            node_gap,
            node_color,
            min_column_width,
            label,
            children,
            ..
        } = self;
        let scroll_id = ElementId::Name(format!("{key}-sankey-scroll").into());
        let plot_id = ElementId::Name(format!("{key}-sankey-plot").into());
        let selector = format!("{key}-sankey-plot");
        let scroll_selector = format!("{key}-sankey-scroll");
        let plot = MeasuredPlot::new(
            ElementId::Name(format!("{key}-measure").into()),
            plot_height,
            move |container, window, cx| {
                let (width, scrolls) = chart_width(f64::from(container), count, min_column_width);
                let layout = compute_layout(
                    &nodes,
                    &links,
                    &LayoutOptions {
                        width,
                        height,
                        node_width,
                        node_gap,
                        label_margin: 28.,
                        columns: columns.as_deref(),
                    },
                );
                let ctx = SankeyContext {
                    nodes: layout.nodes,
                    links: layout.links,
                    columns: layout.columns,
                    width,
                    height,
                    value_scale: layout.value_scale,
                    max_value: layout.max_value,
                    node_width,
                    node_color,
                };
                let svg = div()
                    .id(plot_id)
                    .debug_selector(move || selector)
                    .relative()
                    .flex_none()
                    .w(px(width as f32))
                    .h(plot_height)
                    .role(Role::Image)
                    .aria_label(label.clone())
                    .children(children);
                let svg = provide(ctx, svg).into_any_element();
                if !scrolls {
                    return Some((svg, plot_height));
                }
                // Wider than its container: a named, focusable region that scrolls sideways.
                let area = use_scrollable_area(
                    &scroll_id,
                    UseScrollableAreaOptions::new(
                        ScrollAxis::Inline,
                        ScrollKeyboardAccess::Viewport {
                            label: label.clone(),
                            role: ScrollRole::Group,
                        },
                    ),
                    window,
                    cx,
                );
                let scroller = area
                    .viewport(div().w(container).h(plot_height).flex())
                    .debug_selector(move || scroll_selector)
                    .role(Role::Group)
                    .aria_label(label)
                    .child(svg);
                Some((scroller.into_any_element(), plot_height))
            },
        );
        div().w_full().child(plot).children(table)
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `SankeyChart` (its doc lists no props; these are its component's).
pub const SANKEY_CHART_DOC: ComponentDoc = ComponentDoc {
    name: "SankeyChart",
    rust_type: "rustryx::lab::sankey::SankeyChart",
    props: &[
        p("nodes", RustApi::Argument("nodes (SankeyNodeDatum)")),
        p("links", RustApi::Argument("links (SankeyLinkDatum)")),
        p("columns", RustApi::Method("columns")),
        p("height", RustApi::Method("height")),
        p("nodeWidth", RustApi::Method("node_width")),
        p("nodeGap", RustApi::Method("node_gap")),
        p("nodeColor", RustApi::Method("node_color")),
        p("minColumnWidth", RustApi::Method("min_column_width")),
        p("label", RustApi::Method("label")),
        p("children", RustApi::Trait("ParentElement")),
    ],
    example: include_str!("examples.rs"),
};
