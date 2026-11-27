//! `ThreeDSurface` (upstream lab `ThreeDSurface.tsx`): a mesh over a grid of x and z values,
//! its quads far to near, colored by their height.

use std::collections::HashMap;

use gpui::prelude::*;
use gpui::{App, Bounds, Hsla, Pixels, Window, black, canvas, div};
use rustryx_charts::shape::PathStyle;
use rustryx_charts::{ChartValue, Datum, js_number, js_string};

use super::bar::polygon;
use super::context::{ThreeDContext, use_3d};
use super::scatter::svg_opacity;
use super::types::ProjectedPoint;
use crate::lab_chart::paint::paint_path;

/// A 3D surface (upstream lab `ThreeDSurface`).
#[derive(IntoElement)]
pub struct ThreeDSurface {
    color_range: Vec<Hsla>,
    opacity: f64,
    wireframe: bool,
}

impl ThreeDSurface {
    /// Upstream `colorRange`: the ramp from low to high (a sequential palette).
    pub fn new(color_range: impl IntoIterator<Item = Hsla>) -> Self {
        ThreeDSurface {
            color_range: color_range.into_iter().collect(),
            opacity: 0.8,
            wireframe: false,
        }
    }

    /// Upstream `opacity`: the faces' fill (0.8).
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }

    /// Upstream `wireframe`: outlines only (off).
    pub fn wireframe(mut self, wireframe: bool) -> Self {
        self.wireframe = wireframe;
        self
    }
}

/// The ramp's color nearest `t` (upstream `lerpColor`, which picks the nearer stop): `None`
/// where upstream's lookup finds no stop (an empty ramp, a NaN height), which SVG paints
/// black.
pub fn lerp_color(colors: &[Hsla], t: f64) -> Option<Hsla> {
    let clamped = t.clamp(0., 1.);
    if colors.len() == 1 {
        return Some(colors[0]);
    }
    if colors.is_empty() || clamped.is_nan() {
        return None;
    }
    let scaled = clamped * (colors.len() - 1) as f64;
    let lo = scaled.floor() as usize;
    let hi = (lo + 1).min(colors.len() - 1);
    Some(if scaled - (lo as f64) < 0.5 {
        colors[lo]
    } else {
        colors[hi]
    })
}

/// One face: its four projected corners, its color and its average depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceFace {
    /// Upstream `corners`: the cell's four corners in grid order.
    pub corners: [ProjectedPoint; 4],
    /// Upstream `color`: the ramp's color for the cell's mean height (none for an empty ramp).
    pub color: Option<Hsla>,
    /// Upstream `depth`: the corners' mean depth, for sorting.
    pub depth: f64,
}

/// JavaScript's SameValueZero, which a `Set` dedupes by.
fn same_value_zero(a: &Option<ChartValue>, b: &Option<ChartValue>) -> bool {
    match (a, b) {
        (Some(ChartValue::Number(x)), Some(ChartValue::Number(y))) => {
            x == y || (x.is_nan() && y.is_nan())
        }
        (Some(ChartValue::Date(_)), Some(ChartValue::Date(_))) => false,
        _ => a == b,
    }
}

/// A key's distinct values in first-seen order, sorted by `a - b` (upstream's `[...new Set()]`
/// and `sort`).
fn distinct_sorted(data: &[Datum], key: &str) -> Vec<Option<ChartValue>> {
    let mut values: Vec<Option<ChartValue>> = Vec::new();
    for d in data {
        let v = d.get(key).cloned();
        if !values.iter().any(|u| same_value_zero(u, &v)) {
            values.push(v);
        }
    }
    super::js_sort(&mut values, |a, b| {
        js_number(a.as_ref()) - js_number(b.as_ref())
    });
    values
}

/// The surface's faces far to near (upstream's `faces` memo): one per grid cell whose four
/// corners exist; none for fewer than two x or z values.
pub fn surface_faces(ctx: &ThreeDContext, colors: &[Hsla]) -> Vec<SurfaceFace> {
    let xs = distinct_sorted(&ctx.data, &ctx.x_key);
    let zs = distinct_sorted(&ctx.data, &ctx.z_key);
    if xs.len() < 2 || zs.len() < 2 {
        return Vec::new();
    }
    let key = |x: Option<&ChartValue>, z: Option<&ChartValue>| {
        format!("{},{}", js_string(x), js_string(z))
    };
    let mut grid: HashMap<String, &Datum> = HashMap::new();
    for d in ctx.data.iter() {
        grid.insert(key(d.get(&ctx.x_key), d.get(&ctx.z_key)), d);
    }
    let mut faces = Vec::new();
    for r in 0..zs.len() - 1 {
        for c in 0..xs.len() - 1 {
            let at = |x: &Option<ChartValue>, z: &Option<ChartValue>| {
                grid.get(&key(x.as_ref(), z.as_ref())).copied()
            };
            let corners = [
                at(&xs[c], &zs[r]),
                at(&xs[c + 1], &zs[r]),
                at(&xs[c + 1], &zs[r + 1]),
                at(&xs[c], &zs[r + 1]),
            ];
            let [Some(a), Some(b), Some(cc), Some(dd)] = corners else {
                continue;
            };
            let projected = [a, b, cc, dd].map(|d| {
                let (nx, ny, nz) = ctx.normalized(d);
                (ctx.project(nx, ny, nz), ny)
            });
            let avg_y = projected.iter().map(|p| p.1).sum::<f64>() / 4.;
            let depth = projected.iter().map(|p| p.0.depth).sum::<f64>() / 4.;
            faces.push(SurfaceFace {
                corners: projected.map(|p| p.0),
                color: lerp_color(colors, avg_y),
                depth,
            });
        }
    }
    super::sort_by_depth(&mut faces, |f| f.depth);
    faces
}

/// How a face is painted: its fill and its stroke (color and width), each absent when SVG would
/// draw none. Wireframe faces are outlines at 0.8 opacity; filled faces take the `opacity` prop
/// (SVG's black when the ramp gives no color) with a faint 0.5-pixel edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceStyle {
    /// The fill with its opacity, or none (SVG's `fill="none"`).
    pub fill: Option<Hsla>,
    /// The stroke with its opacity, and its width.
    pub stroke: Option<(Hsla, f64)>,
}

/// Upstream's per-face `fill`, `fillOpacity`, `stroke`, `strokeWidth` and `strokeOpacity`.
pub fn face_style(color: Option<Hsla>, wireframe: bool, opacity: f64) -> FaceStyle {
    if wireframe {
        FaceStyle {
            fill: None,
            stroke: color.map(|c| (c.opacity(0.8), 1.)),
        }
    } else {
        FaceStyle {
            fill: Some(color.unwrap_or(black()).opacity(svg_opacity(opacity))),
            stroke: color.map(|c| (c.opacity(0.3), 0.5)),
        }
    }
}

impl RenderOnce for ThreeDSurface {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_3d(cx) else {
            return div().into_any_element();
        };
        let faces = surface_faces(&ctx, &self.color_range);
        let (opacity, wireframe) = (self.opacity, self.wireframe);
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                for f in &faces {
                    let path = polygon(&f.corners);
                    let style = face_style(f.color, wireframe, opacity);
                    if let Some(fill) = style.fill {
                        paint_path(window, bounds.origin, &path, PathStyle::Fill, fill);
                    }
                    if let Some((stroke, width)) = style.stroke {
                        paint_path(
                            window,
                            bounds.origin,
                            &path,
                            PathStyle::Stroke {
                                width: width as f32,
                            },
                            stroke,
                        );
                    }
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}
