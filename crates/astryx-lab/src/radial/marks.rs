//! The radial marks (upstream `RadialGrid`, `RadialArea`, `RadialAxis` and
//! `RadialSlice`): each reads the chart's [`RadialContext`] and draws over the whole
//! plot, in the order the chart lists them.

use astryx_charts::ChartValue;
use astryx_theme::ThemeStyled;
use astryx_theme::authoring::js::{js_max, js_min, js_number_to_string, js_round};
use gpui::prelude::*;
use gpui::{
    App, Bounds, Hsla, PathBuilder, Pixels, SharedString, Window, canvas, div, point, px, relative,
};

use super::trig::{js_cos, js_sin};
use super::{RadialContext, use_radial};

/// `Array.from({length})`'s length: NaN and below zero are none, a fraction floors.
fn js_length(n: f64) -> usize {
    if n.is_nan() || n <= 0. {
        0
    } else {
        n.floor().min(9_007_199_254_740_991.) as usize
    }
}

/// The spider grid's geometry (upstream `RadialGrid`): a polygon per ring through
/// every axis at that ring's radius, and a spoke per axis from the inner to the outer
/// radius. `None` outside spider mode.
#[derive(Clone, Debug, PartialEq)]
pub struct GridGeometry {
    pub rings: Vec<Vec<(f64, f64)>>,
    pub spokes: Vec<[(f64, f64); 2]>,
}

/// The grid for `rings` concentric rings.
pub fn grid_geometry(ctx: &RadialContext, rings: f64) -> Option<GridGeometry> {
    let axes = ctx.axes.as_ref()?;
    ctx.angle_by_axis.as_ref()?;
    let ring_polygons = (0..js_length(rings))
        .map(|i| {
            let t = (i as f64 + 1.) / rings;
            let r = ctx.radius_scale(t).unwrap_or_default();
            axes.iter()
                .filter_map(|key| {
                    let angle = ctx.angle(key)?;
                    Some((ctx.cx + js_cos(angle) * r, ctx.cy + js_sin(angle) * r))
                })
                .collect()
        })
        .collect();
    let spokes = axes
        .iter()
        .filter_map(|key| {
            let angle = ctx.angle(key)?;
            Some([
                (
                    ctx.cx + js_cos(angle) * ctx.inner_radius,
                    ctx.cy + js_sin(angle) * ctx.inner_radius,
                ),
                (
                    ctx.cx + js_cos(angle) * ctx.radius,
                    ctx.cy + js_sin(angle) * ctx.radius,
                ),
            ])
        })
        .collect();
    Some(GridGeometry {
        rings: ring_polygons,
        spokes,
    })
}

