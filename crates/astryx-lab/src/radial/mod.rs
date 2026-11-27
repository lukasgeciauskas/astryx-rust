//! `RadialChart`: the root of a radial chart, spider (radar), pie or donut (upstream
//! lab `RadialChart`). It owns the radial coordinate space (center, radius, angle and
//! radius scales) and provides it to the marks inside it: [`RadialGrid`],
//! [`RadialArea`], [`RadialAxis`], [`RadialSlice`] and [`RadialTooltip`].
//!
//! - The chart is as wide as its container and `height` tall; it draws nothing until
//!   it knows its width (upstream's ResizeObserver).
//! - The plot is a named image (`Role::Image`, labelled by `label` or an English
//!   description of the mode and keys), and a small dataset is mirrored in a visually
//!   hidden table: slices in pie mode, axis values in spider mode.

pub(crate) mod marks;
pub(crate) mod plot;
mod tooltip;
pub(crate) use crate::trig;

pub mod examples;

#[cfg(test)]
mod tests;

use std::f64::consts::PI;
use std::rc::Rc;

use astryx_charts::{ChartValue, Datum, js_string};
use astryx_components::doc::{ComponentDoc, PropMapping, RustApi};
use astryx_components::layout::VisuallyHidden;
use astryx_primitives::{provide, use_context};
use astryx_theme::authoring::js::js_to_fixed;
use gpui::prelude::*;
use gpui::{AnyElement, App, ElementId, Role, SharedString, Window, div, px};

pub use marks::{
    Arc, AxisLabel, GridGeometry, RadialArea, RadialAxis, RadialGrid, RadialSlice, SliceGeometry,
    TextAnchor, arc_path, area_points, axis_labels, grid_geometry, slice_geometry, svg_points,
};
pub use tooltip::{RadialTooltip, RadialTooltipDatum, hit_test_pie, hit_test_spider};

use plot::MeasuredPlot;

/// The chart's mode (upstream `RadialMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadialMode {
    /// A spider (radar) chart: one axis per key.
    Spider,
    /// A pie, or a donut with an inner radius.
    Pie,
}

impl RadialMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RadialMode::Spider => "spider",
            RadialMode::Pie => "pie",
        }
    }
}

/// One pie slice (upstream's `slices` entries).
#[derive(Clone, Debug, PartialEq)]
pub struct RadialSliceDatum {
    /// The slice's label: `String(d[labelKey])`, else `String(value)`.
    pub key: SharedString,
    pub value: f64,
    /// Angles in radians, clockwise from the right; the first starts at the top.
    pub start_angle: f64,
    pub end_angle: f64,
    /// Its share of the total, 0 to 1.
    pub percentage: f64,
}

/// What a RadialChart provides to its marks (upstream `RadialContext`).
#[derive(Clone, Debug, PartialEq)]
pub struct RadialContext {
    /// The center, in the plot's pixels.
    pub cx: f64,
    pub cy: f64,
    /// The outer radius, in pixels.
    pub radius: f64,
    /// The inner radius, in pixels (above zero for a donut).
    pub inner_radius: f64,
    /// The full dataset.
    pub data: Rc<[Datum]>,
    pub mode: RadialMode,
    /// Spider mode: the axis keys, as given.
    pub axes: Option<Vec<SharedString>>,
    /// Spider mode: each axis's angle in radians (upstream's `angleByAxis` map, in its
    /// insertion order; a repeated key keeps its first place and its last angle).
    pub angle_by_axis: Option<Vec<(SharedString, f64)>>,
    /// Spider mode: each axis's `[min, max]` (upstream `axisDomains`).
    pub axis_domains: Option<Vec<(SharedString, [f64; 2])>>,
    /// Pie mode: the slices with their angles (upstream `slices`).
    pub slices: Option<Vec<RadialSliceDatum>>,
    /// Upstream's `interactive`: touch scrubs the tooltip.
    pub interactive: bool,
}

impl RadialContext {
    /// A spider axis's angle (upstream `angleByAxis.get(key)`).
    pub fn angle(&self, key: &str) -> Option<f64> {
        self.angle_by_axis
            .as_ref()?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, a)| *a)
    }

    /// A spider axis's domain (upstream `axisDomains.get(key)`).
    pub fn domain(&self, key: &str) -> Option<[f64; 2]> {
        self.axis_domains
            .as_ref()?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, d)| *d)
    }

    /// Upstream `radiusScale`: a normalized value (0 to 1) as a distance from the
    /// center; `None` outside spider mode.
    pub fn radius_scale(&self, t: f64) -> Option<f64> {
        self.angle_by_axis
            .as_ref()
            .map(|_| self.inner_radius + t * (self.radius - self.inner_radius))
    }
}

