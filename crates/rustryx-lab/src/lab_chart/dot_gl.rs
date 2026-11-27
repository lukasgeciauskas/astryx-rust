//! `ChartDotGL` (upstream lab `ChartDotGL.tsx`): a WebGL scatter, drawn with GPUI's quads
//! over the chart's other children.

use gpui::prelude::*;
use gpui::{App, Hsla, SharedString, Window, div};

use super::context::{ChartContext, use_chart};
use super::paint::fill_circle;
use super::utils::{number_or_zero, x_pixel};
use super::webgl::{POINT_SIZE_COMPENSATION, canvas_dpr, hex_to_gl, premultiplied};

/// A WebGL scatter (upstream lab `ChartDotGL`): one point sprite per datum, anti-aliased,
/// drawn above the chart's SVG content as upstream's canvas is.
#[derive(IntoElement)]
pub struct ChartDotGl {
    data_key: SharedString,
    color: Hsla,
    size: f64,
    opacity: f64,
}

impl ChartDotGl {
    /// Upstream `dataKey` and `color`.
    pub fn new(data_key: impl Into<SharedString>, color: Hsla) -> Self {
        ChartDotGl {
            data_key: data_key.into(),
            color,
            size: 6.,
            opacity: 0.8,
        }
    }

    /// Upstream `size`: the points' diameter (6).
    pub fn size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }

    /// Upstream `opacity` (0.8).
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity;
        self
    }
}

/// The points' positions (upstream's `getPositions`, a `Float32Array`).
pub fn dot_gl_positions(ctx: &ChartContext, data_key: &str) -> Vec<[f32; 2]> {
    ctx.data
        .iter()
        .map(|d| {
            [
                x_pixel(d, &ctx.x_key, &ctx.x_scale) as f32,
                ctx.y_scale.scale(number_or_zero(d, data_key)) as f32,
            ]
        })
        .collect()
}

/// The visible circle's radius of a `size` point at `scale_factor`: the sprite is
/// `size * dpr / 0.96` device pixels of the supersampled canvas (at least one, none at a
/// size that is not positive), and the circle ends at 0.48 of it.
pub fn dot_gl_radius(size: f64, scale_factor: f32) -> Option<f64> {
    let dpr = canvas_dpr(scale_factor);
    let sprite = size * dpr * POINT_SIZE_COMPENSATION;
    (sprite > 0.).then(|| sprite.max(1.) * 0.48 / dpr)
}

impl RenderOnce for ChartDotGl {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Some(ctx) = use_chart(cx) else {
            return div();
        };
        if ctx.width <= 0. || ctx.height <= 0. {
            return div();
        }
        let positions = dot_gl_positions(&ctx, &self.data_key);
        let color = premultiplied(hex_to_gl(self.color), self.opacity);
        let size = self.size;
        ctx.paint_over(move |origin, window, _| {
            let (Some(color), Some(r)) = (color, dot_gl_radius(size, window.scale_factor())) else {
                return;
            };
            for [x, y] in &positions {
                fill_circle(window, origin, (f64::from(*x), f64::from(*y)), r, color);
            }
        });
        div()
    }
}