/// Points as SVG writes a `points` attribute (`x,y x,y`).
pub fn svg_points(points: &[(f64, f64)]) -> String {
    points
        .iter()
        .map(|(x, y)| format!("{},{}", js_number_to_string(*x), js_number_to_string(*y)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A spider polygon's vertices (upstream `RadialArea`'s points): the row with a field
/// equal to `data_key` (else the first row), each axis value normalized within its
/// domain to 0–1 and placed on the radius scale.
pub fn area_points(ctx: &RadialContext, data_key: &str) -> Vec<(SharedString, f64, f64)> {
    let (Some(axes), Some(_)) = (ctx.axes.as_ref(), ctx.angle_by_axis.as_ref()) else {
        return Vec::new();
    };
    // The row whose string field matches, or the first.
    let datum = ctx
        .data
        .iter()
        .find(|d| {
            d.fields()
                .any(|(_, v)| matches!(v, ChartValue::Text(s) if s == data_key))
        })
        .or_else(|| ctx.data.first());
    let Some(datum) = datum else {
        return Vec::new();
    };
    axes.iter()
        .map(|key| {
            let (Some(angle), Some([min, max])) = (ctx.angle(key), ctx.domain(key)) else {
                return (key.clone(), ctx.cx, ctx.cy);
            };
            let raw = match datum.get(key) {
                Some(ChartValue::Number(v)) => *v,
                _ => 0.,
            };
            let t = if max > min {
                (raw - min) / (max - min)
            } else {
                0.
            };
            let r = ctx
                .radius_scale(js_max(0., js_min(1., t)))
                .unwrap_or_default();
            (
                key.clone(),
                ctx.cx + js_cos(angle) * r,
                ctx.cy + js_sin(angle) * r,
            )
        })
        .collect()
}

/// How a label lines up with its point (upstream `textAnchor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}

impl TextAnchor {
    pub fn as_str(self) -> &'static str {
        match self {
            TextAnchor::Start => "start",
            TextAnchor::Middle => "middle",
            TextAnchor::End => "end",
        }
    }

    /// The share of its width a label moves back by.
    fn shift(self) -> f32 {
        match self {
            TextAnchor::Start => 0.,
            TextAnchor::Middle => -0.5,
            TextAnchor::End => -1.,
        }
    }
}

/// An axis label (upstream `RadialAxis`'s texts).
#[derive(Clone, Debug, PartialEq)]
pub struct AxisLabel {
    pub key: SharedString,
    pub x: f64,
    pub y: f64,
    pub anchor: TextAnchor,
}

/// The axis labels, `offset` px beyond the outer ring: anchored at their start on the
/// right, their end on the left and their middle at the top and bottom.
pub fn axis_labels(ctx: &RadialContext, offset: f64) -> Vec<AxisLabel> {
    let (Some(axes), Some(_)) = (ctx.axes.as_ref(), ctx.angle_by_axis.as_ref()) else {
        return Vec::new();
    };
    axes.iter()
        .filter_map(|key| {
            let angle = ctx.angle(key)?;
            let cos = js_cos(angle);
            let anchor = if cos > 0.1 {
                TextAnchor::Start
            } else if cos < -0.1 {
                TextAnchor::End
            } else {
                TextAnchor::Middle
            };
            Some(AxisLabel {
                key: key.clone(),
                x: ctx.cx + cos * (ctx.radius + offset),
                y: ctx.cy + js_sin(angle) * (ctx.radius + offset),
                anchor,
            })
        })
        .collect()
}

/// A slice's outline (upstream `arcPath`'s inputs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arc {
    pub cx: f64,
    pub cy: f64,
    pub inner: f64,
    pub outer: f64,
    pub start: f64,
    pub end: f64,
}

/// Upstream `arcPath` as its SVG path: a pie wedge when the inner radius is zero,
/// else an annular sector.
pub fn arc_path(cx: f64, cy: f64, inner: f64, outer: f64, start: f64, end: f64) -> String {
    let n = js_number_to_string;
    let (x1, y1) = (cx + js_cos(start) * outer, cy + js_sin(start) * outer);
    let (x2, y2) = (cx + js_cos(end) * outer, cy + js_sin(end) * outer);
    let (x3, y3) = (cx + js_cos(end) * inner, cy + js_sin(end) * inner);
    let (x4, y4) = (cx + js_cos(start) * inner, cy + js_sin(start) * inner);
    let large = if end - start > std::f64::consts::PI {
        1
    } else {
        0
    };
    if inner == 0. {
        return format!(
            "M {} {} L {} {} A {} {} 0 {large} 1 {} {} Z",
            n(cx),
            n(cy),
            n(x1),
            n(y1),
            n(outer),
            n(outer),
            n(x2),
            n(y2)
        );
    }
    format!(
        "M {} {} A {} {} 0 {large} 1 {} {} L {} {} A {} {} 0 {large} 0 {} {} Z",
        n(x1),
        n(y1),
        n(outer),
        n(outer),
        n(x2),
        n(y2),
        n(x3),
        n(y3),
        n(inner),
        n(inner),
        n(x4),
        n(y4)
    )
}

/// A slice as it draws (upstream `RadialSlice`'s paths and labels).
#[derive(Clone, Debug, PartialEq)]
pub struct SliceGeometry {
    pub index: usize,
    pub key: SharedString,
    pub arc: Arc,
    /// The percentage label at 60% of the way out on the slice's middle angle, when
    /// it shows.
    pub label: Option<(f64, f64, SharedString)>,
}

