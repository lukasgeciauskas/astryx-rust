//! `ThreeDScatter` (upstream lab `ThreeDScatter.tsx`): projected circles, far to near, sized and
//! faded by depth.

use gpui::prelude::*;
use gpui::{App, Bounds, Hsla, Pixels, Window, canvas, div};

use super::context::{ThreeDContext, use_3d};
use crate::lab_chart::paint::fill_circle;

/// A 3D scatter (upstream lab `ThreeDScatter`).
#[derive(IntoElement)]
pub struct ThreeDScatter {
    color: Hsla,
    radius: f64,
    opacity: f64,
}

impl ThreeDScatter {
    /// Upstream `color`.
    pub fn new(color: Hsla) -> Self {
        ThreeDScatter {
            color,
            radius: 4.,
            opacity: 0.85,
        }
    }

    /// Upstream `radius` (4), at the nearest depth.
    pub fn radius(mut self, radius: f64) -> Self {
        self.radius = radius;
        self
    }

    /// Upstream `opacity` (0.85), at the nearest depth.
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }
}

/// One projected point and the datum it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScatterPoint3D {
    /// Upstream `px`: the horizontal position in the plot.
    pub px: f64,
    /// Upstream `py`: the vertical position in the plot.
    pub py: f64,
    /// Upstream `depth`: larger is nearer.
    pub depth: f64,
    /// Upstream `index`: the datum's position in the data.
    pub index: usize,
}

/// A point's depth factor (upstream `0.75 + (depth + 0.5) * 0.25`): about 0.5 far to 1 near.
pub fn depth_factor(depth: f64) -> f64 {
    0.75 + (depth + 0.5) * 0.25
}

/// The points far to near (upstream's `points` memo, the painter's algorithm).
pub fn scatter_points(ctx: &ThreeDContext) -> Vec<ScatterPoint3D> {
    let mut points: Vec<ScatterPoint3D> = ctx
        .data
        .iter()
        .enumerate()
        .map(|(index, d)| {
            let (nx, ny, nz) = ctx.normalized(d);
            let p = ctx.project(nx, ny, nz);
            ScatterPoint3D {
                px: p.px,
                py: p.py,
                depth: p.depth,
                index,
            }
        })
        .collect();
    super::sort_by_depth(&mut points, |p| p.depth);
    points
}

/// An SVG opacity as browsers apply it: clamped to 0 to 1.
pub(crate) fn svg_opacity(opacity: f64) -> f32 {
    if opacity.is_nan() {
        1.
    } else {
        opacity.clamp(0., 1.) as f32
    }
}

impl RenderOnce for ThreeDScatter {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_3d(cx) else {
            return div().into_any_element();
        };
        let points = scatter_points(&ctx);
        canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, _, window, _| {
                for p in &points {
                    let f = depth_factor(p.depth);
                    fill_circle(
                        window,
                        bounds.origin,
                        (p.px, p.py),
                        self.radius * f,
                        self.color.opacity(svg_opacity(self.opacity * f)),
                    );
                }
            },
        )
        .size_full()
        .into_any_element()
    }
}
