//! `ThreeDScatterGL` (upstream lab `ThreeDScatterGL.tsx`): the 3D scatter as upstream's WebGL points,
//! drawn with GPUI's GPU quads above the chart's other children: the vertex shader's
//! projection in 32-bit floats, its clipping, its depth-scaled sprites and their
//! premultiplied, edge-smoothed alpha, in data order (upstream sorts nothing and tests no
//! depth).

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{App, Hsla, Window, div};

use super::context::{ThreeDContext, use_3d};
use super::types::ProjectedPoint;
use crate::lab_chart::paint::fill_circle;
use crate::lab_chart::{canvas_dpr, hex_to_gl};

/// A WebGL 3D scatter (upstream lab `ThreeDScatterGL`).
#[derive(IntoElement)]
pub struct ThreeDScatterGl {
    color: Hsla,
    size: f64,
    opacity: f64,
}

impl ThreeDScatterGl {
    /// Upstream `color`.
    pub fn new(color: Hsla) -> Self {
        ThreeDScatterGl {
            color,
            size: 4.,
            opacity: 0.85,
        }
    }

    /// Upstream `size`: the points' diameter at the nearest depth (4).
    pub fn size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }

    /// Upstream `opacity` (0.85).
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }
}

/// The positions upstream uploads (a `Float32Array` of each datum's normalized x, y, z).
pub fn gl_positions(ctx: &ThreeDContext) -> Vec<[f32; 3]> {
    ctx.data
        .iter()
        .map(|d| {
            let (x, y, z) = ctx.normalized(d);
            [x as f32, y as f32, z as f32]
        })
        .collect()
}

/// A point the GPU draws: where, its sprite's visible radius and its alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlPoint {
    /// The shader's projection of the point (`gl_Position` in plot pixels, and `z2`).
    pub at: ProjectedPoint,
    /// The visible circle's radius in logical pixels.
    pub radius: f64,
    /// The fragment's alpha inside the circle (`u_opacity * v_depthFactor`).
    pub alpha: f64,
}

/// What the GPU draws for each position, in order: none for a point whose center leaves the
/// clip volume (GL drops such points whole) or whose sprite has no size. The sprite is
/// `(size × dpr × depth factor) / 0.96` device pixels of the supersampled canvas (at least one),
/// and its circle ends at 0.48 of it.
pub fn gl_points(
    ctx: &ThreeDContext,
    positions: &[[f32; 3]],
    size: f64,
    opacity: f64,
    scale_factor: f32,
) -> Vec<GlPoint> {
    let dpr = canvas_dpr(scale_factor);
    let u_size = (size * dpr) as f32;
    let (w, h) = (ctx.width as f32, ctx.height as f32);
    positions
        .iter()
        .filter_map(|p| {
            let at = ctx.projection.project_gl(*p);
            let (px, py, z2) = (at.px as f32, at.py as f32, at.depth as f32);
            let clip = [(px / w) * 2. - 1., (py / h) * 2. - 1.];
            if !(clip[0].abs() <= 1. && clip[1].abs() <= 1. && z2.abs() <= 1.) {
                return None;
            }
            let depth_factor = 0.75 + (z2 + 0.5) * 0.25;
            let sprite = (u_size * depth_factor) / 0.96;
            if sprite.is_nan() || sprite <= 0. {
                return None;
            }
            Some(GlPoint {
                at,
                radius: f64::from(sprite.max(1.)) * 0.48 / dpr,
                alpha: opacity * f64::from(depth_factor),
            })
        })
        .collect()
}

impl RenderOnce for ThreeDScatterGl {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_3d(cx) else {
            return div();
        };
        if ctx.width <= 0. || ctx.height <= 0. {
            return div();
        }
        let positions = gl_positions(&ctx);
        let (size, opacity) = (self.size, self.opacity);
        let rgb = hex_to_gl(self.color);
        // Without the overlay list, which would hold this closure in a cycle.
        let paint_ctx = ThreeDContext {
            overlays: Rc::default(),
            ..ctx.clone()
        };
        ctx.paint_over(move |origin, window, _| {
            for p in gl_points(&paint_ctx, &positions, size, opacity, window.scale_factor()) {
                if let Some(color) = crate::lab_chart::webgl_premultiplied(rgb, p.alpha) {
                    fill_circle(window, origin, (p.at.px, p.at.py), p.radius, color);
                }
            }
        });
        div()
    }
}