/// The slices, with a label where its share reaches `label_threshold` percent.
pub fn slice_geometry(
    ctx: &RadialContext,
    labels: bool,
    label_threshold: f64,
) -> Vec<SliceGeometry> {
    let Some(slices) = ctx.slices.as_ref() else {
        return Vec::new();
    };
    slices
        .iter()
        .enumerate()
        .map(|(index, s)| {
            let mid = (s.start_angle + s.end_angle) / 2.;
            let r = ctx.inner_radius + (ctx.radius - ctx.inner_radius) * 0.6;
            let shown = labels && s.percentage * 100. >= label_threshold;
            SliceGeometry {
                index,
                key: s.key.clone(),
                arc: Arc {
                    cx: ctx.cx,
                    cy: ctx.cy,
                    inner: ctx.inner_radius,
                    outer: ctx.radius,
                    start: s.start_angle,
                    end: s.end_angle,
                },
                label: shown.then(|| {
                    (
                        ctx.cx + js_cos(mid) * r,
                        ctx.cy + js_sin(mid) * r,
                        format!("{}%", js_number_to_string(js_round(s.percentage * 100.))).into(),
                    )
                }),
            }
        })
        .collect()
}

// ----------------------------------------------------------------------------
// Painting

pub(crate) fn at(bounds: Bounds<Pixels>, (x, y): (f64, f64)) -> gpui::Point<Pixels> {
    bounds.origin + point(px(x as f32), px(y as f32))
}

pub(crate) fn faded(color: Hsla, opacity: f64) -> Hsla {
    Hsla {
        a: color.a * opacity.clamp(0., 1.) as f32,
        ..color
    }
}

/// Strokes a polyline (closed when `close`).
pub(crate) fn stroke_points(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    points: &[(f64, f64)],
    close: bool,
    width: f64,
    color: Hsla,
) {
    if points.len() < 2 || points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return;
    }
    let mut b = PathBuilder::stroke(px(width as f32));
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            b.move_to(at(bounds, *p));
        } else {
            b.line_to(at(bounds, *p));
        }
    }
    if close {
        b.close();
    }
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// Fills a polygon.
pub(crate) fn fill_points(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    points: &[(f64, f64)],
    color: Hsla,
) {
    if points.len() < 3 || points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return;
    }
    let mut b = PathBuilder::fill();
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            b.move_to(at(bounds, *p));
        } else {
            b.line_to(at(bounds, *p));
        }
    }
    b.close();
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// Fills a circle.
pub(crate) fn fill_circle(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    c: (f64, f64),
    r: f64,
    color: Hsla,
) {
    if r.partial_cmp(&0.) != Some(std::cmp::Ordering::Greater)
        || !c.0.is_finite()
        || !c.1.is_finite()
    {
        return;
    }
    let r = px(r as f32);
    let center = at(bounds, c);
    window.paint_quad(
        gpui::fill(
            Bounds::new(center - point(r, r), gpui::size(r * 2., r * 2.)),
            color,
        )
        .corner_radii(r),
    );
}

/// Strokes a circle.
pub(crate) fn stroke_circle(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    c: (f64, f64),
    r: f64,
    width: f64,
    color: Hsla,
) {
    if r.partial_cmp(&0.) != Some(std::cmp::Ordering::Greater)
        || !c.0.is_finite()
        || !c.1.is_finite()
    {
        return;
    }
    let half = px((width / 2.) as f32);
    let outer = px(r as f32) + half;
    let center = at(bounds, c);
    window.paint_quad(gpui::quad(
        Bounds::new(
            center - point(outer, outer),
            gpui::size(outer * 2., outer * 2.),
        ),
        outer,
        gpui::transparent_black(),
        px(width as f32),
        color,
        gpui::BorderStyle::Solid,
    ));
}