/// Upstream's `Map.set`: a new key goes last, a known key keeps its place.
fn map_set<V>(map: &mut Vec<(SharedString, V)>, key: SharedString, value: V) {
    match map.iter_mut().find(|(k, _)| *k == key) {
        Some(slot) => slot.1 = value,
        None => map.push((key, value)),
    }
}

/// The options [`radial_context`] computes from (RadialChart's props).
#[derive(Clone, Debug)]
pub struct RadialOptions {
    pub axes: Option<Vec<SharedString>>,
    pub value_key: Option<SharedString>,
    pub label_key: Option<SharedString>,
    pub inner_radius: f64,
    pub pad_angle: f64,
    pub interactive: bool,
}

impl Default for RadialOptions {
    fn default() -> Self {
        RadialOptions {
            axes: None,
            value_key: None,
            label_key: None,
            inner_radius: 0.,
            pad_angle: 0.02,
            interactive: false,
        }
    }
}

/// The radial coordinate space for a `width` × `height` plot (upstream RadialChart's
/// computation): the center, the radius (half the smaller side less 40 for labels),
/// the spider axes and domains, and the pie slices.
pub fn radial_context(
    data: Rc<[Datum]>,
    width: f64,
    height: f64,
    options: &RadialOptions,
) -> RadialContext {
    use astryx_theme::authoring::js::js_min;
    let size = js_min(width, height);
    let cx = width / 2.;
    let cy = height / 2.;
    let outer = size / 2. - 40.;
    let inner = outer * options.inner_radius;
    let mode = if options.axes.is_some() {
        RadialMode::Spider
    } else {
        RadialMode::Pie
    };

    let mut angle_by_axis = None;
    let mut axis_domains = None;
    let mut axes = None;
    if let Some(keys) = options.axes.as_ref().filter(|a| !a.is_empty()) {
        let step = (2. * PI) / keys.len() as f64;
        let mut angles = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            // From the top (-PI/2), clockwise.
            map_set(&mut angles, key.clone(), -PI / 2. + step * i as f64);
        }
        let mut domains = Vec::new();
        for key in keys {
            let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
            for d in data.iter() {
                if let Some(ChartValue::Number(v)) = d.get(key) {
                    if *v < min {
                        min = *v;
                    }
                    if *v > max {
                        max = *v;
                    }
                }
            }
            // Zero is the floor.
            if min > 0. {
                min = 0.;
            }
            map_set(&mut domains, key.clone(), [min, max]);
        }
        angle_by_axis = Some(angles);
        axis_domains = Some(domains);
        axes = Some(keys.clone());
    }

    let slices = options
        .value_key
        .as_ref()
        .filter(|k| !k.is_empty())
        .map(|value_key| {
            let number = |d: &Datum| match d.get(value_key) {
                Some(ChartValue::Number(v)) => *v,
                _ => 0.,
            };
            let total: f64 = data.iter().map(number).fold(0., |a, b| a + b);
            if total == 0. {
                return Vec::new();
            }
            let available = 2. * PI - options.pad_angle * data.len() as f64;
            let mut current = -PI / 2.;
            data.iter()
                .map(|d| {
                    let v = number(d);
                    let percentage = v / total;
                    let sweep = percentage * available;
                    let key = match options.label_key.as_ref().filter(|k| !k.is_empty()) {
                        Some(label_key) => js_string(d.get(label_key)),
                        None => ChartValue::Number(v).to_js_string(),
                    };
                    let slice = RadialSliceDatum {
                        key,
                        value: v,
                        start_angle: current,
                        end_angle: current + sweep,
                        percentage,
                    };
                    current += sweep + options.pad_angle;
                    slice
                })
                .collect()
        });

    RadialContext {
        cx,
        cy,
        radius: outer,
        inner_radius: inner,
        data,
        mode,
        axes,
        angle_by_axis,
        axis_domains,
        slices,
        interactive: options.interactive,
    }
}