/// The slice's outline as a path (upstream `arcPath`, drawn).
fn arc_builder(bounds: Bounds<Pixels>, a: &Arc, mut b: PathBuilder) -> Option<gpui::Path> {
    let values = [a.cx, a.cy, a.inner, a.outer, a.start, a.end];
    if values.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let p = |r: f64, angle: f64| at(bounds, (a.cx + js_cos(angle) * r, a.cy + js_sin(angle) * r));
    let large = a.end - a.start > std::f64::consts::PI;
    let (ro, ri) = (px(a.outer.abs() as f32), px(a.inner.abs() as f32));
    if a.inner == 0. {
        b.move_to(at(bounds, (a.cx, a.cy)));
        b.line_to(p(a.outer, a.start));
        b.arc_to(point(ro, ro), px(0.), large, true, p(a.outer, a.end));
    } else {
        b.move_to(p(a.outer, a.start));
        b.arc_to(point(ro, ro), px(0.), large, true, p(a.outer, a.end));
        b.line_to(p(a.inner, a.end));
        b.arc_to(point(ri, ri), px(0.), large, false, p(a.inner, a.start));
    }
    b.close();
    b.build().ok()
}

/// A label at a point, anchored as SVG's `text-anchor` and centered on it vertically
/// (`dominant-baseline: central`).
pub(crate) fn text_at(x: f64, y: f64, anchor: TextAnchor, text: SharedString) -> gpui::Div {
    div()
        .absolute()
        .left(px(x as f32))
        .top(px(y as f32))
        .translate(relative(anchor.shift()), relative(-0.5))
        .whitespace_nowrap()
        .child(text)
}

/// Concentric grid rings and spokes for a spider chart (upstream lab `RadialGrid`).
#[derive(IntoElement)]
pub struct RadialGrid {
    rings: f64,
}

impl Default for RadialGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl RadialGrid {
    /// A grid of five rings.
    pub fn new() -> Self {
        RadialGrid { rings: 5. }
    }

    /// Upstream `rings`: how many concentric rings (5 by default).
    pub fn rings(mut self, rings: impl Into<f64>) -> Self {
        self.rings = rings.into();
        self
    }
}

impl RenderOnce for RadialGrid {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = use_radial(cx);
        let theme = astryx_theme::current(window, cx);
        let color = faded(theme.tokens().color.border, 0.3);
        let geometry = grid_geometry(&ctx, self.rings);
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let Some(g) = &geometry else { return };
                for ring in &g.rings {
                    stroke_points(window, bounds, ring, true, 1., color);
                }
                for spoke in &g.spokes {
                    stroke_points(window, bounds, spoke, false, 1., color);
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}

/// A spider (radar) polygon for one row (upstream lab `RadialArea`).
#[derive(IntoElement)]
pub struct RadialArea {
    data_key: SharedString,
    color: Hsla,
    opacity: f64,
    stroke_width: f64,
    dots: bool,
    dot_radius: f64,
}

impl RadialArea {
    /// The polygon of the row a field of which is `data_key` (upstream `dataKey`; the
    /// first row when none is), in `color` (upstream `color`).
    pub fn new(data_key: impl Into<SharedString>, color: impl Into<Hsla>) -> Self {
        RadialArea {
            data_key: data_key.into(),
            color: color.into(),
            opacity: 0.2,
            stroke_width: 2.,
            dots: false,
            dot_radius: 4.,
        }
    }

    /// Upstream `opacity`: the fill's opacity (0.2 by default).
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }

    /// Upstream `strokeWidth` (2 by default).
    pub fn stroke_width(mut self, width: f64) -> Self {
        self.stroke_width = width;
        self
    }

    /// Upstream `dots`: a dot at each vertex.
    pub fn dots(mut self, dots: bool) -> Self {
        self.dots = dots;
        self
    }

    /// Upstream `dotRadius` (4 by default).
    pub fn dot_radius(mut self, radius: f64) -> Self {
        self.dot_radius = radius;
        self
    }
}

impl RenderOnce for RadialArea {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = use_radial(cx);
        let points: Vec<(f64, f64)> = area_points(&ctx, &self.data_key)
            .into_iter()
            .map(|(_, x, y)| (x, y))
            .collect();
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                if points.is_empty() {
                    return;
                }
                fill_points(window, bounds, &points, faded(self.color, self.opacity));
                stroke_points(window, bounds, &points, true, self.stroke_width, self.color);
                if self.dots {
                    for p in &points {
                        fill_circle(window, bounds, *p, self.dot_radius, self.color);
                    }
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}

/// Axis labels at each spider vertex (upstream lab `RadialAxis`).
#[derive(IntoElement)]
pub struct RadialAxis {
    label_offset: f64,
}

impl Default for RadialAxis {
    fn default() -> Self {
        Self::new()
    }
}

impl RadialAxis {
    /// Labels 16 px beyond the outer ring.
    pub fn new() -> Self {
        RadialAxis { label_offset: 16. }
    }

    /// Upstream `labelOffset`: the labels' distance beyond the outer ring, px.
    pub fn label_offset(mut self, offset: f64) -> Self {
        self.label_offset = offset;
        self
    }
}

impl RenderOnce for RadialAxis {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = use_radial(cx);
        let theme = astryx_theme::current(window, cx);
        let t = theme.tokens();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .font_stack(&t.font.family.body)
            .text_size(px(12.))
            .line_height(relative(1.))
            .text_color(t.color.text_secondary)
            .children(
                axis_labels(&ctx, self.label_offset)
                    .into_iter()
                    .filter(|l| l.x.is_finite() && l.y.is_finite())
                    .map(|l| text_at(l.x, l.y, l.anchor, l.key)),
            )
    }
}

/// Pie or donut slices (upstream lab `RadialSlice`).
#[derive(IntoElement)]
pub struct RadialSlice {
    colors: Vec<Hsla>,
    corner_radius: f64,
    labels: bool,
    label_threshold: f64,
}

impl RadialSlice {
    /// Slices colored in turn by `colors` (upstream `colors`, as
    /// `use_chart_colors(..).categorical(n)` gives them).
    pub fn new(colors: impl IntoIterator<Item = impl Into<Hsla>>) -> Self {
        RadialSlice {
            colors: colors.into_iter().map(Into::into).collect(),
            corner_radius: 0.,
            labels: true,
            label_threshold: 5.,
        }
    }

    /// Upstream `cornerRadius`: above zero the slices lose their separating stroke.
    pub fn corner_radius(mut self, radius: f64) -> Self {
        self.corner_radius = radius;
        self
    }

    /// Upstream `labels`: percentage labels on the slices (on by default).
    pub fn labels(mut self, labels: bool) -> Self {
        self.labels = labels;
        self
    }

    /// Upstream `labelThreshold`: the smallest share, in percent, that gets a label
    /// (5 by default).
    pub fn label_threshold(mut self, percent: f64) -> Self {
        self.label_threshold = percent;
        self
    }
}

impl RenderOnce for RadialSlice {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let ctx = use_radial(cx);
        let theme = astryx_theme::current(window, cx);
        let t = theme.tokens();
        let slices = slice_geometry(&ctx, self.labels, self.label_threshold);
        let surface = t.color.background_surface;
        let stroke = if self.corner_radius > 0. { 0. } else { 1. };
        // No colors leaves SVG's default fill, black.
        let colors = self.colors.clone();
        let fills: Vec<Hsla> = (0..slices.len())
            .map(|i| {
                if colors.is_empty() {
                    gpui::black()
                } else {
                    colors[i % colors.len()]
                }
            })
            .collect();
        let arcs: Vec<Arc> = slices.iter().map(|s| s.arc).collect();
        let labels: Vec<_> = slices.iter().filter_map(|s| s.label.clone()).collect();
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        for (arc, fill) in arcs.iter().zip(&fills) {
                            if let Some(path) = arc_builder(bounds, arc, PathBuilder::fill()) {
                                window.paint_path(path, *fill);
                            }
                            if stroke > 0.
                                && let Some(path) =
                                    arc_builder(bounds, arc, PathBuilder::stroke(px(stroke as f32)))
                            {
                                window.paint_path(path, surface);
                            }
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .font_stack(&t.font.family.body)
            .text_size(px(12.))
            .line_height(relative(1.))
            .font_weight(gpui::FontWeight(500.))
            .text_color(t.color.text_primary)
            .children(
                labels
                    .into_iter()
                    .filter(|(x, y, _)| x.is_finite() && y.is_finite())
                    .map(|(x, y, text)| text_at(x, y, TextAnchor::Middle, text)),
            )
    }
}