/// The chart's accessible name (upstream's default): `label`, else "Radar chart of"
/// the axes in spider mode, else "Pie chart of" the value key.
pub fn radial_label(
    label: Option<&str>,
    axes: Option<&[SharedString]>,
    value_key: Option<&str>,
) -> SharedString {
    if let Some(label) = label {
        return label.to_string().into();
    }
    match axes {
        Some(axes) => format!(
            "Radar chart of {}",
            axes.iter()
                .map(|a| a.as_ref())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .into(),
        None => format!("Pie chart of {}", value_key.unwrap_or("values")).into(),
    }
}

/// Above this many data points (rows × axes, or slices) the hidden table is left out
/// (upstream `MAX_TABLE_POINTS`).
pub const MAX_TABLE_POINTS: usize = 100;

/// A hidden data table: its caption, column headers and rows (row header first).
#[derive(Clone, Debug, PartialEq)]
pub struct DataTable {
    pub caption: SharedString,
    pub columns: Vec<SharedString>,
    pub rows: Vec<Vec<SharedString>>,
}

/// Upstream `spiderRowHeader`: a row's first string field, else "Series n".
fn spider_row_header(row: &Datum, index: usize) -> SharedString {
    row.fields()
        .find_map(|(_, v)| match v {
            ChartValue::Text(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| format!("Series {}", index + 1).into())
}

/// The visually hidden table upstream renders for a small dataset, if any: slice
/// labels, values and percentages in pie mode, a row per series across the axes in
/// spider mode.
pub fn radial_table(
    ctx: &RadialContext,
    label: &str,
    label_key: Option<&str>,
    value_key: Option<&str>,
) -> Option<DataTable> {
    let caption: SharedString = format!("{label} data").into();
    match ctx.mode {
        RadialMode::Pie => {
            let slices = ctx.slices.as_deref().unwrap_or_default();
            if slices.is_empty() || slices.len() > MAX_TABLE_POINTS {
                return None;
            }
            Some(DataTable {
                caption,
                columns: vec![
                    label_key.unwrap_or("Label").to_string().into(),
                    value_key.unwrap_or("").to_string().into(),
                    "Percentage".into(),
                ],
                rows: slices
                    .iter()
                    .map(|s| {
                        vec![
                            s.key.clone(),
                            ChartValue::Number(s.value).to_js_string(),
                            format!("{}%", js_to_fixed(s.percentage * 100., 1)).into(),
                        ]
                    })
                    .collect(),
            })
        }
        RadialMode::Spider => {
            let axes = ctx.axes.as_deref()?;
            if axes.is_empty()
                || ctx.data.is_empty()
                || ctx.data.len() * axes.len() > MAX_TABLE_POINTS
            {
                return None;
            }
            let mut columns = vec![SharedString::from("Series")];
            columns.extend(axes.iter().cloned());
            Some(DataTable {
                caption,
                columns,
                rows: ctx
                    .data
                    .iter()
                    .enumerate()
                    .map(|(i, row)| {
                        let mut cells = vec![spider_row_header(row, i)];
                        cells.extend(axes.iter().map(|key| match row.get(key) {
                            None | Some(ChartValue::Null) => SharedString::default(),
                            Some(v) => v.to_js_string(),
                        }));
                        cells
                    })
                    .collect(),
            })
        }
    }
}

/// The hidden table as accessibility nodes.
pub(crate) fn table_element(prefix: &str, table: DataTable) -> AnyElement {
    let cell = |text: SharedString, role: Role, id: String| {
        div().id(id).role(role).aria_label(text.clone()).child(text)
    };
    let header = div()
        .id(format!("{prefix}-table-head"))
        .role(Role::Row)
        .children(
            table
                .columns
                .iter()
                .enumerate()
                .map(|(i, c)| cell(c.clone(), Role::ColumnHeader, format!("{prefix}-h{i}"))),
        );
    let rows = table.rows.iter().enumerate().map(|(r, row)| {
        div()
            .id(format!("{prefix}-table-row-{r}"))
            .role(Role::Row)
            .children(row.iter().enumerate().map(|(c, text)| {
                let role = if c == 0 { Role::RowHeader } else { Role::Cell };
                cell(text.clone(), role, format!("{prefix}-r{r}-{c}"))
            }))
    });
    let selector = format!("{prefix}-table");
    VisuallyHidden::new()
        .child(
            div()
                .id(format!("{prefix}-table"))
                .debug_selector(move || selector)
                .role(Role::Table)
                .aria_label(table.caption)
                .child(header)
                .children(rows),
        )
        .into_any_element()
}

/// The root of a radial chart, spider, pie or donut (upstream lab `RadialChart`).
///
/// ```ignore
/// RadialChart::new("models", data)
///     .axes(["speed", "handling", "comfort"])
///     .child(RadialGrid::new().rings(5))
///     .child(RadialArea::new("Model A", color))
///     .child(RadialAxis::new())
/// RadialChart::new("regions", data).value_key("revenue").label_key("region").child(RadialSlice::new(colors))
/// ```
#[derive(IntoElement)]
pub struct RadialChart {
    id: ElementId,
    data: Rc<[Datum]>,
    height: f64,
    options: RadialOptions,
    label: Option<SharedString>,
    children: Vec<AnyElement>,
}

impl RadialChart {
    /// A chart of `data` (upstream `data`); `id` keys its parts.
    pub fn new(id: impl Into<ElementId>, data: impl Into<Rc<[Datum]>>) -> Self {
        RadialChart {
            id: id.into(),
            data: data.into(),
            height: 400.,
            options: RadialOptions::default(),
            label: None,
            children: Vec::new(),
        }
    }

    /// Upstream `height`, in pixels (400 by default); the width follows the container.
    pub fn height(mut self, height: f64) -> Self {
        self.height = height;
        self
    }

    /// Upstream `axes`: spider mode, one axis per key.
    pub fn axes(mut self, axes: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.options.axes = Some(axes.into_iter().map(Into::into).collect());
        self
    }

    /// Upstream `valueKey`: pie mode (without `axes`), the field each slice's value is.
    pub fn value_key(mut self, key: impl Into<SharedString>) -> Self {
        self.options.value_key = Some(key.into());
        self
    }

    /// Upstream `labelKey`: the field each slice's label is.
    pub fn label_key(mut self, key: impl Into<SharedString>) -> Self {
        self.options.label_key = Some(key.into());
        self
    }

    /// Upstream `innerRadius`: the inner radius as a fraction of the outer (0 by
    /// default; 0.6 is a donut).
    pub fn inner_radius(mut self, fraction: f64) -> Self {
        self.options.inner_radius = fraction;
        self
    }

    /// Upstream `padAngle`: the gap between pie slices, radians (0.02 by default).
    pub fn pad_angle(mut self, radians: f64) -> Self {
        self.options.pad_angle = radians;
        self
    }

    /// Upstream `interactive`: touch scrubs the tooltip, and the chart keeps the
    /// finger from scrolling the page.
    pub fn interactive(mut self, interactive: bool) -> Self {
        self.options.interactive = interactive;
        self
    }

    /// Upstream `label`: the accessible name (an English description of the chart by
    /// default).
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl ParentElement for RadialChart {
    /// Upstream `children`: the marks, drawn in order.
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// The radial context the marks read (upstream `useRadial`).
///
/// # Panics
///
/// Outside a [`RadialChart`], as upstream throws.
pub fn use_radial(cx: &App) -> RadialContext {
    use_context::<RadialContext>(cx).expect("Radial components must be used inside <RadialChart>")
}

impl RenderOnce for RadialChart {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let key = self.id.to_string();
        let height = if self.height.is_finite() && self.height > 0. {
            self.height
        } else {
            0.
        };
        let label = radial_label(
            self.label.as_deref(),
            self.options.axes.as_deref(),
            self.options.value_key.as_deref(),
        );
        // The table depends only on the data (its slices' shares, not their angles).
        let table_ctx = radial_context(self.data.clone(), 1., 1., &self.options);
        let table = radial_table(
            &table_ctx,
            &label,
            self.options.label_key.as_deref(),
            self.options.value_key.as_deref(),
        )
        .map(|t| table_element(&format!("{key}-radial"), t));
        let data = self.data.clone();
        let options = self.options.clone();
        let children = self.children;
        let plot_id = ElementId::Name(format!("{key}-plot").into());
        let selector = format!("{key}-radial-plot");
        let plot = MeasuredPlot::new(
            ElementId::Name(format!("{key}-measure").into()),
            px(height as f32),
            move |width, _, _| {
                let ctx = radial_context(data, f64::from(width), height, &options);
                let plot = div()
                    .id(plot_id)
                    .debug_selector(move || selector)
                    .relative()
                    .w(width)
                    .h(px(height as f32))
                    .role(Role::Image)
                    .aria_label(label)
                    .children(children);
                Some((provide(ctx, plot).into_any_element(), px(height as f32)))
            },
        );
        div().w_full().child(plot).children(table)
    }
}

const fn p(prop: &'static str, rust: RustApi) -> PropMapping {
    PropMapping { prop, rust }
}

/// Upstream lab `RadialChart` (its doc lists no props; these are its component's).
pub const RADIAL_CHART_DOC: ComponentDoc = ComponentDoc {
    name: "RadialChart",
    rust_type: "astryx::lab::radial::RadialChart",
    props: &[
        p("data", RustApi::Argument("data")),
        p("height", RustApi::Method("height")),
        p("axes", RustApi::Method("axes")),
        p("valueKey", RustApi::Method("value_key")),
        p("labelKey", RustApi::Method("label_key")),
        p("innerRadius", RustApi::Method("inner_radius")),
        p("padAngle", RustApi::Method("pad_angle")),
        p("interactive", RustApi::Method("interactive")),
        p("label", RustApi::Method("label")),
        p("children", RustApi::Trait("ParentElement")),
    ],
    example: include_str!("examples.rs"),
};
